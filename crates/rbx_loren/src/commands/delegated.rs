//! `loren update`, `loren types` and `loren doctor` (`commands/delegated.js`).

use std::path::{Path, PathBuf};

use crate::commands::refresh::{generate_types, relative_dirs};
use crate::constants::{Key, MANAGERS, PROJECT_FILE, PROMISE_DIR, SCRIPT_SYNC_LABEL, SHIM_SOURCE};
use crate::ctx::{CliError, CmdResult, Ctx, DoctorOptions, UpdateOptions};
use crate::fsutil::{
    self, is_dir, is_file, normalize_eol, read_text_if_exists, rel, strip_bom, walk_files,
};
use crate::json::Json;
use crate::project::{project_paths, read_project_file, require_project};
use crate::run::has_command;
use crate::scriptsync::{is_script_sync, plugin_settings, synced_folders, COMPANION_PLUGIN};

pub struct UpdateFlags {
    pub dry_run: bool,
    pub yes: bool,
    pub no_native: bool,
    pub tool: Option<String>,
}

fn is_shim(text: Option<&str>) -> bool {
    text.is_some_and(|t| normalize_eol(t).trim() == SHIM_SOURCE.trim())
}

fn at(root: &Path, p: &str) -> PathBuf {
    p.split('/')
        .fold(root.to_path_buf(), |acc, part| acc.join(part))
}

fn settings_file(root: &Path) -> Option<Result<Json, ()>> {
    let file = root.join(".vscode").join("settings.json");
    if !is_file(&file) {
        return None;
    }
    let parsed = read_text_if_exists(&file)
        .ok_or(())
        .and_then(|t| Json::parse(strip_bom(&t)).map_err(|_| ()));
    Some(parsed)
}

fn report(ctx: &Ctx, problems: &mut usize, msg: &str) {
    *problems += 1;
    ctx.log.error(msg);
}

/// A Script Sync project (.loren.json): nothing to install; the runtime lives in the synced folders.
fn script_sync_checks(ctx: &Ctx, root: &Path, problems: &mut usize) {
    let log = ctx.log;
    log.ok(&format!(
        "Sync: {SCRIPT_SYNC_LABEL} (built into Studio; nothing to install)"
    ));
    let shared = Key::Shared.script_sync_path();
    let runtime_shared = Key::RuntimeShared.script_sync_path();
    let runtime_server = Key::RuntimeServer.script_sync_path();
    let packages = Key::Packages.script_sync_path();
    let shim_file = at(root, shared).join("Loren.luau");
    if !is_shim(read_text_if_exists(&shim_file).as_deref()) {
        report(
            ctx,
            problems,
            &format!(
                "{} is missing or not the current 2.x shim. Run: loren update",
                rel(root, &shim_file)
            ),
        );
    } else if !is_file(&at(root, runtime_shared).join("init.luau"))
        || !is_file(&at(root, runtime_server).join("init.luau"))
    {
        report(
            ctx,
            problems,
            &format!("{runtime_shared} or {runtime_server} is missing. Run: loren update"),
        );
    } else {
        log.ok("Runtime layout: LorenRuntime, LorenServer and the shim are in place.");
    }
    if !is_file(&at(root, packages).join(format!("{PROMISE_DIR}.luau"))) {
        report(
            ctx,
            problems,
            &format!("{packages}/{PROMISE_DIR}.luau is missing (the runtime requires it). Run: loren update"),
        );
    }
    // Script Sync makes X.client.luau a Script with RunContext Client: it runs twice from StarterPlayerScripts.
    for file in walk_files(&root.join("StarterPlayer"), &fsutil::DEFAULT_SKIP) {
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if name.ends_with(".client.luau") || name.ends_with(".client.lua") {
            report(
                ctx,
                problems,
                &format!(
                    "{} syncs as a Script with RunContext Client, which runs twice from StarterPlayerScripts. Rename it to *.local.luau (a LocalScript).",
                    rel(root, &file)
                ),
            );
        }
    }
    match settings_file(root) {
        Some(Ok(settings)) => {
            if !matches!(
                settings.get("luau-lsp.plugin.enabled"),
                Some(Json::Bool(true))
            ) {
                let wanted = Json::object(plugin_settings()).stringify(0);
                log.warn(&format!(
                    ".vscode/settings.json: set {wanted} for Luau-LSP plugin mode."
                ));
            }
        }
        Some(Err(())) => log.warn(
            ".vscode/settings.json is not plain JSON; its Luau-LSP settings were not checked.",
        ),
        None => {}
    }
    let instances: Vec<String> = synced_folders()
        .into_iter()
        .map(|(instance, _)| instance)
        .collect();
    log.info(&format!(
        "Studio: sync {} (right-click > Sync to…; see README.md).",
        instances.join(", ")
    ));
    log.info(&format!(
        "Autocomplete: the \"{COMPANION_PLUGIN}\" Studio plugin with the Luau Language Server extension."
    ));
}

/// A failed lane result's own exit code (2 = "needs --yes"), else 1.
fn fail_code(code: i32) -> i32 {
    if code > 0 && code < 256 {
        code
    } else {
        1
    }
}

pub fn update_command(ctx: &Ctx, flags: &UpdateFlags) -> CmdResult {
    let root = require_project(&ctx.cwd)?;
    let opts = UpdateOptions {
        template_dir: ctx.template_dir.clone(),
        tool: flags.tool.clone(),
        dry_run: flags.dry_run,
        yes: flags.yes,
        no_native: flags.no_native,
        is_tty: ctx.is_tty,
        prompt: Some(&ctx.prompt),
        log: ctx.log,
        runner: ctx.run,
        now: None,
        force: false,
    };
    let res = ctx.lanes.run_update(&root, &opts);
    Ok(if res.exit_code == 0 {
        0
    } else {
        fail_code(res.exit_code)
    })
}

pub fn types_command(ctx: &Ctx) -> CmdResult {
    let root = require_project(&ctx.cwd)?;
    Ok(if generate_types(ctx, &root) { 0 } else { 1 })
}

/// Environment and project checks; returns the number of problems.
pub fn environment_checks(ctx: &Ctx, root: &Path, in_project: bool) -> Result<usize, CliError> {
    let log = ctx.log;
    let run = ctx.run;
    let mut problems = 0usize;

    // No Node version line: there is no Node here.

    if in_project && is_script_sync(root) {
        script_sync_checks(ctx, root, &mut problems);
        return Ok(problems);
    }

    let managers: Vec<&str> = MANAGERS
        .iter()
        .map(|(name, _)| *name)
        .filter(|m| has_command(run, m, root))
        .collect();
    if managers.is_empty() {
        log.warn(
            "No Rokit, Aftman or Foreman found. Install Rokit: https://github.com/rojo-rbx/rokit",
        );
    } else {
        log.ok(&format!("Toolchain manager: {}", managers.join(", ")));
    }

    if !in_project {
        log.info(&format!(
            "No {PROJECT_FILE} here: skipped the project checks. Run loren doctor in a project root to check it."
        ));
        return Ok(problems);
    }

    let tool = match ctx.lanes.resolve_tool(root, None, run) {
        Ok(tool) => Some(tool).filter(|t| t.is_program()),
        Err(message) => {
            report(ctx, &mut problems, &message);
            None
        }
    };
    if let Some(tool) = tool {
        if has_command(run, tool.id(), root) {
            log.ok(&format!("Sync tool: {}", tool.label()));
        } else {
            report(
                ctx,
                &mut problems,
                &format!(
                    "{} does not run in this project. Install it: rokit install",
                    tool.id()
                ),
            );
        }
        log.info(&format!(
            "Studio: install the {} plugin once with: {} plugin install",
            tool.label(),
            tool.id()
        ));
    }

    // The 2.0 layout: the managed runtime, the shim and the project file mappings.
    let data = match read_project_file(root) {
        Ok(data) => Some(data),
        Err(err) => {
            report(ctx, &mut problems, &err.message);
            None
        }
    };
    let mapped = |service: &str, name: &str| {
        data.as_ref()
            .and_then(|d| d.get("tree"))
            .and_then(|t| t.get(service))
            .and_then(|s| s.get(name))
            .is_some()
    };
    let paths = project_paths(root);
    let shim = read_text_if_exists(&paths.shared.join("Loren.luau"));
    if shim.is_some() && !is_shim(shim.as_deref()) {
        report(
            ctx,
            &mut problems,
            "Shared/Loren.luau is not the current 2.x shim (an older shim, or a Loren 1.x project). Run: loren update",
        );
    } else if !mapped("ReplicatedStorage", "LorenRuntime")
        || !mapped("ServerScriptService", "LorenServer")
    {
        report(
            ctx,
            &mut problems,
            &format!("{PROJECT_FILE} does not map LorenRuntime and LorenServer. Run: loren update"),
        );
    } else {
        log.ok("Runtime layout: LorenRuntime, LorenServer and the shim are in place.");
    }
    if !is_dir(&paths.packages.join(PROMISE_DIR)) {
        report(
            ctx,
            &mut problems,
            &format!("loren_packages/{PROMISE_DIR} is missing (the runtime requires it). Run: loren update"),
        );
    }

    // Luau-LSP: the sourcemap must come from the project's tool.
    if let Some(tool) = tool {
        match settings_file(root) {
            Some(Ok(settings)) => {
                let wanted = tool.id() == "rojo";
                let shown = match settings.get("luau-lsp.sourcemap.autogenerate") {
                    Some(Json::Bool(b)) if *b == wanted => None,
                    Some(other) => Some(
                        other
                            .as_str()
                            .map(String::from)
                            .unwrap_or_else(|| other.stringify(0)),
                    ),
                    None => None,
                };
                if let Some(auto) = shown {
                    log.warn(&format!(
                        ".vscode/settings.json has luau-lsp.sourcemap.autogenerate = {auto}; use {wanted} with {}.",
                        tool.label()
                    ));
                }
            }
            Some(Err(())) => log.warn(
                ".vscode/settings.json is not plain JSON; its Luau-LSP settings were not checked.",
            ),
            None => {}
        }
    }
    Ok(problems)
}

pub fn doctor_command(ctx: &Ctx, fix: bool, yes: bool) -> CmdResult {
    let base = std::env::current_dir().unwrap_or_default();
    let root = fsutil::resolve(&base, &ctx.cwd);
    let in_project = is_file(&root.join(PROJECT_FILE)) || is_script_sync(&root);
    let problems = environment_checks(ctx, &root, in_project)?;
    if !in_project {
        return Ok(if problems > 0 { 1 } else { 0 });
    }

    let (_, server_dir, _) = relative_dirs(&root);
    let opts = DoctorOptions {
        fix,
        yes,
        is_tty: ctx.is_tty,
        prompt: Some(&ctx.prompt),
        log: ctx.log,
        server_dir: Some(server_dir),
        src_dir: None,
        legacy: false,
        now: None,
    };
    let lint = ctx.lanes.run_doctor(&root, &opts);
    if lint.exit_code != 0 {
        return Ok(fail_code(lint.exit_code));
    }
    Ok(if problems > 0 { 1 } else { 0 })
}
