//! `loren init`. Left out as Node-only: update-notifier, the Node version floor,
//! npm pack and Windows `.cmd` shims.

mod common;

use std::fs;
use std::io::Cursor;
use std::path::Path;

use common::{
    list_files, read, run_cli, template_dir, tmp_dir, CliOpts, CliOut, Event, FakeRunner,
    MockLanes, TempDir,
};
use rbx_loren::cli::{main, Overrides};
use rbx_loren::constants::{is_spec_file, tool_pin, GLOB_IGNORE_PATHS, SHIM_SOURCE};
use rbx_loren::ctx::Tool;
use rbx_loren::fsutil;
use rbx_loren::log::Log;

fn init(
    dir: &Path,
    args: &[&str],
    lanes: &MockLanes,
    runner: &FakeRunner,
    is_tty: bool,
    input: Option<&str>,
) -> CliOut {
    let mut full = vec!["init"];
    full.extend_from_slice(args);
    run_cli(
        &full,
        CliOpts {
            cwd: Some(dir.to_path_buf()),
            lanes: Some(lanes),
            run: Some(runner),
            is_tty,
            input: input.map(String::from),
            ..CliOpts::default()
        },
    )
}

fn simple(dir: &TempDir, args: &[&str], tool: Tool) -> (CliOut, MockLanes, FakeRunner) {
    let lanes = MockLanes::new(tool);
    let runner = FakeRunner::new();
    let res = init(dir.path(), args, &lanes, &runner, false, None);
    (res, lanes, runner)
}

#[test]
fn init_tool_writes_a_complete_2_0_project() {
    for tool in [Tool::Rojo, Tool::Argon] {
        let dir = tmp_dir();
        let id = tool.id();
        let (r, lanes, runner) = simple(&dir, &["game", "--tool", id, "--yes"], tool);
        assert_eq!(r.code, 0, "{}", r.all);
        let root = dir.path().join("game");
        let files = list_files(&root);

        assert!(files.contains(&"loren/shared/init.luau".to_string()));
        assert!(files.contains(&"loren/server/init.luau".to_string()));
        assert_eq!(read(&root.join("src/shared/Loren.luau")), SHIM_SOURCE);
        assert!(read(&root.join("src/server/Server.server.luau"))
            .contains("Loren.SetOnFire():expect()"));
        assert!(read(&root.join("src/client/Client.client.luau"))
            .contains("Loren.SetOnFire():expect()"));
        for f in [
            "src/server/Services/ExampleService.luau",
            "src/client/Controllers/ExampleController.luau",
        ] {
            assert_eq!(read(&root.join(f)), read(&template_dir().join(f)));
        }

        let packages: Vec<&String> = files
            .iter()
            .filter(|f| f.starts_with("loren_packages/"))
            .collect();
        assert_eq!(
            packages,
            [
                "loren_packages/Promise/LICENSE",
                "loren_packages/Promise/default.project.json",
                "loren_packages/Promise/lib/init.lua"
            ]
        );
        assert_eq!(files.iter().filter(|f| is_spec_file(f)).count(), 0);
        assert!(
            !files.iter().any(|f| f.starts_with("loren_premade/")),
            "premades stay in the CLI"
        );
        assert!(!files.contains(&"aftman.toml".to_string()));

        let pj = read(&root.join("default.project.json"));
        assert!(pj.contains("\"name\": \"game\""));
        for g in GLOB_IGNORE_PATHS {
            assert!(pj.contains(&format!("\"{g}\"")), "{g}");
        }
        for p in [
            "src/shared",
            "loren_packages",
            "loren/shared",
            "loren/server",
            "src/server",
            "src/client",
        ] {
            assert!(pj.contains(&format!("\"$path\": \"{p}\"")), "{p}");
        }

        let rokit = read(&root.join("rokit.toml"));
        assert!(rokit.contains(&format!("{id} = \"{}\"", tool_pin(id).unwrap().spec)));
        assert!(!rokit.contains(if tool == Tool::Rojo {
            "argon"
        } else {
            "rojo-rbx"
        }));
        let gi = read(&root.join(".gitignore"));
        let gi: Vec<&str> = gi.split('\n').collect();
        for line in ["sourcemap.json", ".loren-backup/", "*.rbxl", "*.rbxlx"] {
            assert!(gi.contains(&line), "{line}");
        }
        let vs = read(&root.join(".vscode/settings.json"));
        assert!(vs.contains(&format!(
            "\"luau-lsp.sourcemap.autogenerate\": {}",
            tool == Tool::Rojo
        )));
        assert!(vs.contains("\"luau-lsp.sourcemap.enabled\": true"));

        let events = lanes.events();
        let order: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                Event::GenerateTypes { .. } => Some("generateTypes"),
                Event::Sourcemap { .. } => Some("sourcemap"),
                _ => None,
            })
            .collect();
        assert_eq!(order, ["generateTypes", "sourcemap"]);
        let mapped = events.iter().find_map(|e| match e {
            Event::Sourcemap { tool, .. } => Some(*tool),
            _ => None,
        });
        assert_eq!(mapped, Some(tool));
        let calls = runner.calls();
        let install = calls
            .iter()
            .find(|c| c.cmd == "rokit" && c.args.first().map(String::as_str) == Some("install"));
        let install = install.expect("rokit install ran");
        assert_eq!(
            fsutil::resolve(Path::new("/"), install.opts.cwd.as_ref().unwrap()),
            fsutil::resolve(Path::new("/"), &root)
        );
        assert!(
            install.args.iter().any(|a| a == "--no-trust-check"),
            "--yes skips the trust prompt"
        );
        assert!(r.out.contains("is ready"), "{}", r.out);
    }
}

#[test]
fn init_no_tools_skips_the_install_and_the_sourcemap() {
    let dir = tmp_dir();
    let (r, lanes, runner) = simple(&dir, &["game", "--tool", "rojo", "--no-tools"], Tool::Rojo);
    assert_eq!(r.code, 0, "{}", r.all);
    assert_eq!(runner.calls().len(), 0);
    let events = lanes.events();
    assert!(!events.iter().any(|e| matches!(e, Event::Sourcemap { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::GenerateTypes { .. })));
}

#[test]
fn init_without_a_tty_never_prompts_and_defaults_to_rojo() {
    let dir = tmp_dir();
    let (r, _, _) = simple(&dir, &["game", "--no-tools"], Tool::Rojo);
    assert_eq!(r.code, 0, "{}", r.all);
    assert!(read(&dir.path().join("game/rokit.toml")).contains("rojo-rbx/rojo"));
    assert!(r.out.contains("Using Rojo"), "{}", r.out);
}

#[test]
fn init_in_a_terminal_asks_for_the_tool() {
    let dir = tmp_dir();
    let lanes = MockLanes::new(Tool::Rojo);
    let runner = FakeRunner::new();
    let r = init(
        dir.path(),
        &["game", "--no-tools"],
        &lanes,
        &runner,
        true,
        Some("2\n"),
    );
    assert_eq!(r.code, 0, "{}", r.all);
    assert!(read(&dir.path().join("game/rokit.toml")).contains("argon-rbx/argon"));
}

#[test]
fn init_refuses_an_existing_folder_and_bad_names() {
    let dir = tmp_dir();
    fs::create_dir(dir.path().join("taken")).unwrap();
    fs::write(dir.path().join("taken/keep.txt"), "x").unwrap();
    let (r, _, _) = simple(&dir, &["taken", "--tool", "rojo", "--no-tools"], Tool::Rojo);
    assert_eq!(r.code, 1);
    assert!(r.err.contains("already exists"), "{}", r.err);
    let names: Vec<String> = fs::read_dir(dir.path().join("taken"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["keep.txt"]);

    for bad in ["../escape", "a/b", "a\\b", "con", "..", "bad*name"] {
        let (b, _, _) = simple(&dir, &[bad, "--tool", "rojo", "--no-tools"], Tool::Rojo);
        assert_eq!(b.code, 1, "{bad}");
    }
    let mut top: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    top.sort();
    assert_eq!(top, ["taken"]);
}

#[test]
fn init_into_an_empty_folder_works() {
    let dir = tmp_dir();
    fs::create_dir(dir.path().join("empty")).unwrap();
    let (r, _, _) = simple(
        &dir,
        &["empty", "--tool", "argon", "--no-tools"],
        Tool::Rojo,
    );
    assert_eq!(r.code, 0, "{}", r.all);
    assert!(dir.path().join("empty/loren/shared/init.luau").exists());
}

#[test]
fn init_reports_a_failed_toolchain_or_sourcemap_with_exit_1_and_no_success_line() {
    let dir = tmp_dir();
    let lanes = MockLanes::new(Tool::Rojo);
    let runner = FakeRunner::new()
        .missing("rokit")
        .missing("aftman")
        .missing("foreman");
    let no_manager = init(
        dir.path(),
        &["game", "--tool", "rojo", "--yes"],
        &lanes,
        &runner,
        false,
        None,
    );
    assert_eq!(no_manager.code, 1);
    assert!(
        no_manager.err.contains("No toolchain manager"),
        "{}",
        no_manager.err
    );
    assert!(!no_manager.out.contains("is ready"));
    assert!(
        dir.path().join("game/default.project.json").exists(),
        "the project is still there"
    );

    let dir = tmp_dir();
    let lanes = MockLanes {
        sourcemap_ok: false,
        ..MockLanes::new(Tool::Argon)
    };
    let runner = FakeRunner::new();
    let bad_map = init(
        dir.path(),
        &["game", "--tool", "argon", "--yes"],
        &lanes,
        &runner,
        false,
        None,
    );
    assert_eq!(bad_map.code, 1);
    assert!(
        bad_map.err.contains("sourcemap.json was not regenerated"),
        "{}",
        bad_map.err
    );
    assert!(!bad_map.out.contains("is ready"));
}

#[test]
fn init_rolls_back_when_the_template_is_broken() {
    let dir = tmp_dir();
    let holder = tmp_dir();
    let broken = holder.path().join("project");
    fs::create_dir(&broken).unwrap();
    fs::copy(
        template_dir().join("default.project.json"),
        broken.join("default.project.json"),
    )
    .unwrap();
    let log = Log::memory();
    let lanes = MockLanes::default();
    let runner = FakeRunner::new();
    let args: Vec<String> = ["init", "game", "--tool", "rojo", "--no-tools"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let code = main(
        &args,
        Overrides {
            cwd: Some(dir.path().to_path_buf()),
            log: Some(&log),
            is_tty: Some(false),
            input: Some(Box::new(Cursor::new(Vec::new()))),
            output: Some(Box::new(std::io::sink())),
            run: Some(&runner),
            lanes: Some(&lanes),
            fetch: None,
            template_dir: Some(broken),
        },
    );
    assert_eq!(code, 1);
    assert!(
        log.stderr().contains("Could not create the project"),
        "{}",
        log.stderr()
    );
    assert!(!dir.path().join("game").exists());
}
