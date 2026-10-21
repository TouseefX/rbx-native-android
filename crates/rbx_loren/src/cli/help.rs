//! Help and usage text rendering for the program and for each command,
//! laid out like commander's output.

use super::parse::{opt_flags, Arg, Cmd, COMMANDS};
use crate::constants::{CLI_INSTALL, DOCS_URL};

const PROGRAM_DESC: &str = "Loren, a Roblox framework. Burning like a beating heart.";

fn description(cmd: &Cmd) -> String {
    if cmd.name == "update" {
        format!(
            "update this PROJECT's Loren runtime to the one bundled with this CLI (loren/, the shim, \
             default.project.json, types, sourcemap, lint). To update the CLI itself: {CLI_INSTALL}"
        )
    } else {
        cmd.desc.to_string()
    }
}

fn summary(cmd: &Cmd) -> String {
    if cmd.name == "update" {
        format!("update this PROJECT's Loren runtime (the CLI itself: {CLI_INSTALL})")
    } else {
        cmd.desc.to_string()
    }
}

fn table(rows: &[(String, String)]) -> String {
    let width = rows
        .iter()
        .map(|(t, _)| t.chars().count())
        .max()
        .unwrap_or(0)
        + 2;
    let mut out = String::new();
    for (term, text) in rows {
        let pad = width - term.chars().count();
        out.push_str(&format!("  {term}{}{text}\n", " ".repeat(pad)));
    }
    out
}

fn arg_display(arg: &Arg) -> String {
    if arg.required {
        format!("<{}>", arg.name)
    } else {
        format!("[{}]", arg.name)
    }
}

fn after_text() -> String {
    format!(
        "\nDocs: {DOCS_URL}\nUpdate the CLI itself: {CLI_INSTALL}\n\n{}. Optional in rbx-native: use it, or bring your own.\n",
        crate::CREDIT
    )
}

pub(super) fn program_help() -> String {
    let mut out = format!("Usage: loren [options] [command]\n\n{PROGRAM_DESC}\n\nOptions:\n");
    out.push_str(&table(&[
        ("-v, --version".into(), "print the CLI version".into()),
        ("-h, --help".into(), "display help for command".into()),
    ]));
    out.push_str("\nCommands:\n");
    let mut rows: Vec<(String, String)> = COMMANDS
        .iter()
        .map(|c| {
            let mut term = c.name.to_string();
            if let Some(a) = c.alias {
                term.push_str(&format!("|{a}"));
            }
            term.push_str(" [options]");
            for a in c.args {
                term.push(' ');
                term.push_str(&arg_display(a));
            }
            (term, summary(c))
        })
        .collect();
    rows.push(("help [command]".into(), "display help for command".into()));
    out.push_str(&table(&rows));
    out.push_str(&after_text());
    out
}

pub(super) fn command_help(cmd: &Cmd) -> String {
    let mut usage = format!("loren {}", cmd.name);
    if let Some(a) = cmd.alias {
        usage.push_str(&format!("|{a}"));
    }
    usage.push_str(" [options]");
    for a in cmd.args {
        usage.push(' ');
        usage.push_str(&arg_display(a));
    }
    let mut out = format!("Usage: {usage}\n\n{}\n", description(cmd));
    if !cmd.args.is_empty() {
        out.push_str("\nArguments:\n");
        let rows: Vec<_> = cmd
            .args
            .iter()
            .map(|a| (a.name.to_string(), a.desc.to_string()))
            .collect();
        out.push_str(&table(&rows));
    }
    out.push_str("\nOptions:\n");
    let any_short = cmd.opts.iter().any(|o| o.short.is_some());
    let mut rows: Vec<(String, String)> = cmd
        .opts
        .iter()
        .map(|o| {
            let flags = opt_flags(o);
            let flags = if o.short.is_none() && any_short {
                format!("    {flags}")
            } else {
                flags
            };
            (flags, o.desc.to_string())
        })
        .collect();
    rows.push(("-h, --help".into(), "display help for command".into()));
    out.push_str(&table(&rows));
    out.push_str(&after_text());
    out
}
