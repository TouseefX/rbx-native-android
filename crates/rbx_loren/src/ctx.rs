//! What a command touches from the outside world (`createContext` in
//! upstream's `index.js`), and the seams tests replace: the [`Lanes`] (sync
//! tool, types, lint, update), the process [`Runner`] and the download.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::lint::Issue;
use crate::log::Log;
use crate::prompt::Prompter;
use crate::run::Runner;

/// A sync tool: a program Loren runs, or Roblox Script Sync (inside Studio).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Rojo,
    Argon,
    /// Roblox Script Sync: no program, no project file (`.loren.json` marks it).
    None,
}

impl Tool {
    pub const PROGRAMS: [Tool; 2] = [Tool::Rojo, Tool::Argon];
    pub const CHOICES: [Tool; 3] = [Tool::Rojo, Tool::Argon, Tool::None];

    pub fn id(self) -> &'static str {
        match self {
            Tool::Rojo => "rojo",
            Tool::Argon => "argon",
            Tool::None => "none",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Rojo => "Rojo",
            Tool::Argon => "Argon",
            Tool::None => "Roblox Script Sync",
        }
    }

    /// `normalizeTool`: trimmed, case-insensitive id, or `None` when unknown.
    pub fn parse(value: &str) -> Option<Tool> {
        Tool::CHOICES
            .into_iter()
            .find(|t| t.id() == value.trim().to_lowercase())
    }

    pub fn is_program(self) -> bool {
        self != Tool::None
    }

    /// The other program (`otherTool`).
    pub fn other(self) -> Tool {
        if self == Tool::Rojo {
            Tool::Argon
        } else {
            Tool::Rojo
        }
    }
}

impl std::fmt::Display for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

/// An expected failure: printed as "Error: <message>" (plus a hint), exit code 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    pub message: String,
    pub hint: Option<String>,
}

impl CliError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: None,
        }
    }

    pub fn hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: Some(hint.into()),
        }
    }
}

impl From<String> for CliError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

/// A command's exit code, or the error that ends it with code 1.
pub type CmdResult = Result<i32, CliError>;

/// Downloads `owner/repo[#ref]` into `dest` (upstream uses degit).
pub trait Fetch {
    fn clone_repo(&self, spec: &str, dest: &Path) -> Result<(), String>;
}

#[derive(Default)]
pub struct TypesOptions<'a> {
    /// Project-relative folders; any left `None` come from the project layout.
    pub shared_dir: Option<String>,
    pub server_dir: Option<String>,
    pub client_dir: Option<String>,
    pub dry_run: bool,
    pub log: Option<&'a Log>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Names {
    pub services: Vec<String>,
    pub controllers: Vec<String>,
    pub networked: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TypesResult {
    pub written: Vec<PathBuf>,
    pub planned: Vec<PathBuf>,
    pub unchanged: Vec<PathBuf>,
    /// Each generated file and its text, shared types first.
    pub files: Vec<(PathBuf, String)>,
    pub names: Names,
}

pub struct DoctorOptions<'a> {
    pub fix: bool,
    pub yes: bool,
    pub is_tty: bool,
    /// Asks before rewriting; `None` answers no.
    pub prompt: Option<&'a Prompter>,
    pub log: &'a Log,
    pub server_dir: Option<String>,
    pub src_dir: Option<String>,
    pub legacy: bool,
    pub now: Option<SystemTime>,
}

#[derive(Debug, Clone, Default)]
pub struct DoctorResult {
    pub issues: Vec<Issue>,
    pub fixed: Vec<Issue>,
    pub remaining: Vec<Issue>,
    pub exit_code: i32,
}

pub struct UpdateOptions<'a> {
    /// The CLI's project template (holds `loren/` and `loren_packages/Promise`).
    pub template_dir: PathBuf,
    pub tool: Option<String>,
    pub dry_run: bool,
    pub yes: bool,
    pub no_native: bool,
    pub is_tty: bool,
    pub prompt: Option<&'a Prompter>,
    pub log: &'a Log,
    pub runner: &'a dyn Runner,
    pub now: Option<SystemTime>,
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateAction {
    /// `runtime`, `shim`, `project`, `config`, `promise` or `types`.
    pub kind: &'static str,
    pub path: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateResult {
    pub exit_code: i32,
    pub actions: Vec<UpdateAction>,
    /// `legacy`, `v2` or `none`, once the project was read.
    pub kind: Option<&'static str>,
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    pub tool: Option<Tool>,
    pub backup_dir: Option<String>,
    pub backed_up: Vec<String>,
    pub doctor: Option<DoctorResult>,
    pub dry_run: bool,
    pub cancelled: bool,
}

/// The parts of the CLI tests swap out (`lib/lanes.js`).
pub trait Lanes {
    fn resolve_tool(
        &self,
        root: &Path,
        tool_override: Option<&str>,
        run: &dyn Runner,
    ) -> Result<Tool, String>;
    fn detect_tool(&self, root: &Path, run: &dyn Runner) -> Option<Tool>;
    fn sourcemap(&self, root: &Path, tool: Tool, run: &dyn Runner) -> Result<(), String>;
    fn serve_args(&self, tool: Tool) -> Result<Vec<String>, String>;
    fn generate_types(&self, root: &Path, opts: &TypesOptions) -> Result<TypesResult, String>;
    fn run_doctor(&self, root: &Path, opts: &DoctorOptions) -> DoctorResult;
    fn run_update(&self, root: &Path, opts: &UpdateOptions) -> UpdateResult;
}

/// The real implementations.
pub struct RealLanes;

impl Lanes for RealLanes {
    fn resolve_tool(
        &self,
        root: &Path,
        tool_override: Option<&str>,
        run: &dyn Runner,
    ) -> Result<Tool, String> {
        crate::tool::resolve_tool(root, tool_override, run)
    }
    fn detect_tool(&self, root: &Path, run: &dyn Runner) -> Option<Tool> {
        crate::tool::detect_tool(root, run)
    }
    fn sourcemap(&self, root: &Path, tool: Tool, run: &dyn Runner) -> Result<(), String> {
        crate::tool::sourcemap(root, tool, run)
    }
    fn serve_args(&self, tool: Tool) -> Result<Vec<String>, String> {
        crate::tool::serve_args(tool)
    }
    fn generate_types(&self, root: &Path, opts: &TypesOptions) -> Result<TypesResult, String> {
        crate::types::generate_types(root, opts)
    }
    fn run_doctor(&self, root: &Path, opts: &DoctorOptions) -> DoctorResult {
        crate::lint::run_doctor(root, opts)
    }
    fn run_update(&self, root: &Path, opts: &UpdateOptions) -> UpdateResult {
        crate::update::run_update(root, opts)
    }
}

pub struct Ctx<'a> {
    pub cwd: PathBuf,
    pub log: &'a Log,
    pub is_tty: bool,
    pub run: &'a dyn Runner,
    pub lanes: &'a dyn Lanes,
    pub fetch: &'a dyn Fetch,
    pub template_dir: PathBuf,
    /// Honours the running command's `--yes`.
    pub prompt: Prompter,
}
