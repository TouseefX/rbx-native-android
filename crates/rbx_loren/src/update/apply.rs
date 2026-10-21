//! `runUpdate` itself: confirmation, applying the planned actions, then the
//! sourcemap and the doctor pass.

use std::path::Path;

use super::plan::{finish, plan_update, Plan, PROMISE_FILES};
use crate::constants::{
    BACKUP_DIR, CLI_INSTALL, DOCS_URL, LOREN_CONFIG_FILE, MANAGERS, SHIM_SOURCE,
};
use crate::ctx::{DoctorOptions, Tool, UpdateOptions, UpdateResult};
use crate::fsutil::{copy_path, exists, is_file, rel, relative, remove_path, resolve, write_text};
use crate::lint::run_doctor;
use crate::scriptsync::loren_config_text;
use crate::tool::{resolve_tool, sourcemap};
use crate::types::generate_types;

fn backup(root: &Path, stamp: &str, abs: &Path, backed_up: &mut Vec<String>) -> Result<(), String> {
    if !exists(abs) {
        return Ok(());
    }
    let dest = root.join(BACKUP_DIR).join(stamp).join(relative(root, abs));
    if exists(&dest) {
        return Ok(());
    }
    copy_path(abs, &dest).map_err(|e| e.to_string())?;
    backed_up.push(rel(root, abs));
    Ok(())
}

pub(super) fn run_update(root: &Path, opts: &UpdateOptions) -> UpdateResult {
    let log = opts.log;
    let root = resolve(Path::new("/"), root);
    let root = root.as_path();
    let Plan {
        actions,
        date,
        override_tool,
        to_version,
        from_label,
        kind,
        script_sync,
        layout,
        project,
        project_text,
        project_file,
        patched_data,
        config,
        pkg_promise,
        promise_lib,
        user_promise,
        shim_target,
        stale_shims,
        type_opts,
        types_planned,
        stamp,
        backup_root,
    } = match plan_update(root, opts) {
        Ok(plan) => plan,
        Err(result) => return *result,
    };
    let tool_override = opts.tool.as_deref().filter(|s| !s.is_empty());

    let with_kind = |mut r: UpdateResult| {
        r.kind = Some(kind);
        r
    };

    let legacy = kind == "legacy";
    if opts.dry_run {
        log.info(&format!(
            "Then: {}the middleware lint (preview below). Dry run: nothing was changed.",
            if script_sync { "" } else { "sourcemap, and " }
        ));
        let doctor = run_doctor(
            root,
            &DoctorOptions {
                fix: false,
                yes: false,
                is_tty: opts.is_tty,
                prompt: None,
                log,
                server_dir: Some(layout.server.clone()),
                src_dir: None,
                legacy,
                now: None,
            },
        );
        let mut r = finish(&actions, 0);
        r.kind = Some(kind);
        r.from_version = Some(from_label);
        r.to_version = Some(to_version);
        r.doctor = Some(doctor);
        r.dry_run = true;
        return r;
    }

    if !actions.is_empty() && !opts.yes {
        if !opts.is_tty {
            log.error("loren update needs confirmation. Run it again with --yes, or with --dry-run to preview. Nothing was changed.");
            return with_kind(finish(&actions, 2));
        }
        let ok = opts.prompt.is_some_and(|p| {
            p.confirm(
                &format!("Update this project to Loren {to_version}?"),
                false,
            )
        });
        if !ok {
            log.info("Cancelled. Nothing was changed.");
            let mut r = with_kind(finish(&actions, 1));
            r.cancelled = true;
            return r;
        }
    }

    let mut backed_up: Vec<String> = Vec::new();
    let applied: Result<(), String> = (|| {
        for a in &actions {
            match a.kind {
                "runtime" => {
                    backup(root, &stamp, &a.target, &mut backed_up)?;
                    remove_path(&a.target);
                    for (rel_path, data) in &a.files {
                        write_text(&a.target.join(rel_path), data).map_err(|e| e.to_string())?;
                    }
                }
                "shim" => {
                    for f in &project.existing {
                        backup(root, &stamp, f, &mut backed_up)?;
                    }
                    for f in &stale_shims {
                        remove_path(f);
                    }
                    write_text(&shim_target, SHIM_SOURCE).map_err(|e| e.to_string())?;
                }
                "project" => {
                    backup(root, &stamp, &project_file, &mut backed_up)?;
                    let eol = if project_text.contains("\r\n") {
                        "\r\n"
                    } else {
                        "\n"
                    };
                    let data = patched_data
                        .as_ref()
                        .map(|d| d.stringify(4))
                        .unwrap_or_default();
                    write_text(&project_file, format!("{data}\n").replace('\n', eol))
                        .map_err(|e| e.to_string())?;
                }
                "config" => {
                    let file = root.join(LOREN_CONFIG_FILE);
                    backup(root, &stamp, &file, &mut backed_up)?;
                    write_text(&file, loren_config_text(&to_version, config.as_ref()))
                        .map_err(|e| e.to_string())?;
                }
                "promise" => {
                    backup(root, &stamp, &user_promise, &mut backed_up)?;
                    remove_path(&user_promise);
                    if script_sync {
                        copy_path(&promise_lib, &user_promise).map_err(|e| e.to_string())?;
                    } else {
                        for f in PROMISE_FILES {
                            copy_path(&pkg_promise.join(f), &user_promise.join(f))
                                .map_err(|e| e.to_string())?;
                        }
                    }
                }
                "types" => {
                    for f in &types_planned {
                        backup(root, &stamp, f, &mut backed_up)?;
                    }
                    generate_types(root, &type_opts)?;
                }
                _ => {}
            }
        }
        Ok(())
    })();
    let backup_dir = if backed_up.is_empty() {
        None
    } else {
        Some(backup_root.clone())
    };
    if let Err(message) = applied {
        let note = if backed_up.is_empty() {
            String::new()
        } else {
            format!(" Your previous files are in {backup_root}/.")
        };
        log.error(&format!("loren update stopped: {message}.{note}"));
        let mut r = with_kind(finish(&actions, 1));
        r.backup_dir = backup_dir;
        return r;
    }
    if !actions.is_empty() {
        log.ok(&format!(
            "Updated to runtime {to_version}.{}",
            if backed_up.is_empty() {
                String::new()
            } else {
                format!(" Backup: {backup_root}/")
            }
        ));
    }

    let mut exit_code = 0;
    let mut tool: Option<Tool> = None;
    let run = opts.runner;
    if script_sync || override_tool == Some(Tool::None) {
        tool = Some(Tool::None);
        if !script_sync {
            log.info("Skipped the sourcemap (--tool none).");
        }
    } else {
        match resolve_tool(root, tool_override, run) {
            Ok(t) => tool = Some(t),
            Err(err) => {
                log.error(&format!("sourcemap.json was not regenerated: {err}"));
                exit_code = 1;
            }
        }
    }
    if let Some(t) = tool.filter(|t| t.is_program()) {
        match sourcemap(root, t, run) {
            Ok(()) => log.info(&format!("Sourcemap regenerated with {}.", t.label())),
            Err(err) => {
                let pinned: Vec<&str> = MANAGERS
                    .iter()
                    .map(|(_, f)| *f)
                    .filter(|f| is_file(&root.join(f)))
                    .collect();
                let hint = if pinned.is_empty() {
                    String::new()
                } else {
                    format!(
                        " If {} pins a version you do not have, install it first (rokit install).",
                        pinned.join(" or ")
                    )
                };
                let msg = err.trim_end();
                let msg = msg.strip_suffix('.').unwrap_or(msg);
                log.error(&format!(
                    "sourcemap.json was not regenerated: {msg}.{hint} The project itself is updated; run loren refresh once {} works.",
                    t.label()
                ));
                exit_code = 1;
            }
        }
    }

    let doctor = run_doctor(
        root,
        &DoctorOptions {
            fix: true,
            yes: opts.yes,
            is_tty: opts.is_tty,
            prompt: opts.prompt,
            log,
            server_dir: Some(layout.server.clone()),
            src_dir: None,
            legacy,
            now: Some(date),
        },
    );
    if exit_code == 0 {
        exit_code = doctor.exit_code;
    }

    if legacy && !actions.is_empty() {
        log.info(&format!("Next: press Play in Studio and read Loren's boot report. What changed since 1.5.1: {DOCS_URL}"));
    }
    if script_sync && !actions.is_empty() {
        log.info("Studio picks up these file changes through Script Sync. If it shows the conflict dialog, choose Keep Disk.");
    }
    if !actions.is_empty() {
        log.info(&format!(
            "This updated the project's runtime. To update the Loren CLI itself: {CLI_INSTALL}"
        ));
    }
    let mut r = with_kind(finish(&actions, exit_code));
    r.from_version = Some(from_label);
    r.to_version = Some(to_version);
    r.tool = tool;
    r.backup_dir = backup_dir;
    r.backed_up = backed_up;
    r.doctor = Some(doctor);
    r
}
