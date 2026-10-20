use rbx_dom::{Variant, WeakDom};
use rbx_reflection::ReflectionDatabase;

use super::*;

const SHARED: &str = "return { add = function(a, b) return a + b end }";

/// A place with `ReplicatedStorage.Specs` holding the named fixtures
/// (`fixtures/<name>.luau`) beside a `Shared` helper module.
fn place(specs: &[(&str, &str)]) -> WeakDom {
    let mut dom = WeakDom::new();
    dom.new_instance("Workspace", "Workspace", None);
    let storage = dom.new_instance("ReplicatedStorage", "ReplicatedStorage", None);
    let folder = dom.new_instance("Folder", "Specs", Some(storage));
    let module = |dom: &mut WeakDom, name: &str, source: &str| {
        let r = dom.new_instance("ModuleScript", name, Some(folder));
        dom.set_property(r, "Source", Variant::String(source.into()))
            .unwrap();
    };
    module(&mut dom, "Shared", SHARED);
    for (name, source) in specs {
        module(&mut dom, name, source);
    }
    dom
}

fn run_all(dom: &WeakDom) -> Report {
    let files = discover(dom, &[]).unwrap();
    run(
        dom,
        &files,
        ReflectionDatabase::shared(),
        &Options::default(),
    )
}

fn find<'a>(report: &'a Report, path: &[&str]) -> &'a TestResult {
    report
        .results()
        .find(|r| r.path == path)
        .unwrap_or_else(|| panic!("no result for {path:?} in\n{}", human(report)))
}

#[test]
fn passing_specs_pass() {
    let dom = place(&[("pass.spec", include_str!("fixtures/pass.spec.luau"))]);
    let report = run_all(&dom);
    assert!(report.passed(), "{}", human(&report));
    assert_eq!(report.count(Status::Passed), 4);
}

#[test]
fn failures_carry_message_and_script_line() {
    let dom = place(&[("fail.spec", include_str!("fixtures/fail.spec.luau"))]);
    let report = run_all(&dom);
    let failed = find(&report, &["failures", "a failed expectation"]);
    assert_eq!(failed.status, Status::Failed);
    assert_eq!(
        failed.message.as_deref(),
        Some("Expected 3 (number), got 2 (number)")
    );
    let at = failed.location.as_ref().unwrap();
    assert_eq!(
        (at.name.as_str(), at.line),
        ("ReplicatedStorage.Specs.fail.spec", 4)
    );

    let errored = find(&report, &["failures", "an error"]);
    assert!(errored
        .message
        .as_deref()
        .unwrap()
        .contains("attempt to index nil"));
    assert_eq!(errored.location.as_ref().unwrap().line, 9);
    assert!(errored.stack.as_deref().unwrap().contains("fail.spec:9"));
    assert_eq!(
        find(&report, &["failures", "passes"]).status,
        Status::Passed
    );
}

#[test]
fn hook_errors_fail_the_tests_they_guard() {
    let dom = place(&[("hooks.spec", include_str!("fixtures/hooks.spec.luau"))]);
    let report = run_all(&dom);
    let message = |path: &[&str]| find(&report, path).message.clone().unwrap();
    assert!(
        message(&["beforeEach fails", "never gets to run"]).starts_with("beforeEach hook failed:")
    );
    assert!(message(&["beforeAll fails", "first"]).starts_with("beforeAll hook failed:"));
    assert!(message(&["beforeAll fails", "second"]).contains("suite setup broke"));
    assert!(message(&["afterEach fails", "ran but its teardown failed"])
        .starts_with("afterEach hook failed:"));
    assert_eq!(
        find(&report, &["afterAll fails", "passes before it"]).status,
        Status::Passed
    );
    assert!(message(&["afterAll fails", "afterAll"]).contains("suite teardown broke"));
    assert!(message(&["describe body fails"]).contains("declaring broke"));
    assert_eq!(report.count(Status::Passed), 1);
}

#[test]
fn skipped_tests_do_not_run() {
    let dom = place(&[("skip.spec", include_str!("fixtures/skip.spec.luau"))]);
    let report = run_all(&dom);
    assert!(report.passed(), "{}", human(&report));
    assert_eq!(report.count(Status::Passed), 1);
    assert_eq!(report.count(Status::Skipped), 5);
}

#[test]
fn focus_in_one_file_skips_every_other_test() {
    let dom = place(&[
        ("focus.spec", include_str!("fixtures/focus.spec.luau")),
        ("pass.spec", include_str!("fixtures/pass.spec.luau")),
    ]);
    let report = run_all(&dom);
    assert!(report.passed(), "{}", human(&report));
    assert_eq!(report.count(Status::Passed), 1);
    assert_eq!(find(&report, &["focus", "focused"]).status, Status::Passed);
    assert_eq!(report.count(Status::Skipped), 5);
}

#[test]
fn each_file_gets_a_fresh_place_and_jest_style_works() {
    let dom = place(&[
        ("jest.test", include_str!("fixtures/jest.test.luau")),
        (
            "isolation.spec",
            include_str!("fixtures/isolation.spec.luau"),
        ),
    ]);
    let report = run_all(&dom);
    assert!(report.passed(), "{}", human(&report));
    assert_eq!(report.count(Status::Passed), 2);
}

#[test]
fn a_test_waiting_forever_fails_instead_of_hanging() {
    let dom = place(&[("hang.spec", include_str!("fixtures/hang.spec.luau"))]);
    let report = run_all(&dom);
    let hung = find(&report, &["waits on an event nothing fires"]);
    assert_eq!(hung.status, Status::Failed);
    assert!(
        hung.message.as_deref().unwrap().contains("never finished"),
        "{}",
        human(&report)
    );
}

#[test]
fn a_spec_that_does_not_compile_is_one_failure() {
    let dom = place(&[("broken.spec", "return function( end")]);
    let report = run_all(&dom);
    assert_eq!(report.count(Status::Failed), 1);
    assert!(report.files[0].results[0].path.is_empty());
}

#[test]
fn filter_and_only_narrow_the_run() {
    let dom = place(&[("fail.spec", include_str!("fixtures/fail.spec.luau"))]);
    let files = discover(&dom, &[]).unwrap();
    let db = ReflectionDatabase::shared();
    let filtered = Options {
        filter: Some("passes".into()),
        only: None,
    };
    let report = run(&dom, &files, db, &filtered);
    assert_eq!(report.results().count(), 1);
    assert!(report.passed());
    let one = Options {
        filter: None,
        only: Some(vec!["failures".into(), "an error".into()]),
    };
    let report = run(&dom, &files, db, &one);
    assert_eq!(report.results().count(), 1);
    assert_eq!(report.count(Status::Failed), 1);
}

#[test]
fn plan_lists_describes_and_tests_with_lines() {
    let dom = place(&[("skip.spec", include_str!("fixtures/skip.spec.luau"))]);
    let file = &discover(&dom, &[]).unwrap()[0];
    let plan = plan(&dom, file, ReflectionDatabase::shared());
    assert!(plan.error.is_none());
    let first = &plan.nodes[0];
    assert_eq!(
        (first.kind, first.path.clone(), first.line),
        (NodeKind::Describe, vec!["skips".to_string()], Some(2))
    );
    let skipped = &plan.nodes[1];
    assert_eq!((skipped.line, skipped.skip), (Some(3), true));
    assert_eq!(plan.nodes.len(), 9);
}

#[test]
fn discovery_skips_vendored_packages_and_honours_roots() {
    let mut dom = place(&[("a.spec", "return function() end")]);
    let storage = dom.root_refs()[1];
    let packages = dom.new_instance("Folder", "Packages", Some(storage));
    let index = dom.new_instance("Folder", "_Index", Some(packages));
    dom.new_instance("ModuleScript", "vendored.spec", Some(index));
    let names = |roots: &[String]| -> Vec<String> {
        discover(&dom, roots)
            .unwrap()
            .into_iter()
            .map(|f| f.name)
            .collect()
    };
    assert_eq!(names(&[]), ["ReplicatedStorage.Specs.a.spec"]);
    assert_eq!(
        names(&["ReplicatedStorage.Specs".into()]),
        ["ReplicatedStorage.Specs.a.spec"]
    );
    assert!(names(&["Workspace".into()]).is_empty());
    assert!(discover(&dom, &["Nowhere".into()]).is_err());
}

#[test]
fn junit_and_json_carry_every_test() {
    let dom = place(&[("fail.spec", include_str!("fixtures/fail.spec.luau"))]);
    let report = run_all(&dom);
    let xml = junit(&report);
    assert!(xml.contains(
        "<testsuite name=\"ReplicatedStorage.Specs.fail.spec\" tests=\"3\" failures=\"2\""
    ));
    assert!(xml.contains("<failure message=\"Expected 3 (number), got 2 (number)\">"));
    let value: serde_json::Value = serde_json::from_str(&json(&report)).unwrap();
    assert_eq!(value["failed"], 2);
    assert_eq!(value["files"][0]["tests"][0]["location"]["line"], 4);
}

#[test]
fn a_test_still_waiting_past_the_time_limit_fails() {
    let dom = place(&[(
        "slow.spec",
        "return function() it('sleeps', function() task.wait(3600) end) end",
    )]);
    let report = run_all(&dom);
    let slow = find(&report, &["sleeps"]);
    assert!(
        slow.message.as_deref().unwrap().contains("simulated time"),
        "{}",
        human(&report)
    );
}

#[test]
fn a_runaway_loop_fails_its_test_and_the_file_goes_on() {
    let dom = place(&[(
        "loop.spec",
        "return function() it('spins', function() while true do end end) it('after', function() end) end",
    )]);
    let report = run_all(&dom);
    assert!(
        find(&report, &["spins"])
            .message
            .as_deref()
            .unwrap()
            .contains("Script timeout"),
        "{}",
        human(&report)
    );
    assert_eq!(
        find(&report, &["after"]).status,
        Status::Passed,
        "{}",
        human(&report)
    );
}
