//! Patching `default.project.json` so it maps the runtime, and finding the
//! mappings that point outside the project.

use std::path::Path;

use crate::constants::{Key, GLOB_IGNORE_PATHS, PROJECT_FILE};
use crate::fsutil::{is_inside, resolve};
use crate::json::Json;
use crate::lint::Layout;

#[derive(Debug, Clone, PartialEq)]
pub enum Patched {
    Ok { data: Json, changes: Vec<String> },
    Error { error: String, manual: String },
}

/// A node's `$path` when it is a non-empty string.
fn node_path(node: Option<&Json>) -> Option<&str> {
    node?.get("$path")?.as_str().filter(|p| !p.is_empty())
}

pub(crate) fn globs_for(packages_dir: &str) -> Vec<String> {
    let base = packages_dir.trim_end_matches('/');
    if base == Key::Packages.default_path() {
        return GLOB_IGNORE_PATHS.iter().map(|s| s.to_string()).collect();
    }
    vec![
        format!("{base}/**/*.spec.lua"),
        format!("{base}/**/*.spec.luau"),
    ]
}

fn service(tree: &mut Json, name: &str, changes: &mut Vec<String>) -> bool {
    match tree.get(name) {
        None => {
            tree.set(
                name,
                Json::object(vec![("$className", Json::String(name.to_string()))]),
            );
            changes.push(format!("added {name}"));
            true
        }
        Some(node) => node.is_object(),
    }
}

fn mount(
    parent: &mut Json,
    parent_name: &str,
    name: &str,
    want: &str,
    changes: &mut Vec<String>,
) -> bool {
    let Some(node) = parent.get_mut(name) else {
        parent.set(
            name,
            Json::object(vec![("$path", Json::String(want.to_string()))]),
        );
        changes.push(format!("added {parent_name}.{name} -> {want}"));
        return true;
    };
    if !node.is_object() {
        return false;
    }
    let have = node_path(Some(node)).map(String::from);
    if have.as_deref() != Some(want) {
        node.set("$path", Json::String(want.to_string()));
        let was = have.map(|h| format!(" (was {h})")).unwrap_or_default();
        changes.push(format!("{parent_name}.{name} now points at {want}{was}"));
    }
    // The runtime is a folder module: Rojo refuses a $className next to its $path, Argon any.
    if let Some(class) = node.remove("$className") {
        changes.push(format!(
            "{parent_name}.{name}: removed \"$className\": {} (it conflicts with $path)",
            class.stringify(0)
        ));
    }
    true
}

/// Returns the patched data and its changes, or the error and the manual steps when the tree is too unusual.
pub fn patch_project(data: &Json, layout: &Layout) -> Patched {
    let globs_json = Json::Array(
        globs_for(&layout.packages)
            .into_iter()
            .map(Json::String)
            .collect(),
    );
    let manual = format!(
        "Add these by hand:\n  tree.ReplicatedStorage.LorenRuntime = {{ \"$path\": \"loren/shared\" }}\n  tree.ServerScriptService.LorenServer = {{ \"$path\": \"loren/server\" }}\n  \"globIgnorePaths\": {}",
        globs_json.stringify(0)
    );
    let fail = |error: String| Patched::Error {
        error,
        manual: manual.clone(),
    };
    if !data.is_object() {
        return fail(format!("{PROJECT_FILE} is not a JSON object."));
    }
    let Some(tree) = data.get("tree").filter(|t| t.is_object()) else {
        return fail(format!("{PROJECT_FILE} has no \"tree\" object."));
    };
    if tree.get("$className").and_then(Json::as_str) != Some("DataModel") {
        let class = tree
            .get("$className")
            .cloned()
            .unwrap_or(Json::Null)
            .stringify(0);
        return fail(format!(
            "{PROJECT_FILE} does not describe a place: its tree's $className is {class}, not \"DataModel\"."
        ));
    }
    if data
        .get("globIgnorePaths")
        .is_some_and(|g| g.as_array().is_none())
    {
        return fail(format!(
            "\"globIgnorePaths\" in {PROJECT_FILE} is not a list."
        ));
    }

    let mut out = data.clone();
    let mut changes = Vec::new();
    let Some(tree) = out.get_mut("tree") else {
        return fail(String::new());
    };
    let rs_ok = service(tree, "ReplicatedStorage", &mut changes);
    let sss_ok = service(tree, "ServerScriptService", &mut changes);
    if !rs_ok {
        return fail(format!(
            "tree.ReplicatedStorage in {PROJECT_FILE} is not an object."
        ));
    }
    if !sss_ok {
        return fail(format!(
            "tree.ServerScriptService in {PROJECT_FILE} is not an object."
        ));
    }
    if let Some(rs) = tree.get_mut("ReplicatedStorage") {
        if !mount(
            rs,
            "ReplicatedStorage",
            "LorenRuntime",
            Key::RuntimeShared.default_path(),
            &mut changes,
        ) {
            return fail(format!(
                "tree.ReplicatedStorage.LorenRuntime in {PROJECT_FILE} is not an object."
            ));
        }
    }
    if let Some(sss) = tree.get_mut("ServerScriptService") {
        if !mount(
            sss,
            "ServerScriptService",
            "LorenServer",
            Key::RuntimeServer.default_path(),
            &mut changes,
        ) {
            return fail(format!(
                "tree.ServerScriptService.LorenServer in {PROJECT_FILE} is not an object."
            ));
        }
    }
    if let Some(rs) = tree.get_mut("ReplicatedStorage") {
        if rs.get("LorenPackages").is_none() {
            rs.set(
                "LorenPackages",
                Json::object(vec![("$path", Json::String(layout.packages.clone()))]),
            );
            changes.push(format!(
                "added ReplicatedStorage.LorenPackages -> {}",
                layout.packages
            ));
        }
    }

    let globs = globs_for(&layout.packages);
    let current: Vec<&str> = out
        .get("globIgnorePaths")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(Json::as_str).collect())
        .unwrap_or_default();
    let missing: Vec<String> = globs
        .into_iter()
        .filter(|g| !current.contains(&g.as_str()))
        .collect();
    if missing.is_empty() {
        return Patched::Ok { data: out, changes };
    }
    changes.push(format!("globIgnorePaths += {}", missing.join(", ")));
    let missing_json: Vec<Json> = missing.into_iter().map(Json::String).collect();
    if let Some(Json::Array(list)) = out.get_mut("globIgnorePaths") {
        list.extend(missing_json);
        return Patched::Ok { data: out, changes };
    }
    // Put it right after "name" so it is easy to find.
    let Json::Object(pairs) = out else {
        return fail(String::new());
    };
    let mut ordered = Vec::with_capacity(pairs.len() + 1);
    let mut placed = false;
    let mut missing_json = Some(missing_json);
    for (k, v) in pairs {
        if !placed && k != "name" {
            if let Some(m) = missing_json.take() {
                ordered.push(("globIgnorePaths".to_string(), Json::Array(m)));
            }
            placed = true;
        }
        ordered.push((k, v));
    }
    if let Some(m) = missing_json {
        ordered.push(("globIgnorePaths".to_string(), Json::Array(m)));
    }
    Patched::Ok {
        data: Json::Object(ordered),
        changes,
    }
}

pub(crate) fn outside_mappings(root: &Path, data: &Json) -> Vec<String> {
    let targets: [(&str, &[&str]); 4] = [
        ("ReplicatedStorage.Shared", &["ReplicatedStorage", "Shared"]),
        (
            "ServerScriptService.Server",
            &["ServerScriptService", "Server"],
        ),
        (
            "StarterPlayer.StarterPlayerScripts.Client",
            &["StarterPlayer", "StarterPlayerScripts", "Client"],
        ),
        (
            "ReplicatedStorage.LorenPackages",
            &["ReplicatedStorage", "LorenPackages"],
        ),
    ];
    let mut found = Vec::new();
    for (label, keys) in targets {
        let mut node = data.get("tree");
        for k in keys {
            node = node.and_then(|n| n.get(k));
        }
        if let Some(p) = node_path(node) {
            if !is_inside(root, &resolve(root, p)) {
                found.push(format!("{label} -> {p}"));
            }
        }
    }
    found
}
