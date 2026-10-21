//! A place as a project on disk, the `*.project.json` tree Argon and Rojo
//! sync from, both ways: [`import`] reads a project into a DOM, [`export`]
//! writes a DOM out as one.
//!
//! The file conventions are the ones both tools share: a folder is a
//! `Folder`, `x.server.luau` a `Script`, `x.client.luau` a `LocalScript`,
//! `x.luau` a `ModuleScript` (`.lua` too), a folder holding `init.*.luau`
//! is that script with the folder's contents as its children, `x.txt` a
//! `StringValue`, `x.model.json` an instance tree, `x.rbxm`/`x.rbxmx` a
//! model file, and `init.meta.json` (Rojo) or `.data.json` (Argon) gives a
//! folder its class and properties. Export writes scripts and folders as
//! files and everything else as one `.rbxm` per instance, so nothing is
//! lost: a part with all its properties is a model file, not a lossy JSON
//! guess at each property.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rbx_dom::{Ref, Variant, WeakDom};
use rbx_reflection::ReflectionDatabase;
use serde_json::{json, Map, Value};

use super::clipboard::{detached, graft};

const SCRIPTS: [(&str, &str); 3] = [
    ("Script", ".server.luau"),
    ("LocalScript", ".client.luau"),
    ("ModuleScript", ".luau"),
];

/// For a `*.project.json`, the place the editor opens in its stead: the
/// project imported and written beside it as `<file>.rbxl`
/// (`default.project.json` gives `default.project.rbxl`), rewritten on every
/// open, since the project is the truth. `None` for any other file.
pub(crate) fn place_for(path: &Path) -> Result<Option<PathBuf>, String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let Some(stem) = name
        .strip_suffix(".json")
        .filter(|s| s.ends_with(".project"))
    else {
        return Ok(None);
    };
    let dom = import(path)?;
    let place = path.with_file_name(format!("{stem}.rbxl"));
    crate::save::save(&dom, crate::save::Format::Binary, &place)?;
    Ok(Some(place))
}

/// Reads `project` (a `*.project.json`) into a DOM: a `DataModel` tree's
/// services become the roots, any other tree is the one root.
pub(crate) fn import(project: &Path) -> Result<WeakDom, String> {
    let text =
        std::fs::read_to_string(project).map_err(|err| format!("{}: {err}", project.display()))?;
    let json: Value =
        serde_json::from_str(&text).map_err(|err| format!("{}: {err}", project.display()))?;
    let tree = json
        .get("tree")
        .ok_or_else(|| format!("{}: no \"tree\"", project.display()))?;
    let base = project.parent().unwrap_or(Path::new("."));
    let name = json
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Project");
    let mut importer = Importer {
        dom: WeakDom::new(),
        database: ReflectionDatabase::shared(),
    };
    if tree.get("$className").and_then(Value::as_str) == Some("DataModel") {
        for (key, node) in children_of(tree) {
            importer.node(base, key, node, None)?;
        }
    } else {
        importer.node(base, name, tree, None)?;
    }
    importer.dom.take_changes();
    Ok(importer.dom)
}

struct Importer {
    dom: WeakDom,
    database: &'static ReflectionDatabase,
}

fn children_of(node: &Value) -> impl Iterator<Item = (&str, &Value)> {
    node.as_object()
        .into_iter()
        .flat_map(|map| map.iter())
        .filter(|(key, _)| !key.starts_with('$'))
        .map(|(key, value)| (key.as_str(), value))
}

impl Importer {
    /// One project node: its `$path` (if any) read from disk, then its
    /// `$className`, `$properties` and child nodes on top.
    fn node(
        &mut self,
        base: &Path,
        name: &str,
        node: &Value,
        parent: Option<Ref>,
    ) -> Result<(), String> {
        let explicit = node.get("$className").and_then(Value::as_str);
        // As in Rojo and Argon, a node named after a class is that class
        // unless it says otherwise: `"ReplicatedStorage": { "$path": … }`.
        let inferred = self.database.class(name).map(|_| name);
        let path = match node.get("$path") {
            Some(Value::String(path)) => Some(base.join(path)),
            Some(Value::Object(optional)) => optional
                .get("optional")
                .and_then(Value::as_str)
                .map(|path| base.join(path))
                .filter(|path| path.exists()),
            _ => None,
        };
        let made = match path {
            Some(path) => self.path(&path, Some(name), parent)?,
            None => Some(self.dom.new_instance(
                explicit.or(inferred).unwrap_or("Folder"),
                name,
                parent,
            )),
        };
        let Some(made) = made else {
            return Ok(());
        };
        let is_folder = self.dom.get(made).is_some_and(|i| i.class() == "Folder");
        if let Some(class) = explicit.or(inferred.filter(|_| is_folder)) {
            let _ = self.dom.set_class(made, class);
        }
        if let Some(properties) = node.get("$properties").and_then(Value::as_object) {
            self.properties(made, properties);
        }
        for (key, child) in children_of(node) {
            self.node(base, key, child, Some(made))?;
        }
        Ok(())
    }

    /// A file or folder on disk, named `name` when the project gives it one.
    fn path(
        &mut self,
        path: &Path,
        name: Option<&str>,
        parent: Option<Ref>,
    ) -> Result<Option<Ref>, String> {
        if path.is_dir() {
            return self.folder(path, name, parent).map(Some);
        }
        let file = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let read = || std::fs::read(path).map_err(|err| format!("{}: {err}", path.display()));
        let text = || String::from_utf8_lossy(&read().unwrap_or_default()).into_owned();
        if let Some((class, stem)) = script_of(file) {
            let made = self.dom.new_instance(class, name.unwrap_or(stem), parent);
            let _ = self
                .dom
                .set_property(made, "Source", Variant::String(text()));
            return Ok(Some(made));
        }
        if let Some(stem) = file.strip_suffix(".model.json") {
            let json: Value = serde_json::from_slice(&read()?)
                .map_err(|err| format!("{}: {err}", path.display()))?;
            return Ok(Some(self.model_json(&json, name.unwrap_or(stem), parent)));
        }
        if let Some(stem) = file
            .strip_suffix(".rbxm")
            .or_else(|| file.strip_suffix(".rbxmx"))
        {
            let model = rbx_viewer::parse_place(&read()?, path)?;
            let mut first = None;
            for &root in model.root_refs() {
                let made = match parent {
                    Some(parent) => graft(&mut self.dom, &model, root, parent),
                    None => {
                        // A root model: grafted under a scratch parent, then lifted out.
                        let scratch = self.dom.new_instance("Folder", "", None);
                        let made = graft(&mut self.dom, &model, root, scratch);
                        if let Some(made) = made {
                            self.dom.set_parent(made, None);
                        }
                        self.dom.remove(scratch);
                        made
                    }
                };
                first = first.or(made);
            }
            if let (Some(made), true) = (first, model.root_refs().len() == 1) {
                let _ = self.dom.set_name(made, name.unwrap_or(stem));
            }
            return Ok(first);
        }
        if let Some(stem) = file.strip_suffix(".txt") {
            let made = self
                .dom
                .new_instance("StringValue", name.unwrap_or(stem), parent);
            let _ = self
                .dom
                .set_property(made, "Value", Variant::String(text()));
            return Ok(Some(made));
        }
        // Anything else (`.json` modules, `.csv` tables, editor files) has no
        // instance here.
        Ok(None)
    }

    fn folder(
        &mut self,
        dir: &Path,
        name: Option<&str>,
        parent: Option<Ref>,
    ) -> Result<Ref, String> {
        let name = name.map(str::to_owned).unwrap_or_else(|| {
            dir.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|err| format!("{}: {err}", dir.display()))?
            .flatten()
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        let file_name = |path: &PathBuf| {
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_owned()
        };
        let init = entries.iter().find_map(|path| {
            let file = file_name(path);
            let (class, stem) = script_of(&file)?;
            (stem == "init").then(|| (class, path.clone()))
        });
        let made = match &init {
            Some((class, path)) => {
                let made = self.dom.new_instance(class, &name, parent);
                let source = std::fs::read_to_string(path).unwrap_or_default();
                let _ = self
                    .dom
                    .set_property(made, "Source", Variant::String(source));
                made
            }
            None => self.dom.new_instance("Folder", &name, parent),
        };
        for meta in ["init.meta.json", ".data.json"] {
            let path = dir.join(meta);
            if let Ok(bytes) = std::fs::read(&path) {
                let json: Value = serde_json::from_slice(&bytes)
                    .map_err(|err| format!("{}: {err}", path.display()))?;
                self.meta(made, &json);
            }
        }
        for path in &entries {
            let file = file_name(path);
            let skip = file.starts_with('.')
                || file.ends_with(".meta.json")
                || file.ends_with(".project.json")
                || file == "sourcemap.json"
                || script_of(&file).is_some_and(|(_, stem)| stem == "init");
            if skip {
                continue;
            }
            if let Some(child) = self.path(path, None, Some(made))? {
                let meta = path.with_file_name(format!("{}.meta.json", stem_of(&file)));
                if let Ok(bytes) = std::fs::read(&meta) {
                    if let Ok(json) = serde_json::from_slice::<Value>(&bytes) {
                        self.meta(child, &json);
                    }
                }
            }
        }
        Ok(made)
    }

    /// `className`/`properties` from a meta file (either spelling).
    fn meta(&mut self, made: Ref, json: &Value) {
        let class = json.get("className").or_else(|| json.get("ClassName"));
        if let Some(class) = class.and_then(Value::as_str) {
            let _ = self.dom.set_class(made, class);
        }
        let properties = json.get("properties").or_else(|| json.get("Properties"));
        if let Some(properties) = properties.and_then(Value::as_object) {
            self.properties(made, properties);
        }
    }

    /// A Rojo JSON model: `ClassName`, `Properties`, `Children` (either case).
    fn model_json(&mut self, json: &Value, name: &str, parent: Option<Ref>) -> Ref {
        let field = |upper: &str, lower: &str| json.get(upper).or_else(|| json.get(lower));
        let class = field("ClassName", "className")
            .and_then(Value::as_str)
            .unwrap_or("Folder");
        let name = field("Name", "name")
            .and_then(Value::as_str)
            .unwrap_or(name);
        let made = self.dom.new_instance(class, name, parent);
        if let Some(properties) = field("Properties", "properties").and_then(Value::as_object) {
            self.properties(made, properties);
        }
        for child in field("Children", "children")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            self.model_json(child, "Instance", Some(made));
        }
        made
    }

    fn properties(&mut self, made: Ref, properties: &Map<String, Value>) {
        let class = self
            .dom
            .get(made)
            .map(|i| i.class().to_owned())
            .unwrap_or_default();
        for (key, value) in properties {
            if let Some(variant) = to_variant(self.database, &class, key, value) {
                let _ = self.dom.set_property(made, key, variant);
            }
        }
    }
}

/// The script class and name a file stands for, `None` for a non-script.
fn script_of(file: &str) -> Option<(&'static str, &str)> {
    for extension in [".luau", ".lua"] {
        let Some(stem) = file.strip_suffix(extension) else {
            continue;
        };
        return Some(if let Some(stem) = stem.strip_suffix(".server") {
            ("Script", stem)
        } else if let Some(stem) = stem.strip_suffix(".client") {
            ("LocalScript", stem)
        } else {
            ("ModuleScript", stem)
        });
    }
    None
}

/// `x` for `x.server.luau`, `x.model.json`, `x.rbxm` and the like.
fn stem_of(file: &str) -> &str {
    script_of(file)
        .map(|(_, stem)| stem)
        .or_else(|| file.strip_suffix(".model.json"))
        .or_else(|| file.rsplit_once('.').map(|(stem, _)| stem))
        .unwrap_or(file)
}

/// A project-file property value as a `Variant` of the type the class's
/// property has: the implicit forms (a bool, a number, a string, an enum
/// item's name, an array for a vector or colour).
fn to_variant(
    database: &ReflectionDatabase,
    class: &str,
    key: &str,
    value: &Value,
) -> Option<Variant> {
    let kind = property_type(database, class, key).unwrap_or_default();
    let numbers = || {
        value.as_array().map(|items| {
            items
                .iter()
                .filter_map(Value::as_f64)
                .map(|n| n as f32)
                .collect::<Vec<_>>()
        })
    };
    Some(match (kind.as_str(), value) {
        ("bool", Value::Bool(b)) | (_, Value::Bool(b)) => Variant::Bool(*b),
        ("int" | "int32", Value::Number(n)) => Variant::Int32(n.as_i64()? as i32),
        ("int64", Value::Number(n)) => Variant::Int64(n.as_i64()?),
        ("double", Value::Number(n)) => Variant::Float64(n.as_f64()?),
        (_, Value::Number(n)) => Variant::Float32(n.as_f64()? as f32),
        ("Vector3", _) => {
            let v = numbers()?;
            Variant::Vector3(rbx_dom::Vector3Data {
                x: *v.first()?,
                y: *v.get(1)?,
                z: *v.get(2)?,
            })
        }
        ("Color3", _) => {
            let v = numbers()?;
            Variant::Color3(rbx_dom::Color3Data {
                r: *v.first()?,
                g: *v.get(1)?,
                b: *v.get(2)?,
            })
        }
        (kind, Value::String(text)) => {
            let item = database
                .enum_items(kind)
                .and_then(|items| items.iter().find(|(name, _)| name == text));
            match item {
                Some((_, value)) => Variant::Enum(*value),
                None => Variant::String(text.clone()),
            }
        }
        _ => return None,
    })
}

fn property_type(database: &ReflectionDatabase, class: &str, key: &str) -> Option<String> {
    database
        .resolve_property(class, key)
        .map(|property| property.value_type.clone())
}

/// Writes `dom` out as a project under `dir`: `default.project.json` and a
/// `src/<Service>` folder per one of Studio's fixed services that has
/// anything in it. Returns the
/// project file's path.
pub(crate) fn export(dom: &WeakDom, dir: &Path, name: &str) -> Result<PathBuf, String> {
    let src = dir.join("src");
    std::fs::create_dir_all(&src).map_err(io(&src))?;
    let mut tree = Map::new();
    tree.insert("$className".into(), json!("DataModel"));
    let mut used = BTreeMap::new();
    for &root in dom.root_refs() {
        let Some(service) = dom.get(root) else {
            continue;
        };
        // Studio's own fixed services, the ones its Explorer lists: the rest
        // (AvatarSettings, InsertService, …) are engine state, not something
        // a project syncs, and Argon can't read some of what Studio keeps in
        // them (AvatarRules.Tags).
        if rbx_viewer::services::rank(service.class()).is_none() {
            continue;
        }
        let key = unique(&mut used, service.name());
        let mut node = Map::new();
        let properties = json_properties(dom, root);
        if !properties.is_empty() {
            node.insert("$properties".into(), Value::Object(properties));
        }
        if service.children().is_empty() {
            node.insert("$className".into(), json!(service.class()));
        } else {
            // Argon refuses `$className` beside `$path`; a node named after
            // its class needs neither, and any other gets its class from a
            // meta file in the folder (`.data.json` for Argon,
            // `init.meta.json` for Rojo).
            let folder = src.join(&key);
            std::fs::create_dir_all(&folder).map_err(io(&folder))?;
            if key != service.class() {
                let meta = json!({ "className": service.class() }).to_string();
                for file in [".data.json", "init.meta.json"] {
                    let path = folder.join(file);
                    std::fs::write(&path, &meta).map_err(io(&path))?;
                }
            }
            write_children(dom, root, &folder)?;
            node.insert("$path".into(), json!(format!("src/{key}")));
        }
        tree.insert(key, Value::Object(node));
    }
    let project = json!({ "name": name, "tree": tree });
    let path = dir.join("default.project.json");
    let text = serde_json::to_string_pretty(&project).map_err(|err| err.to_string())?;
    std::fs::write(&path, text + "\n").map_err(io(&path))?;
    Ok(path)
}

fn write_children(dom: &WeakDom, parent: Ref, dir: &Path) -> Result<(), String> {
    let mut used = BTreeMap::new();
    let Some(instance) = dom.get(parent) else {
        return Ok(());
    };
    for &child in instance.children() {
        write_instance(dom, child, dir, &mut used)?;
    }
    Ok(())
}

/// One instance under `dir`: a folder, a script file, a script folder, or
/// a model file. As in Rojo and Argon the file name is the instance's name,
/// so a name a file can't hold comes back with `_` for the bad characters,
/// and siblings whose names differ only in case get a ` (2)` suffix.
fn write_instance(
    dom: &WeakDom,
    reference: Ref,
    dir: &Path,
    used: &mut BTreeMap<String, ()>,
) -> Result<(), String> {
    let Some(instance) = dom.get(reference) else {
        return Ok(());
    };
    let key = unique(used, instance.name());
    let script = SCRIPTS.iter().find(|(class, _)| *class == instance.class());
    let source = || match instance.properties().get("Source") {
        Some(Variant::String(text)) => text.clone(),
        _ => String::new(),
    };
    match (instance.class(), script) {
        ("Folder", _) | (_, Some(_))
            if !instance.children().is_empty() || instance.class() == "Folder" =>
        {
            let folder = dir.join(&key);
            std::fs::create_dir_all(&folder).map_err(io(&folder))?;
            if let Some((_, extension)) = script {
                let init = folder.join(format!("init{extension}"));
                std::fs::write(&init, source()).map_err(io(&init))?;
            }
            write_children(dom, reference, &folder)
        }
        (_, Some((_, extension))) => {
            let path = dir.join(format!("{key}{extension}"));
            std::fs::write(&path, source()).map_err(io(&path))
        }
        _ => {
            let mut model = detached(dom, &[reference]);
            tags_as_strings(&mut model);
            let bytes = rbx_binary::serialize(&model).map_err(|err| err.to_string())?;
            let path = dir.join(format!("{key}.rbxm"));
            std::fs::write(&path, bytes).map_err(io(&path))
        }
    }
}

/// Current Studio saves `Tags` as a SharedString, and Argon and Rojo (rbx-dom)
/// refuse a model holding one ("should be SharedString, but it was Tags").
/// Studio's older String spelling of the same bytes is one both read, and
/// Studio still opens.
fn tags_as_strings(model: &mut WeakDom) {
    let mut pending: Vec<Ref> = model.root_refs().to_vec();
    while let Some(reference) = pending.pop() {
        let Some(instance) = model.get(reference) else {
            continue;
        };
        pending.extend_from_slice(instance.children());
        if let Some(Variant::Unknown { type_id: 0x1C, raw }) = instance.properties().get("Tags") {
            let text = String::from_utf8_lossy(raw).into_owned();
            let _ = model.set_property(reference, "Tags", Variant::String(text));
        }
    }
}

/// An I/O error, with the path it happened at.
fn io(path: &Path) -> impl Fn(std::io::Error) -> String + '_ {
    move |err| format!("{}: {err}", path.display())
}

/// `name` made safe as a file name and unique among its siblings.
fn unique(used: &mut BTreeMap<String, ()>, name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let safe = if safe.is_empty() || safe == "." || safe == ".." {
        "_".to_owned()
    } else {
        safe
    };
    let mut key = safe.clone();
    let mut n = 1;
    while used.contains_key(&key.to_lowercase()) {
        n += 1;
        key = format!("{safe} ({n})");
    }
    used.insert(key.to_lowercase(), ());
    key
}

/// The service properties a project file can say plainly: bools, numbers,
/// strings, vectors, colours and enums (by name). Anything else is left
/// to the class default.
fn json_properties(dom: &WeakDom, reference: Ref) -> Map<String, Value> {
    let database = ReflectionDatabase::shared();
    let Some(instance) = dom.get(reference) else {
        return Map::new();
    };
    instance
        .properties()
        .iter()
        // Only what a script can set: hidden, read-only and file-only
        // spellings (`AttributesSerialize`, `UniqueId`) are not property
        // names a project file can carry.
        .filter(|(key, _)| {
            database
                .resolve_property(instance.class(), key)
                .is_some_and(|property| {
                    property.can_save
                        && !property.tags.iter().any(|tag| {
                            tag == "Hidden" || tag == "ReadOnly" || tag == "NotScriptable"
                        })
                })
        })
        .filter_map(|(key, value)| {
            let json = match value {
                Variant::Bool(b) => json!(b),
                Variant::Float32(n) => json!(n),
                Variant::Float64(n) => json!(n),
                Variant::Int32(n) => json!(n),
                Variant::Int64(n) => json!(n),
                Variant::String(text) => json!(text),
                Variant::Vector3(v) => json!([v.x, v.y, v.z]),
                Variant::Color3(c) => json!([c.r, c.g, c.b]),
                Variant::Enum(item) => {
                    let kind = property_type(database, instance.class(), key)?;
                    json!(database.enum_name(&kind, *item)?)
                }
                _ => return None,
            };
            Some((key.clone(), json))
        })
        .collect()
}

#[cfg(test)]
#[path = "project/tests.rs"]
mod tests;
