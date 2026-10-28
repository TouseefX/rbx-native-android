use rbx_dom::{Content, Variant, WeakDom};
use rbx_ugc::AssetType;

use super::super::state::{Mode, Placement, Settings};
use super::*;
use crate::shell::rig::{BodyScale, BodyShape, JointStyle, RigType};

fn place() -> (WeakDom, Ref, Ref) {
    let mut dom = WeakDom::new();
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    let bow = dom.new_instance("MeshPart", "Bow", Some(workspace));
    dom.set_property(
        bow,
        "MeshId",
        Variant::Content(Content::Uri("rbxassetid://6874004992".into())),
    )
    .unwrap();
    (dom, workspace, bow)
}

fn options() -> RigOptions {
    RigOptions::new(
        RigType::R15,
        BodyShape::Masculine,
        BodyScale::Classic,
        JointStyle::Motor6D,
    )
}

fn close(a: Cf, b: Cf) -> bool {
    a.p.iter().zip(b.p).all(|(x, y)| (x - y).abs() < 1e-4)
        && a.r.iter().zip(b.r).all(|(x, y)| (x - y).abs() < 1e-4)
}

#[test]
fn a_rigid_copy_sits_on_its_attachment_and_leaves_the_source_alone() {
    let (mut dom, workspace, bow) = place();
    let (folder, rig) = build(&mut dom, workspace, &options());
    let settings = Settings::default();
    let acc = wear(&mut dom, folder, rig, bow, &settings, None).unwrap();
    assert_eq!(dom.parent(acc), Some(folder));
    assert_eq!(dom.parent(bow), Some(workspace));
    let handle = find(&dom, acc, "MeshPart", "Handle").unwrap();
    let (_, hat) = attachment(&dom, rig, "HatAttachment").unwrap();
    assert!(close(cframe(&dom, handle, "CFrame"), hat));
    let weld = find(&dom, acc, "Weld", "AccessoryWeld").unwrap();
    let props = dom.get(weld).unwrap().properties();
    assert_eq!(props.get("Part0"), Some(&Variant::Ref(handle)));
    let head = find(&dom, rig, "MeshPart", "Head").unwrap();
    assert_eq!(props.get("Part1"), Some(&Variant::Ref(head)));
}

#[test]
fn the_placement_moves_the_handle_off_the_attachment() {
    let (dom, workspace, _) = place();
    let mut dom = dom;
    let (_, rig) = build(&mut dom, workspace, &options());
    let settings = Settings {
        placement: Placement {
            offset: [0.0, 0.5, 0.0],
            ..Placement::default()
        },
        ..Settings::default()
    };
    let (handle, body, _) = frames(&dom, rig, &settings).unwrap();
    assert!(close(handle, body.mul(&Cf::at([0.0, 0.5, 0.0]))));
}

#[test]
fn a_layered_handle_sits_on_its_body_part() {
    let (mut dom, workspace, _) = place();
    let (_, rig) = build(&mut dom, workspace, &options());
    let mut settings = Settings::default();
    settings.set_mode(Mode::Clothing);
    settings.set_asset(AssetType::Pants);
    let (handle, body, part_ref) = frames(&dom, rig, &settings).unwrap();
    let (lower, at) = part(&dom, rig, "LowerTorso").unwrap();
    assert_eq!(part_ref, lower);
    assert!(close(handle, at));
    let (_, waist) = attachment(&dom, rig, "WaistCenterAttachment").unwrap();
    assert!(close(body, waist));
}

#[test]
fn the_size_box_stands_on_the_attachment_and_rebuilding_clears_the_old_preview() {
    let (mut dom, workspace, _) = place();
    let (folder, rig) = build(&mut dom, workspace, &options());
    let settings = Settings::default();
    let (fit_box, outline) = size_box(&mut dom, folder, rig, &settings, false).unwrap();
    assert_eq!(
        dom.get(outline).unwrap().properties().get("Adornee"),
        Some(&Variant::Ref(fit_box))
    );
    assert_eq!(size(&dom, fit_box), [3.0, 4.0, 3.0]);
    let (again, _) = build(&mut dom, workspace, &options());
    assert!(dom.get(folder).is_none());
    let previews = dom
        .get(workspace)
        .unwrap()
        .children()
        .iter()
        .filter(|&&c| dom.get(c).unwrap().name() == FOLDER)
        .count();
    assert_eq!(previews, 1);
    clear(&mut dom, workspace);
    assert!(dom.get(again).is_none());
}

fn has_preview(dom: &WeakDom, workspace: Ref) -> bool {
    dom.get(workspace)
        .unwrap()
        .children()
        .iter()
        .any(|&c| dom.get(c).unwrap().name() == FOLDER)
}

/// What `Shell::push_history` and `Shell::undo`/`redo` hand the history: a
/// copy of the place with the preview stripped.
fn step(dom: &WeakDom) -> (WeakDom, Vec<Change>) {
    let mut copy = dom.clone();
    let gone = strip(&mut copy);
    (copy, gone)
}

#[test]
fn no_undo_or_redo_brings_the_preview_back() {
    let (mut dom, workspace, bow) = place();
    let (folder, rig) = build(&mut dom, workspace, &options());
    wear(&mut dom, folder, rig, bow, &Settings::default(), None).unwrap();
    // An edit made while the preview is up is one step.
    let mut history = crate::history::History::new(crate::history::DEFAULT_CAP);
    history.push(step(&dom).0);
    dom.set_property(bow, "Transparency", Variant::Float32(0.5))
        .unwrap();

    let (current, gone) = step(&dom);
    assert!(gone.contains(&Change::Removed(folder)));
    let (undone, _) = history.undo(current).unwrap();
    assert!(!has_preview(&undone, workspace));
    assert!(undone
        .get(bow)
        .unwrap()
        .properties()
        .get("Transparency")
        .is_none());

    let (redone, _) = history.redo(step(&undone).0).unwrap();
    assert!(!has_preview(&redone, workspace));
    assert_eq!(
        redone.get(bow).unwrap().properties().get("Transparency"),
        Some(&Variant::Float32(0.5))
    );
}

#[test]
fn undoing_generate_takes_the_accessory_out_and_redo_puts_it_back() {
    let (mut dom, workspace, bow) = place();
    let (folder, rig) = build(&mut dom, workspace, &options());
    let settings = Settings::default();
    wear(&mut dom, folder, rig, bow, &settings, None).unwrap();
    let (out, _) = output(&dom, rig, &settings, None).unwrap();
    // `FittingWindow::generate`: the preview goes, then one step.
    clear(&mut dom, workspace);
    let mut history = crate::history::History::new(crate::history::DEFAULT_CAP);
    history.push(step(&dom).0);
    let accessory = generate::generate(&mut dom, bow, workspace, &out);
    assert_eq!(dom.parent(bow), Some(accessory));

    let (undone, _) = history.undo(step(&dom).0).unwrap();
    assert!(undone.get(accessory).is_none());
    assert_eq!(undone.parent(bow), Some(workspace));
    assert!(!has_preview(&undone, workspace));

    let (redone, _) = history.redo(step(&undone).0).unwrap();
    assert_eq!(redone.parent(bow), Some(accessory));
    assert!(!has_preview(&redone, workspace));
}
