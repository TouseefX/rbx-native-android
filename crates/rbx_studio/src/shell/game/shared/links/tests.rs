use rbx_dom::{Content, Ref, UniqueId, Variant, WeakDom};

use super::super::super::super::clipboard::snapshot;
use super::*;

fn uid(random: i64, index: u32) -> Variant {
    Variant::UniqueId(UniqueId {
        index,
        time: 1,
        random,
    })
}

/// `Workspace` holding `Baseplate` and a `Tree` model with a `Trunk` part
/// and a `Weld`; every instance carries an id from the `random` series.
fn place(random: i64) -> (WeakDom, Ref, Ref) {
    let mut dom = WeakDom::new();
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    dom.new_instance("Part", "Baseplate", Some(workspace));
    let tree = dom.new_instance("Model", "Tree", Some(workspace));
    dom.new_instance("Part", "Trunk", Some(tree));
    dom.new_instance("Weld", "Weld", Some(tree));
    for (index, r) in subtree(&dom, None).into_iter().enumerate() {
        dom.set_property(r, UNIQUE_ID, uid(random, index as u32))
            .unwrap();
    }
    (dom, workspace, tree)
}

fn child(dom: &WeakDom, parent: Ref, name: &str) -> Ref {
    resolve(dom, Some(parent), &[(name.to_owned(), 0)]).unwrap()
}

fn prop<'a>(dom: &'a WeakDom, holder: Ref, name: &str) -> Option<&'a Variant> {
    dom.get(holder).unwrap().properties().get(name)
}

fn sync(target: &mut WeakDom, old: Ref, source: &WeakDom, source_root: Ref) -> Ref {
    let node = snapshot(source, source_root).unwrap();
    swap(target, source, &node, old)
}

#[test]
fn references_from_outside_follow_the_copy_by_path_or_clear() {
    let (mut source, _, source_tree) = place(1);
    // The source no longer has `Weld`, so a pointer to it has nothing left.
    let weld = child(&source, source_tree, "Weld");
    source.remove(weld);

    let (mut target, workspace, old) = place(2);
    let trunk_value = target.new_instance("ObjectValue", "ToTrunk", Some(workspace));
    let old_trunk = child(&target, old, "Trunk");
    target
        .set_property(trunk_value, "Value", Variant::Ref(old_trunk))
        .unwrap();
    let weld_value = target.new_instance("ObjectValue", "ToWeld", Some(workspace));
    let old_weld = child(&target, old, "Weld");
    target
        .set_property(
            weld_value,
            "Value",
            Variant::Content(Content::Object(old_weld)),
        )
        .unwrap();

    let fresh = sync(&mut target, old, &source, source_tree);

    assert_eq!(
        prop(&target, trunk_value, "Value"),
        Some(&Variant::Ref(child(&target, fresh, "Trunk")))
    );
    assert_eq!(prop(&target, weld_value, "Value"), None);
}

#[test]
fn references_out_of_the_copy_resolve_in_the_target_or_clear() {
    let (mut source, source_workspace, source_tree) = place(1);
    let baseplate = child(&source, source_workspace, "Baseplate");
    let only_here = source.new_instance("Part", "OnlyInSource", Some(source_workspace));
    let weld = child(&source, source_tree, "Weld");
    let trunk = child(&source, source_tree, "Trunk");
    source
        .set_property(weld, "Part0", Variant::Ref(trunk))
        .unwrap();
    source
        .set_property(weld, "Part1", Variant::Ref(baseplate))
        .unwrap();
    let lost = source.new_instance("ObjectValue", "Lost", Some(source_tree));
    source
        .set_property(lost, "Value", Variant::Ref(only_here))
        .unwrap();

    let (mut target, workspace, old) = place(2);
    let fresh = sync(&mut target, old, &source, source_tree);

    let weld = child(&target, fresh, "Weld");
    assert_eq!(
        prop(&target, weld, "Part0"),
        Some(&Variant::Ref(child(&target, fresh, "Trunk")))
    );
    assert_eq!(
        prop(&target, weld, "Part1"),
        Some(&Variant::Ref(child(&target, workspace, "Baseplate")))
    );
    assert_eq!(prop(&target, child(&target, fresh, "Lost"), "Value"), None);
}

#[test]
fn copies_keep_the_ids_they_replace_and_never_take_the_sources() {
    let (mut source, _, source_tree) = place(1);
    let leaf = source.new_instance("Part", "Leaf", Some(source_tree));
    source.set_property(leaf, UNIQUE_ID, uid(1, 99)).unwrap();
    let (mut target, _, old) = place(2);
    let id_of = |dom: &WeakDom, r: Ref| prop(dom, r, UNIQUE_ID).unwrap().clone();
    let old_ids: Vec<Variant> = [
        old,
        child(&target, old, "Trunk"),
        child(&target, old, "Weld"),
    ]
    .map(|r| id_of(&target, r))
    .to_vec();

    let fresh = sync(&mut target, old, &source, source_tree);

    let kept = [
        fresh,
        child(&target, fresh, "Trunk"),
        child(&target, fresh, "Weld"),
    ]
    .map(|r| id_of(&target, r))
    .to_vec();
    assert_eq!(kept, old_ids);
    let new_leaf = prop(&target, child(&target, fresh, "Leaf"), UNIQUE_ID).unwrap();
    assert!(!old_ids.contains(new_leaf));
    assert_ne!(new_leaf, &uid(1, 99));
    let all: Vec<&Variant> = subtree(&target, None)
        .into_iter()
        .filter_map(|r| prop(&target, r, UNIQUE_ID))
        .collect();
    assert!(all
        .iter()
        .all(|id| !matches!(id, Variant::UniqueId(UniqueId { random: 1, .. }))));
    let mut unique = all.clone();
    unique.sort_by_key(|id| format!("{id:?}"));
    unique.dedup();
    assert_eq!(unique.len(), all.len());
}

#[test]
fn paths_count_same_named_siblings() {
    let mut dom = WeakDom::new();
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    dom.new_instance("Part", "Part", Some(workspace));
    let second = dom.new_instance("Part", "Part", Some(workspace));
    let steps = path(&dom, None, second).unwrap();
    assert_eq!(
        steps,
        vec![("Workspace".to_owned(), 0), ("Part".to_owned(), 1)]
    );
    assert_eq!(resolve(&dom, None, &steps), Some(second));
}
