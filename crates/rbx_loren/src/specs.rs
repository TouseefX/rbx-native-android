//! Runs Loren-style specs (`return function(t) ... end`, Jest-style
//! `t.describe`/`t.it`/`t.expect`) in the editor's Luau sandbox with Loren's
//! own spec runner (`vendor/loren/test/runner/LorenTest.luau`). The editor's
//! Tests dock and `rbxlua test` call [`plan_mixed`] and [`run_mixed`], which
//! send each spec module [`is_loren_module`] picks out here and the rest to
//! `rbx_lua::test_runner`.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use rbx_dom::{Ref, Variant, WeakDom};
use rbx_lua::test_runner::{
    self, FilePlan, FileReport, Location, NodeKind, Options, PlanNode, Report, SpecFile, Status,
    TestResult,
};
use rbx_lua::{Output, RunState, Runtime};
use rbx_reflection::ReflectionDatabase;
use regex::Regex;

use crate::place::insert_path;

/// Fills in what Loren's runtime and specs use that the sandbox lacks; it
/// heads every spec chunk because the sandbox gives each chunk its own globals.
const PRELUDE: &str = include_str!("specs/prelude.luau");

const SUMMARY_MARK: &str = "@@LORENSPECS ";
const FAILURE_MARK: &str = "@@LORENFAIL ";
const NODE_MARK: &str = "@@LORENNODE ";
const START_MARK: &str = "@@LORENSTART ";
/// Between a path's names in [`NODE_MARK`] and [`START_MARK`] lines.
const NAME_SEPARATOR: char = '\u{1f}';

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

/// [`is_loren_spec`] for a ModuleScript in `dom`.
pub fn is_loren_module(dom: &WeakDom, module: Ref) -> bool {
    dom.get(module).is_some_and(|instance| {
        matches!(
            instance.properties().get("Source"),
            Some(Variant::String(source)) if is_loren_spec(instance.name(), source)
        )
    })
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
        insert_path(dom, parent, name, &crate::template::vendor_dir()?.join(rel))?;
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
    parse_report(drive(dom, database, &source, SUMMARY_MARK)?.text())
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
    let output = drive(dom, database, &chunk, DONE_MARK)?.text();
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
) -> Result<Output, String> {
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
            return Ok(output);
        }
        match step.state {
            RunState::Finished => return Ok(output),
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

/// Runs one spec module through Loren's runner. Its `t` is Loren's own with
/// `describe` and `it` wrapped, so each test reports itself when declared
/// and when it starts: that gives the tree, which tests passed and what
/// each one printed.
const FILE_DRIVER: &str = r#"
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
local function joined(names: { string }): string
	local parts = {}
	for i, name in names do
		parts[i] = esc(name)
	end
	return table.concat(parts, "\31")
end
local function selected(names: { string }): boolean
	if only then
		if #names < #only then
			return false
		end
		for i, name in only do
			if names[i] ~= name then
				return false
			end
		end
	end
	return filter == nil or string.find(table.concat(names, " "), filter, 1, true) ~= nil
end
local function adapt(t: any): any
	local stack = {}
	local adapted = setmetatable({}, { __index = t })
	local function here(name: any): { string }
		local names = table.clone(stack)
		table.insert(names, tostring(name))
		return names
	end
	function adapted.describe(name: any, body: () -> ())
		local names = here(name)
		print(NODE_MARK .. "describe\t" .. tostring(debug.info(2, "l")) .. "\t0\t" .. joined(names))
		t.describe(name, function()
			table.insert(stack, tostring(name))
			local ok, err = pcall(body)
			table.remove(stack)
			if not ok then
				error(err, 0)
			end
		end)
	end
	function adapted.it(name: any, fn: any, opts: any)
		local names = here(name)
		if not selected(names) then
			return
		end
		local skip = if typeof(opts) == "table" and opts.skip then "1" else "0"
		print(NODE_MARK .. "test\t" .. tostring(debug.info(2, "l")) .. "\t" .. skip .. "\t" .. joined(names))
		local run = fn
		if typeof(fn) == "function" then
			run = function(...)
				print(START_MARK .. joined(names))
				return fn(...)
			end
		end
		t.it(name, run, opts)
	end
	return adapted
end
local function noop() end
local loaded, spec = pcall(require, at)
local passed, failed, skipped = 0, 0, 0
if not loaded then
	print(FAILURE_MARK .. "\t" .. esc("failed to load: " .. tostring(spec)))
	failed = 1
elseif typeof(spec) ~= "function" then
	print(FAILURE_MARK .. "\t" .. esc("a spec must return function(t), got " .. typeof(spec)))
	failed = 1
elseif planOnly then
	local declare = {
		describe = function(_, body)
			body()
		end,
		it = noop,
		beforeAll = noop,
		afterAll = noop,
		beforeEach = noop,
		afterEach = noop,
		expect = LorenTest.expect,
		spy = LorenTest.spy,
	}
	local ok, err = pcall(spec, adapt(declare))
	if not ok then
		print(FAILURE_MARK .. "\t" .. esc(tostring(err)))
	end
else
	local results = LorenTest.run({ { name = "spec", fn = function(t)
		return spec(adapt(t))
	end } })
	for _, f in results.failures do
		print(FAILURE_MARK .. esc(f.path) .. "\t" .. esc(f.message))
	end
	passed, failed, skipped = results.passed, results.failed, results.skipped
end
stopFrames()
print(string.format(SUMMARY_MARK .. "%d %d %d", passed, failed, skipped))
"#;

/// The names from the DataModel down to `module`.
fn path_of(dom: &WeakDom, module: Ref) -> Vec<String> {
    let mut names = Vec::new();
    let mut at = Some(module);
    while let Some(r) = at {
        let Some(instance) = dom.get(r) else { break };
        names.push(instance.name().to_string());
        at = dom.parent(r);
    }
    names.reverse();
    names
}

fn lua_list(items: &[String]) -> String {
    let items: Vec<String> = items.iter().map(|i| lua_string(i)).collect();
    format!("{{ {} }}", items.join(", "))
}

/// The driver's output for `file`, with Loren's runner placed if missing.
fn drive_file(
    dom: &WeakDom,
    file: &SpecFile,
    options: &Options,
    plan_only: bool,
) -> Result<Output, String> {
    let mut dom = dom.clone();
    // The prelude reads `workspace` and the services up front.
    for svc in ["Workspace", "Players", "ReplicatedStorage"] {
        service(&mut dom, svc);
    }
    place_at(
        &mut dom,
        &["ReplicatedStorage", "LorenTest", "LorenTest"],
        "test/runner/LorenTest.luau",
    )?;
    let database = ReflectionDatabase::embedded();
    let enums = enum_names(&database);
    let optional = |value: Option<String>| value.unwrap_or_else(|| "nil".into());
    let source = format!(
        "{enums}{PRELUDE}\nsetfenv(1, __lorenEnv)\nlocal path = {}\nlocal only = {}\nlocal filter = {}\nlocal planOnly = {plan_only}\nlocal FAILURE_MARK = {}\nlocal SUMMARY_MARK = {}\nlocal NODE_MARK = {}\nlocal START_MARK = {}\n{FILE_DRIVER}",
        lua_list(&path_of(&dom, file.module)),
        optional(options.only.as_deref().map(lua_list)),
        optional(options.filter.as_deref().map(lua_string)),
        lua_string(FAILURE_MARK),
        lua_string(SUMMARY_MARK),
        lua_string(NODE_MARK),
        lua_string(START_MARK),
    );
    drive(dom, database, &source, SUMMARY_MARK)
}

/// One line of the driver's output.
enum Line {
    Node(PlanNode),
    Start(Vec<String>),
    Failure(SpecFailure),
    Summary,
    Printed,
}

fn names(joined: &str) -> Vec<String> {
    match joined {
        "" => Vec::new(),
        _ => joined.split(NAME_SEPARATOR).map(unescape).collect(),
    }
}

fn line(text: &str) -> Line {
    if let Some(rest) = text.strip_prefix(NODE_MARK) {
        let fields: Vec<&str> = rest.splitn(4, '\t').collect();
        if let [kind, line, skip, path] = fields[..] {
            return Line::Node(PlanNode {
                kind: match kind {
                    "describe" => NodeKind::Describe,
                    _ => NodeKind::Test,
                },
                path: names(path),
                line: line.parse().ok(),
                skip: skip == "1",
                focus: false,
            });
        }
    } else if let Some(rest) = text.strip_prefix(START_MARK) {
        return Line::Start(names(rest));
    } else if let Some(rest) = text.strip_prefix(FAILURE_MARK) {
        let (path, message) = rest.split_once('\t').unwrap_or((rest, ""));
        return Line::Failure(SpecFailure {
            path: unescape(path),
            message: unescape(message),
        });
    } else if text.starts_with(SUMMARY_MARK) {
        return Line::Summary;
    }
    Line::Printed
}

/// LorenTest's `spec > a > b` as the dock's `["a", "b"]`.
fn failure_path(path: &str) -> Vec<String> {
    path.split(" > ").skip(1).map(str::to_string).collect()
}

static RAISED_AT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\S+\.spec):(\d+)").expect("valid regex"));

/// What a Loren spec module declares, without running its tests.
pub fn plan_file(dom: &WeakDom, file: &SpecFile) -> FilePlan {
    let mut plan = FilePlan {
        file: file.clone(),
        nodes: Vec::new(),
        error: None,
        focus: false,
    };
    match drive_file(dom, file, &Options::default(), true) {
        Ok(output) => {
            for text in output.lines() {
                match line(text) {
                    Line::Node(node) => plan.nodes.push(node),
                    Line::Failure(failure) => plan.error = Some(failure.message),
                    _ => {}
                }
            }
        }
        Err(error) => plan.error = Some(error),
    }
    plan
}

/// Runs one Loren spec module's selected tests.
pub fn run_file(dom: &WeakDom, file: &SpecFile, options: &Options) -> FileReport {
    let started = Instant::now();
    let output = match drive_file(dom, file, options, false) {
        Ok(output) => output,
        Err(error) => {
            return FileReport {
                file: file.clone(),
                results: vec![result(file, Vec::new(), Status::Failed, Some(error), None)],
                output: Output::default(),
                duration: started.elapsed(),
            }
        }
    };
    let mut nodes = Vec::new();
    let mut failures = Vec::new();
    let mut printed = Output::default();
    let mut started_tests: Vec<(Vec<String>, Output)> = Vec::new();
    for (i, text) in output.lines().iter().enumerate() {
        match line(text) {
            Line::Node(node) => nodes.push(node),
            Line::Start(path) => started_tests.push((path, Output::default())),
            Line::Failure(failure) => failures.push(failure),
            Line::Summary => {}
            Line::Printed => {
                let one = output.slice(i..i + 1);
                if let Some((_, own)) = started_tests.last_mut() {
                    own.extend(one.clone());
                }
                printed.extend(one);
            }
        }
    }
    let line_of = |path: &[String]| nodes.iter().find(|n| n.path == path).and_then(|n| n.line);
    let mut results = Vec::new();
    for failure in &failures {
        let path = failure_path(&failure.path);
        let mut failed = result(
            file,
            path.clone(),
            Status::Failed,
            Some(failure.message.clone()),
            line_of(&path),
        );
        if let Some(captures) = RAISED_AT.captures(&failure.message) {
            if captures[1] == file.name {
                failed.location = captures[2].parse().ok().map(|line| Location {
                    script: file.module,
                    name: file.name.clone(),
                    line,
                });
            }
        }
        if let Some((_, own)) = started_tests.iter().find(|(p, _)| *p == path) {
            failed.output = own.clone();
        }
        results.push(failed);
    }
    for node in nodes.iter().filter(|n| n.kind == NodeKind::Test) {
        if results.iter().any(|r| r.path == node.path) {
            continue;
        }
        // A declared test that never started was skipped (`{ skip = true }`
        // or no function): LorenTest counts those as skipped too.
        let own = started_tests.iter().find(|(p, _)| *p == node.path);
        let status = match own {
            Some(_) => Status::Passed,
            None => Status::Skipped,
        };
        let mut one = result(file, node.path.clone(), status, None, node.line);
        if let Some((_, own)) = own {
            one.output = own.clone();
        }
        results.push(one);
    }
    // In declaration order, a whole-file failure first.
    results.sort_by_key(|r| match r.path.is_empty() {
        true => 0,
        false => nodes
            .iter()
            .position(|n| n.path == r.path)
            .map_or(usize::MAX, |i| i + 1),
    });
    FileReport {
        file: file.clone(),
        results,
        output: printed,
        duration: started.elapsed(),
    }
}

fn result(
    file: &SpecFile,
    path: Vec<String>,
    status: Status,
    message: Option<String>,
    line: Option<u32>,
) -> TestResult {
    TestResult {
        path,
        status,
        message,
        stack: None,
        location: line.map(|line| Location {
            script: file.module,
            name: file.name.clone(),
            line,
        }),
        duration: Duration::ZERO,
        output: Output::default(),
    }
}

/// What `file` declares: Loren's runner reads a Loren spec, `test_runner` the rest.
pub fn plan_mixed(dom: &WeakDom, file: &SpecFile, database: &ReflectionDatabase) -> FilePlan {
    match is_loren_module(dom, file.module) {
        true => plan_file(dom, file),
        false => test_runner::plan(dom, file, database),
    }
}

/// Runs `files`: Loren specs one by one through Loren's runner, the rest
/// through `test_runner` (honouring `FOCUS` across them). Reports come back
/// in `files`' order.
pub fn run_mixed(
    dom: &WeakDom,
    files: &[SpecFile],
    database: &ReflectionDatabase,
    options: &Options,
) -> Report {
    let (lorens, plain): (Vec<SpecFile>, Vec<SpecFile>) = files
        .iter()
        .cloned()
        .partition(|f| is_loren_module(dom, f.module));
    let mut reports = match (plain.as_slice(), options.only.is_some()) {
        ([], _) => Vec::new(),
        ([file], true) => vec![test_runner::run_file(dom, file, database, options, false)],
        _ => test_runner::run(dom, &plain, database, options).files,
    };
    reports.extend(lorens.iter().map(|f| run_file(dom, f, options)));
    reports.sort_by_key(|r| files.iter().position(|f| f.module == r.file.module));
    Report { files: reports }
}
