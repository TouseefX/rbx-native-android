mod common;

use std::fs;
use std::path::Path;

use common::{project, FakeRunner};
use rbx_loren::ctx::Tool;
use rbx_loren::json::Json;
use rbx_loren::run::{RunOpts, RunResult, Runner, SpawnError};
use rbx_loren::tool::{self, ToolInfo};
use rbx_loren::toolchain::vscode_settings;

fn runner(installed: &[&str]) -> FakeRunner {
    let mut r = FakeRunner::new();
    for id in ["rojo", "argon"] {
        if !installed.contains(&id) {
            r = r.missing(id);
        }
    }
    r
}

fn write(dir: &Path, name: &str, text: &str) {
    fs::write(dir.join(name), text).expect("write");
}

fn info(tool: Tool, source: &str) -> ToolInfo {
    ToolInfo {
        tool: Some(tool),
        source: Some(source.to_string()),
    }
}

#[test]
fn manifest_tools_reads_rokit_aftman_strings_and_foreman_tables_ignoring_comments() {
    let m = tool::manifest_tools;
    assert_eq!(
        m("[tools]\nrojo = \"rojo-rbx/rojo@7.5.1\"\n"),
        vec![Tool::Rojo]
    );
    assert_eq!(
        m("[tools]\nargon = \"argon-rbx/argon@2.0.29\"\n"),
        vec![Tool::Argon]
    );
    assert_eq!(
        m("[tools]\nrojo = { source = \"rojo-rbx/rojo\", version = \"7.5.1\" }\n"),
        vec![Tool::Rojo]
    );
    assert_eq!(
        m("[tools]\nrojo = { github = \"rojo-rbx/rojo\", version = \"7\" }\n"),
        vec![Tool::Rojo]
    );
    assert_eq!(
        m("[tools]\n# argon = \"argon-rbx/argon@2.0.29\"\nrojo = \"rojo-rbx/rojo@7.5.1\" # sync\n"),
        vec![Tool::Rojo]
    );
    assert_eq!(
        m("[tools]\nx = \"rojo-rbx/rojo-plugin@1.0.0\"\n"),
        Vec::<Tool>::new()
    );
    assert_eq!(
        m("[tools]\nrojo = \"Rojo-Rbx/Rojo@7.5.1\"\nargon = \"argon-rbx/argon@2\"\n"),
        vec![Tool::Rojo, Tool::Argon]
    );
}

#[test]
fn detect_tool_prefers_manifests_argon_when_one_lists_both() {
    let dir = project(None);
    let d = dir.path();
    assert_eq!(tool::detect_tool(d, &runner(&[])), None);

    let r = runner(&[]);
    write(
        d,
        "foreman.toml",
        "[tools]\nargon = { source = \"argon-rbx/argon\", version = \"2.0.29\" }\n",
    );
    assert_eq!(
        tool::detect_tool_info(d, &r),
        info(Tool::Argon, "foreman.toml")
    );

    write(
        d,
        "aftman.toml",
        "[tools]\nrojo = \"rojo-rbx/rojo@7.5.1\"\n",
    );
    assert_eq!(
        tool::detect_tool_info(d, &r),
        info(Tool::Rojo, "aftman.toml")
    );

    write(
        d,
        "rokit.toml",
        "[tools]\nrojo = \"rojo-rbx/rojo@7.5.1\"\nargon = \"argon-rbx/argon@2.0.29\"\n",
    );
    assert_eq!(
        tool::detect_tool_info(d, &r),
        info(Tool::Argon, "rokit.toml")
    );
    assert_eq!(r.calls().len(), 0, "a manifest answer never runs a program");
}

#[test]
fn detect_tool_falls_back_to_what_is_installed_rojo_first() {
    let dir = project(None);
    let d = dir.path();
    write(
        d,
        "rokit.toml",
        "[tools]\nstylua = \"johnnymorganz/stylua@2.5.2\"\n",
    );
    assert_eq!(tool::detect_tool(d, &runner(&["argon"])), Some(Tool::Argon));
    assert_eq!(
        tool::detect_tool(d, &runner(&["rojo", "argon"])),
        Some(Tool::Rojo)
    );
    assert_eq!(tool::detect_tool(d, &runner(&[])), None);
    let r = runner(&["rojo"]);
    tool::detect_tool(d, &r);
    let first = &r.calls()[0];
    assert_eq!(first.cmd, "rojo");
    assert_eq!(first.args, vec!["--version".to_string()]);
    assert_eq!(first.opts.cwd.as_deref(), Some(d));
}

#[test]
fn detect_tool_reads_the_fixtures_151_aftman_rojo_2x_rokit_rojo_argon_project() {
    let none = runner(&[]);
    assert_eq!(
        tool::detect_tool(project(Some("v151")).path(), &none),
        Some(Tool::Rojo)
    );
    assert_eq!(
        tool::detect_tool(project(Some("v2")).path(), &none),
        Some(Tool::Rojo)
    );
    assert_eq!(
        tool::detect_tool(project(Some("argon")).path(), &none),
        Some(Tool::Argon)
    );
}

#[test]
fn resolve_tool_tool_wins_any_case_bad_values_and_nothing_found_throw_clear_errors() {
    let dir = project(Some("v2"));
    let d = dir.path();
    let none = runner(&[]);
    assert_eq!(tool::resolve_tool(d, Some("ARGON"), &none), Ok(Tool::Argon));
    assert_eq!(tool::resolve_tool(d, None, &none), Ok(Tool::Rojo));
    assert_eq!(tool::resolve_tool(d, Some(""), &none), Ok(Tool::Rojo));
    let bad = tool::resolve_tool(d, Some("wally"), &none).unwrap_err();
    assert!(bad.contains("Unknown sync tool \"wally\""), "{bad}");
    assert!(bad.contains("--tool rojo or --tool argon"), "{bad}");
    let empty = project(None);
    let nothing = tool::resolve_tool(empty.path(), None, &none).unwrap_err();
    assert!(nothing.contains("No sync tool found"), "{nothing}");
    assert!(nothing.contains("--tool rojo|argon"), "{nothing}");
}

#[test]
fn sourcemap_runs_the_same_command_with_rojo_and_with_argon() {
    let dir = project(Some("v2"));
    let d = dir.path();
    for t in [Tool::Rojo, Tool::Argon] {
        let r = FakeRunner::new();
        assert_eq!(tool::sourcemap(d, t, &r), Ok(()));
        let calls = r.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].cmd, t.id());
        assert_eq!(
            calls[0].args,
            ["sourcemap", "default.project.json", "-o", "sourcemap.json"]
        );
        assert_eq!(calls[0].opts.cwd.as_deref(), Some(d));
    }
}

struct Throwing;

impl Runner for Throwing {
    fn run(&self, _: &str, _: &[String], _: &RunOpts) -> RunResult {
        RunResult {
            error: Some(SpawnError::Other("boom".to_string())),
            ..RunResult::default()
        }
    }
}

#[test]
fn sourcemap_reports_a_missing_tool_a_failing_tool_and_a_bad_id_without_throwing() {
    let dir = project(Some("v2"));
    let d = dir.path();
    let missing = tool::sourcemap(d, Tool::Argon, &runner(&["rojo"])).unwrap_err();
    assert!(
        missing.contains("argon is not installed or not on PATH"),
        "{missing}"
    );
    assert!(missing.contains("rokit add argon-rbx/argon"), "{missing}");

    let failing = FakeRunner::new().handler("rojo", |_, _| {
        Some(RunResult {
            status: Some(3),
            stderr: "\x1b[31mERROR\x1b[0m bad project\n".to_string(),
            ..RunResult::default()
        })
    });
    assert_eq!(
        tool::sourcemap(d, Tool::Rojo, &failing),
        Err("Rojo sourcemap failed (exit 3): ERROR bad project".to_string())
    );

    assert_eq!(
        tool::sourcemap(d, Tool::Rojo, &Throwing),
        Err("Rojo could not start: boom".to_string())
    );
    assert_eq!(Tool::parse("wally"), None);
}

#[test]
fn serve_args_rojo_serve_argon_serve_argon_writes_its_own_sourcemap() {
    assert_eq!(
        tool::serve_args(Tool::Rojo),
        Ok(vec![
            "serve".to_string(),
            "default.project.json".to_string()
        ])
    );
    assert_eq!(
        tool::serve_args(Tool::parse("Argon").unwrap()),
        Ok(vec![
            "serve".to_string(),
            "default.project.json".to_string(),
            "--sourcemap".to_string()
        ])
    );
    assert_eq!(Tool::parse("wally"), None);
}

fn setting<'a>(settings: &'a [(&'static str, Json)], key: &str) -> &'a Json {
    &settings.iter().find(|(k, _)| *k == key).expect("key").1
}

#[test]
fn vscode_settings_luau_lsp_autogenerates_the_sourcemap_only_for_rojo() {
    let rojo = vscode_settings(Tool::Rojo);
    assert!(matches!(
        setting(&rojo, "luau-lsp.sourcemap.autogenerate"),
        Json::Bool(true)
    ));
    let argon = vscode_settings(Tool::Argon);
    assert!(matches!(
        setting(&argon, "luau-lsp.sourcemap.autogenerate"),
        Json::Bool(false)
    ));
    assert_eq!(
        setting(&argon, "luau-lsp.sourcemap.sourcemapFile").as_str(),
        Some("sourcemap.json")
    );
}

// Left out: the Windows `.cmd` shim test of defaultRun (Node's spawn workaround; ProcessRunner covers it natively).

#[cfg(unix)]
#[test]
fn default_run_runs_a_real_program_and_reports_a_missing_one_as_enoent() {
    use rbx_loren::run::ProcessRunner;
    let cwd = std::env::temp_dir();
    let args = ["-c".to_string(), "printf hi".to_string()];
    let ok = ProcessRunner.run(
        "sh",
        &args,
        &RunOpts {
            cwd: Some(cwd.clone()),
            ..RunOpts::default()
        },
    );
    assert_eq!(ok.status, Some(0));
    assert_eq!(ok.stdout, "hi");
    let missing = ProcessRunner.run(
        "loren-no-such-program-xyz",
        &["--version".to_string()],
        &RunOpts::default(),
    );
    assert_eq!(missing.error, Some(SpawnError::NotFound));
    assert!(!rbx_loren::run::has_command(
        &ProcessRunner,
        "loren-no-such-program-xyz",
        &cwd
    ));
}
