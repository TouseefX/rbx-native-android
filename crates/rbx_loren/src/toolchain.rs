//! Toolchain manifests and editor settings (`lib/toolchain.js`): Rokit,
//! Aftman and Foreman files, `rokit install`, `.vscode/settings.json`.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::constants::{tool_pin, MANAGERS, PROJECT_FILE};
use crate::ctx::Tool;
use crate::fsutil::{is_file, read_text, read_text_if_exists, strip_bom, write_text};
use crate::json::Json;
use crate::run::{describe_failure, has_command, RunOpts, Runner, Stdio};
use crate::scriptsync::plugin_settings;
use crate::tool::SOURCEMAP_FILE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub manager: &'static str,
    pub file: PathBuf,
    pub text: String,
    pub tools: Vec<Tool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    pub ok: bool,
    pub manager: Option<&'static str>,
    pub reason: Option<String>,
}

static TOOLS_HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\[tools\]\s*(#.*)?$").expect("valid regex"));

// One `[tools]` line for a manager's manifest (Rokit and Aftman share a format; Foreman uses tables).
pub fn manifest_entry(manager: &str, tool: Tool) -> String {
    let Some(pin) = tool_pin(tool.id()) else {
        return String::new();
    };
    let version = &pin.spec[pin.spec.find('@').map_or(0, |i| i + 1)..];
    if manager == "foreman" {
        return format!(
            "{} = {{ source = \"{}\", version = \"={}\" }}",
            tool.id(),
            pin.repo,
            version
        );
    }
    format!("{} = \"{}\"", tool.id(), pin.spec)
}

/// `rokit.toml` for `loren init`.
pub fn new_manifest(tool: Tool) -> String {
    [
        "# Toolchain for this project. Install it with: rokit install",
        "[tools]",
        &manifest_entry("rokit", tool),
        "",
    ]
    .join("\n")
}

pub fn line_has_tool(line: &str, tool: Tool) -> bool {
    let code = line.find('#').map_or(line, |i| &line[..i]);
    if code.trim().is_empty() {
        return false;
    }
    let Some(pin) = tool_pin(tool.id()) else {
        return false;
    };
    if code.to_lowercase().contains(&pin.repo.to_lowercase()) {
        return true;
    }
    Regex::new(&format!(r"(?i)^\s*{}\s*=", tool.id())).is_ok_and(|re| re.is_match(code))
}

pub fn manifest_tools(text: &str) -> Vec<Tool> {
    let mut found = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        for tool in Tool::PROGRAMS {
            if line_has_tool(line, tool) && !found.contains(&tool) {
                found.push(tool);
            }
        }
    }
    found
}

pub fn read_manifests(root: &Path) -> Vec<Manifest> {
    let mut out = Vec::new();
    for (manager, name) in MANAGERS {
        let file = root.join(name);
        if let Some(text) = read_text_if_exists(&file) {
            let tools = manifest_tools(&text);
            out.push(Manifest {
                manager,
                file,
                text,
                tools,
            });
        }
    }
    out
}

pub fn switch_manifest_tool(text: &str, manager: &str, from: Option<Tool>, to: Tool) -> String {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect();
    if let Some(from) = from.filter(|f| *f != to) {
        lines.retain(|line| !line_has_tool(line, from));
    }
    if !lines.iter().any(|line| line_has_tool(line, to)) {
        let entry = manifest_entry(manager, to);
        match lines.iter().position(|line| TOOLS_HEADER.is_match(line)) {
            None => {
                while lines.last().is_some_and(|l| l.trim().is_empty()) {
                    lines.pop();
                }
                if !lines.is_empty() {
                    lines.push(String::new());
                }
                lines.push("[tools]".to_string());
                lines.push(entry);
            }
            Some(header) => lines.insert(header + 1, entry),
        }
    }
    let mut out = lines.join(eol);
    if !out.ends_with(eol) {
        out.push_str(eol);
    }
    out
}

pub fn install_toolchain(root: &Path, run: &dyn Runner, is_tty: bool, yes: bool) -> Install {
    let manifests = read_manifests(root);
    let has = |m: &str| has_command(run, m, root);
    if manifests.is_empty() {
        return Install {
            ok: false,
            manager: None,
            reason: Some("No rokit.toml, aftman.toml or foreman.toml in the project.".to_string()),
        };
    }
    let manager: Option<&'static str> = if has("rokit") {
        Some("rokit")
    } else if manifests.iter().any(|m| m.manager == "aftman") && has("aftman") {
        Some("aftman")
    } else if manifests.iter().any(|m| m.manager == "foreman") && has("foreman") {
        Some("foreman")
    } else {
        None
    };
    let Some(manager) = manager else {
        return Install {
            ok: false,
            manager: None,
            reason: Some(
                "No toolchain manager found (Rokit, Aftman or Foreman). Install Rokit: https://github.com/rojo-rbx/rokit"
                    .to_string(),
            ),
        };
    };
    let mut args = vec!["install".to_string()];
    if yes && manager != "foreman" {
        args.push("--no-trust-check".to_string());
    }
    let stdio = if is_tty {
        Stdio::Inherit
    } else {
        Stdio::InheritOutput
    };
    let result = run.run(
        manager,
        &args,
        &RunOpts {
            cwd: Some(root.to_path_buf()),
            stdio,
        },
    );
    if result.ok() {
        return Install {
            ok: true,
            manager: Some(manager),
            reason: None,
        };
    }
    Install {
        ok: false,
        manager: Some(manager),
        reason: Some(format!(
            "{manager} install failed: {}.",
            describe_failure(manager, &result)
        )),
    }
}

/// The Luau-LSP keys for `tool`, in order.
pub fn vscode_settings(tool: Tool) -> Vec<(&'static str, Json)> {
    if tool == Tool::None {
        return plugin_settings();
    }
    vec![
        ("luau-lsp.sourcemap.enabled", Json::Bool(true)),
        (
            "luau-lsp.sourcemap.autogenerate",
            Json::Bool(tool == Tool::Rojo),
        ),
        (
            "luau-lsp.sourcemap.rojoProjectFile",
            Json::String(PROJECT_FILE.to_string()),
        ),
        (
            "luau-lsp.sourcemap.sourcemapFile",
            Json::String(SOURCEMAP_FILE.to_string()),
        ),
    ]
}

/// Merges [`vscode_settings`] into `.vscode/settings.json`; `Err` is the reason it could not.
pub fn apply_vscode_settings(root: &Path, tool: Tool) -> Result<PathBuf, String> {
    let file = root.join(".vscode").join("settings.json");
    let mut next = Json::object(vec![]);
    if is_file(&file) {
        let raw = read_text(&file).map_err(|e| e.to_string())?;
        let text = strip_bom(&raw);
        if !text.trim().is_empty() {
            let current = Json::parse(text).map_err(|_| {
                ".vscode/settings.json is not plain JSON (it has comments?)".to_string()
            })?;
            if !current.is_object() {
                return Err(".vscode/settings.json is not a JSON object".to_string());
            }
            next = current;
        }
    }
    for (key, value) in vscode_settings(tool) {
        next.set(key, value);
    }
    write_text(&file, format!("{}\n", next.stringify(4))).map_err(|e| e.to_string())?;
    Ok(file)
}
