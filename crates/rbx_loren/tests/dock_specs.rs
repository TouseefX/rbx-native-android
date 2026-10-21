use rbx_dom::{Variant, WeakDom};
use rbx_loren::specs::{is_loren_module, plan_file, run_file, run_mixed};
use rbx_lua::test_runner::{discover, NodeKind, Options, Status};
use rbx_reflection::ReflectionDatabase;

fn place() -> WeakDom {
    let mut dom = WeakDom::new();
    dom.new_instance("Workspace", "Workspace", None);
    let storage = dom.new_instance("ReplicatedStorage", "ReplicatedStorage", None);
    let module = |dom: &mut WeakDom, name: &str, source: &str| {
        let r = dom.new_instance("ModuleScript", name, Some(storage));
        dom.set_property(r, "Source", Variant::String(source.into()))
            .unwrap();
    };
    module(
        &mut dom,
        "Shared",
        "return { add = function(a, b) return a + b end }",
    );
    module(
        &mut dom,
        "Counter.spec",
        include_str!("fixtures/specs/Counter.spec.luau"),
    );
    module(&mut dom, "Plain.spec", "return function() end");
    dom
}

#[test]
fn a_loren_spec_plans_and_runs_test_by_test() {
    let dom = place();
    let files = discover(&dom, &[]).unwrap();
    let loren: Vec<_> = files
        .iter()
        .filter(|f| is_loren_module(&dom, f.module))
        .collect();
    assert_eq!(loren.len(), 1, "only the `return function(t)` one");
    let file = loren[0];

    let plan = plan_file(&dom, file);
    assert_eq!(plan.error, None);
    let tests: Vec<_> = plan
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Test)
        .map(|n| (n.path.join(" / "), n.line, n.skip))
        .collect();
    assert_eq!(
        tests,
        [
            ("Counter / adds".to_string(), Some(10), false),
            ("Counter / waits a beat".to_string(), Some(15), false),
            ("Counter / later".to_string(), Some(19), true),
        ]
    );

    let report = run_file(&dom, file, &Options::default());
    let status = |name: &str| {
        let r = report
            .results
            .iter()
            .find(|r| r.path == ["Counter", name])
            .unwrap_or_else(|| panic!("no {name} in {:#?}", report.results));
        (r.status, r)
    };
    let (adds, passed) = status("adds");
    assert_eq!(adds, Status::Passed);
    assert_eq!(passed.output.lines(), ["count is 2"]);
    let (waits, failed) = status("waits a beat");
    assert_eq!(waits, Status::Failed);
    assert!(
        failed.message.as_deref().unwrap_or("").contains('1'),
        "{failed:?}"
    );
    assert_eq!(
        failed.location.as_ref().map(|l| l.line),
        Some(17),
        "{failed:?}"
    );
    assert_eq!(status("later").0, Status::Skipped);
    assert_eq!(report.output.lines(), ["count is 2"]);

    let only = Options {
        only: Some(vec!["Counter".into(), "adds".into()]),
        ..Options::default()
    };
    let narrowed = run_file(&dom, file, &only);
    assert_eq!(narrowed.results.len(), 1, "{:#?}", narrowed.results);
    assert_eq!(narrowed.results[0].status, Status::Passed);
}

#[test]
fn a_mixed_run_sends_each_spec_to_its_own_runner() {
    let mut dom = place();
    let storage = dom.root_refs()[1];
    let testez = dom.new_instance("ModuleScript", "A.spec", Some(storage));
    let source = "return function()\n\tit('prints', function() print('from testez') end)\nend\n";
    dom.set_property(testez, "Source", Variant::String(source.into()))
        .unwrap();
    let files = discover(&dom, &[]).unwrap();
    let report = run_mixed(
        &dom,
        &files,
        &ReflectionDatabase::embedded(),
        &Options::default(),
    );
    let names: Vec<_> = report.files.iter().map(|r| r.file.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "ReplicatedStorage.Counter.spec",
            "ReplicatedStorage.Plain.spec",
            "ReplicatedStorage.A.spec"
        ]
    );
    let a = &report.files[2].results[0];
    assert_eq!(a.status, Status::Passed);
    assert_eq!(a.output.lines(), ["from testez"]);
    assert_eq!(
        report.count(Status::Failed),
        1,
        "only Counter's `waits a beat`"
    );
}
