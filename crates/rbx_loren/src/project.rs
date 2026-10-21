//! Finding and reading a Loren project (`lib/project.js`), plus the backup
//! (`lib/backup.js`) and module-template (`lib/templates.js`) helpers.

use std::path::{Path, PathBuf};

use regex::{NoExpand, Regex};

use crate::constants::{
    Key, BACKUP_DIR, FOLDER_CONTROLLERS, FOLDER_SERVICES, LOREN_CONFIG_FILE, MODULE_TEMPLATES,
    PROJECT_FILE,
};
use crate::ctx::CliError;
use crate::fsutil;
use crate::json::Json;
use crate::scriptsync::{is_script_sync, read_loren_config, LorenConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPaths {
    pub shared: PathBuf,
    pub server: PathBuf,
    pub client: PathBuf,
    pub packages: PathBuf,
    pub services: PathBuf,
    pub controllers: PathBuf,
}

/// The project root (`cwd` resolved), or why this is not a Loren project.
pub fn require_project(cwd: &Path) -> Result<PathBuf, CliError> {
    let base = std::env::current_dir().unwrap_or_default();
    let root = fsutil::resolve(&base, cwd);
    if let LorenConfig::Error(error) = read_loren_config(&root) {
        return Err(CliError::hint(
            format!("{error}."),
            format!("Fix {LOREN_CONFIG_FILE}, then run this again."),
        ));
    }
    if !fsutil::is_file(&root.join(PROJECT_FILE)) && !is_script_sync(&root) {
        return Err(CliError::hint(
            format!("No {PROJECT_FILE} in {}.", root.display()),
            "Run this from the root of a Loren project (the folder that holds default.project.json).",
        ));
    }
    Ok(root)
}

pub fn read_project_file(root: &Path) -> Result<Json, CliError> {
    let text = fsutil::read_text(&root.join(PROJECT_FILE))
        .map_err(|err| CliError::new(format!("Could not read {PROJECT_FILE}: {err}")))?;
    Json::parse(fsutil::strip_bom(&text))
        .map_err(|err| CliError::new(format!("{PROJECT_FILE} is not valid JSON: {err}")))
}

/// A tree node's `$path`: a string, or `{ "optional": "..." }`.
pub fn node_path(node: &Json) -> Option<&str> {
    if !node.is_object() {
        return None;
    }
    let p = node.get("$path")?;
    if let Some(s) = p.as_str() {
        return Some(s);
    }
    if p.is_object() {
        return p.get("optional").and_then(Json::as_str);
    }
    None
}

fn tree_node<'a>(tree: &'a Json, keys: &[&str]) -> Option<&'a Json> {
    let mut node = tree;
    for key in keys {
        if !node.is_object() {
            return None;
        }
        node = node.get(key)?;
    }
    Some(node)
}

/// Absolute folders from `default.project.json` (or the Script Sync layout).
pub fn project_paths(root: &Path) -> ProjectPaths {
    let synced = is_script_sync(root);
    let data = if synced {
        None
    } else {
        read_project_file(root).ok()
    };
    let tree = data.as_ref().and_then(|d| d.get("tree"));
    let pick = |keys: &[&str], key: Key| {
        let rel = if synced {
            key.script_sync_path()
        } else {
            tree.and_then(|t| tree_node(t, keys))
                .and_then(node_path)
                .unwrap_or(key.default_path())
        };
        fsutil::resolve(root, rel)
    };
    let shared = pick(&["ReplicatedStorage", "Shared"], Key::Shared);
    let server = pick(&["ServerScriptService", "Server"], Key::Server);
    let client = pick(
        &["StarterPlayer", "StarterPlayerScripts", "Client"],
        Key::Client,
    );
    let packages = pick(&["ReplicatedStorage", "LorenPackages"], Key::Packages);
    ProjectPaths {
        services: server.join(FOLDER_SERVICES),
        controllers: client.join(FOLDER_CONTROLLERS),
        shared,
        server,
        client,
        packages,
    }
}

fn entry_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The entry named `name` (`X.luau`, `X.lua` or a folder `X`), case-insensitively.
pub fn find_module_entry(dir: &Path, name: &str) -> Option<String> {
    if !fsutil::is_dir(dir) {
        return None;
    }
    let want = name.to_lowercase();
    entry_names(dir).into_iter().find(|entry| {
        let lower = entry.to_lowercase();
        lower == format!("{want}.luau")
            || lower == format!("{want}.lua")
            || (lower == want && fsutil::is_dir(&dir.join(entry)))
    })
}

/// Any entry whose name without `.lua`/`.luau` matches, case-insensitively.
pub fn find_entry_loose(dir: &Path, name: &str) -> Option<String> {
    if !fsutil::is_dir(dir) {
        return None;
    }
    let want = name.to_lowercase();
    entry_names(dir).into_iter().find(|entry| {
        let lower = entry.to_lowercase();
        let stem = lower
            .strip_suffix(".luau")
            .or_else(|| lower.strip_suffix(".lua"))
            .unwrap_or(&lower);
        stem == want
    })
}

/// Copies `target` to `.loren-backup/<stamp>/<relative path>`; `None` when it does not exist.
pub fn backup_path(root: &Path, target: &Path, stamp: &str) -> Result<Option<PathBuf>, String> {
    if !fsutil::exists(target) {
        return Ok(None);
    }
    if !fsutil::is_inside(root, target) {
        return Err(format!(
            "Refusing to back up {}: it is outside the project",
            target.display()
        ));
    }
    let dest = root
        .join(BACKUP_DIR)
        .join(stamp)
        .join(fsutil::relative(root, target));
    fsutil::copy_path(target, &dest).map_err(|err| err.to_string())?;
    Ok(Some(dest))
}

/// `loren make`'s module text: the template with its placeholder renamed (CRLF normalized).
pub fn render_module(kind: &str, name: &str, template_dir: &Path) -> Result<String, String> {
    let (_, file, placeholder) = MODULE_TEMPLATES
        .iter()
        .find(|(k, _, _)| *k == kind)
        .ok_or_else(|| format!("Unknown module kind \"{kind}\""))?;
    let path = file
        .split('/')
        .fold(template_dir.to_path_buf(), |p, part| p.join(part));
    let text = fsutil::normalize_eol(&fsutil::read_text(&path).map_err(|err| err.to_string())?);
    let re = Regex::new(&format!(r"\b{}\b", regex::escape(placeholder)))
        .map_err(|err| err.to_string())?;
    Ok(re.replace_all(&text, NoExpand(name)).into_owned())
}
