//! The command line (`index.js`): `rbxstudio loren <command>`. Parses the
//! same commands, options and aliases as upstream's commander program and
//! returns the exit code; never exits the process itself.

mod help;
mod parse;

use std::io::{BufRead, Write};
use std::path::PathBuf;

use crate::commands::add::{add_command, GithubFetch};
use crate::commands::delegated::{doctor_command, types_command, update_command, UpdateFlags};
use crate::commands::init::{init_command, InitOptions};
use crate::commands::migrate::migrate_command;
use crate::commands::modules::{inject_command, make_command};
use crate::commands::refresh::refresh_command;
use crate::commands::serve::serve_command;
use crate::constants::{CLI_VERSION, PREFIX};
use crate::ctx::{CmdResult, Ctx, Fetch, Lanes, RealLanes, Tool};
use crate::log::{Channel, Log};
use crate::prompt::{detect_tty, Prompter};
use crate::run::{ProcessRunner, Runner};

use help::{command_help, program_help};
use parse::{find_command, parse_command, Cmd, Parsed, Stop};

const USAGE_HINT: &str = "(run loren --help for usage)";

/// Everything [`main`] would otherwise take from the process; tests replace any of it.
#[derive(Default)]
pub struct Overrides<'a> {
    pub cwd: Option<PathBuf>,
    pub log: Option<&'a Log>,
    pub is_tty: Option<bool>,
    pub input: Option<Box<dyn BufRead>>,
    pub output: Option<Box<dyn Write>>,
    pub run: Option<&'a dyn Runner>,
    pub lanes: Option<&'a dyn Lanes>,
    pub fetch: Option<&'a dyn Fetch>,
    pub template_dir: Option<PathBuf>,
}

fn usage_error(log: &Log, message: &str) -> i32 {
    log.raw(Channel::Err, &format!("error: {message}\n{USAGE_HINT}\n"));
    1
}

fn dispatch(cmd: &Cmd, p: &Parsed, ctx: &Ctx) -> CmdResult {
    let tool = p.value("tool");
    match cmd.name {
        "init" => init_command(
            ctx,
            p.arg(0).unwrap_or_default(),
            &InitOptions {
                tool: tool.and_then(Tool::parse),
                yes: p.flag("yes"),
                no_tools: p.flag("no-tools"),
            },
        ),
        "make" => make_command(
            ctx,
            p.arg(0).unwrap_or_default(),
            p.arg(1).unwrap_or_default(),
            p.flag("force"),
            tool,
        ),
        "inject" => inject_command(
            ctx,
            p.arg(0),
            p.arg(1),
            p.flag("list"),
            p.flag("force"),
            tool,
        ),
        "add" => add_command(
            ctx,
            p.arg(0).unwrap_or_default(),
            p.arg(1),
            p.flag("force"),
            tool,
        ),
        "refresh" => refresh_command(ctx, p.flag("watch"), tool),
        "serve" => serve_command(ctx, tool),
        "migrate" => migrate_command(ctx, p.value("to"), p.flag("yes")),
        "update" => update_command(
            ctx,
            &UpdateFlags {
                dry_run: p.flag("dry-run"),
                yes: p.flag("yes"),
                no_native: p.flag("no-native"),
                tool: tool.map(String::from),
            },
        ),
        "types" => types_command(ctx),
        _ => doctor_command(ctx, p.flag("fix"), p.flag("yes")),
    }
}

/// Runs one command; `args` excludes the program name (`["init", "game", "--yes"]`).
pub fn main(args: &[String], overrides: Overrides) -> i32 {
    let Overrides {
        cwd,
        log,
        is_tty,
        input,
        output,
        run,
        lanes,
        fetch,
        template_dir,
    } = overrides;
    let console_log;
    let log: &Log = match log {
        Some(l) => l,
        None => {
            console_log = Log::console();
            &console_log
        }
    };

    let Some(first) = args.first() else {
        log.raw(Channel::Err, &program_help());
        return 1;
    };
    let (cmd, parsed) = match first.as_str() {
        "-v" | "--version" => {
            log.raw(Channel::Out, &format!("{CLI_VERSION}\n"));
            return 0;
        }
        "-h" | "--help" => {
            log.raw(Channel::Out, &program_help());
            return 0;
        }
        "help" => {
            return match args.get(1) {
                None => {
                    log.raw(Channel::Out, &program_help());
                    0
                }
                Some(name) => match find_command(name) {
                    Some(c) => {
                        log.raw(Channel::Out, &command_help(c));
                        0
                    }
                    None => usage_error(log, &format!("unknown command '{name}'")),
                },
            };
        }
        s if s.starts_with('-') && s != "-" => {
            return usage_error(log, &format!("unknown option '{s}'"))
        }
        name => {
            let Some(cmd) = find_command(name) else {
                return usage_error(log, &format!("unknown command '{name}'"));
            };
            match parse_command(cmd, &args[1..]) {
                Ok(p) => (cmd, p),
                Err(Stop::Help) => {
                    log.raw(Channel::Out, &command_help(cmd));
                    return 0;
                }
                Err(Stop::Error(message)) => return usage_error(log, &message),
            }
        }
    };

    let template_dir = match template_dir {
        Some(dir) => dir,
        None => match crate::template::template_dir() {
            Ok(dir) => dir,
            Err(err) => {
                log.error(&err.to_string());
                return 1;
            }
        },
    };
    let is_tty = is_tty.unwrap_or_else(detect_tty);
    let yes = parsed.flag("yes");
    let prompt = if input.is_none() && output.is_none() {
        Prompter::stdio(yes, is_tty)
    } else {
        let input: Box<dyn BufRead> =
            input.unwrap_or_else(|| Box::new(std::io::BufReader::new(std::io::stdin())));
        let output: Box<dyn Write> = output.unwrap_or_else(|| Box::new(std::io::stdout()));
        Prompter::new(yes, is_tty, input, output)
    };

    let process_runner = ProcessRunner;
    let real_lanes = RealLanes;
    let github = GithubFetch;
    let ctx = Ctx {
        cwd: cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_default()),
        log,
        is_tty,
        run: run.unwrap_or(&process_runner),
        lanes: lanes.unwrap_or(&real_lanes),
        fetch: fetch.unwrap_or(&github),
        template_dir,
        prompt,
    };

    match dispatch(cmd, &parsed, &ctx) {
        Ok(code) => code,
        Err(err) => {
            log.error(&err.message);
            if let Some(hint) = err.hint {
                log.raw(Channel::Err, &format!("{PREFIX} {hint}\n"));
            }
            1
        }
    }
}
