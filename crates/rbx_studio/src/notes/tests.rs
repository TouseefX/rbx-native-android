use super::*;

fn cframe(x: f32, y: f32, z: f32) -> Variant {
    Variant::CFrame(rbx_dom::CFrameData {
        position: Vector3Data { x, y, z },
        rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    })
}

fn place() -> (WeakDom, Ref) {
    let mut dom = WeakDom::new();
    let root = dom.new_instance("Workspace", "Workspace", None);
    let part = dom.new_instance("Part", "Part", Some(root));
    dom.set_property(part, "CFrame", cframe(10.0, 0.0, 0.0))
        .unwrap();
    (dom, part)
}

const WHITE: Rgb = (255, 255, 255);

#[test]
fn an_unpinned_note_stands_at_its_world_position() {
    let (mut dom, _) = place();
    let id = add_note(&mut dom, "hi", "me", (255, 128, 0), [1.0, 2.0, 3.0], None).unwrap();
    let all = notes(&dom);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].referent, id);
    assert_eq!(all[0].text, "hi");
    assert_eq!(all[0].author, "me");
    assert_eq!(all[0].color, (255, 128, 0));
    assert_eq!(all[0].target, None);
    assert_eq!(all[0].position, [1.0, 2.0, 3.0]);
}

#[test]
fn a_pinned_note_keeps_its_world_position_and_its_target() {
    let (mut dom, part) = place();
    add_note(&mut dom, "x", "me", WHITE, [11.0, 1.0, 0.0], Some(part)).unwrap();
    let note = &notes(&dom)[0];
    assert_eq!(note.target, Some(part));
    assert_eq!(note.position, [11.0, 1.0, 0.0]);
}

#[test]
fn the_folder_and_server_storage_are_created_once() {
    let (mut dom, _) = place();
    add_note(&mut dom, "a", "me", WHITE, [0.0; 3], None).unwrap();
    add_note(&mut dom, "b", "me", WHITE, [0.0; 3], None).unwrap();
    let storages = dom
        .root_refs()
        .iter()
        .filter(|r| dom.get(**r).unwrap().class() == "ServerStorage")
        .count();
    assert_eq!(storages, 1);
    assert_eq!(notes(&dom).len(), 2);
    assert!(folder(&dom).is_some());
}

#[test]
fn a_pinned_note_follows_its_target_when_it_moves() {
    let (mut dom, part) = place();
    add_note(&mut dom, "x", "me", WHITE, [11.0, 0.0, 0.0], Some(part)).unwrap();
    dom.set_property(part, "CFrame", cframe(20.0, 0.0, 0.0))
        .unwrap();
    assert_eq!(notes(&dom)[0].position, [21.0, 0.0, 0.0]);
}

#[test]
fn a_note_whose_target_is_removed_falls_back_to_its_offset() {
    let (mut dom, part) = place();
    add_note(&mut dom, "x", "me", WHITE, [11.0, 0.0, 0.0], Some(part)).unwrap();
    dom.remove(part);
    let note = &notes(&dom)[0];
    assert_eq!(note.target, None);
    assert_eq!(note.position, [1.0, 0.0, 0.0]);
}

#[test]
fn set_text_rewrites_the_text() {
    let (mut dom, _) = place();
    let id = add_note(&mut dom, "old", "me", WHITE, [0.0; 3], None).unwrap();
    set_text(&mut dom, id, "new").unwrap();
    assert_eq!(notes(&dom)[0].text, "new");
}

#[test]
fn blank_text_is_refused_and_the_old_text_kept() {
    let (mut dom, _) = place();
    let id = add_note(&mut dom, DEFAULT_TEXT, "me", WHITE, [0.0; 3], None).unwrap();
    assert!(set_text(&mut dom, id, "  ").is_err());
    assert_eq!(notes(&dom)[0].text, DEFAULT_TEXT);
    set_text(&mut dom, id, " Fix the door ").unwrap();
    assert_eq!(notes(&dom)[0].text, "Fix the door");
}

#[test]
fn remove_note_deletes_it() {
    let (mut dom, _) = place();
    let id = add_note(&mut dom, "a", "me", WHITE, [0.0; 3], None).unwrap();
    remove_note(&mut dom, id).unwrap();
    assert!(notes(&dom).is_empty());
}

#[test]
fn children_that_are_not_notes_are_ignored() {
    let (mut dom, _) = place();
    add_note(&mut dom, "a", "me", WHITE, [0.0; 3], None).unwrap();
    let folder = folder(&dom).unwrap();
    dom.new_instance(NOTE_CLASS, "Plain", Some(folder));
    dom.new_instance("Folder", "Other", Some(folder));
    assert_eq!(notes(&dom).len(), 1);
}

#[test]
fn editing_or_removing_a_non_note_is_refused() {
    let (mut dom, part) = place();
    assert!(set_text(&mut dom, part, "x").is_err());
    assert!(remove_note(&mut dom, part).is_err());
    assert!(dom.get(part).is_some());
}

#[test]
fn pinning_to_a_missing_instance_is_refused() {
    let (mut dom, part) = place();
    dom.remove(part);
    assert!(add_note(&mut dom, "x", "me", WHITE, [0.0; 3], Some(part)).is_err());
}

#[test]
fn a_model_anchors_at_its_pivot_else_its_first_descendant_with_a_cframe() {
    let (mut dom, part) = place();
    let root = dom.parent(part).unwrap();
    let model = dom.new_instance("Model", "M", Some(root));
    assert_eq!(anchor(&dom, model), None);
    let inner = dom.new_instance("Part", "P", Some(model));
    dom.set_property(inner, "CFrame", cframe(4.0, 5.0, 6.0))
        .unwrap();
    assert_eq!(anchor(&dom, model), Some([4.0, 5.0, 6.0]));
    dom.set_property(model, "WorldPivot", cframe(1.0, 1.0, 1.0))
        .unwrap();
    assert_eq!(anchor(&dom, model), Some([1.0, 1.0, 1.0]));
}

#[test]
fn a_part_is_pinned_at_the_top_of_its_box_a_model_at_its_anchor() {
    let (mut dom, part) = place();
    assert_eq!(pin_point(&dom, part), Some([10.0, 0.0, 0.0]));
    // Parts store their size as `size`, the name in the place file.
    dom.set_property(
        part,
        "size",
        Variant::Vector3(Vector3Data {
            x: 4.0,
            y: 16.0,
            z: 2.0,
        }),
    )
    .unwrap();
    assert_eq!(pin_point(&dom, part), Some([10.0, 8.0, 0.0]));
    // Lying on its side (90 degrees about Z), its X size is what stands up.
    dom.set_property(
        part,
        "CFrame",
        Variant::CFrame(rbx_dom::CFrameData {
            position: Vector3Data {
                x: 10.0,
                y: 0.0,
                z: 0.0,
            },
            rotation: [0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        }),
    )
    .unwrap();
    assert_eq!(pin_point(&dom, part), Some([10.0, 2.0, 0.0]));
    let root = dom.parent(part).unwrap();
    let model = dom.new_instance("Model", "M", Some(root));
    dom.set_property(model, "WorldPivot", cframe(1.0, 1.0, 1.0))
        .unwrap();
    assert_eq!(pin_point(&dom, model), Some([1.0, 1.0, 1.0]));
}
