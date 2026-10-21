//! The bundled runtime and Promise compared with what the project holds,
//! and the folders the runtime would never load.

use std::path::Path;

use super::project::strip_native;
use crate::constants::{FOLDER_CONTROLLERS, FOLDER_SERVICES, PROJECT_FILE};
use crate::fsutil::{
    is_dir, is_file, list_rel_files, normalize_eol, read_text_if_exists, strip_bom,
};
use crate::json::Json;
use crate::lint::Layout;

fn is_spec(name: &str) -> bool {
    crate::constants::is_spec_file(name)
}

fn same_text(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => normalize_eol(strip_bom(a)) == normalize_eol(strip_bom(b)),
        _ => false,
    }
}

fn base_name(rel_path: &str) -> &str {
    rel_path.rsplit('/').next().unwrap_or(rel_path)
}

fn is_luau_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".lua") || lower.ends_with(".luau")
}

/// `(relative path, bytes)` of the runtime under `pkg_runtime`, sorted; dotfiles and specs left out.
pub(crate) fn runtime_files(
    pkg_runtime: &Path,
    no_native: bool,
    transform: Option<fn(&str) -> String>,
) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    for rel_path in list_rel_files(pkg_runtime) {
        let name = base_name(&rel_path);
        if name.starts_with('.') || is_spec(name) {
            continue;
        }
        let Ok(mut data) = std::fs::read(pkg_runtime.join(&rel_path)) else {
            continue;
        };
        if (no_native || transform.is_some()) && is_luau_name(name) {
            let mut text = String::from_utf8_lossy(&data).into_owned();
            if no_native {
                text = strip_native(&text);
            }
            if let Some(transform) = transform {
                text = transform(&text);
            }
            data = text.into_bytes();
        }
        files.push((rel_path, data));
    }
    files
}

pub(crate) fn runtime_differs(target: &Path, files: &[(String, Vec<u8>)]) -> bool {
    if !is_dir(target) {
        return true;
    }
    let have: Vec<String> = list_rel_files(target)
        .into_iter()
        .filter(|f| !base_name(f).starts_with('.'))
        .collect();
    if have.len() != files.len() {
        return true;
    }
    for rel_path in &have {
        let Some((_, want)) = files.iter().find(|(p, _)| p == rel_path) else {
            return true;
        };
        let text = read_text_if_exists(&target.join(rel_path));
        if !same_text(text.as_deref(), Some(&String::from_utf8_lossy(want))) {
            return true;
        }
    }
    false
}

/// Why the project's Promise needs replacing (empty when it is fine).
pub fn promise_problems(pkg_promise: &Path, user_promise: &Path) -> Vec<String> {
    if !is_dir(user_promise) {
        return vec!["it is missing".to_string()];
    }
    let mut problems = Vec::new();
    let lib_want = read_text_if_exists(&pkg_promise.join("lib").join("init.lua"));
    let lib_have = read_text_if_exists(&user_promise.join("lib").join("init.lua"));
    match &lib_have {
        None => problems.push("lib/init.lua is missing".to_string()),
        Some(have) if !same_text(Some(have), lib_want.as_deref()) => {
            problems.push("lib/init.lua differs from the bundled Promise".to_string())
        }
        _ => {}
    }
    let proj_have = read_text_if_exists(&user_promise.join(PROJECT_FILE));
    let proj_want = read_text_if_exists(&pkg_promise.join(PROJECT_FILE));
    let canonical = |t: &str| Json::parse(strip_bom(t)).ok().map(|j| j.stringify(0));
    let same_project = match (&proj_have, &proj_want) {
        (Some(h), Some(w)) => matches!((canonical(h), canonical(w)), (Some(a), Some(b)) if a == b),
        _ => false,
    };
    if !same_project {
        let state = if proj_have.is_none() {
            "missing"
        } else {
            "not the minimal one"
        };
        problems.push(format!("{PROJECT_FILE} is {state}"));
    }
    if !is_file(&user_promise.join("LICENSE")) {
        problems.push("LICENSE is missing".to_string());
    }
    let specs: Vec<String> = list_rel_files(user_promise)
        .into_iter()
        .filter(|f| is_spec(base_name(f)))
        .collect();
    if !specs.is_empty() {
        problems.push(format!(
            "it has test files ({}) that would build into your place",
            specs.join(", ")
        ));
    }
    problems
}

/// Case-sensitive file systems: 1.5.1's `loren inject` wrote lowercase folders the runtime never reads.
pub(crate) fn misplaced_folders(root: &Path, layout: &Layout) -> Vec<String> {
    let mut found = Vec::new();
    for (dir, want) in [
        (&layout.server, FOLDER_SERVICES),
        (&layout.client, FOLDER_CONTROLLERS),
    ] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for n in names {
            if n != want
                && n.to_lowercase() == want.to_lowercase()
                && is_dir(&root.join(dir).join(&n))
            {
                found.push(format!("{dir}/{n} (Loren only registers {dir}/{want})"));
            }
        }
    }
    found
}
