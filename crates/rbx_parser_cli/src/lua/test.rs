//! `rbxlua test`: runs a place's specs (see `rbx_lua::test_runner`) and
//! exits non-zero when any fails.

use std::path::PathBuf;
use std::process::ExitCode;

use rbx_lua::test_runner::{self, Options};

use crate::dom_io;

pub const USAGE: &str = "usage: rbxlua test <place.rbxl|.rbxlx> [--filter <text>] [--root <Path.To.Folder>]... [--format human|json] [--junit <out.xml>]";

#[derive(Debug, Default, PartialEq)]
pub struct TestArgs {
    pub place: PathBuf,
    pub filter: Option<String>,
    pub roots: Vec<String>,
    pub json: bool,
    pub junit: Option<PathBuf>,
}

pub fn parse<I: Iterator<Item = String>>(argv: I) -> Result<TestArgs, String> {
    let mut args = TestArgs::default();
    let mut place = None;
    let mut argv = argv.peekable();
    while let Some(arg) = argv.next() {
        let mut value = |flag: &str| argv.next().ok_or(format!("{flag} requires a value"));
        match arg.as_str() {
            "--filter" => args.filter = Some(value("--filter")?),
            "--root" => args.roots.push(value("--root")?),
            "--junit" => args.junit = Some(PathBuf::from(value("--junit")?)),
            "--format" => match value("--format")?.as_str() {
                "human" => args.json = false,
                "json" => args.json = true,
                other => return Err(format!("unknown format '{other}'")),
            },
            _ if arg.starts_with("--") => return Err(format!("unknown flag '{arg}'")),
            _ if place.is_none() => place = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected argument '{arg}'")),
        }
    }
    args.place = place.ok_or("missing <place> argument")?;
    Ok(args)
}

/// Exit code 0 when every test passed (or there were none), 1 when one
/// failed or the run could not start.
pub fn run(args: &TestArgs) -> ExitCode {
    let report = (|| {
        let dom = dom_io::read_dom(&args.place)?;
        let files = test_runner::discover(&dom, &args.roots)?;
        let database = rbx_parser_cli::default_reflection_database();
        let options = Options {
            filter: args.filter.clone(),
            only: None,
        };
        let report = test_runner::run(&dom, &files, &database, &options);
        if let Some(path) = &args.junit {
            std::fs::write(path, test_runner::junit(&report))
                .map_err(|err| format!("failed to write '{}': {err}", path.display()))?;
        }
        Ok::<_, String>(report)
    })();
    match report {
        Ok(report) => {
            if args.json {
                println!("{}", test_runner::json(&report));
            } else {
                print!("{}", test_runner::human(&report));
            }
            if report.passed() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
    }
}
