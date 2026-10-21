use super::*;

const BLUE: Rgb = (0x4F, 0x8C, 0xFF);

/// Workspace with a Model holding a Part, plus a loose Part.
fn place() -> (WeakDom, Ref, Ref, Ref) {
    let mut dom = WeakDom::new();
    let ws = dom.new_instance("Workspace", "Workspace", None);
    let model = dom.new_instance("Model", "Model", Some(ws));
    let part = dom.new_instance("Part", "Part", Some(model));
    let loose = dom.new_instance("Part", "Loose", Some(ws));
    (dom, model, part, loose)
}

#[test]
fn a_layer_is_added_renamed_recolored_and_deleted() {
    let (mut dom, ..) = place();
    let id = add_layer(&mut dom, "Walls", BLUE).unwrap();
    assert_eq!(
        layers(&dom),
        vec![Layer {
            id: id.clone(),
            name: "Walls".into(),
            color: BLUE
        }]
    );
    rename_layer(&mut dom, &id, "Big walls").unwrap();
    recolor_layer(&mut dom, &id, (1, 2, 3)).unwrap();
    let layer = &layers(&dom)[0];
    assert_eq!((layer.name.as_str(), layer.color), ("Big walls", (1, 2, 3)));
    delete_layer(&mut dom, &id).unwrap();
    assert!(layers(&dom).is_empty());
}

#[test]
fn ids_count_up_and_a_deleted_id_is_reused() {
    let (mut dom, ..) = place();
    assert_eq!(add_layer(&mut dom, "A", BLUE).unwrap(), "L1");
    assert_eq!(add_layer(&mut dom, "B", BLUE).unwrap(), "L2");
    delete_layer(&mut dom, "L1").unwrap();
    assert_eq!(add_layer(&mut dom, "C", BLUE).unwrap(), "L1");
}

#[test]
fn blank_names_and_unknown_ids_are_refused() {
    let (mut dom, ..) = place();
    assert!(add_layer(&mut dom, "  ", BLUE).is_err());
    assert!(rename_layer(&mut dom, "L9", "x").is_err());
    assert!(recolor_layer(&mut dom, "L9", BLUE).is_err());
    assert!(delete_layer(&mut dom, "L9").is_err());
}

#[test]
fn a_malformed_definition_is_skipped() {
    let (mut dom, ..) = place();
    let ws = workspace(&dom).unwrap();
    put_attribute(
        &mut dom,
        ws,
        "RbxNativeLayerDef_L1",
        Some(Variant::String("zzzzzz x".into())),
    )
    .unwrap();
    put_attribute(
        &mut dom,
        ws,
        "RbxNativeLayerDef_L2",
        Some(Variant::Float64(1.0)),
    )
    .unwrap();
    assert!(layers(&dom).is_empty());
    assert_eq!(add_layer(&mut dom, "Ok", BLUE).unwrap(), "L3");
    assert_eq!(layers(&dom).len(), 1);
}

#[test]
fn names_with_spaces_round_trip() {
    let (mut dom, ..) = place();
    add_layer(&mut dom, "Far  away hills", (0, 255, 16)).unwrap();
    let layer = &layers(&dom)[0];
    assert_eq!(
        (layer.name.as_str(), layer.color),
        ("Far  away hills", (0, 255, 16))
    );
}

#[test]
fn assigning_and_unassigning_changes_own_layer() {
    let (mut dom, _, part, _) = place();
    let id = add_layer(&mut dom, "A", BLUE).unwrap();
    assign(&mut dom, &[part], Some(&id)).unwrap();
    assert_eq!(own_layer(&dom, part), Some(id));
    assign(&mut dom, &[part], None).unwrap();
    assert_eq!(own_layer(&dom, part), None);
}

#[test]
fn assign_refuses_unknown_layers_services_and_workspace() {
    let (mut dom, model, ..) = place();
    let ws = workspace(&dom).unwrap();
    let id = add_layer(&mut dom, "A", BLUE).unwrap();
    assert!(assign(&mut dom, &[model], Some("L9")).is_err());
    assert!(assign(&mut dom, &[ws], Some(&id)).is_err());
    assert!(assign(&mut dom, &[model, ws], Some(&id)).is_err());
    assert_eq!(own_layer(&dom, model), None);
}

#[test]
fn children_inherit_their_ancestors_layer() {
    let (mut dom, model, part, loose) = place();
    let id = add_layer(&mut dom, "A", BLUE).unwrap();
    assign(&mut dom, &[model], Some(&id)).unwrap();
    assert_eq!(layer_of(&dom, part), Some(id));
    assert_eq!(own_layer(&dom, part), None);
    assert_eq!(layer_of(&dom, loose), None);
}

#[test]
fn deleting_a_layer_clears_its_members() {
    let (mut dom, model, part, _) = place();
    let id = add_layer(&mut dom, "A", BLUE).unwrap();
    assign(&mut dom, &[model, part], Some(&id)).unwrap();
    delete_layer(&mut dom, &id).unwrap();
    assert_eq!(own_layer(&dom, model), None);
    assert_eq!(own_layer(&dom, part), None);
}

#[test]
fn counts_and_members_follow_tree_order() {
    let (mut dom, model, part, loose) = place();
    let a = add_layer(&mut dom, "A", BLUE).unwrap();
    let b = add_layer(&mut dom, "B", BLUE).unwrap();
    assign(&mut dom, &[loose, part], Some(&a)).unwrap();
    assign(&mut dom, &[model], Some(&b)).unwrap();
    assert_eq!(members(&dom, &a), vec![part, loose]);
    let totals = counts(&dom);
    assert_eq!((totals[&a], totals[&b]), (2, 1));
}

#[test]
fn solo_hides_every_layer_that_is_not_soloed() {
    let mut view = LayerView::default();
    assert!(view.is_shown("L1"));
    view.hidden.insert("L1".into());
    assert!(!view.is_shown("L1"));
    view.solo.insert("L2".into());
    assert!(view.is_shown("L2"));
    assert!(!view.is_shown("L3"));
    view.hidden.insert("L2".into());
    assert!(!view.is_shown("L2"));
}

#[test]
fn hidden_roots_lists_members_of_hidden_layers_only() {
    let (mut dom, model, _, loose) = place();
    let a = add_layer(&mut dom, "A", BLUE).unwrap();
    let b = add_layer(&mut dom, "B", BLUE).unwrap();
    assign(&mut dom, &[model], Some(&a)).unwrap();
    assign(&mut dom, &[loose], Some(&b)).unwrap();
    let mut view = LayerView::default();
    view.hidden.insert(a);
    assert_eq!(view.hidden_roots(&dom), vec![model]);
    view.hidden.insert(b);
    assert_eq!(view.hidden_roots(&dom), vec![model, loose]);
}

#[test]
fn isolate_roots_needs_a_defined_layer() {
    let (mut dom, model, ..) = place();
    let a = add_layer(&mut dom, "A", BLUE).unwrap();
    assign(&mut dom, &[model], Some(&a)).unwrap();
    let mut view = LayerView::default();
    assert_eq!(view.isolate_roots(&dom), None);
    view.isolate = Some("L9".into());
    assert_eq!(view.isolate_roots(&dom), None);
    view.isolate = Some(a);
    assert_eq!(view.isolate_roots(&dom), Some(vec![model]));
}

#[test]
fn locking_and_dimming_apply_to_descendants() {
    let (mut dom, model, part, loose) = place();
    let a = add_layer(&mut dom, "A", BLUE).unwrap();
    assign(&mut dom, &[model], Some(&a)).unwrap();
    let mut view = LayerView::default();
    view.locked.insert(a.clone());
    assert!(view.is_locked(&dom, part));
    assert!(view.dims(&dom, part));
    assert!(!view.is_locked(&dom, loose));
    view.locked.clear();
    view.hidden.insert(a);
    assert!(!view.is_locked(&dom, part));
    assert!(view.dims(&dom, part));
}

#[test]
fn prune_drops_ids_that_are_no_longer_defined() {
    let mut view = LayerView::default();
    view.hidden.extend(["L1".to_owned(), "L2".to_owned()]);
    view.locked.insert("L2".into());
    view.solo.insert("L2".into());
    view.isolate = Some("L2".into());
    view.prune(&[Layer {
        id: "L1".into(),
        name: "A".into(),
        color: BLUE,
    }]);
    assert_eq!(view.hidden.len(), 1);
    assert!(view.locked.is_empty() && view.solo.is_empty());
    assert_eq!(view.isolate, None);
}

#[test]
fn layer_views_store_per_place_and_drop_default_entries() {
    let mut store = LayerViews::default();
    let path = Path::new("/tmp/a.rbxl");
    let mut view = LayerView::default();
    view.hidden.insert("L1".into());
    store.set(path, view.clone());
    assert_eq!(store.get(path), view);
    assert_eq!(store.get(Path::new("/tmp/b.rbxl")), LayerView::default());
    store.set(path, LayerView::default());
    assert_eq!(store, LayerViews::default());
}

#[test]
fn layer_edits_only_write_attributes() {
    let (mut dom, model, part, _) = place();
    dom.set_property(part, "Transparency", Variant::Float32(0.25))
        .unwrap();
    dom.set_property(part, "Locked", Variant::Bool(true))
        .unwrap();
    let before = dom.get(part).unwrap().properties().len();
    let id = add_layer(&mut dom, "A", BLUE).unwrap();
    assign(&mut dom, &[part], Some(&id)).unwrap();
    let instance = dom.get(part).unwrap();
    assert_eq!(
        instance.properties().get("Transparency"),
        Some(&Variant::Float32(0.25))
    );
    assert_eq!(
        instance.properties().get("Locked"),
        Some(&Variant::Bool(true))
    );
    assert_eq!(instance.properties().len(), before + 1);
    assert_eq!(dom.parent(part), Some(model));
}

/// Hiding, locking, soloing and isolating are a way of looking: they write
/// nothing to the place, so the undo history (which records the DOM's
/// change log) never sees them and `Transparency`/`Locked`/`Parent` stay put.
#[test]
fn view_toggles_leave_the_place_and_its_change_log_alone() {
    let (mut dom, _, part, _) = place();
    let id = add_layer(&mut dom, "Props", PALETTE[0]).unwrap();
    assign(&mut dom, &[part], Some(&id)).unwrap();
    dom.take_changes();
    let mutations = dom.mutations();
    let before = dom.get(part).unwrap().properties().clone();
    let parent = dom.parent(part);

    let view = LayerView {
        hidden: [id.clone()].into(),
        locked: [id.clone()].into(),
        solo: [id.clone()].into(),
        isolate: Some(id.clone()),
    };
    assert_eq!(view.hidden_roots(&dom), vec![part]);
    assert_eq!(view.isolate_roots(&dom), Some(vec![part]));
    assert!(view.is_locked(&dom, part));
    assert!(view.dims(&dom, part));

    assert_eq!(dom.mutations(), mutations);
    assert!(dom.take_changes().is_empty());
    assert_eq!(dom.get(part).unwrap().properties(), &before);
    assert_eq!(dom.parent(part), parent);
}
