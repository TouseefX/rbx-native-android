//! Reading the project before an update (`update.js` lines 1-320).

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::constants::{
    Key, FOLDER_CONTROLLERS, FOLDER_SERVICES, GLOB_IGNORE_PATHS, PROJECT_FILE, SHIM_SOURCE,
};
use crate::fsutil::{
    is_dir, is_file, is_inside, list_rel_files, normalize_eol, read_text_if_exists, rel, resolve,
    strip_bom,
};
use crate::json::Json;
use crate::lint::Layout;
use crate::scriptsync::runtime_version;

const RUNTIME_DIR: &str = "loren";
const SHIM_NAMES: [&str; 2] = ["Loren.luau", "Loren.lua"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    /// `legacy`, `v2` or `none`.
    pub kind: &'static str,
    pub reason: String,
    pub loren_file: PathBuf,
    /// Every `Loren.luau`/`Loren.lua` in the shared folder.
    pub existing: Vec<PathBuf>,
    /// `missing`, `shim`, `runtime`, `shim-modified` or `other`.
    pub loren_state: &'static str,
    pub installed_version: Option<String>,
    pub has_runtime: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Patched {
    Ok { data: Json, changes: Vec<String> },
    Error { error: String, manual: String },
}

/// Markers of the 1.5.1 single-file runtime (its remotes and its own definitions).
static LEGACY_MARKERS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"\bLorenBridge\b",
        r"\bLorenHandshake\b",
        r"function\s+Loren\s*[:.]\s*SetOnFire\s*\(",
        r"function\s+Loren\s*[:.]\s*AddServices\s*\(",
    ]
    .iter()
    .map(|p| Regex::new(p).unwrap())
    .collect()
});
static LOREN_RUNTIME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bLorenRuntime\b").unwrap());

fn is_spec(name: &str) -> bool {
    crate::constants::is_spec_file(name)
}

fn same_text(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => normalize_eol(strip_bom(a)) == normalize_eol(strip_bom(b)),
        _ => false,
    }
}

/// A node's `$path` when it is a non-empty string.
fn node_path(node: Option<&Json>) -> Option<&str> {
    node?.get("$path")?.as_str().filter(|p| !p.is_empty())
}

fn parse_int(s: &str) -> i64 {
    let digits: String = s
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().unwrap_or(0)
}

fn parse_version(v: &str) -> (Vec<i64>, Option<Vec<String>>) {
    let s = v.trim();
    let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
    let s = s.split('+').next().unwrap_or("");
    let (core, pre) = match s.find('-') {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    let nums = core.split('.').map(parse_int).collect();
    let pre = pre
        .filter(|p| !p.is_empty())
        .map(|p| p.split('.').map(String::from).collect());
    (nums, pre)
}

fn ordering_int(o: Ordering) -> i32 {
    match o {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

/// -1, 0 or 1, semver-style with prereleases.
pub fn compare_versions(a: &str, b: &str) -> i32 {
    let (xn, xp) = parse_version(a);
    let (yn, yp) = parse_version(b);
    for i in 0..3 {
        let d = xn
            .get(i)
            .copied()
            .unwrap_or(0)
            .cmp(&yn.get(i).copied().unwrap_or(0));
        if d != Ordering::Equal {
            return ordering_int(d);
        }
    }
    let (x, y) = match (xp, yp) {
        (None, None) => return 0,
        (None, Some(_)) => return 1,
        (Some(_), None) => return -1,
        (Some(x), Some(y)) => (x, y),
    };
    let numeric = |p: &str| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit());
    for i in 0..x.len().max(y.len()) {
        let (Some(p), Some(q)) = (x.get(i), y.get(i)) else {
            return if x.get(i).is_none() { -1 } else { 1 };
        };
        if p == q {
            continue;
        }
        if numeric(p) && numeric(q) {
            let (pn, qn) = (
                p.parse::<f64>().unwrap_or(0.0),
                q.parse::<f64>().unwrap_or(0.0),
            );
            return if pn < qn { -1 } else { 1 };
        }
        if numeric(p) != numeric(q) {
            return if numeric(p) { -1 } else { 1 };
        }
        return if p < q { -1 } else { 1 };
    }
    0
}

fn strip_native_line(line: &str) -> String {
    let (content, cr) = match line.strip_suffix('\r') {
        Some(c) => (c, "\r"),
        None => (line, ""),
    };
    let blank = |c: char| c == ' ' || c == '\t';
    let body = content.trim_start_matches(blank);
    for marker in ["--!native", "@native"] {
        if body
            .strip_prefix(marker)
            .is_some_and(|r| r.trim_start_matches(blank).is_empty())
        {
            return cr.to_string();
        }
    }
    if let Some(rest) = body.strip_prefix("@native") {
        let trimmed = rest.trim_start_matches(blank);
        if trimmed.len() < rest.len() && trimmed.chars().next().is_some_and(|c| !c.is_whitespace())
        {
            let indent = &content[..content.len() - body.len()];
            return format!("{indent}{trimmed}{cr}");
        }
    }
    line.to_string()
}

/// `--no-native`: drops `--!native` and `@native`, keeping line numbers.
pub fn strip_native(text: &str) -> String {
    text.split('\n')
        .map(strip_native_line)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn classify_loren_file(text: Option<&str>) -> &'static str {
    let Some(text) = text else { return "missing" };
    let t = normalize_eol(strip_bom(text));
    if t == SHIM_SOURCE {
        return "shim";
    }
    if LEGACY_MARKERS.iter().any(|re| re.is_match(&t)) {
        return "runtime";
    }
    if LOREN_RUNTIME.is_match(&t) {
        return "shim-modified";
    }
    "other"
}

pub fn detect_project(root: &Path, layout: &Layout) -> Detected {
    let shared_dir = root.join(&layout.shared);
    let existing: Vec<PathBuf> = SHIM_NAMES
        .iter()
        .map(|n| shared_dir.join(n))
        .filter(|p| is_file(p))
        .collect();
    let loren_file = existing
        .first()
        .cloned()
        .unwrap_or_else(|| shared_dir.join(SHIM_NAMES[0]));
    let loren_state = classify_loren_file(read_text_if_exists(&loren_file).as_deref());
    let runtime_label = layout
        .runtime_shared
        .clone()
        .unwrap_or_else(|| format!("{RUNTIME_DIR}/shared"));
    let runtime_shared = root.join(&runtime_label);
    let has_runtime =
        is_file(&runtime_shared.join("init.luau")) || is_file(&runtime_shared.join("init.lua"));
    let installed_version = if has_runtime {
        runtime_version(&runtime_shared)
    } else {
        None
    };
    let where_ = rel(root, &loren_file);
    let (kind, reason) = if loren_state == "runtime" {
        ("legacy", format!("{where_} is the whole 1.x runtime"))
    } else if loren_state == "shim" || loren_state == "shim-modified" {
        ("v2", format!("{where_} is the 2.x shim"))
    } else if has_runtime {
        ("v2", format!("{runtime_label} holds the 2.x runtime"))
    } else if loren_state == "missing" {
        let dir = layout.runtime_shared.as_deref().unwrap_or(RUNTIME_DIR);
        (
            "none",
            format!("there is no {where_} and no {dir}/ runtime"),
        )
    } else {
        (
            "none",
            format!("{where_} is neither the Loren 1.x runtime nor the 2.x shim"),
        )
    };
    Detected {
        kind,
        reason,
        loren_file,
        existing,
        loren_state,
        installed_version,
        has_runtime,
    }
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
