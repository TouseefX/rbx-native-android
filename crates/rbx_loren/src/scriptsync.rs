//! Roblox Script Sync projects (`lib/scriptsync.js`): `.loren.json`, the four
//! synced folders and the Studio steps.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::constants::{Key, LOREN_CONFIG_FILE, SCRIPT_SYNC_ID, SCRIPT_SYNC_LAYOUT};
use crate::fsutil::{read_text_if_exists, strip_bom, write_text};
use crate::json::Json;

pub const COMPANION_PLUGIN: &str = "Luau Language Server Companion";

pub enum LorenConfig {
    Missing,
    Error(String),
    Config(Json),
}

static VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"Version\s*=\s*"([^"]+)""#).unwrap());
static SERVER_REQUIRE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^([ \t]*local[ \t]+[A-Za-z_][A-Za-z0-9_]*[ \t]*=[ \t]*ReplicatedStorage)\.LorenRuntime\b").unwrap()
});

/// `(Studio instance path, project directory)` for shared, packages, server, client.
pub fn synced_folders() -> Vec<(String, &'static str)> {
    [Key::Shared, Key::Packages, Key::Server, Key::Client]
        .into_iter()
        .map(|key| {
            let dir = key.script_sync_path();
            (dir.split('/').collect::<Vec<_>>().join("."), dir)
        })
        .collect()
}

/// The Luau-LSP plugin-mode settings, in order.
pub fn plugin_settings() -> Vec<(&'static str, Json)> {
    vec![
        ("luau-lsp.plugin.enabled", Json::Bool(true)),
        ("luau-lsp.sourcemap.enabled", Json::Bool(false)),
        ("luau-lsp.sourcemap.autogenerate", Json::Bool(false)),
    ]
}

pub fn read_loren_config(root: &Path) -> LorenConfig {
    let Some(text) = read_text_if_exists(&root.join(LOREN_CONFIG_FILE)) else {
        return LorenConfig::Missing;
    };
    match Json::parse(strip_bom(&text)) {
        Ok(config) if config.is_object() => LorenConfig::Config(config),
        Ok(_) => LorenConfig::Error(format!("{LOREN_CONFIG_FILE} is not a JSON object")),
        Err(err) => LorenConfig::Error(format!("{LOREN_CONFIG_FILE} is not valid JSON ({err})")),
    }
}

pub fn is_script_sync(root: &Path) -> bool {
    match read_loren_config(root) {
        LorenConfig::Config(config) => config
            .get("tool")
            .and_then(Json::as_str)
            .is_some_and(|t| t.to_lowercase() == SCRIPT_SYNC_ID),
        _ => false,
    }
}

/// `{ ...base, tool, layout, runtime }` as 4-space JSON plus a newline.
pub fn loren_config_text(runtime: &str, base: Option<&Json>) -> String {
    let mut config = match base {
        Some(b) if b.is_object() => b.clone(),
        _ => Json::object(vec![]),
    };
    config.set("tool", Json::String(SCRIPT_SYNC_ID.to_string()));
    config.set("layout", Json::String(SCRIPT_SYNC_LAYOUT.to_string()));
    config.set("runtime", Json::String(runtime.to_string()));
    format!("{}\n", config.stringify(4))
}

pub fn write_loren_config(root: &Path, runtime: &str, base: Option<&Json>) -> std::io::Result<()> {
    write_text(
        &root.join(LOREN_CONFIG_FILE),
        loren_config_text(runtime, base),
    )
}

/// The runtime version in `<dir>/Version.luau`.
pub fn runtime_version(runtime_shared_dir: &Path) -> Option<String> {
    let text = read_text_if_exists(&runtime_shared_dir.join("Version.luau"))?;
    VERSION.captures(&text).map(|m| m[1].to_string())
}

pub fn server_runtime_source(text: &str) -> String {
    SERVER_REQUIRE
        .replace_all(text, "${1}.Shared.LorenRuntime")
        .into_owned()
}

/// The numbered Studio steps, as lines (init prints them; the project README holds them too).
pub fn sync_steps() -> Vec<String> {
    let folders = synced_folders();
    let width = folders
        .iter()
        .map(|(i, _)| i.chars().count())
        .max()
        .unwrap_or(0);
    let names = |part: &[(String, &'static str)]| {
        part.iter()
            .map(|(i, _)| i.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut lines = vec![
        "1. Open your place in Studio. In the Explorer, create these Folders if they are missing:"
            .to_string(),
        format!("   {},", names(&folders[..2])),
        format!("   {}.", names(&folders[2..])),
        "2. Right-click each Folder > Sync to… and pick the matching directory of this project:"
            .to_string(),
    ];
    lines.extend(
        folders
            .iter()
            .map(|(instance, dir)| format!("     {instance:<width$}  ->  {dir}/")),
    );
    lines.push("   When Studio shows the conflict dialog, choose Keep Disk.".to_string());
    lines.push(
        "3. Autocomplete (optional): the Luau Language Server extension in VS Code plus the"
            .to_string(),
    );
    lines.push(format!(
        "   \"{COMPANION_PLUGIN}\" Studio plugin (.vscode/settings.json turns plugin mode on)."
    ));
    lines.push("4. Press Play.".to_string());
    lines
}

pub fn readme_text(name: &str) -> String {
    let mut lines: Vec<String> = vec![
        format!("# {name}"),
        String::new(),
        "A [Loren](https://fabideveloper.github.io/Loren-Framework/) project synced with Roblox Script Sync,".into(),
        "which is built into Studio. There is no Rojo or Argon and no project file; `.loren.json` marks the project."
            .into(),
        String::new(),
        "## Sync it in Studio".into(),
        String::new(),
    ];
    lines.extend(sync_steps());
    lines.extend(
        [
            "",
            "Script Sync only syncs scripts and folders, and drops attributes and tags on scripts: keep those on",
            "other instances. Client scripts are `*.local.luau` (a LocalScript); never `*.client.luau`, which",
            "Script Sync turns into a Script with RunContext Client that runs twice from StarterPlayerScripts.",
            "",
            "## Commands",
            "",
            "- `loren make service <Name>` / `loren make controller <Name>`, `loren inject --list`",
            "- `loren types`: regenerate `LorenTypes` (and `LorenServerTypes`)",
            "- `loren update`: update the runtime (`ReplicatedStorage/Shared/LorenRuntime`, `ServerScriptService/Server/LorenServer`)",
            "- `loren doctor`: check the project",
            "",
            "Studio picks up the file changes through Script Sync. If it shows the conflict dialog, choose Keep Disk.",
            "",
        ]
        .map(String::from),
    );
    lines.join("\n")
}
