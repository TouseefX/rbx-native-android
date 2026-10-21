use std::collections::HashSet;

use rbx_dom::{Ref, Variant, WeakDom};
use rbx_reflection::ReflectionDatabase;

use super::*;
use crate::script_editor::source;

/// `Workspace` and `ReplicatedStorage` as roots, the way a place file reads.
fn place() -> (WeakDom, Ref, Ref) {
    let mut dom = WeakDom::new();
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    let storage = dom.new_instance("ReplicatedStorage", "ReplicatedStorage", None);
    (dom, workspace, storage)
}

/// A shared `Model` named `Tree` with a `Part` and a `ModuleScript`.
fn tree(dom: &mut WeakDom, parent: Ref, id: &str, source_text: &str) -> Ref {
    let model = dom.new_instance("Model", "Tree", Some(parent));
    dom.new_instance("Part", "Trunk", Some(model));
    let module = dom.new_instance("ModuleScript", "Grow", Some(model));
    source::write(dom, module, source_text);
    mark(dom, model, id).unwrap();
    model
}

fn same(a: &WeakDom, a_root: Ref, b: &WeakDom, b_root: Ref) -> bool {
    use super::super::super::clipboard::detached;
    rbx_dom::diff::diff(&detached(a, &[a_root]), &detached(b, &[b_root])).is_empty()
}

#[test]
fn models_folders_and_module_scripts_can_be_shared() {
    let db = ReflectionDatabase::embedded();
    let (mut dom, workspace, _) = place();
    for (class, expected) in [
        ("Model", true),
        ("Folder", true),
        ("ModuleScript", true),
        ("Actor", true),
        ("Part", false),
        ("Script", false),
    ] {
        let reference = dom.new_instance(class, class, Some(workspace));
        assert_eq!(shareable(&dom, &db, reference), expected, "{class}");
    }
}

#[test]
fn marking_keeps_other_attributes_and_unmarking_drops_only_the_id() {
    let (mut dom, workspace, _) = place();
    let model = dom.new_instance("Model", "M", Some(workspace));
    let mut extra = std::collections::BTreeMap::new();
    extra.insert("Speed".to_owned(), Variant::Float64(3.0));
    crate::properties::attributes::write_attributes(&mut dom, model, &extra).unwrap();

    mark(&mut dom, model, "abc").unwrap();
    assert_eq!(shared_id(&dom, model).as_deref(), Some("abc"));
    let attrs = crate::properties::attributes::attributes(&dom, model);
    assert_eq!(attrs.get("Speed"), Some(&Variant::Float64(3.0)));

    assert!(unmark(&mut dom, model).unwrap());
    assert_eq!(shared_id(&dom, model), None);
    assert!(!unmark(&mut dom, model).unwrap());
    let attrs = crate::properties::attributes::attributes(&dom, model);
    assert_eq!(attrs.get("Speed"), Some(&Variant::Float64(3.0)));
}

#[test]
fn ids_are_32_hex_digits_and_distinct() {
    let ids: HashSet<String> = (0..1000).map(|_| new_id()).collect();
    assert_eq!(ids.len(), 1000);
    assert!(ids.iter().all(|id| id.len() == 32
        && id
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())));
}

#[test]
fn a_shared_instance_inside_another_travels_with_it() {
    let (mut dom, workspace, storage) = place();
    let outer = tree(&mut dom, workspace, "outer", "return 1");
    let inner = dom.new_instance("Folder", "Inner", Some(outer));
    mark(&mut dom, inner, "inner").unwrap();
    let alone = dom.new_instance("Folder", "Alone", Some(storage));
    mark(&mut dom, alone, "alone").unwrap();
    assert_eq!(
        shared_roots(&dom),
        vec![("outer".to_owned(), outer), ("alone".to_owned(), alone)]
    );
}

#[test]
fn plan_pairs_by_id_wherever_the_target_keeps_it() {
    let (mut source, workspace, _) = place();
    let shared = tree(&mut source, workspace, "t1", "return 2");
    let only_here = source.new_instance("Folder", "OnlyHere", Some(workspace));
    mark(&mut source, only_here, "lonely").unwrap();

    let (mut target, _, storage) = place();
    let folder = target.new_instance("Folder", "Props", Some(storage));
    let copy = tree(&mut target, folder, "t1", "return 1");
    target.set_name(copy, "OldTree").unwrap();
    let elsewhere = target.new_instance("Folder", "Other", Some(storage));
    mark(&mut target, elsewhere, "other").unwrap();

    let pairs = plan(&source, &target, &ReflectionDatabase::embedded());
    assert_eq!(pairs.len(), 1);
    let pair = &pairs[0];
    assert_eq!((pair.source, pair.target), (shared, copy));
    assert_eq!(pair.name, "Tree");
    assert_eq!(pair.target_path, "ReplicatedStorage.Props.OldTree");
    assert!(!pair.nodes.is_empty());
}

#[test]
fn an_identical_copy_plans_no_changes_and_apply_skips_it() {
    let (mut source, workspace, _) = place();
    tree(&mut source, workspace, "t1", "return 1");
    let (mut target, workspace_t, _) = place();
    let copy = tree(&mut target, workspace_t, "t1", "return 1");

    let pairs = plan(&source, &target, &ReflectionDatabase::embedded());
    assert_eq!(pairs.len(), 1);
    assert!(pairs[0].nodes.is_empty());
    target.take_changes();
    assert!(apply(&mut target, &source, &pairs).is_empty());
    assert!(target.take_changes().is_empty());
    assert!(target.get(copy).is_some());
}

#[test]
fn apply_makes_the_copy_match_in_place_and_leaves_the_rest_alone() {
    let (mut source, workspace, _) = place();
    let shared = tree(&mut source, workspace, "t1", "return 2");
    source.new_instance("Part", "Leaf", Some(shared));

    let (mut target, workspace_t, _) = place();
    let before = target.new_instance("Part", "Before", Some(workspace_t));
    let copy = tree(&mut target, workspace_t, "t1", "return 1");
    let after = target.new_instance("Part", "After", Some(workspace_t));

    let pairs = plan(&source, &target, &ReflectionDatabase::embedded());
    target.take_changes();
    let roots = apply(&mut target, &source, &pairs);
    assert_eq!(roots.len(), 1);
    let fresh = roots[0];
    assert!(!target.take_changes().is_empty());
    assert!(target.get(copy).is_none());
    assert_eq!(target.parent(fresh), Some(workspace_t));
    assert_eq!(
        target.get(workspace_t).unwrap().children(),
        &[before, fresh, after]
    );
    assert!(same(&source, shared, &target, fresh));
    assert_eq!(shared_id(&target, fresh).as_deref(), Some("t1"));
    assert!(plan(&source, &target, &ReflectionDatabase::embedded())
        .iter()
        .all(|pair| pair.nodes.is_empty()));
}

#[test]
fn a_synced_place_still_saves_and_reads_back_with_only_the_attribute_added() {
    let (mut source, workspace, _) = place();
    let shared = tree(&mut source, workspace, "t1", "return 2");
    let (mut target, workspace_t, _) = place();
    tree(&mut target, workspace_t, "t1", "return 1");
    let pairs = plan(&source, &target, &ReflectionDatabase::embedded());
    apply(&mut target, &source, &pairs);

    let bytes = crate::save::Format::Binary.encode(&target).unwrap();
    let read = rbx_binary::deserialize(&bytes).unwrap();
    let roots = shared_roots(&read);
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].0, "t1");
    // The writer fills in class defaults, so the copy is compared by shape
    // and by what was written, not property for property.
    let shape = |dom: &WeakDom, root: Ref| {
        let instance = dom.get(root).unwrap();
        let mut names: Vec<String> = instance
            .children()
            .iter()
            .map(|&child| dom.get(child).unwrap().name().to_owned())
            .collect();
        names.insert(0, instance.name().to_owned());
        names
    };
    assert_eq!(shape(&source, shared), shape(&read, roots[0].1));
    let module = |dom: &WeakDom, root: Ref| {
        let child = dom
            .get(root)
            .unwrap()
            .children()
            .iter()
            .copied()
            .find(|&child| dom.get(child).unwrap().class() == "ModuleScript")
            .unwrap();
        source::read(dom, child)
    };
    assert_eq!(module(&read, roots[0].1).as_deref(), Some("return 2"));
    assert_eq!(module(&source, shared).as_deref(), Some("return 2"));
    let count = |dom: &WeakDom| {
        let mut stack = dom.root_refs().to_vec();
        let mut n = 0;
        while let Some(r) = stack.pop() {
            n += 1;
            stack.extend(dom.get(r).unwrap().children());
        }
        n
    };
    assert_eq!(count(&read), count(&target));
}

#[test]
fn identities_and_stored_defaults_are_not_changes() {
    let db = ReflectionDatabase::embedded();
    let (mut source, workspace, _) = place();
    let model = tree(&mut source, workspace, "t1", "return 1");
    let (mut target, workspace_t, _) = place();
    let copy = tree(&mut target, workspace_t, "t1", "return 1");
    let default = db
        .stored_default("Model", "LevelOfDetail")
        .cloned()
        .unwrap();
    source
        .set_property(model, "LevelOfDetail", default)
        .unwrap();
    source
        .set_property(model, "UniqueId", Variant::String("a".into()))
        .unwrap();
    target
        .set_property(copy, "UniqueId", Variant::String("b".into()))
        .unwrap();
    source
        .set_property(model, "Tags", Variant::String(String::new()))
        .unwrap();
    target
        .set_property(
            copy,
            "Tags",
            Variant::Unknown {
                type_id: 0x1d,
                raw: Vec::new(),
            },
        )
        .unwrap();
    let pairs = plan(&source, &target, &db);
    assert!(pairs[0].nodes.is_empty(), "{:?}", pairs[0].nodes);
    source
        .set_property(model, "Archivable", Variant::Bool(false))
        .unwrap();
    assert!(!plan(&source, &target, &db)[0].nodes.is_empty());
}
