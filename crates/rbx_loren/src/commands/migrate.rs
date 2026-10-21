//! `loren migrate [--to rojo|argon]` (`commands/migrate.js`).

use std::time::SystemTime;

use crate::commands::refresh::sourcemap;
use crate::constants::{LOREN_CONFIG_FILE, MANAGERS, SCRIPT_SYNC_LABEL};
use crate::ctx::{CliError, CmdResult, Ctx, Tool};
use crate::fsutil::{rel, timestamp, write_text};
use crate::json::Json;
use crate::project::backup_path;
use crate::project::require_project;
use crate::run::has_command;
use crate::scriptsync::is_script_sync;
use crate::toolchain::{
    apply_vscode_settings, install_toolchain, new_manifest, read_manifests, switch_manifest_tool,
    vscode_settings,
};

pub fn migrate_command(ctx: &Ctx, to: Option<&str>, yes: bool) -> CmdResult {
    let root = require_project(&ctx.cwd)?;
    let requested = match to {
        Some(value) => Some(Tool::parse(value).ok_or_else(|| {
            CliError::hint(
                format!("Unknown tool \"{value}\"."),
                "Pass --to rojo or --to argon.",
            )
        })?),
        None => None,
    };
    // Script Sync has another folder layout and no project file: not automated.
    let from_none = is_script_sync(&root);
    if from_none || requested == Some(Tool::None) {
        return Err(CliError::hint(
            format!(
                "Migrating {} {SCRIPT_SYNC_LABEL} is not automated yet. Nothing was changed.",
                if from_none { "from" } else { "to" }
            ),
            if from_none {
                format!("Create a project with loren init <name> --tool rojo (or argon) and move your Services and Controllers into it. This one is marked by {LOREN_CONFIG_FILE}.")
            } else {
                "Create a project with loren init <name> --tool none and move your Services and Controllers into it."
                    .to_string()
            },
        ));
    }
    let manifests = read_manifests(&root);
    let mut named: Vec<Tool> = Vec::new();
    for t in manifests.iter().flat_map(|m| m.tools.iter().copied()) {
        if !named.contains(&t) {
            named.push(t);
        }
    }

    let mut current = ctx
        .lanes
        .detect_tool(&root, ctx.run)
        .filter(|t| t.is_program());
    if current.is_none() && named.len() == 1 {
        current = Some(named[0]);
    }

    let target = requested.or(current.map(Tool::other)).ok_or_else(|| {
        CliError::hint(
            "Could not tell whether this project uses Rojo or Argon.",
            "Pass --to rojo or --to argon.",
        )
    })?;

    let from = match current {
        Some(c) if c != target => Some(c),
        _ => named.iter().copied().find(|t| *t != target),
    };
    if from.is_none() && named.contains(&target) {
        ctx.log
            .ok(&format!("This project already uses {}.", target.label()));
        return Ok(0);
    }

    let question = match from {
        Some(f) => format!(
            "Switch this project from {} to {}?",
            f.label(),
            target.label()
        ),
        None => format!("Set this project up for {}?", target.label()),
    };
    if !yes && !ctx.is_tty {
        return Err(CliError::hint(
            "loren migrate needs a confirmation.",
            "Run it in a terminal, or pass --yes.",
        ));
    }
    if !ctx.prompt.confirm(&question, false) {
        ctx.log.info("Cancelled; nothing was changed.");
        return Ok(1);
    }

    let stamp = timestamp(SystemTime::now());
    let mut problems = 0;

    // 1. Toolchain manifests (backed up first).
    if manifests.is_empty() {
        let name = MANAGERS[0].1;
        if let Err(err) = write_text(&root.join(name), new_manifest(target)) {
            return Err(CliError::new(format!("Could not write {name}: {err}")));
        }
        ctx.log
            .info(&format!("Created {name} with {}.", target.label()));
    } else {
        for m in &manifests {
            if m.tools.is_empty() && manifests.iter().any(|o| !o.tools.is_empty()) {
                continue;
            }
            let next = switch_manifest_tool(&m.text, m.manager, from, target);
            if next == m.text {
                continue;
            }
            let saved = backup_path(&root, &m.file, &stamp)
                .map_err(CliError::new)?
                .unwrap_or_else(|| m.file.clone());
            if let Err(err) = write_text(&m.file, next) {
                return Err(CliError::new(format!(
                    "Could not write {}: {err}",
                    rel(&root, &m.file)
                )));
            }
            ctx.log.info(&format!(
                "Updated {} (backup: {}).",
                rel(&root, &m.file),
                rel(&root, &saved)
            ));
        }
    }

    // 2. Luau-LSP settings: only Rojo autogenerates the sourcemap from VS Code.
    match apply_vscode_settings(&root, target) {
        Ok(_) => ctx.log.info(&format!(
            "Updated .vscode/settings.json for {}.",
            target.label()
        )),
        Err(reason) => {
            problems += 1;
            let settings = Json::object(vscode_settings(target)).stringify(0);
            ctx.log
                .error(&format!("{reason}. Set these by hand: {settings}"));
        }
    }

    // 3. Install the new tool, then make sure it runs.
    ctx.log.info(&format!("Installing {}...", target.label()));
    let install = install_toolchain(&root, ctx.run, ctx.is_tty, yes);
    let reason = install.reason.clone().unwrap_or_default();
    let runs = has_command(ctx.run, target.id(), &root);
    if !runs {
        problems += 1;
        let why = if install.ok {
            String::new()
        } else {
            format!(" ({reason})")
        };
        ctx.log.error(&format!(
            "{} does not run in this project{why}. Install it (rokit install), then run: loren refresh",
            target.id()
        ));
    } else if !install.ok {
        ctx.log
            .warn(&format!("{reason} {} is available anyway.", target.id()));
    }

    // 4. A fresh sourcemap from the new tool.
    if runs && !sourcemap(ctx, &root, target) {
        problems += 1;
    }

    if problems > 0 {
        ctx.log.error(&format!(
            "The project files now name {}, but {problems} step(s) failed (see above).",
            target.label()
        ));
        return Ok(1);
    }
    ctx.log.ok(&format!(
        "Migrated to {}. Start it with: loren serve",
        target.label()
    ));
    Ok(0)
}
