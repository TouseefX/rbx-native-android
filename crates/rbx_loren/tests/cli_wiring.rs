mod common;

use std::fs;

use common::{
    list_files, make_project, read, run_cli, tmp_dir, CliOpts, Event, FakeRunner, MockLanes,
};
use rbx_loren::constants::CLI_INSTALL;
use rbx_loren::ctx::{DoctorResult, Tool, UpdateResult};
use rbx_loren::run::{RunResult, Stdio};

// Left out as Node-only: update-notifier, the Node version floor, npm pack,
// Windows .cmd shims, luau-lsp/stylua availability, the "update returns false"
// and "missing module" cases (the typed lanes cannot be absent), and the
// refresh --watch case (a file watcher loop with no in-process seam).

#[test]
fn update_passes_its_flags_to_run_update_and_returns_its_exit_code() {
    let (_keep, root) = make_project(Tool::Rojo, "game");
    let lanes = MockLanes::default();
    let runner = FakeRunner::new();
    let r = run_cli(
        &[
            "update",
            "--dry-run",
            "--yes",
            "--no-native",
            "--tool",
            "argon",
        ],
        CliOpts {
            cwd: Some(root.clone()),
            lanes: Some(&lanes),
            run: Some(&runner),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 0, "{}", r.all);
    let call = lanes
        .events()
        .into_iter()
        .find(|e| matches!(e, Event::RunUpdate { .. }))
        .expect("run_update called");
    assert_eq!(
        call,
        Event::RunUpdate {
            root: root.clone(),
            dry_run: true
        }
    );

    let plain = MockLanes::default();
    run_cli(
        &["update"],
        CliOpts {
            cwd: Some(root.clone()),
            lanes: Some(&plain),
            ..CliOpts::default()
        },
    );
    assert!(plain.events().contains(&Event::RunUpdate {
        root: root.clone(),
        dry_run: false
    }));

    let needs_yes = MockLanes {
        update_result: Some(UpdateResult {
            exit_code: 2,
            ..UpdateResult::default()
        }),
        ..MockLanes::default()
    };
    let r = run_cli(
        &["update"],
        CliOpts {
            cwd: Some(root),
            lanes: Some(&needs_yes),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 2);

    let outside = tmp_dir();
    let r = run_cli(
        &["update"],
        CliOpts {
            cwd: Some(outside.path().to_path_buf()),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 1);
}

#[test]
fn update_help_says_it_updates_the_project_not_the_cli() {
    let dir = tmp_dir();
    let r = run_cli(
        &["update", "--help"],
        CliOpts {
            cwd: Some(dir.path().to_path_buf()),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 0);
    assert!(r.out.contains("PROJECT's Loren runtime"), "{}", r.out);
    assert!(r.out.contains(CLI_INSTALL), "{}", r.out);
}

#[test]
fn types_calls_generate_types_with_the_project_folders_a_throw_exits_1() {
    let (_keep, root) = make_project(Tool::Rojo, "game");
    let lanes = MockLanes::default();
    let r = run_cli(
        &["types"],
        CliOpts {
            cwd: Some(root.clone()),
            lanes: Some(&lanes),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 0, "{}", r.all);
    assert!(lanes.events().contains(&Event::GenerateTypes {
        root: root.clone(),
        dry_run: false
    }));

    let bad = MockLanes {
        types_result: Some(Err("disk full".to_string())),
        ..MockLanes::default()
    };
    let r = run_cli(
        &["types"],
        CliOpts {
            cwd: Some(root),
            lanes: Some(&bad),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 1);
    assert!(r.err.contains("disk full"), "{}", r.err);
}

#[test]
fn doctor_runs_the_checks_and_the_lint_and_reports_failures() {
    let (_keep, root) = make_project(Tool::Rojo, "game");
    let lanes = MockLanes::default();
    let ok = run_cli(
        &["doctor", "--fix", "--yes"],
        CliOpts {
            cwd: Some(root.clone()),
            lanes: Some(&lanes),
            ..CliOpts::default()
        },
    );
    assert_eq!(ok.code, 0, "{}", ok.all);
    assert!(lanes.events().contains(&Event::RunDoctor {
        root: root.clone(),
        fix: true
    }));
    assert!(ok.out.contains("Runtime layout"), "{}", ok.out);

    let failing = MockLanes {
        doctor_result: Some(DoctorResult {
            exit_code: 1,
            ..DoctorResult::default()
        }),
        ..MockLanes::default()
    };
    let lint = run_cli(
        &["doctor"],
        CliOpts {
            cwd: Some(root.clone()),
            lanes: Some(&failing),
            ..CliOpts::default()
        },
    );
    assert_eq!(lint.code, 1);

    let missing = FakeRunner::new().missing("rojo");
    let no_tool = run_cli(
        &["doctor"],
        CliOpts {
            cwd: Some(root.clone()),
            run: Some(&missing),
            ..CliOpts::default()
        },
    );
    assert_eq!(no_tool.code, 1);
    assert!(no_tool.err.contains("rojo does not run"), "{}", no_tool.err);

    fs::write(
        root.join("src").join("shared").join("Loren.luau"),
        "-- 1.5.1 runtime\nlocal Loren = {}\nreturn Loren\n",
    )
    .expect("write legacy runtime");
    let legacy = run_cli(
        &["doctor"],
        CliOpts {
            cwd: Some(root),
            ..CliOpts::default()
        },
    );
    assert_eq!(legacy.code, 1);
    assert!(legacy.err.contains("loren update"), "{}", legacy.err);

    let outside_lanes = MockLanes::default();
    let outside_dir = tmp_dir();
    let outside = run_cli(
        &["doctor"],
        CliOpts {
            cwd: Some(outside_dir.path().to_path_buf()),
            lanes: Some(&outside_lanes),
            ..CliOpts::default()
        },
    );
    assert_eq!(outside.code, 0, "{}", outside.all);
    assert!(!outside_lanes
        .events()
        .iter()
        .any(|e| matches!(e, Event::RunDoctor { .. })));
}

#[test]
fn serve_and_ignite_run_the_tool_in_the_foreground_and_pass_on_its_exit_code() {
    let (_keep, root) = make_project(Tool::Rojo, "game");
    let runner = FakeRunner::new();
    let r = run_cli(
        &["serve"],
        CliOpts {
            cwd: Some(root.clone()),
            run: Some(&runner),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 0, "{}", r.all);
    let calls = runner.calls();
    assert_eq!(
        calls[0].args,
        ["serve", "default.project.json", "--sourcemap"]
    );
    assert_eq!(calls[0].cmd, "argon");
    assert_eq!(calls[0].opts.stdio, Stdio::Inherit);
    assert_eq!(calls[0].opts.cwd.as_deref(), Some(root.as_path()));

    let argon = FakeRunner::new();
    let r = run_cli(
        &["ignite", "--tool", "argon"],
        CliOpts {
            cwd: Some(root.clone()),
            run: Some(&argon),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 0);
    let calls = argon.calls();
    assert_eq!(calls[0].cmd, "argon");
    assert_eq!(
        calls[0].args,
        ["serve", "default.project.json", "--sourcemap"]
    );

    let crashed = FakeRunner::new().handler("argon", |_, _| {
        Some(RunResult {
            status: Some(3),
            ..RunResult::default()
        })
    });
    let r = run_cli(
        &["serve"],
        CliOpts {
            cwd: Some(root.clone()),
            run: Some(&crashed),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 3);

    let missing = FakeRunner::new().missing("argon");
    let r = run_cli(
        &["serve"],
        CliOpts {
            cwd: Some(root.clone()),
            run: Some(&missing),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 1);
    assert!(r.err.contains("not installed"), "{}", r.err);

    let r = run_cli(
        &["serve", "--tool", "wally"],
        CliOpts {
            cwd: Some(root.clone()),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 1);

    let no_tool_lanes = MockLanes {
        tool: Err("No sync tool found.".to_string()),
        ..MockLanes::default()
    };
    let r = run_cli(
        &["serve"],
        CliOpts {
            cwd: Some(root),
            lanes: Some(&no_tool_lanes),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 1);
    assert!(r.err.contains("No sync tool found"), "{}", r.err);

    let outside = tmp_dir();
    let r = run_cli(
        &["serve"],
        CliOpts {
            cwd: Some(outside.path().to_path_buf()),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 1);
}

#[test]
fn refresh_runs_the_types_then_the_sourcemap_either_failure_exits_1() {
    let (_keep, root) = make_project(Tool::Rojo, "game");
    let lanes = MockLanes::new(Tool::Argon);
    let ok = run_cli(
        &["refresh"],
        CliOpts {
            cwd: Some(root.clone()),
            lanes: Some(&lanes),
            ..CliOpts::default()
        },
    );
    assert_eq!(ok.code, 0, "{}", ok.all);
    let steps: Vec<Event> = lanes
        .events()
        .into_iter()
        .filter(|e| matches!(e, Event::GenerateTypes { .. } | Event::Sourcemap { .. }))
        .collect();
    assert_eq!(steps.len(), 2);
    assert!(matches!(steps[0], Event::GenerateTypes { .. }));
    assert_eq!(
        steps[1],
        Event::Sourcemap {
            root: root.clone(),
            tool: Tool::Argon
        }
    );

    let bad_lanes = MockLanes {
        sourcemap_ok: false,
        ..MockLanes::default()
    };
    let bad = run_cli(
        &["refresh"],
        CliOpts {
            cwd: Some(root),
            lanes: Some(&bad_lanes),
            ..CliOpts::default()
        },
    );
    assert_eq!(bad.code, 1);
    assert!(!bad.out.contains("up to date"), "{}", bad.out);
}

#[test]
fn migrate_needs_yes_or_a_terminal_without_either_nothing_changes() {
    let (_keep, root) = make_project(Tool::Rojo, "game");
    let before = read(&root.join("rokit.toml"));
    let r = run_cli(
        &["migrate"],
        CliOpts {
            cwd: Some(root.clone()),
            is_tty: false,
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 1);
    assert!(r.err.contains("needs a confirmation"), "{}", r.err);
    assert_eq!(read(&root.join("rokit.toml")), before);

    let declined = run_cli(
        &["migrate"],
        CliOpts {
            cwd: Some(root.clone()),
            is_tty: true,
            input: Some("n\n".to_string()),
            ..CliOpts::default()
        },
    );
    assert_eq!(declined.code, 1);
    assert_eq!(read(&root.join("rokit.toml")), before);
}

#[test]
fn migrate_rojo_to_argon_rewrites_the_manifest_and_settings_installs_and_rebuilds_the_sourcemap() {
    let (_keep, root) = make_project(Tool::Rojo, "game");
    fs::write(
        root.join("rokit.toml"),
        "[tools]\nrojo = \"rojo-rbx/rojo@7.4.1\"\nstylua = \"JohnnyMorganz/StyLua@2.0.0\"\n",
    )
    .expect("write rokit.toml");
    let lanes = MockLanes::new(Tool::Rojo);
    let runner = FakeRunner::new();
    let r = run_cli(
        &["migrate", "--yes"],
        CliOpts {
            cwd: Some(root.clone()),
            lanes: Some(&lanes),
            run: Some(&runner),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 0, "{}", r.all);
    let toml = read(&root.join("rokit.toml"));
    assert!(
        toml.contains("argon = \"argon-rbx/argon@2.0.29\""),
        "{toml}"
    );
    assert!(!toml.contains("rojo-rbx"), "{toml}");
    assert!(toml.contains("stylua"), "{toml}");
    let settings = read(&root.join(".vscode").join("settings.json"));
    assert!(
        settings.contains("\"luau-lsp.sourcemap.autogenerate\": false"),
        "{settings}"
    );
    let backups = fs::read_dir(root.join(".loren-backup"))
        .expect("backup dir")
        .count();
    assert_eq!(backups, 1);
    assert!(runner
        .calls()
        .iter()
        .any(|c| c.cmd == "rokit" && c.args.join(" ") == "install --no-trust-check"));
    assert!(lanes.events().contains(&Event::Sourcemap {
        root: root.clone(),
        tool: Tool::Argon
    }));
    assert!(r.out.contains("Migrated to Argon"), "{}", r.out);
    assert!(!list_files(&root).is_empty());
}

#[test]
fn migrate_handles_aftman_toml_and_foreman_toml_and_a_terminal_y() {
    let (_keep, root) = make_project(Tool::Rojo, "game");
    fs::remove_file(root.join("rokit.toml")).expect("remove rokit.toml");
    fs::write(
        root.join("aftman.toml"),
        "[tools]\r\nargon = \"argon-rbx/argon@2.0.6\"\r\n",
    )
    .expect("write aftman");
    fs::write(
        root.join("foreman.toml"),
        "[tools]\nargon = { source = \"argon-rbx/argon\", version = \"2.0.6\" }\n",
    )
    .expect("write foreman");
    let lanes = MockLanes::new(Tool::Argon);
    let r = run_cli(
        &["migrate", "--to", "rojo"],
        CliOpts {
            cwd: Some(root.clone()),
            lanes: Some(&lanes),
            is_tty: true,
            input: Some("y\n".to_string()),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 0, "{}", r.all);
    assert_eq!(
        read(&root.join("aftman.toml")),
        "[tools]\r\nrojo = \"rojo-rbx/rojo@7.5.1\"\r\n"
    );
    let foreman = read(&root.join("foreman.toml"));
    assert!(
        foreman.contains("rojo = { source = \"rojo-rbx/rojo\", version = \"=7.5.1\" }"),
        "{foreman}"
    );
    assert!(!foreman.contains("argon"), "{foreman}");
}

#[test]
fn migrate_fails_without_a_success_line_when_the_new_tool_does_not_run() {
    let (_keep, root) = make_project(Tool::Rojo, "game");
    let lanes = MockLanes::new(Tool::Rojo);
    let runner = FakeRunner::new().missing("argon").handler("rokit", |_, _| {
        Some(RunResult {
            status: Some(1),
            ..RunResult::default()
        })
    });
    let r = run_cli(
        &["migrate", "--yes"],
        CliOpts {
            cwd: Some(root),
            lanes: Some(&lanes),
            run: Some(&runner),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 1);
    assert!(r.err.contains("argon does not run"), "{}", r.err);
    assert!(!r.out.contains("Migrated"), "{}", r.out);
    assert!(!lanes
        .events()
        .iter()
        .any(|e| matches!(e, Event::Sourcemap { .. })));

    let (_keep2, other) = make_project(Tool::Rojo, "game");
    let same_lanes = MockLanes::new(Tool::Rojo);
    let same = run_cli(
        &["migrate", "--to", "rojo", "--yes"],
        CliOpts {
            cwd: Some(other),
            lanes: Some(&same_lanes),
            ..CliOpts::default()
        },
    );
    assert_eq!(same.code, 0);
    assert!(same.out.contains("already uses Rojo"), "{}", same.out);
}
