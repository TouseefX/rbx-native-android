//! Roblox Script Sync projects (`.loren.json` with `"tool": "none"`).
//!
//! Left out: the luau-lsp/stylua availability checks and the IMPLEMENTATION.md
//! shim block (Node tooling and a `design/` folder this repository does not carry).

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::*;
use rbx_loren::cli::{main, Overrides};
use rbx_loren::constants::{Key, SHIM_SOURCE};
use rbx_loren::ctx::{Fetch, Lanes, RealLanes, Tool, UpdateOptions, UpdateResult};
use rbx_loren::log::Log;
use rbx_loren::project::{render_module, require_project};
use rbx_loren::prompt::Prompter;
use rbx_loren::run::Runner;
use rbx_loren::tool;
use regex::Regex;

const OLD_SHIM: &str = "--!strict\n--!optimize 2\nlocal rt = game:GetService(\"ReplicatedStorage\"):WaitForChild(\"LorenRuntime\", 30)\nassert(rt, \"[Loren] ReplicatedStorage.LorenRuntime missing: run loren update\")\nreturn (require :: any)(rt)\n";

static VERSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"Version\s*=\s*"([^"]+)""#).unwrap());

fn p(key: Key) -> &'static str {
    key.script_sync_path()
}

fn pkg_version() -> String {
    let text = read(&template_dir().join("loren/shared/Version.luau"));
    VERSION_RE.captures(&text).expect("version")[1].to_string()
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap()
}

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn snapshot(dir: &Path) -> BTreeMap<String, String> {
    list_files(dir)
        .into_iter()
        .map(|f| (f.clone(), read(&dir.join(&f))))
        .collect()
}

fn is_spec(f: &str) -> bool {
    let l = f.to_lowercase();
    l.ends_with(".spec.lua") || l.ends_with(".spec.luau")
}

struct NoneProject {
    _dir: TempDir,
    root: PathBuf,
    res: CliOut,
    runner: FakeRunner,
}

fn none_project() -> NoneProject {
    let dir = tmp_dir();
    let runner = FakeRunner::new();
    let res = run_cli(
        &["init", "game", "--tool", "none", "--yes"],
        CliOpts {
            cwd: Some(dir.path().to_path_buf()),
            real_lanes: true,
            run: Some(&runner),
            ..CliOpts::default()
        },
    );
    assert_eq!(res.code, 0, "{}", res.all);
    let root = dir.path().join("game");
    NoneProject {
        _dir: dir,
        root,
        res,
        runner,
    }
}

fn cli(root: &Path, args: &[&str], runner: &FakeRunner) -> CliOut {
    run_cli(
        args,
        CliOpts {
            cwd: Some(root.to_path_buf()),
            real_lanes: true,
            run: Some(runner),
            ..CliOpts::default()
        },
    )
}

fn runtime_refs(text: &str) -> Vec<String> {
    let word = re(r"LorenRuntime\b");
    text.lines()
        .filter(|l| !l.trim_start().starts_with("--") && word.is_match(l))
        .map(|l| l.trim().to_string())
        .collect()
}

#[test]
fn init_tool_none_writes_the_script_sync_layout_no_project_file_no_toolchain() {
    let NoneProject {
        root,
        res,
        runner,
        _dir,
    } = none_project();
    let files = list_files(&root);
    let rs = format!("{}/", p(Key::RuntimeShared));
    let rv = format!("{}/", p(Key::RuntimeServer));
    let outside: Vec<&String> = files
        .iter()
        .filter(|f| !f.starts_with(&rs) && !f.starts_with(&rv))
        .collect();
    assert_eq!(
        outside,
        [
            ".gitignore",
            ".loren.json",
            ".vscode/settings.json",
            "README.md",
            "ReplicatedStorage/LorenPackages/Promise.luau",
            "ReplicatedStorage/Shared/Loren.luau",
            "ReplicatedStorage/Shared/LorenTypes.luau",
            "ServerScriptService/Server/LorenServerTypes.luau",
            "ServerScriptService/Server/Server.server.luau",
            "ServerScriptService/Server/Services/ExampleService.luau",
            "StarterPlayer/StarterPlayerScripts/Client/Client.local.luau",
            "StarterPlayer/StarterPlayerScripts/Client/Controllers/ExampleController.luau",
        ]
    );
    assert_eq!(runner.calls().len(), 0, "nothing to install, no sourcemap");
    let client_re = re(r"(?i)\.client\.luau?$");
    assert_eq!(
        files
            .iter()
            .filter(|f| is_spec(f) || client_re.is_match(f))
            .count(),
        0,
        "no spec file, no .client.luau"
    );

    let tpl = template_dir();
    let shipped = |side: &str| -> Vec<String> {
        list_files(&tpl.join("loren").join(side))
            .into_iter()
            .filter(|f| !is_spec(f))
            .collect()
    };
    assert_eq!(
        list_files(&root.join(p(Key::RuntimeShared))),
        shipped("shared")
    );
    assert_eq!(
        list_files(&root.join(p(Key::RuntimeServer))),
        shipped("server")
    );
    for f in shipped("shared") {
        assert_eq!(
            read(&root.join(p(Key::RuntimeShared)).join(&f)),
            read(&tpl.join("loren/shared").join(&f)),
            "{f}"
        );
    }
    let mut rewritten = 0;
    for f in shipped("server") {
        for line in runtime_refs(&read(&root.join(p(Key::RuntimeServer)).join(&f))) {
            assert_eq!(
                line, "local Runtime = ReplicatedStorage.Shared.LorenRuntime",
                "{f}: {line}"
            );
            rewritten += 1;
        }
    }
    assert!(
        rewritten >= 7,
        "every server module finds the runtime in Shared ({rewritten})"
    );

    let shared = root.join(p(Key::Shared));
    let server = root.join(p(Key::Server));
    let client = root.join(p(Key::Client));
    assert_eq!(read(&shared.join("Loren.luau")), SHIM_SOURCE);
    assert_eq!(
        read(&server.join("Server.server.luau")),
        read(&tpl.join("src/server/Server.server.luau"))
    );
    assert_eq!(
        read(&client.join("Client.local.luau")),
        read(&tpl.join("src/client/Client.client.luau"))
    );
    assert!(read(&client.join("Client.local.luau")).contains("script.Parent.Controllers"));
    assert_eq!(
        read(&root.join(p(Key::Packages)).join("Promise.luau")),
        read(&tpl.join("loren_packages/Promise/lib/init.lua"))
    );

    let service_name = re(r#"export type ServiceName = "ExampleService""#);
    assert!(service_name.is_match(&read(&shared.join("LorenTypes.luau"))));
    assert!(service_name.is_match(&read(&server.join("LorenServerTypes.luau"))));

    let config: serde_like::Config = serde_like::parse(&read(&root.join(".loren.json")));
    assert_eq!(
        config,
        serde_like::Config {
            tool: "none".into(),
            layout: "scriptsync".into(),
            runtime: pkg_version()
        }
    );
    let vs = read(&root.join(".vscode/settings.json"));
    assert!(re(r#""luau-lsp.plugin.enabled":\s*true"#).is_match(&vs));
    assert!(re(r#""luau-lsp.sourcemap.enabled":\s*false"#).is_match(&vs));
    let readme = read(&root.join("README.md"));
    for s in [
        "Sync to…",
        "Keep Disk",
        "Luau Language Server Companion",
        "Press Play",
        p(Key::Shared),
        p(Key::Server),
        p(Key::Client),
        p(Key::Packages),
    ] {
        assert!(readme.contains(s), "README mentions {s}");
    }
    for folder in [
        "ReplicatedStorage.Shared",
        "ReplicatedStorage.LorenPackages",
        "ServerScriptService.Server",
        "StarterPlayer.StarterPlayerScripts.Client",
    ] {
        assert!(
            readme.contains(folder) && res.out.contains(folder),
            "{folder}"
        );
    }
    assert!(re(r"is ready \(Roblox Script Sync: nothing to install\)").is_match(&res.out));
    assert!(res.out.contains("choose Keep Disk"));
    assert!(!re(r"loren serve|rokit").is_match(&res.out));
}

/// Minimal reader for the three string fields of `.loren.json`.
mod serde_like {
    use regex::Regex;

    #[derive(Debug, PartialEq, Eq)]
    pub struct Config {
        pub tool: String,
        pub layout: String,
        pub runtime: String,
    }

    pub fn parse(text: &str) -> Config {
        let field = |name: &str| {
            let re = Regex::new(&format!(r#""{name}"\s*:\s*"([^"]*)""#)).unwrap();
            re.captures(text)
                .unwrap_or_else(|| panic!("{name} in {text}"))[1]
                .to_string()
        };
        Config {
            tool: field("tool"),
            layout: field("layout"),
            runtime: field("runtime"),
        }
    }
}

#[test]
fn init_in_a_terminal_offers_script_sync_as_option_3() {
    // The prompt text goes to the injected output writer, which the shared driver discards.
    let dir = tmp_dir();
    let r = run_cli(
        &["init", "game"],
        CliOpts {
            cwd: Some(dir.path().to_path_buf()),
            real_lanes: true,
            is_tty: true,
            input: Some("3\n".to_string()),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 0, "{}", r.all);
    assert_eq!(
        serde_like::parse(&read(&dir.path().join("game/.loren.json"))).tool,
        "none"
    );
    assert!(!dir.path().join("game/default.project.json").exists());
}

#[test]
fn detection_a_loren_json_with_tool_none_wins_over_manifests_and_installed_tools() {
    let dir = tmp_dir();
    let d = dir.path();
    write(d, "rokit.toml", "[tools]\nrojo = \"rojo-rbx/rojo@7.5.1\"\n");
    let runner = FakeRunner::new();
    assert_eq!(tool::detect_tool(d, &runner), Some(Tool::Rojo));

    write(
        d,
        ".loren.json",
        "{ \"tool\": \"none\", \"layout\": \"scriptsync\" }",
    );
    let info = tool::detect_tool_info(d, &runner);
    assert_eq!(info.tool, Some(Tool::None));
    assert_eq!(info.source.as_deref(), Some(".loren.json"));
    assert_eq!(tool::resolve_tool(d, None, &runner), Ok(Tool::None));
    assert_eq!(tool::resolve_tool(d, Some("None"), &runner), Ok(Tool::None));
    let err = tool::resolve_tool(d, Some("rojo"), &runner).unwrap_err();
    assert!(
        re(r"uses Roblox Script Sync.*--tool rojo does not apply").is_match(&err),
        "{err}"
    );
    assert_eq!(runner.calls().len(), 0, "nothing runs");

    write(d, ".loren.json", "{ \"tool\": \"argon\" }");
    assert_eq!(tool::detect_tool(d, &runner), Some(Tool::Rojo));
    write(d, ".loren.json", "{ \"tool\": ");
    assert_eq!(tool::detect_tool(d, &runner), Some(Tool::Rojo));
    let err = require_project(d).unwrap_err();
    assert!(
        err.message.contains(".loren.json is not valid JSON"),
        "{}",
        err.message
    );

    let v2 = project(Some("v2"));
    assert_eq!(
        tool::resolve_tool(v2.path(), Some("none"), &runner),
        Ok(Tool::None)
    );
    assert_eq!(tool::sourcemap(d, Tool::None, &runner), Ok(()));
    assert!(tool::serve_args(Tool::None)
        .unwrap_err()
        .contains("nothing to serve"));
    assert_eq!(runner.calls().len(), 0);
}

#[test]
fn tool_none_on_a_rojo_project_make_runs_no_sourcemap() {
    let (_dir, root) = make_project(Tool::Rojo, "game");
    let lanes = MockLanes::default();
    let r = run_cli(
        &["make", "service", "Shop", "--tool", "none"],
        CliOpts {
            cwd: Some(root.clone()),
            lanes: Some(&lanes),
            ..CliOpts::default()
        },
    );
    assert_eq!(r.code, 0, "{}", r.all);
    assert!(root.join("src/server/Services/Shop.luau").exists());
    let events = lanes.events();
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::GenerateTypes { .. })));
    assert!(!events.iter().any(|e| matches!(e, Event::Sourcemap { .. })));
}

#[derive(Default)]
struct Up<'a> {
    tool: Option<&'a str>,
    dry_run: bool,
    yes: bool,
    no_native: bool,
    now: Option<SystemTime>,
    /// Update from the real CLI template instead of the fake package.
    real: bool,
}

fn update(dir: &Path, up: Up) -> (UpdateResult, Log, FakeRunner) {
    let log = Log::memory();
    let runner = FakeRunner::new();
    let prompt = Prompter::new(
        up.yes,
        false,
        Box::new(Cursor::new(Vec::<u8>::new())),
        Box::new(std::io::sink()),
    );
    let res = RealLanes.run_update(
        dir,
        &UpdateOptions {
            template_dir: if up.real {
                template_dir()
            } else {
                package_dir()
            },
            tool: up.tool.map(String::from),
            dry_run: up.dry_run,
            yes: up.yes,
            no_native: up.no_native,
            is_tty: false,
            prompt: Some(&prompt),
            log: &log,
            runner: &runner as &dyn Runner,
            now: Some(up.now.unwrap_or_else(now)),
            force: false,
        },
    );
    (res, log, runner)
}

fn yes() -> Up<'static> {
    Up {
        yes: true,
        ..Up::default()
    }
}

fn older_none_project() -> TempDir {
    let t = tmp_dir();
    let d = t.path();
    write(
        d,
        ".loren.json",
        "{\n    \"tool\": \"none\",\n    \"layout\": \"scriptsync\",\n    \"runtime\": \"2.0.0-beta.1\"\n}\n",
    );
    write(d, &format!("{}/Loren.luau", p(Key::Shared)), OLD_SHIM);
    write(
        d,
        &format!("{}/init.luau", p(Key::RuntimeShared)),
        "return {}\n",
    );
    write(
        d,
        &format!("{}/Version.luau", p(Key::RuntimeShared)),
        "return table.freeze({ Version = \"2.0.0-beta.1\" })\n",
    );
    write(
        d,
        &format!("{}/Old.luau", p(Key::RuntimeShared)),
        "return nil\n",
    );
    write(
        d,
        &format!("{}/init.luau", p(Key::RuntimeServer)),
        "-- old server index\nreturn {}\n",
    );
    let tpl = template_dir();
    write(
        d,
        &format!("{}/Server.server.luau", p(Key::Server)),
        &read(&tpl.join("src/server/Server.server.luau")),
    );
    write(
        d,
        &format!("{}/Services/PingService.luau", p(Key::Server)),
        "local PingService = { Client = {} }\nfunction PingService.Client:Ping(_player) return \"pong\" end\nreturn PingService\n",
    );
    write(
        d,
        &format!("{}/Client.local.luau", p(Key::Client)),
        &read(&tpl.join("src/client/Client.client.luau")),
    );
    write(
        d,
        &format!("{}/Controllers/PingController.luau", p(Key::Client)),
        "return {}\n",
    );
    write(
        d,
        &format!("{}/Promise.luau", p(Key::Packages)),
        "-- an old Promise\nreturn {}\n",
    );
    t
}

#[test]
fn update_script_sync_an_older_2x_project_gets_the_runtime_shim_promise_and_types_no_sourcemap() {
    let t = older_none_project();
    let dir = t.path();
    let (res, log, runner) = update(dir, yes());
    assert_eq!(res.exit_code, 0, "{}", log.output());
    assert_eq!(res.kind, Some("v2"));
    assert_eq!(res.tool, Some(Tool::None));
    let kinds: Vec<&str> = res.actions.iter().map(|a| a.kind).collect();
    assert_eq!(
        kinds,
        ["runtime", "runtime", "shim", "config", "promise", "types"]
    );
    assert_eq!(runner.calls().len(), 0, "no sourcemap, no program");

    let tpl = package_dir();
    let pkg = |side: &str| snapshot(&tpl.join("loren").join(side));
    assert_eq!(
        snapshot(&dir.join(p(Key::RuntimeShared))),
        pkg("shared"),
        "stale Old.luau removed"
    );
    assert_eq!(snapshot(&dir.join(p(Key::RuntimeServer))), pkg("server"));
    assert!(
        dir.join(format!(
            ".loren-backup/{STAMP}/{}/Old.luau",
            p(Key::RuntimeShared)
        ))
        .exists(),
        "but backed up"
    );
    assert_eq!(
        read(&dir.join(format!("{}/Loren.luau", p(Key::Shared)))),
        SHIM_SOURCE
    );
    assert_eq!(
        read(&dir.join(format!("{}/Promise.luau", p(Key::Packages)))),
        read(&tpl.join("loren_packages/Promise/lib/init.lua"))
    );
    assert!(dir
        .join(format!(
            ".loren-backup/{STAMP}/{}/Promise.luau",
            p(Key::Packages)
        ))
        .exists());
    assert_eq!(
        serde_like::parse(&read(&dir.join(".loren.json"))),
        serde_like::Config {
            tool: "none".into(),
            layout: "scriptsync".into(),
            runtime: "2.0.0".into()
        }
    );
    assert!(
        read(&dir.join(format!("{}/LorenTypes.luau", p(Key::Shared))))
            .contains("export type NetworkedServiceName = \"PingService\"")
    );
    assert!(
        read(&dir.join(format!("{}/LorenServerTypes.luau", p(Key::Server))))
            .contains("export type ServiceName = \"PingService\"")
    );
    assert!(
        !dir.join("default.project.json").exists()
            && !dir.join("loren").exists()
            && !dir.join("sourcemap.json").exists()
    );

    let out = log.output();
    assert!(re(
        r"Loren 2\.0\.0-beta\.1 project \(ReplicatedStorage/Shared/Loren\.luau is the 2\.x shim\)"
    )
    .is_match(&out));
    assert!(
        re(r"replace ReplicatedStorage/Shared/LorenRuntime/ with runtime 2\.0\.0").is_match(&out)
    );
    assert!(out.contains("Studio picks up these file changes through Script Sync. If it shows the conflict dialog, choose Keep Disk."));
    assert!(!re(r"(?i)sourcemap").is_match(&out));
}

#[test]
fn update_script_sync_dry_run_changes_nothing_a_second_run_is_idempotent() {
    let t = older_none_project();
    let dir = t.path();
    let before = snapshot(dir);
    let dry = update(
        dir,
        Up {
            dry_run: true,
            ..Up::default()
        },
    );
    assert_eq!(dry.0.exit_code, 0);
    assert_eq!(dry.0.actions.len(), 6);
    assert_eq!(snapshot(dir), before);
    assert!(dry.1.output().contains("Would do:"));
    assert!(
        re(r"Then: the middleware lint \(preview below\)\. Dry run: nothing was changed\.")
            .is_match(&dry.1.output())
    );

    update(dir, yes());
    let after = snapshot(dir);
    let later = UNIX_EPOCH + Duration::from_secs(1_893_456_000);
    let again = update(
        dir,
        Up {
            yes: true,
            now: Some(later),
            ..Up::default()
        },
    );
    assert_eq!(again.0.exit_code, 0);
    assert!(again.0.actions.is_empty());
    assert_eq!(snapshot(dir), after, "nothing changed, no new backup");
    assert!(again
        .1
        .output()
        .contains("Already up to date: runtime 2.0.0, shim, Promise and types match."));
    assert!(!again.1.output().contains("Studio picks up"));
}

#[test]
fn update_script_sync_no_tty_without_yes_exits_2_another_tool_is_refused_nothing_changes() {
    let t = older_none_project();
    let dir = t.path();
    let before = snapshot(dir);
    let no_tty = update(dir, Up::default());
    assert_eq!(no_tty.0.exit_code, 2);
    assert!(no_tty.1.stderr().contains("needs confirmation"));
    let rojo = update(
        dir,
        Up {
            tool: Some("rojo"),
            yes: true,
            ..Up::default()
        },
    );
    assert_eq!(rojo.0.exit_code, 1);
    assert!(re(r"uses Roblox Script Sync \(\.loren\.json\), so --tool rojo does not apply\. Nothing was changed\.")
        .is_match(&rojo.1.stderr()));
    assert_eq!(snapshot(dir), before);
    assert_eq!(
        update(
            dir,
            Up {
                tool: Some("none"),
                yes: true,
                ..Up::default()
            }
        )
        .0
        .exit_code,
        0
    );
}

#[test]
fn update_script_sync_a_fresh_init_is_up_to_date_no_native_strips_native_and_keeps_the_server_path()
{
    let np = none_project();
    let root = &np.root;
    let fresh = update(
        root,
        Up {
            real: true,
            ..yes()
        },
    );
    assert_eq!(fresh.0.exit_code, 0, "{}", fresh.1.output());
    assert!(
        fresh.0.actions.is_empty(),
        "init and update install the same files"
    );
    assert_eq!(fresh.2.calls().len(), 0);

    let nn = || Up {
        yes: true,
        no_native: true,
        real: true,
        ..Up::default()
    };
    let native = update(root, nn());
    assert_eq!(native.0.exit_code, 0);
    let attr = re(r"(?m)^\s*@native");
    for dir in [p(Key::RuntimeShared), p(Key::RuntimeServer)] {
        for (file, text) in snapshot(&root.join(dir)) {
            assert!(
                !attr.is_match(&text) && !text.contains("--!native"),
                "{dir}/{file}"
            );
        }
    }
    assert!(
        re(r"(?m)^local Runtime = ReplicatedStorage\.Shared\.LorenRuntime$").is_match(&read(
            &root.join(p(Key::RuntimeServer)).join("Dispatch.luau")
        ))
    );
    assert!(
        update(root, nn()).0.actions.is_empty(),
        "idempotent with --no-native"
    );
}

#[test]
fn types_make_inject_and_refresh_use_the_script_sync_folders() {
    let np = none_project();
    let root = &np.root;
    let runner = FakeRunner::new();
    let go = |args: &[&str]| cli(root, args, &runner);

    assert_eq!(go(&["make", "service", "Shop"]).code, 0);
    assert_eq!(
        read(&root.join(p(Key::Server)).join("Services/Shop.luau")),
        render_module("service", "Shop", &template_dir()).unwrap()
    );
    assert_eq!(go(&["make", "controller", "Hud"]).code, 0);
    assert!(root
        .join(p(Key::Client))
        .join("Controllers/Hud.luau")
        .exists());
    assert_eq!(go(&["inject", "shared", "ExampleShared"]).code, 0);
    assert!(root
        .join(p(Key::Shared))
        .join("ExampleShared.luau")
        .exists());
    assert_eq!(go(&["inject", "service", "PointsService"]).code, 0);
    assert!(root
        .join(p(Key::Server))
        .join("Services/PointsService.luau")
        .exists());

    assert!(
        read(&root.join(p(Key::Server)).join("LorenServerTypes.luau"))
            .contains("\"ExampleService\" | \"PointsService\" | \"Shop\"")
    );
    assert!(read(&root.join(p(Key::Shared)).join("LorenTypes.luau"))
        .contains("export type ControllerName = \"ExampleController\" | \"Hud\""));
    let types = go(&["types"]);
    assert_eq!(types.code, 0);
    assert!(types.out.contains("Types are up to date"));
    let refresh = go(&["refresh"]);
    assert_eq!(refresh.code, 0, "{}", refresh.all);
    assert!(refresh.out.contains("Script Sync needs no sourcemap"));
    assert_eq!(runner.calls().len(), 0, "no sourcemap, no program");
    assert!(!root.join("src").exists() && !root.join("default.project.json").exists());
}

#[test]
fn doctor_on_a_script_sync_project_layout_checks_lint_skips_the_runtime_flags_client_luau() {
    let np = none_project();
    let root = &np.root;
    let runner = FakeRunner::new();
    let go = |args: &[&str]| cli(root, args, &runner);
    let ok = go(&["doctor"]);
    assert_eq!(ok.code, 0, "{}", ok.all);
    assert!(ok.out.contains("Sync: Roblox Script Sync"));
    assert!(ok
        .out
        .contains("Runtime layout: LorenRuntime, LorenServer and the shim are in place"));
    assert!(ok.out.contains("Middleware lint: no issues"));
    assert!(!re(r"rokit|Rokit").is_match(&ok.all));

    let colon = |name: &str| {
        format!(
            "local {name} = {{ Client = {{}}, Middleware = {{}} }}\nfunction {name}.Client:Buy(_player) end\nfunction {name}.Middleware:Buy(_player)\n\treturn true\nend\nreturn {name}\n"
        )
    };
    let server = root.join(p(Key::Server));
    fs::write(
        server.join("Services/ColonService.luau"),
        colon("ColonService"),
    )
    .unwrap();
    fs::write(
        root.join(p(Key::RuntimeServer)).join("Evil.luau"),
        colon("Evil"),
    )
    .unwrap();
    let lint = go(&["doctor"]);
    assert_eq!(lint.code, 1);
    assert!(re(r"ColonService\.luau:\d+ \[fixable\]").is_match(&lint.out));
    assert!(!lint.all.contains("Evil"));
    let fixed = go(&["doctor", "--fix", "--yes"]);
    assert_eq!(fixed.code, 0, "{}", fixed.all);
    assert!(
        read(&root.join(p(Key::RuntimeServer)).join("Evil.luau")).contains("Evil.Middleware:Buy"),
        "the runtime is never rewritten"
    );

    let client = root.join(p(Key::Client));
    fs::rename(
        client.join("Client.local.luau"),
        client.join("Client.client.luau"),
    )
    .unwrap();
    let twice = go(&["doctor"]);
    assert_eq!(twice.code, 1);
    assert!(twice
        .err
        .contains("Client.client.luau syncs as a Script with RunContext Client, which runs twice"));

    fs::remove_dir_all(root.join(p(Key::RuntimeServer))).unwrap();
    let missing = go(&["doctor"]);
    assert_eq!(missing.code, 1);
    assert!(missing
        .err
        .contains("ServerScriptService/Server/LorenServer is missing. Run: loren update"));
}

struct FakeFetch;

impl Fetch for FakeFetch {
    fn clone_repo(&self, _spec: &str, dest: &Path) -> Result<(), String> {
        for (rel, text) in [
            ("init.luau", "return {}\n"),
            ("Util.luau", "return {}\n"),
            ("Util.spec.luau", "return nil\n"),
            ("tests/x.spec.lua", "return nil\n"),
        ] {
            write(dest, rel, text);
        }
        Ok(())
    }
}

fn add(root: &Path, args: &[&str]) -> CliOut {
    let log = Log::memory();
    let runner = FakeRunner::new();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let code = main(
        &args,
        Overrides {
            cwd: Some(root.to_path_buf()),
            log: Some(&log),
            is_tty: Some(false),
            input: Some(Box::new(Cursor::new(Vec::<u8>::new()))),
            output: Some(Box::new(std::io::sink())),
            run: Some(&runner),
            lanes: None,
            fetch: Some(&FakeFetch),
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

#[test]
fn add_on_a_script_sync_project_into_lorenpackages_test_files_left_out() {
    let np = none_project();
    let root = &np.root;
    let r = add(root, &["add", "someone/Janitor"]);
    assert_eq!(r.code, 0, "{}", r.all);
    assert_eq!(
        list_files(&root.join(p(Key::Packages)).join("Janitor")),
        ["Util.luau", "init.luau"]
    );
    assert!(r.out.contains("Left out 2 test file"));
    let promise = add(root, &["add", "evaera/roblox-lua-promise", "Promise"]);
    assert_eq!(promise.code, 1, "the bundled Promise.luau is reserved");
}

#[test]
fn serve_and_ignite_explain_that_script_sync_runs_in_studio_exit_0_migrate_is_not_automated_exit_1()
{
    let np = none_project();
    let root = &np.root;
    let runner = FakeRunner::new();
    for cmd in ["serve", "ignite"] {
        let r = cli(root, &[cmd], &runner);
        assert_eq!(r.code, 0, "{}", r.all);
        assert!(r
            .out
            .contains("Roblox Script Sync, which runs inside Studio: there is nothing to serve"));
    }
    assert_eq!(runner.calls().len(), 0);

    let before = snapshot(root);
    for args in [
        vec!["migrate", "--yes"],
        vec!["migrate", "--to", "rojo", "--yes"],
    ] {
        let r = cli(root, &args, &runner);
        assert_eq!(r.code, 1);
        assert!(r.err.contains(
            "Migrating from Roblox Script Sync is not automated yet. Nothing was changed."
        ));
    }
    assert_eq!(snapshot(root), before);

    let (_dir, rojo) = make_project(Tool::Rojo, "game");
    let toml = read(&rojo.join("rokit.toml"));
    let to = run_cli(
        &["migrate", "--to", "none", "--yes"],
        CliOpts {
            cwd: Some(rojo.clone()),
            ..CliOpts::default()
        },
    );
    assert_eq!(to.code, 1);
    assert!(to
        .err
        .contains("Migrating to Roblox Script Sync is not automated yet"));
    assert_eq!(read(&rojo.join("rokit.toml")), toml);
    assert!(!rojo.join(".loren.json").exists());
}

#[test]
fn the_shim_shim_source_and_project_src_shared_loren_luau_are_identical() {
    assert_eq!(
        read(&template_dir().join("src/shared/Loren.luau")),
        SHIM_SOURCE
    );
    assert!(SHIM_SOURCE.contains("script.Parent:FindFirstChild(\"LorenRuntime\")"));
}
