//! Work out what `loren update` would do: validate the project, then list
//! the actions, without touching anything.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::patch::{outside_mappings, patch_project, Patched};
use super::project::{compare_versions, detect_project, Detected};
use super::runtime::{misplaced_folders, promise_problems, runtime_differs, runtime_files};
use crate::constants::{
    BACKUP_DIR, CLI_INSTALL, LOREN_CONFIG_FILE, PROJECT_FILE, PROMISE_DIR, SCRIPT_SYNC_ID,
    SCRIPT_SYNC_LABEL, SCRIPT_SYNC_LAYOUT,
};
use crate::ctx::{Tool, TypesOptions, UpdateAction, UpdateOptions, UpdateResult};
use crate::fsutil::{
    is_dir, is_file, normalize_eol, read_text_if_exists, rel, relative, resolve, strip_bom,
    timestamp, to_posix,
};
use crate::json::Json;
use crate::lint::{project_layout, Layout};
use crate::scriptsync::{
    is_script_sync, read_loren_config, runtime_version, server_runtime_source, LorenConfig,
};
use crate::types::generate_types;

const RUNTIME_DIR: &str = "loren";
const SHIM_NAME: &str = "Loren.luau";
pub(super) const PROMISE_FILES: [&str; 3] = ["lib/init.lua", "LICENSE", PROJECT_FILE];

pub(super) type Files = Vec<(String, Vec<u8>)>;

pub(super) struct Act {
    pub(super) kind: &'static str,
    pub(super) path: String,
    pub(super) detail: String,
    pub(super) target: PathBuf,
    pub(super) files: Files,
}

/// Everything `run_update` needs after planning.
pub(super) struct Plan {
    pub(super) actions: Vec<Act>,
    pub(super) date: SystemTime,
    pub(super) override_tool: Option<Tool>,
    pub(super) to_version: String,
    pub(super) from_label: String,
    pub(super) kind: &'static str,
    pub(super) script_sync: bool,
    pub(super) layout: Layout,
    pub(super) project: Detected,
    pub(super) project_text: String,
    pub(super) project_file: PathBuf,
    pub(super) patched_data: Option<Json>,
    pub(super) config: Option<Json>,
    pub(super) pkg_promise: PathBuf,
    pub(super) promise_lib: PathBuf,
    pub(super) user_promise: PathBuf,
    pub(super) shim_target: PathBuf,
    pub(super) stale_shims: Vec<PathBuf>,
    pub(super) type_opts: TypesOptions<'static>,
    pub(super) types_planned: Vec<PathBuf>,
    pub(super) stamp: String,
    pub(super) backup_root: String,
}

pub(super) fn finish(actions: &[Act], exit_code: i32) -> UpdateResult {
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

fn with_kind(kind: &'static str, mut r: UpdateResult) -> UpdateResult {
    r.kind = Some(kind);
    r
}

/// `Err` carries the finished result of an update that stops while planning.
pub(super) fn plan_update(root: &Path, opts: &UpdateOptions) -> Result<Plan, Box<UpdateResult>> {
    let log = opts.log;
    let mut actions: Vec<Act> = Vec::new();
    let date = opts.now.unwrap_or_else(SystemTime::now);
    let tool_override = opts.tool.as_deref().filter(|s| !s.is_empty());
    let override_tool = tool_override.and_then(Tool::parse);

    if let (Some(value), None) = (tool_override, override_tool) {
        log.error(&format!(
            "Unknown sync tool \"{value}\". Use --tool rojo or --tool argon. Nothing was changed."
        ));
        return Err(Box::new(finish(&actions, 1)));
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
        return Err(Box::new(finish(&actions, 1)));
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
            return Err(Box::new(finish(&actions, 1)));
        }
        layout = project_layout(root, None);
    } else {
        let Some(text) = read_text_if_exists(&project_file) else {
            log.error(&format!(
                "{} is not a Loren project: there is no {PROJECT_FILE}. Run loren update from your project's root folder.",
                root.display()
            ));
            return Err(Box::new(finish(&actions, 1)));
        };
        let data = match Json::parse(strip_bom(&text)) {
            Ok(d) => d,
            Err(err) => {
                log.error(&format!("{PROJECT_FILE} is not valid JSON ({err}). Fix it, then run loren update again."));
                return Err(Box::new(finish(&actions, 1)));
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
        return Err(Box::new(r));
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

    if let Some(installed) = &project.installed_version {
        if to_version != "unknown" && compare_versions(installed, &to_version) > 0 && !opts.force {
            log.error(&format!(
                "This project's runtime ({installed}) is newer than this CLI's ({to_version}). Update the CLI first: {CLI_INSTALL}"
            ));
            return Err(Box::new(with_kind(kind, finish(&actions, 1))));
        }
    }

    plan_runtimes(
        root,
        opts,
        &layout,
        &project,
        &pkg_runtime,
        &to_version,
        script_sync,
        &mut actions,
    );
    let (shim_target, stale_shims) = plan_shim(root, &layout, &project, &mut actions);

    let mut patched_data: Option<Json> = None;
    if let Some(data) = &project_data {
        match patch_project(data, &layout) {
            Patched::Error { error, manual } => {
                log.error(&format!(
                    "{error} Loren cannot patch it safely, so nothing was changed."
                ));
                log.plain(&manual);
                return Err(Box::new(with_kind(kind, finish(&actions, 1))));
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

    let (user_promise, promise_lib) =
        plan_promise(root, &layout, script_sync, &pkg_promise, &mut actions);
    let (type_opts, types_planned) = plan_types(root, &layout, &mut actions);

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

    Ok(Plan {
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
    })
}

#[allow(clippy::too_many_arguments)]
fn plan_runtimes(
    root: &Path,
    opts: &UpdateOptions,
    layout: &Layout,
    project: &Detected,
    pkg_runtime: &Path,
    to_version: &str,
    script_sync: bool,
    actions: &mut Vec<Act>,
) {
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
            runtime_files(pkg_runtime, opts.no_native, None),
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
}

fn plan_shim(
    root: &Path,
    layout: &Layout,
    project: &Detected,
    actions: &mut Vec<Act>,
) -> (PathBuf, Vec<PathBuf>) {
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
    (shim_target, stale_shims)
}

fn plan_promise(
    root: &Path,
    layout: &Layout,
    script_sync: bool,
    pkg_promise: &Path,
    actions: &mut Vec<Act>,
) -> (PathBuf, PathBuf) {
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
        promise_problems(pkg_promise, &user_promise)
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
    (user_promise, promise_lib)
}

fn plan_types(
    root: &Path,
    layout: &Layout,
    actions: &mut Vec<Act>,
) -> (TypesOptions<'static>, Vec<PathBuf>) {
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
    (type_opts, types_plan.planned)
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
