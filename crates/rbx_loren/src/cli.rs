//! The command line (`index.js`): `rbxstudio loren <command>`. Parses the
//! same commands, options and aliases as upstream's commander program and
//! returns the exit code; never exits the process itself.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use crate::commands::add::{add_command, GithubFetch};
use crate::commands::delegated::{doctor_command, types_command, update_command, UpdateFlags};
use crate::commands::init::{init_command, InitOptions};
use crate::commands::migrate::migrate_command;
use crate::commands::modules::{inject_command, make_command};
use crate::commands::refresh::refresh_command;
use crate::commands::serve::serve_command;
use crate::constants::{CLI_INSTALL, CLI_VERSION, DOCS_URL, PREFIX};
use crate::ctx::{CmdResult, Ctx, Fetch, Lanes, RealLanes, Tool};
use crate::log::{Channel, Log};
use crate::prompt::{detect_tty, Prompter};
use crate::run::{ProcessRunner, Runner};

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

struct Opt {
    short: Option<char>,
    long: &'static str,
    arg: Option<&'static str>,
    choices: bool,
    desc: &'static str,
}

struct Arg {
    name: &'static str,
    required: bool,
    desc: &'static str,
}

struct Cmd {
    name: &'static str,
    alias: Option<&'static str>,
    desc: &'static str,
    args: &'static [Arg],
    opts: &'static [Opt],
}

const TOOL_CHOICES: [&str; 3] = ["rojo", "argon", "none"];
const USAGE_HINT: &str = "(run loren --help for usage)";
const PROGRAM_DESC: &str = "Loren, a Roblox framework. Burning like a beating heart.";

const TOOL: Opt = Opt {
    short: None,
    long: "tool",
    arg: Some("tool"),
    choices: true,
    desc: "sync tool (default: from .loren.json / rokit.toml / aftman.toml / foreman.toml)",
};
const FORCE: Opt = Opt {
    short: Some('f'),
    long: "force",
    arg: None,
    choices: false,
    desc: "replace an existing module of that name (backed up first)",
};
const YES: Opt = Opt {
    short: Some('y'),
    long: "yes",
    arg: None,
    choices: false,
    desc: "do not ask for confirmation",
};

static COMMANDS: [Cmd; 10] = [
    Cmd {
        name: "init",
        alias: None,
        desc: "create a new Loren project (Rojo, Argon or Roblox Script Sync)",
        args: &[Arg {
            name: "name",
            required: true,
            desc: "folder to create",
        }],
        opts: &[
            TOOL,
            Opt {
                short: Some('y'),
                long: "yes",
                arg: None,
                choices: false,
                desc: "never prompt; use the defaults (Rojo unless --tool)",
            },
            Opt {
                short: None,
                long: "no-tools",
                arg: None,
                choices: false,
                desc: "skip the toolchain install and the sourcemap",
            },
        ],
    },
    Cmd {
        name: "make",
        alias: None,
        desc: "create a Service or Controller from the template",
        args: &[
            Arg {
                name: "type",
                required: true,
                desc: "service or controller",
            },
            Arg {
                name: "name",
                required: true,
                desc: "module name (a Luau identifier, PascalCase)",
            },
        ],
        opts: &[FORCE, TOOL],
    },
    Cmd {
        name: "inject",
        alias: None,
        desc: "copy a premade module into your project (see --list)",
        args: &[
            Arg {
                name: "type",
                required: false,
                desc: "service, controller or shared",
            },
            Arg {
                name: "name",
                required: false,
                desc: "premade name",
            },
        ],
        opts: &[
            Opt {
                short: Some('l'),
                long: "list",
                arg: None,
                choices: false,
                desc: "list the available premades",
            },
            FORCE,
            TOOL,
        ],
    },
    Cmd {
        name: "add",
        alias: None,
        desc: "download a GitHub repository into loren_packages",
        args: &[
            Arg {
                name: "repo",
                required: true,
                desc: "user/repo[#ref] or a GitHub URL",
            },
            Arg {
                name: "alias",
                required: false,
                desc: "folder name under loren_packages (default: the repo name)",
            },
        ],
        opts: &[
            Opt {
                short: Some('f'),
                long: "force",
                arg: None,
                choices: false,
                desc: "replace an existing package of that name (backed up first)",
            },
            TOOL,
        ],
    },
    Cmd {
        name: "refresh",
        alias: None,
        desc: "regenerate LorenTypes, then the sourcemap",
        args: &[],
        opts: &[
            Opt {
                short: Some('w'),
                long: "watch",
                arg: None,
                choices: false,
                desc: "keep regenerating while Services and Controllers change",
            },
            TOOL,
        ],
    },
    Cmd {
        name: "serve",
        alias: Some("ignite"),
        desc: "start the sync server (rojo serve / argon serve; Script Sync runs inside Studio)",
        args: &[],
        opts: &[TOOL],
    },
    Cmd {
        name: "migrate",
        alias: None,
        desc: "switch this project between Rojo and Argon",
        args: &[],
        opts: &[
            Opt {
                short: None,
                long: "to",
                arg: Some("tool"),
                choices: true,
                desc: "the tool to switch to (default: the other one)",
            },
            YES,
        ],
    },
    Cmd {
        name: "update",
        alias: None,
        desc: "",
        args: &[],
        opts: &[
            Opt {
                short: None,
                long: "dry-run",
                arg: None,
                choices: false,
                desc: "show what would change; change nothing",
            },
            YES,
            Opt {
                short: None,
                long: "no-native",
                arg: None,
                choices: false,
                desc: "install the runtime without @native attributes",
            },
            TOOL,
        ],
    },
    Cmd {
        name: "types",
        alias: None,
        desc: "regenerate LorenTypes (and LorenServerTypes) from your Services and Controllers",
        args: &[],
        opts: &[],
    },
    Cmd {
        name: "doctor",
        alias: None,
        desc: "check the toolchain and the project (including colon-style middleware)",
        args: &[],
        opts: &[
            Opt {
                short: None,
                long: "fix",
                arg: None,
                choices: false,
                desc: "offer to rewrite colon-style middleware to dot style (with a backup)",
            },
            Opt {
                short: Some('y'),
                long: "yes",
                arg: None,
                choices: false,
                desc: "apply fixes without asking",
            },
        ],
    },
];

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

fn opt_flags(opt: &Opt) -> String {
    let mut s = match opt.short {
        Some(c) => format!("-{c}, --{}", opt.long),
        None => format!("--{}", opt.long),
    };
    if let Some(arg) = opt.arg {
        s.push_str(&format!(" <{arg}>"));
    }
    s
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

fn program_help() -> String {
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

fn command_help(cmd: &Cmd) -> String {
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

enum Stop {
    Help,
    Error(String),
}

#[derive(Default)]
struct Parsed {
    args: Vec<String>,
    flags: Vec<&'static str>,
    values: Vec<(&'static str, String)>,
}

impl Parsed {
    fn flag(&self, long: &str) -> bool {
        self.flags.contains(&long)
    }
    fn value(&self, long: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(k, _)| *k == long)
            .map(|(_, v)| v.as_str())
    }
    fn arg(&self, i: usize) -> Option<&str> {
        self.args.get(i).map(String::as_str)
    }
}

fn take_value(opt: &'static Opt, value: String, out: &mut Parsed) -> Result<(), Stop> {
    if opt.choices && !TOOL_CHOICES.contains(&value.as_str()) {
        return Err(Stop::Error(format!(
            "option '{}' argument '{value}' is invalid. Allowed choices are {}.",
            opt_flags(opt),
            TOOL_CHOICES.join(", ")
        )));
    }
    out.values.push((opt.long, value));
    Ok(())
}

fn parse_command(cmd: &'static Cmd, rest: &[String]) -> Result<Parsed, Stop> {
    let mut out = Parsed::default();
    let mut only_args = false;
    let mut i = 0;
    while i < rest.len() {
        let a = &rest[i];
        i += 1;
        if only_args || a == "-" || !a.starts_with('-') {
            out.args.push(a.clone());
            continue;
        }
        if a == "--" {
            only_args = true;
            continue;
        }
        let missing =
            |opt: &Opt| Stop::Error(format!("option '{}' argument missing", opt_flags(opt)));
        if let Some(body) = a.strip_prefix("--") {
            let (name, inline) = match body.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (body, None),
            };
            if name == "help" {
                return Err(Stop::Help);
            }
            let Some(opt) = cmd.opts.iter().find(|o| o.long == name) else {
                return Err(Stop::Error(format!("unknown option '{a}'")));
            };
            if opt.arg.is_some() {
                let value = match inline {
                    Some(v) => v,
                    None if i < rest.len() && !rest[i].starts_with('-') => {
                        i += 1;
                        rest[i - 1].clone()
                    }
                    None => return Err(missing(opt)),
                };
                take_value(opt, value, &mut out)?;
            } else {
                out.flags.push(opt.long);
            }
        } else {
            let chars: Vec<char> = a[1..].chars().collect();
            let mut k = 0;
            while k < chars.len() {
                let c = chars[k];
                k += 1;
                if c == 'h' {
                    return Err(Stop::Help);
                }
                let Some(opt) = cmd.opts.iter().find(|o| o.short == Some(c)) else {
                    return Err(Stop::Error(format!("unknown option '-{c}'")));
                };
                if opt.arg.is_some() {
                    let tail: String = chars[k..].iter().collect();
                    let value = if !tail.is_empty() {
                        tail
                    } else if i < rest.len() && !rest[i].starts_with('-') {
                        i += 1;
                        rest[i - 1].clone()
                    } else {
                        return Err(missing(opt));
                    };
                    take_value(opt, value, &mut out)?;
                    break;
                }
                out.flags.push(opt.long);
            }
        }
    }
    if let Some(arg) = cmd
        .args
        .iter()
        .enumerate()
        .find(|(n, a)| a.required && *n >= out.args.len())
        .map(|(_, a)| a)
    {
        return Err(Stop::Error(format!(
            "missing required argument '{}'",
            arg.name
        )));
    }
    if out.args.len() > cmd.args.len() {
        let n = cmd.args.len();
        return Err(Stop::Error(format!(
            "too many arguments for '{}'. Expected {n} argument{} but got {}.",
            cmd.name,
            if n == 1 { "" } else { "s" },
            out.args.len()
        )));
    }
    Ok(out)
}

fn find_command(name: &str) -> Option<&'static Cmd> {
    COMMANDS
        .iter()
        .find(|c| c.name == name || c.alias == Some(name))
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
