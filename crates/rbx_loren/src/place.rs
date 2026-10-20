//! A Loren project read into a place the way Rojo builds one: folders,
//! `init.luau` folder modules, `.server`/`.client` scripts and nested
//! project files. The editor's Home uses it so a new Loren project opens
//! ready to play; the spec runner builds its test place with it.

use std::path::Path;

use rbx_dom::{Ref, Variant, WeakDom};

use crate::constants::{Key, PROJECT_FILE};
use crate::fsutil;
use crate::json::Json;
use crate::project::node_path;
use crate::scriptsync::is_script_sync;

const SCRIPT_SUFFIXES: [(&str, &str); 8] = [
    (".server.luau", "Script"),
    (".server.lua", "Script"),
    (".client.luau", "LocalScript"),
    (".client.lua", "LocalScript"),
    (".local.luau", "LocalScript"),
    (".local.lua", "LocalScript"),
    (".luau", "ModuleScript"),
    (".lua", "ModuleScript"),
];

/// The script class and bare name for a file name, if it is a script file.
fn script_kind(file: &str) -> Option<(&'static str, &str)> {
    SCRIPT_SUFFIXES
        .iter()
        .find_map(|(suffix, class)| file.strip_suffix(suffix).map(|stem| (*class, stem)))
}

fn entry_names(dir: &Path) -> Result<Vec<String>, String> {
    let entries =
        std::fs::read_dir(dir).map_err(|err| format!("Could not read {}: {err}", dir.display()))?;
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    Ok(names)
}

fn set_source(dom: &mut WeakDom, instance: Ref, path: &Path) -> Result<(), String> {
    let text = fsutil::read_text(path)
        .map_err(|err| format!("Could not read {}: {err}", path.display()))?;
    dom.set_property(instance, "Source", Variant::String(text))
        .map_err(|err| format!("{err}"))?;
    Ok(())
}

/// Adds the file or folder at `path` under `parent` as `name`, with Rojo's
/// rules (`X.luau` ModuleScript, `X.server.luau` Script, `X.client.luau`
/// LocalScript, a folder with `init*.luau` becomes that script, a folder
/// holding `default.project.json` follows that project's tree).
pub fn insert_path(dom: &mut WeakDom, parent: Ref, name: &str, path: &Path) -> Result<Ref, String> {
    if fsutil::is_file(path) {
        let file = path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (class, _) =
            script_kind(&file).ok_or_else(|| format!("Unsupported file {}", path.display()))?;
        let made = dom.new_instance(class, name, Some(parent));
        set_source(dom, made, path)?;
        return Ok(made);
    }
    if !fsutil::is_dir(path) {
        return Err(format!("{} does not exist", path.display()));
    }

    let project = path.join(PROJECT_FILE);
    if fsutil::is_file(&project) {
        let text = fsutil::read_text(&project)
            .map_err(|err| format!("Could not read {}: {err}", project.display()))?;
        let data = Json::parse(fsutil::strip_bom(&text))
            .map_err(|err| format!("{} is not valid JSON: {err}", project.display()))?;
        let tree = data
            .get("tree")
            .ok_or_else(|| format!("{} has no tree", project.display()))?;
        return build_node(dom, parent, name, tree, path)?
            .ok_or_else(|| format!("{} points at nothing", project.display()));
    }

    let names = entry_names(path)?;
    let init = names.iter().find_map(|file| match script_kind(file) {
        Some((class, "init")) => Some((class, file.clone())),
        _ => None,
    });
    let (class, init_file) = match init {
        Some((class, file)) => (class, Some(file)),
        None => ("Folder", None),
    };
    let made = dom.new_instance(class, name, Some(parent));
    if let Some(file) = &init_file {
        set_source(dom, made, &path.join(file))?;
    }
    insert_children(dom, made, path, &names, init_file.as_deref())?;
    Ok(made)
}

fn insert_children(
    dom: &mut WeakDom,
    parent: Ref,
    dir: &Path,
    names: &[String],
    skip: Option<&str>,
) -> Result<(), String> {
    for file in names {
        if Some(file.as_str()) == skip || file == PROJECT_FILE {
            continue;
        }
        let full = dir.join(file);
        if fsutil::is_dir(&full) {
            insert_path(dom, parent, file, &full)?;
        } else if let Some((_, stem)) = script_kind(file) {
            insert_path(dom, parent, stem, &full)?;
        }
    }
    Ok(())
}

/// Builds one project-tree node; `None` when it is an optional path that is absent.
fn build_node(
    dom: &mut WeakDom,
    parent: Ref,
    name: &str,
    node: &Json,
    base: &Path,
) -> Result<Option<Ref>, String> {
    let made = if let Some(rel) = node_path(node) {
        let target = fsutil::resolve(base, rel);
        let optional = node.get("$path").is_some_and(Json::is_object);
        if optional && !fsutil::exists(&target) {
            return Ok(None);
        }
        insert_path(dom, parent, name, &target)?
    } else {
        let class = node
            .get("$className")
            .and_then(Json::as_str)
            .unwrap_or("Folder");
        dom.new_instance(class, name, Some(parent))
    };
    build_children(dom, made, node, base)?;
    Ok(Some(made))
}

fn build_children(dom: &mut WeakDom, parent: Ref, node: &Json, base: &Path) -> Result<(), String> {
    let Some(entries) = node.as_object() else {
        return Ok(());
    };
    for (key, child) in entries.iter() {
        if key.starts_with('$') || !child.is_object() {
            continue;
        }
        build_node(dom, parent, key, child, base)?;
    }
    Ok(())
}

fn find_service(dom: &mut WeakDom, name: &str) -> Ref {
    let found = dom
        .root_refs()
        .iter()
        .copied()
        .find(|r| dom.get(*r).is_some_and(|i| i.class() == name));
    found.unwrap_or_else(|| dom.new_instance(name, name, None))
}

fn find_child(dom: &WeakDom, parent: Ref, name: &str) -> Option<Ref> {
    let children = dom.get(parent)?.children();
    children
        .iter()
        .copied()
        .find(|c| dom.get(*c).is_some_and(|i| i.name() == name))
}

fn apply_script_sync(dom: &mut WeakDom, root: &Path) -> Result<(), String> {
    let layout: [(Key, &[&str]); 4] = [
        (Key::Shared, &["ReplicatedStorage", "Shared"]),
        (Key::Packages, &["ReplicatedStorage", "LorenPackages"]),
        (Key::Server, &["ServerScriptService", "Server"]),
        (
            Key::Client,
            &["StarterPlayer", "StarterPlayerScripts", "Client"],
        ),
    ];
    for (key, parts) in layout {
        let dir = fsutil::resolve(root, key.script_sync_path());
        if !fsutil::is_dir(&dir) {
            continue;
        }
        let Some((last, middle)) = parts.split_last() else {
            continue;
        };
        let mut at = find_service(dom, middle[0]);
        for part in &middle[1..] {
            at = match find_child(dom, at, part) {
                Some(existing) => existing,
                None => dom.new_instance("Folder", part, Some(at)),
            };
        }
        insert_path(dom, at, last, &dir)?;
    }
    Ok(())
}

/// Applies the project's `default.project.json` tree (or a Script Sync
/// project's four folders) onto `dom`, reusing services already there.
pub fn apply_project(dom: &mut WeakDom, root: &Path) -> Result<(), String> {
    if is_script_sync(root) {
        return apply_script_sync(dom, root);
    }
    let file = root.join(PROJECT_FILE);
    let text =
        fsutil::read_text(&file).map_err(|err| format!("Could not read {PROJECT_FILE}: {err}"))?;
    let data = Json::parse(fsutil::strip_bom(&text))
        .map_err(|err| format!("{PROJECT_FILE} is not valid JSON: {err}"))?;
    let Some(tree) = data.get("tree") else {
        return Ok(());
    };
    let Some(entries) = tree.as_object() else {
        return Ok(());
    };
    for (name, node) in entries.iter() {
        if name.starts_with('$') || !node.is_object() {
            continue;
        }
        let service = find_service(dom, name);
        build_children(dom, service, node, root)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/loren/project")
    }

    fn at(dom: &WeakDom, start: Ref, path: &[&str]) -> Ref {
        let mut cur = start;
        for part in path {
            cur = find_child(dom, cur, part).unwrap_or_else(|| panic!("missing {part}"));
        }
        cur
    }

    fn built() -> WeakDom {
        let mut dom = WeakDom::new();
        apply_project(&mut dom, &project_dir()).unwrap();
        dom
    }

    fn service(dom: &WeakDom, name: &str) -> Ref {
        dom.root_refs()
            .iter()
            .copied()
            .find(|r| dom.get(*r).unwrap().class() == name)
            .unwrap()
    }

    fn class(dom: &WeakDom, r: Ref) -> String {
        dom.get(r).unwrap().class().to_string()
    }

    #[test]
    fn builds_runtime_and_packages() {
        let dom = built();
        let rs = service(&dom, "ReplicatedStorage");
        let runtime = at(&dom, rs, &["LorenRuntime"]);
        assert_eq!(class(&dom, runtime), "ModuleScript");
        at(&dom, runtime, &["Codec"]);
        let promise = at(&dom, rs, &["LorenPackages", "Promise"]);
        assert_eq!(class(&dom, promise), "ModuleScript");
    }

    #[test]
    fn builds_server_and_client() {
        let dom = built();
        let server = at(
            &dom,
            service(&dom, "ServerScriptService"),
            &["Server", "Server"],
        );
        assert_eq!(class(&dom, server), "Script");
        let client = at(
            &dom,
            service(&dom, "StarterPlayer"),
            &["StarterPlayerScripts", "Client", "Client"],
        );
        assert_eq!(class(&dom, client), "LocalScript");
    }

    #[test]
    fn source_matches_file() {
        let dom = built();
        let example = at(
            &dom,
            service(&dom, "ServerScriptService"),
            &["Server", "Services", "ExampleService"],
        );
        let expected = std::fs::read_dir(project_dir().join("src/server/Services"))
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| {
                p.file_stem()
                    .is_some_and(|s| s.to_string_lossy().starts_with("ExampleService"))
            })
            .unwrap();
        let text = std::fs::read_to_string(expected).unwrap();
        assert_eq!(
            dom.get(example).unwrap().properties().get("Source"),
            Some(&Variant::String(text))
        );
    }

    #[test]
    fn reuses_existing_services() {
        let mut dom = WeakDom::new();
        let existing = dom.new_instance("ReplicatedStorage", "ReplicatedStorage", None);
        apply_project(&mut dom, &project_dir()).unwrap();
        let count = dom
            .root_refs()
            .iter()
            .filter(|r| dom.get(**r).unwrap().class() == "ReplicatedStorage")
            .count();
        assert_eq!(count, 1);
        assert_eq!(service(&dom, "ReplicatedStorage"), existing);
    }
}
