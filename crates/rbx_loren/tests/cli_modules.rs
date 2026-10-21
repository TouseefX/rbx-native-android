mod common;

use std::fs;

use common::*;
use rbx_loren::ctx::Tool;
use rbx_loren::project::render_module;
use regex::Regex;

const SERVICES: &str = "src/server/Services";
const CONTROLLERS: &str = "src/client/Controllers";

fn render(kind: &str, name: &str) -> String {
    render_module(kind, name, &template_dir()).expect("render module")
}

fn cli<'a>(root: &std::path::Path) -> CliOpts<'a> {
    CliOpts {
        cwd: Some(root.to_path_buf()),
        ..CliOpts::default()
    }
}

fn without_resolve(lanes: &MockLanes) -> Vec<&'static str> {
    lanes
        .events()
        .iter()
        .filter_map(|e| match e {
            Event::ResolveTool { .. } => None,
            Event::GenerateTypes { .. } => Some("generate_types"),
            Event::Sourcemap { .. } => Some("sourcemap"),
            Event::DetectTool { .. } => Some("detect_tool"),
            Event::RunDoctor { .. } => Some("run_doctor"),
            Event::RunUpdate { .. } => Some("run_update"),
        })
        .collect()
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("pattern")
}

#[test]
fn make_service_controller_writes_the_scaffold_template_into_services_controllers() {
    let (_keep, root) = make_project(Tool::Rojo, "Game");
    let lanes = MockLanes::default();
    let s = run_cli(
        &["make", "service", "CombatService"],
        CliOpts {
            lanes: Some(&lanes),
            ..cli(&root)
        },
    );
    assert_eq!(s.code, 0, "{}", s.all);
    let file = root.join(SERVICES).join("CombatService.luau");
    assert_eq!(read(&file), render("service", "CombatService"));
    assert!(read(&file).starts_with("local CombatService = {"));
    assert!(!read(&file).contains("ExampleService"));
    assert_eq!(without_resolve(&lanes), ["generate_types", "sourcemap"]);

    let c = run_cli(&["make", "Controller", "HudController"], cli(&root));
    assert_eq!(c.code, 0, "{}", c.all);
    assert_eq!(
        read(&root.join(CONTROLLERS).join("HudController.luau")),
        render("controller", "HudController")
    );
}

#[test]
fn make_uses_exactly_the_scaffold_examples_as_its_templates() {
    for (kind, file, name) in [
        (
            "service",
            format!("{SERVICES}/ExampleService.luau"),
            "ExampleService",
        ),
        (
            "controller",
            format!("{CONTROLLERS}/ExampleController.luau"),
            "ExampleController",
        ),
    ] {
        let scaffold = read(&template_dir().join(file)).replace("\r\n", "\n");
        assert_eq!(render(kind, name), scaffold);
        let word = re(&format!(r"\b{name}\b"));
        assert_eq!(
            render(kind, "Zed"),
            word.replace_all(&scaffold, "Zed").into_owned()
        );
    }
}

#[test]
fn make_validates_names_and_never_writes_outside_the_folder() {
    let (_keep, root) = make_project(Tool::Rojo, "Game");
    let before = list_files(&root);
    for bad in [
        "Combat/Damage",
        "..",
        "../Evil",
        "1Service",
        "end",
        "my-service",
        "CON",
        "",
        "a\\b",
    ] {
        let r = run_cli(&["make", "service", bad], cli(&root));
        assert_eq!(r.code, 1, "accepted {bad:?}");
    }
    assert_eq!(list_files(&root), before);
}

#[test]
fn make_refuses_existing_names_any_case_either_kind_unless_force() {
    let (_keep, root) = make_project(Tool::Rojo, "Game");
    let same = run_cli(&["make", "service", "exampleservice"], cli(&root));
    assert_eq!(same.code, 1);
    assert!(same.err.contains("already exists"), "{}", same.err);

    let cross = run_cli(&["make", "controller", "ExampleService"], cli(&root));
    assert_eq!(cross.code, 1);
    assert!(cross.err.contains("already a Service"), "{}", cross.err);
    assert!(!root.join(CONTROLLERS).join("ExampleService.luau").exists());

    fs::write(
        root.join(SERVICES).join("ExampleService.luau"),
        "-- mine\nreturn {}\n",
    )
    .unwrap();
    let forced = run_cli(
        &["make", "service", "ExampleService", "--force"],
        cli(&root),
    );
    assert_eq!(forced.code, 0, "{}", forced.all);
    assert_eq!(
        read(&root.join(SERVICES).join("ExampleService.luau")),
        render("service", "ExampleService")
    );
    let backups = list_files(&root.join(".loren-backup"));
    assert_eq!(backups.len(), 1);
    assert_eq!(
        read(&root.join(".loren-backup").join(&backups[0])),
        "-- mine\nreturn {}\n"
    );
}

#[test]
fn make_warns_about_non_pascal_case_names_rejects_unknown_types_and_non_projects() {
    let (_keep, root) = make_project(Tool::Rojo, "Game");
    let lower = run_cli(&["make", "controller", "hud"], cli(&root));
    assert_eq!(lower.code, 0);
    assert!(lower.err.contains("not PascalCase"), "{}", lower.err);

    let kind = run_cli(&["make", "widget", "Foo"], cli(&root));
    assert_eq!(kind.code, 1);
    assert!(kind.err.contains("Unknown type \"widget\""), "{}", kind.err);

    let empty = tmp_dir();
    let outside = run_cli(&["make", "service", "Foo"], cli(empty.path()));
    assert_eq!(outside.code, 1);
    assert!(
        re(r"No default\.project\.json").is_match(&outside.err),
        "{}",
        outside.err
    );
}

#[test]
fn make_exits_1_when_the_sourcemap_or_the_types_fail_after_saying_what_was_created() {
    let (_keep, root) = make_project(Tool::Rojo, "Game");
    let lanes = MockLanes {
        sourcemap_ok: false,
        ..MockLanes::default()
    };
    let r = run_cli(
        &["make", "service", "Foo"],
        CliOpts {
            lanes: Some(&lanes),
            ..cli(&root)
        },
    );
    assert_eq!(r.code, 1);
    assert!(r.out.contains("Created Service Foo"), "{}", r.out);
    assert!(
        r.err.contains("sourcemap.json was not regenerated"),
        "{}",
        r.err
    );

    let lanes2 = MockLanes {
        types_result: Some(Err("boom".to_string())),
        ..MockLanes::default()
    };
    let r2 = run_cli(
        &["make", "service", "Bar"],
        CliOpts {
            lanes: Some(&lanes2),
            ..cli(&root)
        },
    );
    assert_eq!(r2.code, 1);
    assert!(
        r2.err.contains("LorenTypes were not regenerated: boom"),
        "{}",
        r2.err
    );
}

#[test]
fn make_follows_the_folders_in_default_project_json() {
    let (_keep, root) = make_project(Tool::Rojo, "Game");
    let path = root.join("default.project.json");
    let text = read(&path);
    let moved = re(r#""\$path"\s*:\s*"src/server""#)
        .replace(&text, r#""$$path": "game/server""#)
        .into_owned();
    assert_ne!(moved, text, "server path not found in the project file");
    fs::write(&path, moved).unwrap();
    let r = run_cli(&["make", "service", "Moved"], cli(&root));
    assert_eq!(r.code, 0, "{}", r.all);
    assert!(root
        .join("game")
        .join("server")
        .join("Services")
        .join("Moved.luau")
        .exists());
}

#[test]
fn inject_list_shows_the_built_in_premades_no_retired_example_ones_even_outside_a_project() {
    let dir = tmp_dir();
    let r = run_cli(&["inject", "--list"], cli(dir.path()));
    assert_eq!(r.code, 0, "{}", r.all);
    assert!(
        re(r"service\s+PointsService\s+built-in").is_match(&r.out),
        "{}",
        r.out
    );
    assert!(
        re(r"controller\s+PointsController\s+built-in").is_match(&r.out),
        "{}",
        r.out
    );
    assert!(
        re(r"shared\s+ExampleShared\s+built-in").is_match(&r.out),
        "{}",
        r.out
    );
    assert!(
        !re(r"ExampleService|ExampleController").is_match(&r.out),
        "{}",
        r.out
    );

    let dir2 = tmp_dir();
    let filtered = run_cli(&["inject", "--list", "controller"], cli(dir2.path()));
    assert_eq!(filtered.code, 0);
    assert!(!filtered.out.contains("PointsService"), "{}", filtered.out);
}

#[test]
fn inject_writes_into_the_correctly_cased_services_controllers_shared_folders() {
    let (_keep, root) = make_project(Tool::Rojo, "Game");
    let lanes = MockLanes::default();
    assert_eq!(
        run_cli(
            &["inject", "service", "PointsService"],
            CliOpts {
                lanes: Some(&lanes),
                ..cli(&root)
            }
        )
        .code,
        0
    );
    assert_eq!(
        run_cli(&["inject", "controller", "pointscontroller"], cli(&root)).code,
        0
    );
    assert_eq!(
        run_cli(&["inject", "shared", "ExampleShared"], cli(&root)).code,
        0
    );
    let files = list_files(&root);
    assert!(files.contains(&format!("{SERVICES}/PointsService.luau")));
    assert!(
        files.contains(&format!("{CONTROLLERS}/PointsController.luau")),
        "premade casing kept"
    );
    assert!(files.contains(&"src/shared/ExampleShared.luau".to_string()));
    let lowercase = re(r"/services/|/controllers/");
    assert!(
        !files.iter().any(|f| lowercase.is_match(f)),
        "no lowercase folders"
    );
    assert_eq!(
        read(&root.join(SERVICES).join("PointsService.luau")),
        read(&template_dir().join("loren_premade/services/PointsService.luau")),
    );
    assert_eq!(without_resolve(&lanes), ["generate_types", "sourcemap"]);
}

#[test]
fn inject_prefers_the_projects_own_loren_premade_and_refuses_collisions_without_force() {
    let (_keep, root) = make_project(Tool::Rojo, "Game");
    let own = root.join("loren_premade").join("services");
    fs::create_dir_all(&own).unwrap();
    fs::write(own.join("MyThing.luau"), "return {}\n").unwrap();
    let list = run_cli(&["inject", "--list"], cli(&root));
    assert!(
        re(r"service\s+MyThing\s+project").is_match(&list.out),
        "{}",
        list.out
    );
    assert_eq!(
        run_cli(&["inject", "service", "MyThing"], cli(&root)).code,
        0
    );

    let again = run_cli(&["inject", "service", "MyThing"], cli(&root));
    assert_eq!(again.code, 1);
    assert!(again.err.contains("already exists"), "{}", again.err);
    assert_eq!(
        run_cli(&["inject", "service", "MyThing", "--force"], cli(&root)).code,
        0
    );

    let missing = run_cli(&["inject", "service", "Nope"], cli(&root));
    assert_eq!(missing.code, 1);
    assert!(
        re(r#"No service premade named "Nope"\. Available: .*PointsService"#)
            .is_match(&missing.err),
        "{}",
        missing.err
    );

    assert_eq!(run_cli(&["inject"], cli(&root)).code, 1);
    assert_eq!(run_cli(&["inject", "widget", "X"], cli(&root)).code, 1);
}
