//! `loren refresh [--watch]`, and the types/sourcemap steps every
//! module-changing command ends with (`lib/refresh.js`, `commands/refresh.js`).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::ctx::{CliError, CmdResult, Ctx, Tool, TypesOptions};
use crate::fsutil;
use crate::project::{project_paths, require_project};

pub struct RefreshOutcome {
    pub ok: bool,
    pub types_ok: bool,
    pub map_ok: bool,
}

pub fn resolve_project_tool(
    ctx: &Ctx,
    root: &Path,
    tool_override: Option<&str>,
) -> Result<Tool, CliError> {
    let tool_override = tool_override.filter(|t| !t.is_empty());
    ctx.lanes
        .resolve_tool(root, tool_override, ctx.run)
        .map_err(CliError::new)
}

/// `(shared, server, client)` relative to the root (`relativeDirs`).
pub fn relative_dirs(root: &Path) -> (String, String, String) {
    let p = project_paths(root);
    (
        fsutil::rel(root, &p.shared),
        fsutil::rel(root, &p.server),
        fsutil::rel(root, &p.client),
    )
}

/// Logs the outcome; true when the types were written or already current.
pub fn generate_types(ctx: &Ctx, root: &Path) -> bool {
    let (shared, server, client) = relative_dirs(root);
    let opts = TypesOptions {
        shared_dir: Some(shared),
        server_dir: Some(server),
        client_dir: Some(client),
        dry_run: false,
        log: Some(ctx.log),
    };
    match ctx.lanes.generate_types(root, &opts) {
        Ok(res) => {
            ctx.log.info(if res.written.is_empty() {
                "Types are up to date."
            } else {
                "Types regenerated."
            });
            true
        }
        Err(reason) => {
            ctx.log
                .error(&format!("LorenTypes were not regenerated: {reason}."));
            false
        }
    }
}

pub fn sourcemap(ctx: &Ctx, root: &Path, tool: Tool) -> bool {
    // Script Sync: Luau-LSP reads the DataModel from its Studio plugin.
    if tool == Tool::None {
        return true;
    }
    match ctx.lanes.sourcemap(root, tool, ctx.run) {
        Ok(()) => {
            ctx.log
                .info(&format!("Sourcemap regenerated with {}.", tool.label()));
            true
        }
        Err(reason) => {
            ctx.log
                .error(&format!("sourcemap.json was not regenerated: {reason}."));
            ctx.log
                .info("Install the toolchain (rokit install), then run: loren refresh");
            false
        }
    }
}

/// Types first, so a new LorenTypes file is on disk when the sourcemap is built.
pub fn refresh_project(
    ctx: &Ctx,
    root: &Path,
    tool: Tool,
    types: bool,
    map: bool,
) -> RefreshOutcome {
    let types_ok = if types {
        generate_types(ctx, root)
    } else {
        true
    };
    let map_ok = if map && tool != Tool::None {
        sourcemap(ctx, root, tool)
    } else {
        true
    };
    RefreshOutcome {
        ok: types_ok && map_ok,
        types_ok,
        map_ok,
    }
}

pub fn refresh_command(ctx: &Ctx, watch: bool, tool: Option<&str>) -> CmdResult {
    let root = require_project(&ctx.cwd)?;
    let tool = resolve_project_tool(ctx, &root, tool)?;
    let result = refresh_project(ctx, &root, tool, true, true);
    if !watch {
        if result.ok {
            ctx.log.ok(if tool == Tool::None {
                "Refreshed (Script Sync needs no sourcemap)."
            } else {
                "Types and sourcemap are up to date."
            });
        }
        return Ok(if result.ok { 0 } else { 1 });
    }
    watch_project(ctx, &root, tool)
}

type Snapshot = Vec<Vec<(String, Option<SystemTime>)>>;

fn snapshot(dirs: &[PathBuf]) -> Snapshot {
    dirs.iter()
        .map(|dir| {
            fsutil::list_rel_files(dir)
                .into_iter()
                .map(|rel| {
                    let mtime = std::fs::metadata(dir.join(&rel))
                        .and_then(|m| m.modified())
                        .ok();
                    (rel, mtime)
                })
                .collect()
        })
        .collect()
}

fn file_set(snap: &Snapshot) -> Vec<Vec<&str>> {
    snap.iter()
        .map(|dir| dir.iter().map(|(rel, _)| rel.as_str()).collect())
        .collect()
}

/// Polls rather than using a file-watch crate; runs until the process is interrupted.
fn watch_project(ctx: &Ctx, root: &Path, tool: Tool) -> CmdResult {
    let paths = project_paths(root);
    let dirs: Vec<PathBuf> = [paths.services, paths.controllers]
        .into_iter()
        .filter(|d| fsutil::is_dir(d))
        .collect();
    if dirs.is_empty() {
        return Err(CliError::new("No Services or Controllers folder to watch."));
    }
    let mut last = snapshot(&dirs);
    ctx.log
        .info("Watching Services and Controllers (Ctrl+C to stop)...");
    loop {
        std::thread::sleep(Duration::from_millis(300));
        let now = snapshot(&dirs);
        if now == last {
            continue;
        }
        let set_changed = file_set(&now) != file_set(&last);
        last = now;
        refresh_project(ctx, root, tool, true, set_changed);
    }
}
