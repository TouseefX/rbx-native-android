//! `loren init <name>` (`commands/init.js`).

use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::refresh::{generate_types, sourcemap};
use crate::constants::{
    is_spec_file, Key, CLI_INSTALL, GITIGNORE_LINES, GLOB_IGNORE_PATHS, PROJECT_FILE, PROMISE_DIR,
    PROMISE_KEEP, SCAFFOLD_ENTRIES, SHIM_SOURCE,
};
use crate::ctx::{CliError, CmdResult, Ctx, Tool};
use crate::fsutil::{
    copy_dir, exists, is_dir, read_text, rel, remove_path, resolve, strip_bom, walk_files,
    write_text,
};
use crate::json::Json;
use crate::names::validate_project_name;
use crate::prompt::Choice;
use crate::scriptsync::{
    readme_text, runtime_version, server_runtime_source, sync_steps, write_loren_config,
};
use crate::toolchain::{apply_vscode_settings, install_toolchain, new_manifest};

pub struct InitOptions {
    pub tool: Option<Tool>,
    pub yes: bool,
    /// `--no-tools`: skip the toolchain install and the sourcemap.
    pub no_tools: bool,
}

/// What `loren init` offers, in this order (the first is the default).
fn choice_label(tool: Tool) -> &'static str {
    match tool {
        Tool::Rojo => "Rojo",
        Tool::Argon => "Argon",
        Tool::None => "None: Roblox Script Sync (built into Studio)",
    }
}

fn scaffold_filter(entry: &str, rel_path: &str, directory: bool) -> bool {
    let parts: Vec<&str> = rel_path.split('/').collect();
    if !directory && is_spec_file(parts[parts.len() - 1]) {
        return false;
    }
    if entry == "loren_packages" && parts[0] == PROMISE_DIR && parts.len() >= 2 {
        return PROMISE_KEEP.contains(&parts[1]);
    }
    true
}

fn io_err(e: std::io::Error) -> String {
    e.to_string()
}

/// The scaffold's project file named after the project, with the spec-file ignore globs.
pub fn project_file_for(template_dir: &Path, name: &str) -> Result<Json, String> {
    let raw = read_text(&template_dir.join(PROJECT_FILE)).map_err(io_err)?;
    let mut data = Json::parse(strip_bom(&raw)).map_err(|e| e.to_string())?;
    let mut globs: Vec<String> = Vec::new();
    if let Some(existing) = data.get("globIgnorePaths").and_then(Json::as_array) {
        globs.extend(existing.iter().filter_map(|g| g.as_str().map(String::from)));
    }
    for g in GLOB_IGNORE_PATHS {
        if !globs.iter().any(|x| x == g) {
            globs.push(g.to_string());
        }
    }
    let tree = data.get("tree").cloned();
    let _ = data.remove("tree");
    data.set("name", Json::String(name.to_string()));
    data.set(
        "globIgnorePaths",
        Json::Array(globs.into_iter().map(Json::String).collect()),
    );
    if let Some(tree) = tree {
        data.set("tree", tree);
    }
    Ok(data)
}

/// Writes every file of a Rojo/Argon project. Used by the editor's Home too.
pub fn write_scaffold(
    template_dir: &Path,
    target: &Path,
    name: &str,
    tool: Tool,
) -> Result<(), String> {
    fs::create_dir_all(target).map_err(io_err)?;
    for entry in SCAFFOLD_ENTRIES {
        let src = template_dir.join(entry);
        if !exists(&src) {
            return Err(format!(
                "the CLI's project template is missing {entry} (reinstall: {CLI_INSTALL})"
            ));
        }
        if entry == PROJECT_FILE {
            continue;
        }
        if is_dir(&src) {
            copy_dir(
                &src,
                &target.join(entry),
                &|r, d| scaffold_filter(entry, r, d),
                None,
            )
            .map_err(io_err)?;
        } else {
            fs::copy(&src, target.join(entry)).map_err(io_err)?;
        }
    }
    let project = project_file_for(template_dir, name)?;
    write_text(
        &target.join(PROJECT_FILE),
        format!("{}\n", project.stringify(4)),
    )
    .map_err(io_err)?;
    write_text(
        &target.join("src").join("shared").join("Loren.luau"),
        SHIM_SOURCE,
    )
    .map_err(io_err)?;
    write_text(
        &target.join(".gitignore"),
        format!("{}\n", GITIGNORE_LINES.join("\n")),
    )
    .map_err(io_err)?;
    write_text(&target.join("rokit.toml"), new_manifest(tool)).map_err(io_err)?;
    apply_vscode_settings(target, tool)?;
    Ok(())
}

fn is_lua_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".lua") || lower.ends_with(".luau")
}

pub fn write_script_sync_scaffold(
    template_dir: &Path,
    target: &Path,
    name: &str,
) -> Result<(), String> {
    let from = |p: &str| -> PathBuf {
        p.split('/')
            .fold(template_dir.to_path_buf(), |acc, s| acc.join(s))
    };
    let to = |p: &str| -> PathBuf {
        p.split('/')
            .fold(target.to_path_buf(), |acc, s| acc.join(s))
    };
    for entry in [
        "loren",
        "src",
        &format!("loren_packages/{PROMISE_DIR}/lib/init.lua"),
    ] {
        if !exists(&from(entry)) {
            return Err(format!(
                "the CLI's project template is missing {entry} (reinstall: {CLI_INSTALL})"
            ));
        }
    }
    let no_spec = |r: &str, d: bool| scaffold_filter("src", r, d);
    fs::create_dir_all(target).map_err(io_err)?;
    let copy =
        |src: &str, dest: PathBuf| copy_dir(&from(src), &dest, &no_spec, None).map_err(io_err);
    copy("src/shared", to(Key::Shared.script_sync_path()))?;
    copy("src/server", to(Key::Server.script_sync_path()))?;
    copy("src/client", to(Key::Client.script_sync_path()))?;
    for file in walk_files(&to(Key::Client.script_sync_path()), &[]) {
        let s = file.to_string_lossy().into_owned();
        let lower = s.to_ascii_lowercase();
        for ext in [".lua", ".luau"] {
            let suffix = format!(".client{ext}");
            if lower.ends_with(&suffix) {
                let base = &s[..s.len() - suffix.len()];
                let tail = &s[s.len() - ext.len()..];
                fs::rename(&file, format!("{base}.local{tail}")).map_err(io_err)?;
                break;
            }
        }
    }
    copy("loren/shared", to(Key::RuntimeShared.script_sync_path()))?;
    let transform = |r: &str, data: Vec<u8>| -> Vec<u8> {
        if is_lua_file(r) {
            server_runtime_source(&String::from_utf8_lossy(&data)).into_bytes()
        } else {
            data
        }
    };
    copy_dir(
        &from("loren/server"),
        &to(Key::RuntimeServer.script_sync_path()),
        &no_spec,
        Some(&transform),
    )
    .map_err(io_err)?;
    // Promise as one ModuleScript: its lib/init.lua, never its specs.
    let promise =
        fs::read(from(&format!("loren_packages/{PROMISE_DIR}/lib/init.lua"))).map_err(io_err)?;
    write_text(
        &to(Key::Packages.script_sync_path()).join(format!("{PROMISE_DIR}.luau")),
        promise,
    )
    .map_err(io_err)?;
    write_text(
        &to(Key::Shared.script_sync_path()).join("Loren.luau"),
        SHIM_SOURCE,
    )
    .map_err(io_err)?;
    write_text(
        &target.join(".gitignore"),
        format!("{}\n", GITIGNORE_LINES.join("\n")),
    )
    .map_err(io_err)?;
    write_text(&target.join("README.md"), readme_text(name)).map_err(io_err)?;
    let version = runtime_version(&from("loren/shared")).unwrap_or_else(|| "unknown".to_string());
    write_loren_config(target, &version, None).map_err(io_err)?;
    apply_vscode_settings(target, Tool::None)?;
    Ok(())
}

fn pick_tool(ctx: &Ctx, opts: &InitOptions) -> Tool {
    if let Some(tool) = opts.tool {
        return tool;
    }
    if !ctx.is_tty || opts.yes {
        ctx.log.info(
            "Using Rojo (pass --tool argon for Argon, or --tool none for Roblox Script Sync).",
        );
        return Tool::Rojo;
    }
    let choices: Vec<Choice<Tool>> = Tool::CHOICES
        .iter()
        .map(|t| Choice {
            value: *t,
            label: choice_label(*t).into(),
        })
        .collect();
    ctx.prompt
        .choose("Which sync tool should this project use?", &choices, 0)
}

fn finish_script_sync(ctx: &Ctx, target: &Path, problems: u32) -> CmdResult {
    let where_ = rel(&ctx.cwd, target);
    if problems > 0 {
        ctx.log.error(&format!(
            "Created \"{where_}\", but {problems} step(s) failed (see above)."
        ));
    } else {
        ctx.log.ok(&format!(
            "Project \"{where_}\" is ready (Roblox Script Sync: nothing to install)."
        ));
    }
    ctx.log.plain("");
    ctx.log.plain(&format!(
        "Next (these steps are also in {where_}/README.md):"
    ));
    for line in sync_steps() {
        ctx.log.plain(&format!("  {line}"));
    }
    Ok(i32::from(problems > 0))
}

pub fn init_command(ctx: &Ctx, name: &str, opts: &InitOptions) -> CmdResult {
    validate_project_name(name).map_err(CliError::new)?;

    let target = resolve(&ctx.cwd, name);
    let existed = exists(&target);
    if existed {
        let empty = is_dir(&target) && fs::read_dir(&target).is_ok_and(|mut d| d.next().is_none());
        if !empty {
            return Err(CliError::hint(
                format!("\"{name}\" already exists in {}.", ctx.cwd.display()),
                "Pick another name, or remove that folder first.",
            ));
        }
    }

    let tool = pick_tool(ctx, opts);
    let script_sync = tool == Tool::None;
    ctx.log
        .info(&format!("Creating \"{name}\" ({})...", tool.label()));

    let created = if script_sync {
        write_script_sync_scaffold(&ctx.template_dir, &target, name)
    } else {
        write_scaffold(&ctx.template_dir, &target, name, tool)
    };
    if let Err(err) = created {
        // Leave nothing half-made behind: remove what this run created.
        if existed {
            if let Ok(entries) = fs::read_dir(&target) {
                for e in entries.flatten() {
                    remove_path(&e.path());
                }
            }
        } else {
            remove_path(&target);
        }
        return Err(CliError::new(format!(
            "Could not create the project: {err}"
        )));
    }

    let mut problems = 0u32;
    if !generate_types(ctx, &target) {
        problems += 1;
    }
    if script_sync {
        return finish_script_sync(ctx, &target, problems);
    }

    if opts.no_tools {
        ctx.log
            .info("Skipped the toolchain install and the sourcemap (--no-tools).");
    } else {
        ctx.log
            .info(&format!("Installing the toolchain ({})...", tool.label()));
        let install = install_toolchain(&target, ctx.run, ctx.is_tty, opts.yes);
        if !install.ok {
            ctx.log.error(install.reason.as_deref().unwrap_or(""));
            problems += 1;
        }
        if !sourcemap(ctx, &target, tool) {
            problems += 1;
        }
    }

    let where_ = rel(&ctx.cwd, &target);
    if problems > 0 {
        ctx.log.error(&format!(
            "Created \"{where_}\", but {problems} step(s) failed (see above)."
        ));
    } else {
        ctx.log.ok(&format!("Project \"{where_}\" is ready."));
    }
    ctx.log.plain("");
    ctx.log.plain("Next:");
    let shown = if where_.contains(' ') {
        format!("\"{where_}\"")
    } else {
        where_
    };
    ctx.log.plain(&format!("  cd {shown}"));
    if opts.no_tools || problems > 0 {
        ctx.log
            .plain("  rokit install     (installs the sync tool)");
    }
    ctx.log.plain(&format!(
        "  loren serve       (then connect the {} plugin in Studio and press Play)",
        tool.label()
    ));
    Ok(i32::from(problems > 0))
}
