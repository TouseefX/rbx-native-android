//! Runs Loren-style specs (`return function(t) ... end`, Jest-style
//! `t.describe`/`t.it`/`t.expect`) in the editor's Luau sandbox with Loren's
//! own spec runner (`vendor/loren/test/runner/LorenTest.luau`). The editor's
//! Tests dock calls [`run_specs`] for a place whose specs use `Loren.Testing`.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use rbx_dom::{Ref, WeakDom};
use rbx_lua::{Output, RunState, Runtime};
use rbx_reflection::ReflectionDatabase;
use regex::Regex;

use crate::place::insert_path;

/// Fills in what Loren's runtime and specs use that the sandbox lacks; it
/// heads every spec chunk because the sandbox gives each chunk its own globals.
const PRELUDE: &str = include_str!("specs/prelude.luau");

const SUMMARY_MARK: &str = "@@LORENSPECS ";
const FAILURE_MARK: &str = "@@LORENFAIL ";

static SPEC_RETURN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*return\s+function\s*\(\s*t\b").expect("valid regex"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecFailure {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpecReport {
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub failures: Vec<SpecFailure>,
    /// The runner's own report text.
    pub report: String,
    /// Everything the run printed.
    pub output: String,
}

/// True when a ModuleScript looks like a Loren spec (a `*.spec` name whose source returns a function of `t`).
pub fn is_loren_spec(name: &str, source: &str) -> bool {
    name.ends_with(".spec") && SPEC_RETURN.is_match(source)
}

fn vendor_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/loren")
}

fn service(dom: &mut WeakDom, name: &str) -> Ref {
    let found = dom
        .root_refs()
        .iter()
        .copied()
        .find(|r| dom.get(*r).is_some_and(|i| i.class() == name));
    found.unwrap_or_else(|| dom.new_instance(name, name, None))
}

fn child(dom: &WeakDom, parent: Ref, name: &str) -> Option<Ref> {
    let children = dom.get(parent)?.children();
    children
        .iter()
        .copied()
        .find(|c| dom.get(*c).is_some_and(|i| i.name() == name))
}

/// The instance at `path` under `root`, with Folders made where missing.
fn folder(dom: &mut WeakDom, root: Ref, path: &[&str]) -> Ref {
    let mut at = root;
    for part in path {
        at = match child(dom, at, part) {
            Some(existing) => existing,
            None => dom.new_instance("Folder", part, Some(at)),
        };
    }
    at
}

/// Inserts the vendored file or folder `rel` at `at` (service first) unless something is already there.
fn place_at(dom: &mut WeakDom, at: &[&str], rel: &str) -> Result<(), String> {
    let Some((name, parents)) = at.split_last() else {
        return Ok(());
    };
    let Some((svc, middle)) = parents.split_first() else {
        return Ok(());
    };
    let root = service(dom, svc);
    let parent = folder(dom, root, middle);
    if child(dom, parent, name).is_none() {
        insert_path(dom, parent, name, &vendor_dir().join(rel))?;
    }
    Ok(())
}

/// A Luau string literal holding `text`.
fn lua_string(text: &str) -> String {
    let mut out = String::from("\"");
    for byte in text.bytes() {
        match byte {
            b'"' | b'\\' => {
                out.push('\\');
                out.push(byte as char);
            }
            0x20..=0x7e => out.push(byte as char),
            _ => out.push_str(&format!("\\{byte:03}")),
        }
    }
    out.push('"');
    out
}

fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

const DRIVER: &str = r#"
local LorenTest = require(game:GetService("ReplicatedStorage"):WaitForChild("LorenTest"):WaitForChild("LorenTest"))
local at: Instance = game:GetService(path[1])
for i = 2, #path do
	local nextAt = at:FindFirstChild(path[i])
	if not nextAt then
		error("no " .. path[i] .. " under " .. at:GetFullName(), 0)
	end
	at = nextAt
end
local function esc(s: string): string
	local out = string.gsub(s, "\\", "\\\\")
	out = string.gsub(out, "\n", "\\n")
	out = string.gsub(out, "\t", "\\t")
	return out
end
local results = LorenTest.run(LorenTest.collect(at), { filter = filter })
stopFrames()
print(LorenTest.report(results))
for _, f in results.failures do
	print(FAILURE_MARK .. esc(f.path) .. "\t" .. esc(f.message))
end
print(string.format(SUMMARY_MARK .. "%d %d %d", results.passed, results.failed, results.skipped))
"#;

/// Every Enum the engine's properties use, as a Luau list for `Enum:GetEnums()`.
fn enum_names(database: &ReflectionDatabase) -> String {
    let mut names: Vec<&str> = database
        .class_names()
        .filter_map(|class| database.class(class))
        .flat_map(|class| class.properties.iter().map(|p| p.value_type.as_str()))
        .filter(|ty| database.enum_items(ty).is_some())
        .collect();
    names.sort_unstable();
    names.dedup();
    let list: Vec<String> = names.into_iter().map(lua_string).collect();
    format!("local __lorenEnumNames = {{ {} }}\n", list.join(", "))
}

fn driver(container: &[&str], filter: Option<&str>, enums: &str) -> String {
    let path: Vec<String> = container.iter().map(|p| lua_string(p)).collect();
    let filter = filter.map_or_else(|| "nil".to_string(), lua_string);
    format!(
        "{enums}{PRELUDE}\nsetfenv(1, __lorenEnv)\nlocal path = {{ {} }}\nlocal filter = {filter}\nlocal FAILURE_MARK = {}\nlocal SUMMARY_MARK = {}\n{DRIVER}",
        path.join(", "),
        lua_string(FAILURE_MARK),
        lua_string(SUMMARY_MARK),
    )
}

fn parse_report(output: String) -> Result<SpecReport, String> {
    let mut report = SpecReport::default();
    let mut text = Vec::new();
    let mut summary = false;
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix(FAILURE_MARK) {
            let (path, message) = rest.split_once('\t').unwrap_or((rest, ""));
            report.failures.push(SpecFailure {
                path: unescape(path),
                message: unescape(message),
            });
        } else if let Some(rest) = line.strip_prefix(SUMMARY_MARK) {
            let counts: Vec<u32> = rest
                .split(' ')
                .filter_map(|n| n.trim().parse().ok())
                .collect();
            if let [passed, failed, skipped] = counts[..] {
                (report.passed, report.failed, report.skipped) = (passed, failed, skipped);
                summary = true;
            }
        } else {
            text.push(line);
        }
    }
    if !summary {
        return Err(format!("The spec run ended without a report:\n{output}"));
    }
    report.report = text.join("\n");
    report.output = output;
    Ok(report)
}

/// Runs every `*.spec` ModuleScript under the instance at `container`
/// (a path from the DataModel, e.g. `["ServerStorage", "Tests"]`).
pub fn run_specs(
    mut dom: WeakDom,
    container: &[&str],
    filter: Option<&str>,
) -> Result<SpecReport, String> {
    if container.is_empty() {
        return Err("No spec container given".to_string());
    }
    place_at(
        &mut dom,
        &["ReplicatedStorage", "LorenTest", "LorenTest"],
        "test/runner/LorenTest.luau",
    )?;
    let database = ReflectionDatabase::embedded();
    let enums = enum_names(&database);
    let source = driver(container, filter, &enums);
    parse_report(drive(dom, database, &source, SUMMARY_MARK)?)
}

/// Runs `source` with the spec prelude on the same virtual clock as
/// [`run_specs`] and returns everything it printed.
pub fn run_script(dom: WeakDom, source: &str) -> Result<String, String> {
    let database = ReflectionDatabase::embedded();
    let enums = enum_names(&database);
    let chunk = format!(
        "{enums}{PRELUDE}\nsetfenv(1, __lorenEnv)\ndo\n{source}\nend\nprint({})\n",
        lua_string(DONE_MARK)
    );
    let output = drive(dom, database, &chunk, DONE_MARK)?;
    Ok(output
        .lines()
        .filter(|line| !line.starts_with(DONE_MARK))
        .map(|line| format!("{line}\n"))
        .collect())
}

const DONE_MARK: &str = "@@LORENDONE";

/// Runs `chunk` until it prints `mark` or finishes; the printed text either way.
fn drive(
    dom: WeakDom,
    database: ReflectionDatabase,
    chunk: &str,
    mark: &str,
) -> Result<String, String> {
    let mut runtime = Runtime::new(dom, database).map_err(|err| err.to_string())?;
    // Specs wait on timeouts and `task.wait`; a virtual clock that jumps to
    // the next wake-up keeps a run quick without changing what it observes.
    let clock = Rc::new(Cell::new(Duration::ZERO));
    let read = clock.clone();
    runtime.set_clock(move || read.get());
    let started = Instant::now();
    runtime.start(chunk);
    let mut output = Output::default();
    loop {
        let step = runtime.resume();
        let reported = step.output.text().contains(mark);
        output.extend(step.output);
        if reported {
            // Loops the runtime and the specs leave running (heartbeats,
            // retry timers) never end on their own; the mark is all a run owes.
            runtime.cancel();
            return Ok(output.text());
        }
        match step.state {
            RunState::Finished => return Ok(output.text()),
            RunState::Failed(error) => return Err(format!("{}\n{error}", output.text())),
            RunState::Pending { wake_in } => {
                // Roblox resumes waiting threads once per frame at most.
                clock.set(clock.get() + wake_in.max(FRAME));
                if clock.get() > VIRTUAL_LIMIT || started.elapsed() > WALL_LIMIT {
                    runtime.cancel();
                    let text = output.text();
                    return Err(format!("The run did not finish in time:\n{text}"));
                }
            }
        }
    }
}

const VIRTUAL_LIMIT: Duration = Duration::from_secs(900);
// Debug builds (the test suite) run the interpreter several times slower, and
// the full Loren unit suite takes about 90 s there on Linux, more on CI's Windows runners.
const WALL_LIMIT: Duration = if cfg!(debug_assertions) {
    Duration::from_secs(900)
} else {
    Duration::from_secs(180)
};
const FRAME: Duration = Duration::from_micros(16_667);

/// A DataModel laid out like upstream's `test-project` for its unit specs:
/// the vendored runtime, Promise, the shim, the spec runner, the frozen
/// wire reference and `ServerStorage.LorenTest.Specs.Unit`.
pub fn loren_unit_place() -> Result<WeakDom, String> {
    let mut dom = WeakDom::new();
    for svc in [
        "Workspace",
        "Players",
        "ReplicatedStorage",
        "ServerScriptService",
        "ServerStorage",
    ] {
        service(&mut dom, svc);
    }
    let layout: [(&[&str], &str); 8] = [
        (
            &["ReplicatedStorage", "LorenRuntime"],
            "project/loren/shared",
        ),
        (
            &["ServerScriptService", "LorenServer"],
            "project/loren/server",
        ),
        (
            &["ReplicatedStorage", "LorenPackages"],
            "project/loren_packages",
        ),
        (
            &["ReplicatedStorage", "Shared", "Loren"],
            "project/src/shared/Loren.luau",
        ),
        (
            &["ReplicatedStorage", "LorenTest", "LorenTest"],
            "test/runner/LorenTest.luau",
        ),
        (
            &["ServerStorage", "LorenTest", "Frozen"],
            "test/specs/frozen",
        ),
        (
            &["ServerStorage", "LorenTest", "Specs", "Unit"],
            "test/specs/unit",
        ),
        (
            &["ServerStorage", "LorenTest", "Specs", "Runner"],
            "test/specs/runner",
        ),
    ];
    for (at, rel) in layout {
        place_at(&mut dom, at, rel)?;
    }
    Ok(dom)
}
