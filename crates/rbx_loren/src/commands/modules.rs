//! `loren make` and `loren inject` (`commands/modules.js`).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::commands::refresh::{refresh_project, resolve_project_tool};
use crate::constants::{PREMADE_DIR, PREMADE_KINDS, RETIRED_PREMADES};
use crate::ctx::{CliError, CmdResult, Ctx};
use crate::fsutil::{self, rel};
use crate::names::{is_pascal_case, validate_identifier};
use crate::project::{
    backup_path, find_module_entry, project_paths, render_module, require_project, ProjectPaths,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Premade {
    pub name: String,
    pub file: PathBuf,
    pub entry: String,
    /// `project` or `built-in`.
    pub source: &'static str,
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "service" => "Service",
        "controller" => "Controller",
        _ => "shared module",
    }
}

fn parse_kind(kind_in: &str, allowed: &[&str]) -> Result<String, CliError> {
    let kind = kind_in.to_lowercase();
    if !allowed.contains(&kind.as_str()) {
        let list = match allowed.split_last() {
            Some((last, head)) if !head.is_empty() => format!("{} or {last}", head.join(", ")),
            _ => allowed.join(", "),
        };
        return Err(CliError::new(format!(
            "Unknown type \"{kind_in}\". Use {list}."
        )));
    }
    Ok(kind)
}

fn all_kinds() -> Vec<&'static str> {
    PREMADE_KINDS.iter().map(|(k, _, _)| *k).collect()
}

fn destination_dir<'a>(paths: &'a ProjectPaths, kind: &str) -> &'a Path {
    let target = PREMADE_KINDS
        .iter()
        .find(|(k, _, _)| *k == kind)
        .map_or("shared", |(_, _, t)| *t);
    match target {
        "services" => &paths.services,
        "controllers" => &paths.controllers,
        _ => &paths.shared,
    }
}

fn check_collisions(
    root: &Path,
    paths: &ProjectPaths,
    kind: &str,
    name: &str,
    force: bool,
) -> Result<Option<PathBuf>, CliError> {
    let dir = destination_dir(paths, kind);
    let existing = find_module_entry(dir, name);
    if let (Some(existing), false) = (&existing, force) {
        return Err(CliError::hint(
            format!("{} already exists.", rel(root, &dir.join(existing))),
            "Pick another name, or pass --force to replace it (the old file is backed up to .loren-backup/).",
        ));
    }
    if kind != "shared" {
        let other_kind = if kind == "service" {
            "controller"
        } else {
            "service"
        };
        let other_dir = destination_dir(paths, other_kind);
        if let (Some(clash), false) = (find_module_entry(other_dir, name), force) {
            return Err(CliError::hint(
                format!(
                    "\"{name}\" is already a {} ({}); the two would clash in Dependencies.",
                    kind_label(other_kind),
                    rel(root, &other_dir.join(clash))
                ),
                "Pick another name, or pass --force to create it anyway.",
            ));
        }
    }
    Ok(existing.map(|e| dir.join(e)))
}

/// Moves an entry that --force replaces into .loren-backup/ first.
fn replace_existing(
    ctx: &Ctx,
    root: &Path,
    existing: Option<PathBuf>,
    new_path: &Path,
) -> Result<(), CliError> {
    let Some(existing) = existing else {
        return Ok(());
    };
    let stamp = fsutil::timestamp(SystemTime::now());
    let saved = backup_path(root, &existing, &stamp)
        .map_err(CliError::new)?
        .unwrap_or_else(|| existing.clone());
    let base = std::env::current_dir().unwrap_or_default();
    if fsutil::resolve(&base, &existing) != fsutil::resolve(&base, new_path)
        || fsutil::is_dir(&existing)
    {
        fsutil::remove_path(&existing);
    }
    ctx.log.warn(&format!(
        "Replaced {} (backup: {}).",
        rel(root, &existing),
        rel(root, &saved)
    ));
    Ok(())
}

pub fn make_command(
    ctx: &Ctx,
    kind: &str,
    name: &str,
    force: bool,
    tool: Option<&str>,
) -> CmdResult {
    let kind = parse_kind(kind, &["service", "controller"])?;
    validate_identifier(name, &format!("{} name", kind_label(&kind))).map_err(CliError::new)?;

    let root = require_project(&ctx.cwd)?;
    let tool = resolve_project_tool(ctx, &root, tool)?;
    let paths = project_paths(&root);
    let existing = check_collisions(&root, &paths, &kind, name, force)?;
    if !is_pascal_case(name) {
        ctx.log.warn(&format!(
            "\"{name}\" is not PascalCase; Services and Controllers usually are."
        ));
    }

    let file = destination_dir(&paths, &kind).join(format!("{name}.luau"));
    let text = render_module(&kind, name, &ctx.template_dir).map_err(CliError::new)?;
    replace_existing(ctx, &root, existing, &file)?;
    fsutil::write_text(&file, text).map_err(|err| CliError::new(err.to_string()))?;
    ctx.log.ok(&format!(
        "Created {} {name}: {}",
        kind_label(&kind),
        rel(&root, &file)
    ));

    Ok(if refresh_project(ctx, &root, tool, true, true).ok {
        0
    } else {
        1
    })
}

/// Premades by kind (`service`, `controller`, `shared`), the project's first.
pub fn list_premades(
    template_dir: &Path,
    root: Option<&Path>,
) -> Vec<(&'static str, Vec<Premade>)> {
    let mut out: Vec<(&'static str, Vec<Premade>)> = PREMADE_KINDS
        .iter()
        .map(|(k, _, _)| (*k, Vec::new()))
        .collect();
    let mut sources: Vec<(&'static str, PathBuf, bool)> = Vec::new();
    if let Some(root) = root {
        sources.push(("project", root.join(PREMADE_DIR), false));
    }
    sources.push(("built-in", template_dir.join(PREMADE_DIR), true));

    for (label, source_dir, built_in) in sources {
        for (i, (_, premade_dir, _)) in PREMADE_KINDS.iter().enumerate() {
            let dir = source_dir.join(premade_dir);
            if !fsutil::is_dir(&dir) {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut names: Vec<String> = entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            for entry in names {
                let full = dir.join(&entry);
                let lower = entry.to_lowercase();
                let name = if fsutil::is_file(&full)
                    && (lower.ends_with(".luau") || lower.ends_with(".lua"))
                {
                    let stem = if lower.ends_with(".luau") {
                        entry.len() - 5
                    } else {
                        entry.len() - 4
                    };
                    entry[..stem].to_string()
                } else if fsutil::is_dir(&full) {
                    entry.clone()
                } else {
                    continue;
                };
                if validate_identifier(&name, "Name").is_err() {
                    continue;
                }
                if built_in && RETIRED_PREMADES.contains(&format!("{premade_dir}/{entry}").as_str())
                {
                    continue;
                }
                let list = &mut out[i].1;
                if list
                    .iter()
                    .any(|p| p.name.to_lowercase() == name.to_lowercase())
                {
                    continue;
                }
                list.push(Premade {
                    name,
                    file: full,
                    entry,
                    source: label,
                });
            }
        }
    }
    out
}

fn print_list(ctx: &Ctx, premades: &[(&'static str, Vec<Premade>)], kind_filter: Option<&str>) {
    ctx.log.info("Premades (loren inject <type> <name>):");
    let mut count = 0;
    for (kind, list) in premades {
        if kind_filter.is_some_and(|f| f != *kind) {
            continue;
        }
        for p in list {
            ctx.log
                .plain(&format!("  {kind:<11} {:<24} {}", p.name, p.source));
            count += 1;
        }
    }
    if count == 0 {
        ctx.log.plain("  (none)");
    }
}

pub fn inject_command(
    ctx: &Ctx,
    kind: Option<&str>,
    name: Option<&str>,
    list: bool,
    force: bool,
    tool: Option<&str>,
) -> CmdResult {
    let kind = kind.filter(|k| !k.is_empty());
    let name = name.filter(|n| !n.is_empty());
    if list {
        let filter = match kind {
            Some(k) => Some(parse_kind(k, &all_kinds())?),
            None => None,
        };
        let root = require_project(&ctx.cwd).ok();
        print_list(
            ctx,
            &list_premades(&ctx.template_dir, root.as_deref()),
            filter.as_deref(),
        );
        return Ok(0);
    }
    let (Some(kind), Some(name)) = (kind, name) else {
        return Err(CliError::hint(
            "Usage: loren inject <service|controller|shared> <name>",
            "See what is available: loren inject --list",
        ));
    };
    let kind = parse_kind(kind, &all_kinds())?;
    validate_identifier(name, "Premade name").map_err(CliError::new)?;

    let root = require_project(&ctx.cwd)?;
    let premades = list_premades(&ctx.template_dir, Some(&root))
        .into_iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, v)| v)
        .unwrap_or_default();
    let Some(premade) = premades
        .iter()
        .find(|p| p.name.to_lowercase() == name.to_lowercase())
    else {
        let names = premades
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let names = if names.is_empty() { "none" } else { &names };
        return Err(CliError::hint(
            format!("No {kind} premade named \"{name}\". Available: {names}."),
            "See all premades: loren inject --list",
        ));
    };

    let tool = resolve_project_tool(ctx, &root, tool)?;
    let paths = project_paths(&root);
    let existing = check_collisions(&root, &paths, &kind, &premade.name, force)?;
    let dest = destination_dir(&paths, &kind).join(&premade.entry);
    replace_existing(ctx, &root, existing, &dest)?;
    fsutil::copy_path(&premade.file, &dest).map_err(|err| CliError::new(err.to_string()))?;
    ctx.log.ok(&format!(
        "Injected {kind} {} ({}): {}",
        premade.name,
        premade.source,
        rel(&root, &dest)
    ));

    Ok(if refresh_project(ctx, &root, tool, true, true).ok {
        0
    } else {
        1
    })
}
