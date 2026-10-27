use super::*;
use crate::coedit::Batch;
use rbx_dom::Vector3Data;

struct Fixture {
    dom: WeakDom,
    ws: Ref,
    part: Ref,
    script: Ref,
}

fn fixture() -> Fixture {
    let mut dom = WeakDom::new();
    let ws = dom.new_instance("Workspace", "Workspace", None);
    let part = dom.new_instance("Part", "P", Some(ws));
    dom.set_property(part, "Anchored", Variant::Bool(true))
        .unwrap();
    dom.set_property(
        part,
        "Size",
        Variant::Vector3(Vector3Data {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        }),
    )
    .unwrap();
    let script = dom.new_instance("Script", "S", Some(ws));
    dom.set_property(script, "Source", Variant::String("a\n".into()))
        .unwrap();
    dom.take_changes();
    Fixture {
        dom,
        ws,
        part,
        script,
    }
}

fn diff(shadow: &WeakDom, current: &mut WeakDom) -> (Vec<Op>, IdMap) {
    let mut ids = IdMap::build(shadow);
    let mut eng = Engine::new("1:studio".into());
    let ops = local_ops(shadow, current, &mut ids, &mut eng);
    (ops, ids)
}

fn dump(dom: &WeakDom, r: Ref) -> String {
    let i = dom.get(r).unwrap();
    let props: std::collections::BTreeMap<_, _> = i
        .properties()
        .iter()
        .filter(|(k, _)| k.as_str() != "UniqueId")
        .collect();
    let mut kids: Vec<String> = i.children().iter().map(|&c| dump(dom, c)).collect();
    kids.sort();
    format!("{}:{}{:?}[{}]", i.name(), i.class(), props, kids.join(","))
}

fn dump_all(dom: &WeakDom) -> Vec<String> {
    let mut v: Vec<String> = dom.root_refs().iter().map(|&r| dump(dom, r)).collect();
    v.sort();
    v
}

#[test]
fn no_change_no_ops() {
    let f = fixture();
    let mut cur = f.dom.clone();
    assert!(diff(&f.dom, &mut cur).0.is_empty());
}

#[test]
fn rename_and_reparent() {
    let f = fixture();
    let mut cur = f.dom.clone();
    cur.set_name(f.part, "Q").unwrap();
    let ops = diff(&f.dom, &mut cur).0;
    assert_eq!(ops.len(), 1);
    assert!(matches!(&ops[0], Op::Set { n, .. } if n == "Name"));

    let mut cur = f.dom.clone();
    cur.set_parent(f.part, Some(f.script));
    let ops = diff(&f.dom, &mut cur).0;
    assert_eq!(ops.len(), 1);
    match &ops[0] {
        Op::Set { n, v, at, .. } => {
            assert_eq!(n, "Parent");
            assert_eq!(v["v"]["p"], "Workspace.S");
            // Where a path-only peer still has it.
            assert_eq!(at.p, "Workspace.P");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn property_set_and_remove() {
    let f = fixture();
    let mut cur = f.dom.clone();
    cur.set_property(f.part, "Anchored", Variant::Bool(false))
        .unwrap();
    cur.remove_property(f.part, "Size").unwrap();
    let ops = diff(&f.dom, &mut cur).0;
    assert_eq!(ops.len(), 2);
    assert!(matches!(&ops[0], Op::Set { n, v, .. } if n == "Anchored" && v["v"] == false));
    assert!(matches!(&ops[1], Op::Set { n, v, .. } if n == "Size" && v.is_null()));
}

#[test]
fn add_subtree_parent_first() {
    let f = fixture();
    let mut cur = f.dom.clone();
    let m = cur.new_instance("Model", "M", Some(f.ws));
    let c = cur.new_instance("Part", "C", Some(m));
    cur.set_property(c, "Anchored", Variant::Bool(true))
        .unwrap();
    let ops = diff(&f.dom, &mut cur).0;
    assert_eq!(ops.len(), 2);
    match (&ops[0], &ops[1]) {
        (
            Op::Add {
                cls, props, parent, ..
            },
            Op::Add {
                props: cp,
                parent: cparent,
                ..
            },
        ) => {
            assert_eq!(cls, "Model");
            assert_eq!(props["Name"]["v"], "M");
            assert_eq!(parent.as_ref().unwrap().p, "Workspace");
            assert_eq!(cp["Name"]["v"], "C");
            assert_eq!(cp["Anchored"]["v"], true);
            assert_eq!(cparent.as_ref().unwrap().p, "Workspace.M");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn remove_subtree_one_del() {
    let mut f = fixture();
    let m = f.dom.new_instance("Model", "M", Some(f.ws));
    f.dom.new_instance("Part", "C", Some(m));
    let mut cur = f.dom.clone();
    cur.remove(m);
    let ops = diff(&f.dom, &mut cur).0;
    assert_eq!(ops.len(), 1);
    assert!(matches!(&ops[0], Op::Del { at, .. } if at.p == "Workspace.M"));
}

#[test]
fn class_change_del_then_add() {
    let f = fixture();
    let mut cur = f.dom.clone();
    let _ = cur.set_class(f.part, "Model");
    let ops = diff(&f.dom, &mut cur).0;
    assert!(matches!(&ops[0], Op::Del { .. }));
    assert!(matches!(&ops[1], Op::Add { cls, .. } if cls == "Model"));
}

#[test]
fn script_edit_is_src_against_old_text() {
    let f = fixture();
    let mut cur = f.dom.clone();
    cur.set_property(f.script, "Source", Variant::String("b\n".into()))
        .unwrap();
    let ops = diff(&f.dom, &mut cur).0;
    assert_eq!(ops.len(), 1);
    match &ops[0] {
        Op::Src { base, text, .. } => {
            assert_eq!(*base, crate::coedit::merge::text_hash("a\n"));
            assert_eq!(text, "b\n");
        }
        other => panic!("unexpected {other:?}"),
    }
}

fn converge(shadow: &WeakDom, current: &mut WeakDom) {
    let (ops, ids) = diff(shadow, current);
    let mut target = shadow.clone();
    let mut ids = ids.clone();
    let mut eng = Engine::new("2:rbx-native".into());
    let batch = Batch {
        a: "1:studio".into(),
        ops,
    };
    eng.apply(&mut target, &mut ids, &batch);
    assert_eq!(dump_all(&target), dump_all(current));
}

#[test]
fn round_trip_converges() {
    let f = fixture();
    let mut cur = f.dom.clone();
    cur.set_name(f.part, "Q").unwrap();
    cur.set_property(f.part, "Anchored", Variant::Bool(false))
        .unwrap();
    cur.remove_property(f.part, "Size").unwrap();
    cur.set_property(f.script, "Source", Variant::String("b\n".into()))
        .unwrap();
    let m = cur.new_instance("Model", "M", Some(f.ws));
    let c = cur.new_instance("Part", "C", Some(m));
    cur.set_property(c, "Anchored", Variant::Bool(true))
        .unwrap();
    cur.set_parent(f.part, Some(m));
    converge(&f.dom, &mut cur);
}

#[test]
fn undo_reverts() {
    let f = fixture();
    let mut modified = f.dom.clone();
    modified.set_name(f.part, "Q").unwrap();
    modified
        .set_property(f.part, "Anchored", Variant::Bool(false))
        .unwrap();
    modified.new_instance("Part", "New", Some(f.ws));
    let mut older = f.dom.clone();
    converge(&modified, &mut older);
}

#[test]
fn attributes_and_tags_go_one_by_one() {
    use crate::coedit::attrs;
    let f = fixture();
    let mut cur = f.dom.clone();
    attrs::write(
        &mut cur,
        f.part,
        "@Speed",
        &json!({"t": "Float64", "v": 16.0}),
    );
    attrs::write(&mut cur, f.part, "#Lava", &json!({"t": "Bool", "v": true}));
    let ops = diff(&f.dom, &mut cur).0;
    let names: Vec<&str> = ops
        .iter()
        .map(|op| match op {
            Op::Set { n, .. } => n.as_str(),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(names, ["#Lava", "@Speed"]);

    let shadow = cur.clone();
    attrs::write(&mut cur, f.part, "#Lava", &Value::Null);
    let ops = diff(&shadow, &mut cur).0;
    assert!(matches!(&ops[..], [Op::Set { n, v, .. }] if n == "#Lava" && v.is_null()));

    let m = cur.new_instance("Model", "M", Some(f.ws));
    attrs::write(&mut cur, m, "@Team", &json!({"t": "String", "v": "red"}));
    let ops = diff(&f.dom, &mut cur).0;
    let add = ops.iter().find_map(|op| match op {
        Op::Add { props, .. } => Some(props),
        _ => None,
    });
    let add = add.unwrap();
    assert_eq!(add["@Team"]["v"], "red");
    assert!(!add.contains_key("AttributesSerialize"));
}

#[test]
fn an_add_leaves_out_class_defaults() {
    let f = fixture();
    let mut cur = f.dom.clone();
    let p = cur.new_instance("Part", "New", Some(f.ws));
    cur.set_property(p, "Transparency", Variant::Float32(0.0))
        .unwrap();
    cur.set_property(p, "Reflectance", Variant::Float32(0.5))
        .unwrap();
    let ops = diff(&f.dom, &mut cur).0;
    let Some(Op::Add { props, .. }) = ops.first() else {
        panic!("unexpected {ops:?}");
    };
    assert!(!props.contains_key("Transparency"));
    assert_eq!(props["Reflectance"]["v"], 0.5);
}

#[test]
fn a_rename_and_move_go_to_the_paths_peers_still_have() {
    let f = fixture();
    let mut cur = f.dom.clone();
    cur.set_name(f.part, "Q").unwrap();
    cur.set_parent(f.part, Some(f.script));
    let ops = diff(&f.dom, &mut cur).0;
    let targets: Vec<(&str, &str)> = ops
        .iter()
        .map(|op| match op {
            Op::Set { n, at, .. } => (n.as_str(), at.p.as_str()),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        targets,
        [("Name", "Workspace.P"), ("Parent", "Workspace.Q")]
    );

    // Applied in that order to a DOM that only knows paths, both land.
    let mut peer = f.dom.clone();
    let mut ids = IdMap::default();
    let mut eng = Engine::new("2:studio".into());
    let stripped: Vec<Op> = ops
        .into_iter()
        .map(|op| match op {
            Op::Set { at, n, v, c } => Op::Set {
                at: Target {
                    id: String::new(),
                    p: at.p,
                },
                n,
                v,
                c,
            },
            other => other,
        })
        .collect();
    eng.apply(
        &mut peer,
        &mut ids,
        &Batch {
            a: "1:studio".into(),
            ops: stripped,
        },
    );
    assert_eq!(
        paths::path_of(&peer, f.part).as_deref(),
        Some("Workspace.S.Q")
    );
}

/// 12,000 parts, one of them edited: what one co-edit tick costs.
/// `cargo test --release -p rbx_presence big_place_tick -- --ignored --nocapture`
#[test]
#[ignore]
fn big_place_tick() {
    let mut dom = WeakDom::new();
    let ws = dom.new_instance("Workspace", "Workspace", None);
    let folder = dom.new_instance("Folder", "Big", Some(ws));
    for i in 0..12_000 {
        let p = dom.new_instance("Part", &format!("P{i}"), Some(folder));
        for (n, v) in [
            ("Anchored", Variant::Bool(true)),
            ("Transparency", Variant::Float32(0.0)),
            (
                "Size",
                Variant::Vector3(Vector3Data {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                }),
            ),
        ] {
            dom.set_property(p, n, v).unwrap();
        }
    }
    let shadow = dom.clone();
    let mut ids = IdMap::build(&shadow);
    let mut eng = Engine::new("1:x".into());
    let target = dom.get(folder).unwrap().children()[7000];
    dom.set_property(target, "Transparency", Variant::Float32(0.5))
        .unwrap();
    let t = std::time::Instant::now();
    let ops = local_ops(&shadow, &mut dom, &mut ids, &mut eng);
    let diffed = t.elapsed();
    let t = std::time::Instant::now();
    let _copy = dom.clone();
    println!(
        "diff {diffed:?} ({} ops), clone {:?}",
        ops.len(),
        t.elapsed()
    );
    assert_eq!(ops.len(), 1);
}
