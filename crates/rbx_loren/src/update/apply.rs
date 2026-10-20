//! `runUpdate` itself (`update.js` lines 320-628).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::project::{
    compare_versions, detect_project, globs_for, misplaced_folders, outside_mappings,
    patch_project, promise_problems, runtime_differs, runtime_files, Patched,
};
use crate::constants::{
    BACKUP_DIR, CLI_INSTALL, DOCS_URL, LOREN_CONFIG_FILE, MANAGERS, PROJECT_FILE, PROMISE_DIR,
    SCRIPT_SYNC_ID, SCRIPT_SYNC_LABEL, SCRIPT_SYNC_LAYOUT, SHIM_SOURCE,
};
use crate::ctx::TypesOptions;
use crate::ctx::{DoctorOptions, Tool, UpdateAction, UpdateOptions, UpdateResult};
use crate::fsutil::{
    copy_path, exists, is_dir, is_file, normalize_eol, read_text_if_exists, rel, relative,
    remove_path, resolve, strip_bom, timestamp, to_posix, write_text,
};
use crate::json::Json;
use crate::lint::{project_layout, run_doctor};
use crate::scriptsync::{
    is_script_sync, loren_config_text, read_loren_config, runtime_version, server_runtime_source,
    LorenConfig,
};
use crate::tool::{resolve_tool, sourcemap};
use crate::types::generate_types;

const RUNTIME_DIR: &str = "loren";
const SHIM_NAME: &str = "Loren.luau";
const PROMISE_FILES: [&str; 3] = ["lib/init.lua", "LICENSE", PROJECT_FILE];

type Files = Vec<(String, Vec<u8>)>;

struct Act {
    kind: &'static str,
    path: String,
    detail: String,
    target: PathBuf,
    files: Files,
}

fn finish(actions: &[Act], exit_code: i32) -> UpdateResult {
    UpdateResult {
        exit_code,
        actions: actions
            .iter()
            .map(|a| UpdateAction {
                kind: a.kind,
                path: a.path.clone(),
                detail: a.detail.clone(),
            })
            .collect(),
        ..UpdateResult::default()
    }
}

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
    let mut actions: Vec<Act> = Vec::new();
    let date = opts.now.unwrap_or_else(SystemTime::now);
    let tool_override = opts.tool.as_deref().filter(|s| !s.is_empty());
    let override_tool = tool_override.and_then(Tool::parse);

    if let (Some(value), None) = (tool_override, override_tool) {
        log.error(&format!(
            "Unknown sync tool \"{value}\". Use --tool rojo or --tool argon. Nothing was changed."
        ));
        return finish(&actions, 1);
    }

    let pkg_project = resolve(Path::new("/"), &opts.template_dir);
    let pkg_runtime = pkg_project.join(RUNTIME_DIR);
    let pkg_promise = pkg_project.join("loren_packages").join(PROMISE_DIR);
    let mut missing_pkg = vec![pkg_runtime.join("shared").join("init.luau")];
    missing_pkg.extend(PROMISE_FILES.iter().map(|f| pkg_promise.join(f)));
    let missing: Vec<String> = missing_pkg
        .iter()
        .filter(|f| !is_file(f))
        .map(|f| to_posix(&relative(&pkg_project, f)))
        .collect();
    if !missing.is_empty() {
        log.error(&format!(
            "This install of the Loren CLI is incomplete (missing {}). Reinstall it: {CLI_INSTALL}",
            missing.join(", ")
        ));
        return finish(&actions, 1);
    }
    let to_version =
        runtime_version(&pkg_runtime.join("shared")).unwrap_or_else(|| "unknown".to_string());

    let script_sync = is_script_sync(root);
    let project_file = root.join(PROJECT_FILE);
    let mut project_text = String::new();
    let mut project_data: Option<Json> = None;
    let layout;
    if script_sync {
        if override_tool.is_some_and(|t| t.id() != SCRIPT_SYNC_ID) {
            log.error(&format!(
                "This project uses {SCRIPT_SYNC_LABEL} ({LOREN_CONFIG_FILE}), so --tool {} does not apply. Nothing was changed.",
                tool_override.unwrap_or_default()
            ));
            return finish(&actions, 1);
        }
        layout = project_layout(root, None);
    } else {
        let Some(text) = read_text_if_exists(&project_file) else {
            log.error(&format!(
                "{} is not a Loren project: there is no {PROJECT_FILE}. Run loren update from your project's root folder.",
                root.display()
            ));
            return finish(&actions, 1);
        };
        let data = match Json::parse(strip_bom(&text)) {
            Ok(d) => d,
            Err(err) => {
                log.error(&format!("{PROJECT_FILE} is not valid JSON ({err}). Fix it, then run loren update again."));
                return finish(&actions, 1);
            }
        };
        layout = project_layout(root, Some(&data));
        for m in outside_mappings(root, &data) {
            log.warn(&format!(
                "{PROJECT_FILE} maps {m}, outside this project. loren update only manages folders inside it, so it uses the default folder there instead."
            ));
        }
        project_text = text;
        project_data = Some(data);
    }
    let project = detect_project(root, &layout);
    if project.kind == "none" {
        log.error(&format!(
            "{} is not a Loren project: {}. Nothing was changed.",
            root.display(),
            project.reason
        ));
        let mut r = finish(&actions, 1);
        r.kind = Some("none");
        return r;
    }
    let kind = project.kind;
    let from_label = if kind == "legacy" {
        "1.5.1".to_string()
    } else {
        project
            .installed_version
            .clone()
            .unwrap_or_else(|| "2.x".to_string())
    };
    log.info(&format!(
        "Loren {from_label} project ({}). This CLI ships runtime {to_version}.",
        project.reason
    ));

    let with_kind = |mut r: UpdateResult| {
        r.kind = Some(kind);
        r
    };

    if let Some(installed) = &project.installed_version {
        if to_version != "unknown" && compare_versions(installed, &to_version) > 0 && !opts.force {
            log.error(&format!(
                "This project's runtime ({installed}) is newer than this CLI's ({to_version}). Update the CLI first: {CLI_INSTALL}"
            ));
            return with_kind(finish(&actions, 1));
        }
    }

    let runtimes: Vec<(String, Files)> = if script_sync {
        vec![
            (
                layout.runtime_shared.clone().unwrap_or_default(),
                runtime_files(&pkg_runtime.join("shared"), opts.no_native, None),
            ),
            (
                layout.runtime_server.clone().unwrap_or_default(),
                runtime_files(
                    &pkg_runtime.join("server"),
                    opts.no_native,
                    Some(server_runtime_source),
                ),
            ),
        ]
    } else {
        vec![(
            RUNTIME_DIR.to_string(),
            runtime_files(&pkg_runtime, opts.no_native, None),
        )]
    };
    for (dir, files) in runtimes {
        let target = root.join(&dir);
        if !runtime_differs(&target, &files) {
            continue;
        }
        let looks_ours = !is_dir(&target) || project.has_runtime;
        let detail = format!(
            "{} {dir}/ with runtime {to_version} ({} files{}){}",
            if is_dir(&target) { "replace" } else { "add" },
            files.len(),
            if opts.no_native { ", no @native" } else { "" },
            if looks_ours {
                String::new()
            } else {
                format!("; the current {dir}/ does not look like a Loren runtime")
            }
        );
        actions.push(Act {
            kind: "runtime",
            path: dir,
            detail,
            target,
            files,
        });
    }

    let shim_target = root.join(&layout.shared).join(SHIM_NAME);
    let stale_shims: Vec<PathBuf> = project
        .existing
        .iter()
        .filter(|f| **f != shim_target)
        .cloned()
        .collect();
    if project.loren_state != "shim" || !stale_shims.is_empty() {
        let what = match project.loren_state {
            "runtime" => "the 1.5.1 runtime",
            "missing" => "nothing",
            "shim" => "a second Loren file",
            "shim-modified" => "a changed shim",
            _ => "a file that is not Loren",
        };
        let r = rel(root, &shim_target);
        actions.push(Act {
            kind: "shim",
            detail: format!("write the shim to {r} (replaces {what})"),
            path: r,
            target: shim_target.clone(),
            files: Vec::new(),
        });
    }

    let mut patched_data: Option<Json> = None;
    if let Some(data) = &project_data {
        match patch_project(data, &layout) {
            Patched::Error { error, manual } => {
                log.error(&format!(
                    "{error} Loren cannot patch it safely, so nothing was changed."
                ));
                log.plain(&manual);
                return with_kind(finish(&actions, 1));
            }
            Patched::Ok { data, changes } => {
                if !changes.is_empty() {
                    actions.push(Act {
                        kind: "project",
                        path: PROJECT_FILE.to_string(),
                        detail: format!("patch {PROJECT_FILE}: {}", changes.join("; ")),
                        target: project_file.clone(),
                        files: Vec::new(),
                    });
                }
                patched_data = Some(data);
            }
        }
    }
    let config: Option<Json> = if script_sync {
        match read_loren_config(root) {
            LorenConfig::Config(c) => Some(c),
            _ => None,
        }
    } else {
        None
    };
    if let Some(c) = &config {
        let same = c.get("runtime").and_then(Json::as_str) == Some(to_version.as_str())
            && c.get("layout").and_then(Json::as_str) == Some(SCRIPT_SYNC_LAYOUT);
        if to_version != "unknown" && !same {
            actions.push(Act {
                kind: "config",
                path: LOREN_CONFIG_FILE.to_string(),
                detail: format!("record runtime {to_version} in {LOREN_CONFIG_FILE}"),
                target: root.join(LOREN_CONFIG_FILE),
                files: Vec::new(),
            });
        }
    }

    let user_promise = if script_sync {
        root.join(&layout.packages)
            .join(format!("{PROMISE_DIR}.luau"))
    } else {
        root.join(&layout.packages).join(PROMISE_DIR)
    };
    let promise_lib = pkg_promise.join("lib").join("init.lua");
    let promise_why: Vec<String> = if script_sync {
        let have = read_text_if_exists(&user_promise);
        let want = read_text_if_exists(&promise_lib);
        if have.as_deref().map(normalize_eol) == want.as_deref().map(normalize_eol) {
            Vec::new()
        } else {
            vec![if have.is_none() {
                "it is missing"
            } else {
                "it differs from the bundled Promise"
            }
            .to_string()]
        }
    } else {
        promise_problems(&pkg_promise, &user_promise)
    };
    if !promise_why.is_empty() {
        let r = rel(root, &user_promise);
        let detail = if script_sync {
            format!(
                "write {r} (the bundled Promise, lib/init.lua): {}",
                promise_why.join("; ")
            )
        } else {
            format!(
                "vendor {r} (lib/init.lua, LICENSE, {PROJECT_FILE}): {}",
                promise_why.join("; ")
            )
        };
        actions.push(Act {
            kind: "promise",
            path: r,
            detail,
            target: user_promise.clone(),
            files: Vec::new(),
        });
    }

    let type_opts = TypesOptions {
        shared_dir: Some(layout.shared.clone()),
        server_dir: Some(layout.server.clone()),
        client_dir: Some(layout.client.clone()),
        dry_run: false,
        log: None,
    };
    let types_plan = generate_types(
        root,
        &TypesOptions {
            dry_run: true,
            ..clone_types(&type_opts)
        },
    )
    .unwrap_or_default();
    if !types_plan.planned.is_empty() {
        let rels: Vec<String> = types_plan.planned.iter().map(|f| rel(root, f)).collect();
        actions.push(Act {
            kind: "types",
            path: rels.join(", "),
            detail: format!("regenerate {}", rels.join(" and ")),
            target: root.to_path_buf(),
            files: Vec::new(),
        });
    }

    for m in misplaced_folders(root, &layout) {
        log.warn(&format!("Found {m}. Move its modules, or they never load."));
    }

    let stamp = timestamp(date);
    let backup_root = format!("{BACKUP_DIR}/{stamp}");
    if actions.is_empty() {
        log.ok(&format!(
            "Already up to date: runtime {to_version}, shim, {}Promise and types match.",
            if script_sync {
                String::new()
            } else {
                format!("{PROJECT_FILE}, ")
            }
        ));
    } else {
        log.info(if opts.dry_run { "Would do:" } else { "Plan:" });
        for a in &actions {
            log.plain(&format!("  - {}", a.detail));
        }
        log.plain(&format!(
            "  Replaced files are backed up to {backup_root}/."
        ));
    }

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
                    for f in &types_plan.planned {
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

fn clone_types<'a>(o: &TypesOptions<'a>) -> TypesOptions<'a> {
    TypesOptions {
        shared_dir: o.shared_dir.clone(),
        server_dir: o.server_dir.clone(),
        client_dir: o.client_dir.clone(),
        dry_run: o.dry_run,
        log: o.log,
    }
}

#[allow(dead_code)]
fn _globs(p: &str) -> Vec<String> {
    globs_for(p)
}
