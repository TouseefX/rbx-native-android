use rbx_dom::Variant;

use super::*;

struct Fixture {
    dom: WeakDom,
    db: ReflectionDatabase,
    server: Ref,
    local: Ref,
    module: Ref,
    part: Ref,
}

fn script(dom: &mut WeakDom, class: &str, name: &str, parent: Ref, text: &str) -> Ref {
    let reference = dom.new_instance(class, name, Some(parent));
    dom.set_property(reference, "Source", Variant::String(text.to_string()))
        .unwrap();
    reference
}

fn fixture() -> Fixture {
    let mut dom = WeakDom::new();
    let root = dom.new_instance("DataModel", "Game", None);
    let workspace = dom.new_instance("Workspace", "Workspace", Some(root));
    let sss = dom.new_instance("ServerScriptService", "ServerScriptService", Some(root));
    let part = dom.new_instance("Part", "Brick", Some(workspace));
    dom.set_property(part, "Source", Variant::String("print(hello)".into()))
        .unwrap();
    let server = script(
        &mut dom,
        "Script",
        "Main",
        sss,
        "local a = 1\n\tprint(Hello) print(hello)\nreturn a",
    );
    let local = script(&mut dom, "LocalScript", "Client", workspace, "-- HELLO");
    let module = script(&mut dom, "ModuleScript", "Lib", sss, "return 1");
    dom.take_changes();
    Fixture {
        dom,
        db: ReflectionDatabase::embedded(),
        server,
        local,
        module,
        part,
    }
}

#[test]
fn groups_lines_with_counts_and_ranges() {
    let f = fixture();
    let mut budget = 100;
    let hits = search(&f.dom, &f.db, "hello", &mut budget);
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].script, f.local);
    assert_eq!(hits[0].path, "Workspace.Client");
    assert_eq!(hits[0].class, "LocalScript");
    assert_eq!(hits[1].script, f.server);
    assert_eq!(hits[1].path, "ServerScriptService.Main");
    let line = &hits[1].lines[0];
    assert_eq!(hits[1].lines.len(), 1);
    assert_eq!(line.line, 2);
    assert_eq!(line.count, 2);
    assert_eq!(line.range, 19..24);
    assert_eq!(line.text, "print(Hello) print(hello)");
    assert_eq!(budget, 98);
}

#[test]
fn ignores_non_scripts_and_empty_query() {
    let f = fixture();
    let mut budget = 100;
    let hits = search(&f.dom, &f.db, "hello", &mut budget);
    assert!(hits.iter().all(|hit| hit.script != f.part));
    assert!(search(&f.dom, &f.db, "", &mut budget).is_empty());
    assert_eq!(budget, 98);
    assert!(search(&f.dom, &f.db, "return", &mut budget)
        .iter()
        .any(|hit| hit.script == f.module));
}

#[test]
fn budget_stops_the_walk() {
    let f = fixture();
    let mut budget = 1;
    let hits = search(&f.dom, &f.db, "hello", &mut budget);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].script, f.local);
    assert_eq!(budget, 0);
}

#[test]
fn full_name_skips_datamodel() {
    let f = fixture();
    assert_eq!(full_name(&f.dom, f.server), "ServerScriptService.Main");
    assert_eq!(full_name(&f.dom, Ref::new(u32::MAX)), "");
}

#[test]
fn preview_lists_changed_lines() {
    let rows = preview("a\n  foo FOO\nb\nfoo", "foo", "x");
    assert_eq!(
        rows,
        vec![
            Preview {
                line: 2,
                before: "  foo FOO".into(),
                after: "  x x".into()
            },
            Preview {
                line: 4,
                before: "foo".into(),
                after: "x".into()
            },
        ]
    );
}

#[test]
fn replace_changes_only_matching_scripts() {
    let mut f = fixture();
    let changed = replace(&mut f.dom, &f.db, "hello", "bye");
    assert_eq!(changed, vec![f.local, f.server]);
    assert_eq!(source::read(&f.dom, f.local).unwrap(), "-- bye");
    assert_eq!(source::read(&f.dom, f.module).unwrap(), "return 1");
    assert!(!f.dom.take_changes().is_empty());
}

#[test]
fn replace_with_empty_query_is_a_no_op() {
    let mut f = fixture();
    assert!(replace(&mut f.dom, &f.db, "", "x").is_empty());
    assert!(f.dom.take_changes().is_empty());
}
