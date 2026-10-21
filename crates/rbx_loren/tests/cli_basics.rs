//! Name validation, prompts without a TTY, and exit codes of the in-process CLI.
//!
//! Left out because they only make sense for Node: the update notifier, the
//! Node version floor, and `detectTTY` (the Rust `detect_tty` reads the real
//! stdio and environment, with no injectable streams).

mod common;

use std::fs;
use std::io::Cursor;

use common::{make_project, read, run_cli, tmp_dir, CliOpts};
use rbx_loren::constants::CLI_VERSION;
use rbx_loren::names::{validate_identifier, validate_project_name};
use rbx_loren::prompt::{Choice, Prompter};

fn prompter(yes: bool, is_tty: bool, input: &str) -> Prompter {
    Prompter::new(
        yes,
        is_tty,
        Box::new(Cursor::new(input.as_bytes().to_vec())),
        Box::new(std::io::sink()),
    )
}

#[test]
fn validate_identifier_luau_identifiers_only_no_paths_no_keywords() {
    for good in ["Foo", "FooService", "_private", "a1", "HTTPService"] {
        assert!(validate_identifier(good, "Name").is_ok(), "{good}");
    }
    let long = "x".repeat(101);
    for bad in [
        "", "1abc", "foo-bar", "foo bar", "a/b", "a\\b", "..", "a..b", "end", "local", "nil",
        "Foo.Bar", "CON", "nul", &long,
    ] {
        assert!(validate_identifier(bad, "Name").is_err(), "{bad}");
    }
    assert!(validate_identifier("a/b", "Name")
        .unwrap_err()
        .contains("path separators"));
    assert!(validate_identifier("end", "Name")
        .unwrap_err()
        .contains("keyword"));
}

#[test]
fn validate_project_name_one_folder_windows_safe() {
    for good in ["my-game", "My Game", "game_2", "v1.0"] {
        assert!(validate_project_name(good).is_ok(), "{good}");
    }
    for bad in [
        "", " ", ".", "..", "../x", "a/b", "a\\b", "con", "LPT1", "x.", "x ", "-x", "a:b", "a*b",
    ] {
        assert!(validate_project_name(bad).is_err(), "{bad}");
    }
}

#[test]
fn prompts_never_read_without_a_tty() {
    let p = prompter(false, false, "yes\n");
    assert!(!p.confirm("Go?", false));
    assert!(p.confirm("Go?", true));
    let choices = [
        Choice {
            value: "rojo".to_string(),
            label: "Rojo".to_string(),
        },
        Choice {
            value: "argon".to_string(),
            label: "Argon".to_string(),
        },
    ];
    assert_eq!(p.choose("Tool?", &choices, 0), "rojo");
    let yes = prompter(true, false, "");
    assert!(yes.confirm("Go?", false));
}

#[test]
fn a_terminal_prompt_resolves_on_eof_instead_of_hanging() {
    let p = prompter(false, true, "");
    assert!(!p.confirm("Go?", false));
    let p2 = prompter(false, true, "yes\n");
    assert!(p2.confirm("Rewrite? (y/N)", false));
}

fn cli(args: &[&str], cwd: &std::path::Path) -> common::CliOut {
    run_cli(
        args,
        CliOpts {
            cwd: Some(cwd.to_path_buf()),
            real_lanes: true,
            ..CliOpts::default()
        },
    )
}

#[test]
fn exit_codes_of_the_real_binary() {
    let dir = tmp_dir();
    let d = dir.path();
    let v = cli(&["--version"], d);
    assert_eq!(v.code, 0);
    assert_eq!(v.out.trim(), CLI_VERSION);
    assert_eq!(cli(&["--help"], d).code, 0);
    assert_eq!(cli(&["frobnicate"], d).code, 1);
    assert_eq!(cli(&["init"], d).code, 1, "missing argument");
    assert_eq!(
        cli(&["init", "x", "--tool", "wally"], d).code,
        1,
        "bad --tool"
    );

    let make = cli(&["make", "service", "Foo"], d);
    assert_eq!(make.code, 1);
    assert!(
        make.err.contains("Error: No default.project.json"),
        "{}",
        make.err
    );
    assert_eq!(make.out, "", "no success output on failure");
    assert_eq!(cli(&["init", "../escape", "--no-tools"], d).code, 1);
    assert_eq!(cli(&["refresh"], d).code, 1);
    assert_eq!(cli(&["serve"], d).code, 1);
    assert_eq!(cli(&["migrate", "--yes"], d).code, 1);
    assert_eq!(fs::read_dir(d).unwrap().count(), 0);
}

#[test]
fn the_real_binary_never_waits_for_input_without_a_tty() {
    let dir = tmp_dir();
    let init = cli(&["init", "game", "--no-tools"], dir.path());
    assert_eq!(init.code, 0, "{}", init.err);
    let root = dir.path().join("game");
    assert!(read(&root.join("rokit.toml")).contains("rojo-rbx/rojo"));
    assert!(
        read(&root.join("src").join("shared").join("LorenTypes.luau"))
            .contains("return table.freeze({})")
    );

    let migrate = cli(&["migrate"], &root);
    assert_eq!(migrate.code, 1);
    assert!(migrate.err.contains("--yes"), "{}", migrate.err);
    assert!(read(&root.join("rokit.toml")).contains("rojo-rbx/rojo"));
}

#[test]
fn make_project_helper_builds_a_project() {
    let (_dir, root) = make_project(rbx_loren::ctx::Tool::Rojo, "game");
    assert!(root.join("default.project.json").is_file());
}
