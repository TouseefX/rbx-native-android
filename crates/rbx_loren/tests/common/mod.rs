#![allow(dead_code)]
//! Shared test helpers: temp projects, a fake process runner, mock lanes and
//! an in-process CLI driver.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rbx_loren::cli::{main, Overrides};
use rbx_loren::ctx::{
    DoctorOptions, DoctorResult, Lanes, Tool, TypesOptions, TypesResult, UpdateOptions,
    UpdateResult,
};
use rbx_loren::log::Log;
use rbx_loren::run::{RunOpts, RunResult, Runner, SpawnError};

pub use tempfile::TempDir;

/// 2026-09-24T12:00:00Z.
pub fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_790_251_200)
}

pub const STAMP: &str = "2026-09-24T12-00-00-000Z";

pub fn template_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/loren/project")
}

pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// The fake CLI package's project folder (`PACKAGE_ROOT/project`).
pub fn package_dir() -> PathBuf {
    fixtures_dir().join("package").join("project")
}

pub fn tmp_dir() -> TempDir {
    tempfile::Builder::new()
        .prefix("loren-cli-test-")
        .tempdir()
        .expect("temp dir")
}

fn copy_tree(src: &Path, dest: &Path) {
    fs::create_dir_all(dest).expect("create dir");
    for entry in fs::read_dir(src).expect("read dir") {
        let entry = entry.expect("entry");
        let to = dest.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            fs::copy(entry.path(), &to).expect("copy file");
        }
    }
}

/// A fresh copy of `fixtures/<name>` (an empty folder when `None`).
pub fn project(name: Option<&str>) -> TempDir {
    let dir = tmp_dir();
    if let Some(name) = name {
        copy_tree(&fixtures_dir().join(name), dir.path());
    }
    dir
}

pub fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Every file under `dir`, as sorted posix relative paths.
pub fn list_files(dir: &Path) -> Vec<String> {
    fn walk(d: &Path, base: &str, out: &mut Vec<String>) {
        for e in fs::read_dir(d).expect("read dir") {
            let e = e.expect("entry");
            let name = e.file_name().to_string_lossy().into_owned();
            let rel = if base.is_empty() {
                name
            } else {
                format!("{base}/{name}")
            };
            if e.path().is_dir() {
                walk(&e.path(), &rel, out);
            } else {
                out.push(rel);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, "", &mut out);
    out.sort();
    out
}

#[derive(Debug, Clone)]
pub struct Call {
    pub cmd: String,
    pub args: Vec<String>,
    pub opts: RunOpts,
}

type Handler = Box<dyn Fn(&[String], &RunOpts) -> Option<RunResult>>;

#[derive(Default)]
pub struct FakeRunner {
    pub calls: RefCell<Vec<Call>>,
    handlers: HashMap<String, Handler>,
    missing: Vec<String>,
}

impl FakeRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn missing(mut self, cmd: &str) -> Self {
        self.missing.push(cmd.to_string());
        self
    }

    pub fn handler(
        mut self,
        cmd: &str,
        f: impl Fn(&[String], &RunOpts) -> Option<RunResult> + 'static,
    ) -> Self {
        self.handlers.insert(cmd.to_string(), Box::new(f));
        self
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.borrow().clone()
    }
}

impl Runner for FakeRunner {
    fn run(&self, cmd: &str, args: &[String], opts: &RunOpts) -> RunResult {
        self.calls.borrow_mut().push(Call {
            cmd: cmd.to_string(),
            args: args.to_vec(),
            opts: opts.clone(),
        });
        if self.missing.iter().any(|m| m == cmd) {
            return RunResult {
                error: Some(SpawnError::NotFound),
                ..RunResult::default()
            };
        }
        if let Some(result) = self.handlers.get(cmd).and_then(|h| h(args, opts)) {
            return result;
        }
        RunResult {
            status: Some(0),
            ..RunResult::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    ResolveTool {
        root: PathBuf,
        tool_override: Option<String>,
    },
    DetectTool {
        root: PathBuf,
    },
    Sourcemap {
        root: PathBuf,
        tool: Tool,
    },
    GenerateTypes {
        root: PathBuf,
        dry_run: bool,
    },
    RunDoctor {
        root: PathBuf,
        fix: bool,
    },
    RunUpdate {
        root: PathBuf,
        dry_run: bool,
    },
}

pub struct MockLanes {
    pub events: RefCell<Vec<Event>>,
    /// `Err` makes resolving and detecting fail.
    pub tool: Result<Tool, String>,
    pub sourcemap_ok: bool,
    pub types_result: Option<Result<TypesResult, String>>,
    pub doctor_result: Option<DoctorResult>,
    pub update_result: Option<UpdateResult>,
}

impl Default for MockLanes {
    fn default() -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            tool: Ok(Tool::Rojo),
            sourcemap_ok: true,
            types_result: None,
            doctor_result: None,
            update_result: None,
        }
    }
}

impl MockLanes {
    pub fn new(tool: Tool) -> Self {
        Self {
            tool: Ok(tool),
            ..Self::default()
        }
    }

    pub fn events(&self) -> Vec<Event> {
        self.events.borrow().clone()
    }
}

impl Lanes for MockLanes {
    fn resolve_tool(
        &self,
        root: &Path,
        tool_override: Option<&str>,
        _run: &dyn Runner,
    ) -> Result<Tool, String> {
        self.events.borrow_mut().push(Event::ResolveTool {
            root: root.to_path_buf(),
            tool_override: tool_override.map(String::from),
        });
        if let Some(o) = tool_override {
            return Tool::parse(o).ok_or_else(|| format!("unknown tool {o}"));
        }
        self.tool.clone()
    }

    fn detect_tool(&self, root: &Path, _run: &dyn Runner) -> Option<Tool> {
        self.events.borrow_mut().push(Event::DetectTool {
            root: root.to_path_buf(),
        });
        self.tool.clone().ok()
    }

    fn sourcemap(&self, root: &Path, tool: Tool, _run: &dyn Runner) -> Result<(), String> {
        self.events.borrow_mut().push(Event::Sourcemap {
            root: root.to_path_buf(),
            tool,
        });
        if self.sourcemap_ok {
            Ok(())
        } else {
            Err(format!("{} is not installed or not on PATH.", tool.id()))
        }
    }

    fn serve_args(&self, tool: Tool) -> Result<Vec<String>, String> {
        let mut args = vec!["serve".to_string(), "default.project.json".to_string()];
        if tool == Tool::Argon {
            args.push("--sourcemap".to_string());
        }
        Ok(args)
    }

    fn generate_types(&self, root: &Path, opts: &TypesOptions) -> Result<TypesResult, String> {
        self.events.borrow_mut().push(Event::GenerateTypes {
            root: root.to_path_buf(),
            dry_run: opts.dry_run,
        });
        match &self.types_result {
            Some(r) => r.clone(),
            None => Ok(TypesResult {
                written: vec![root.join("src").join("shared").join("LorenTypes.luau")],
                ..TypesResult::default()
            }),
        }
    }

    fn run_doctor(&self, root: &Path, opts: &DoctorOptions) -> DoctorResult {
        self.events.borrow_mut().push(Event::RunDoctor {
            root: root.to_path_buf(),
            fix: opts.fix,
        });
        self.doctor_result.clone().unwrap_or_default()
    }

    fn run_update(&self, root: &Path, opts: &UpdateOptions) -> UpdateResult {
        self.events.borrow_mut().push(Event::RunUpdate {
            root: root.to_path_buf(),
            dry_run: opts.dry_run,
        });
        self.update_result.clone().unwrap_or_default()
    }
}

#[derive(Default)]
pub struct CliOpts<'a> {
    pub cwd: Option<PathBuf>,
    pub lanes: Option<&'a MockLanes>,
    /// Use the real lanes instead of mocks.
    pub real_lanes: bool,
    pub run: Option<&'a dyn Runner>,
    pub is_tty: bool,
    /// Text typed at the prompts.
    pub input: Option<String>,
}

pub struct CliOut {
    pub code: i32,
    pub out: String,
    pub err: String,
    pub all: String,
}

pub fn run_cli(args: &[&str], opts: CliOpts) -> CliOut {
    let log = Log::memory();
    let default_lanes = MockLanes::default();
    let default_run = FakeRunner::new();
    let lanes: Option<&dyn Lanes> = if opts.real_lanes {
        None
    } else {
        Some(opts.lanes.unwrap_or(&default_lanes))
    };
    let run: &dyn Runner = opts.run.unwrap_or(&default_run);
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let code = main(
        &args,
        Overrides {
            cwd: opts.cwd,
            log: Some(&log),
            is_tty: Some(opts.is_tty),
            input: Some(Box::new(Cursor::new(
                opts.input.unwrap_or_default().into_bytes(),
            ))),
            output: Some(Box::new(std::io::sink())),
            run: Some(run),
            lanes,
            fetch: None,
            template_dir: Some(template_dir()),
        },
    );
    CliOut {
        code,
        out: log.stdout(),
        err: log.stderr(),
        all: log.output(),
    }
}

/// A fresh project made by `loren init --no-tools` with mocks: (temp dir to keep alive, project root).
pub fn make_project(tool: Tool, name: &str) -> (TempDir, PathBuf) {
    let dir = tmp_dir();
    let lanes = MockLanes::new(tool);
    let res = run_cli(
        &["init", name, "--tool", tool.id(), "--yes", "--no-tools"],
        CliOpts {
            cwd: Some(dir.path().to_path_buf()),
            lanes: Some(&lanes),
            ..CliOpts::default()
        },
    );
    assert_eq!(res.code, 0, "init failed: {}", res.all);
    let root = dir.path().join(name);
    (dir, root)
}
