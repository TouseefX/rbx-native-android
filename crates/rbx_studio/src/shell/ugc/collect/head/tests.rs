use rbx_dom::{CFrameData, Content, Variant, Vector3Data, WeakDom};
use rbx_reflection::ReflectionDatabase;
use rbx_ugc::head::HeadRule;
use rbx_ugc::Rule;

use super::{head_part, head_plan, resolve_head};

/// A `version 2.00` mesh of one triangle spanning a 2 stud cube.
fn mesh_bytes() -> Vec<u8> {
    let mut body = Vec::from(b"version 2.00\n".as_slice());
    body.extend_from_slice(&12u16.to_le_bytes());
    body.extend_from_slice(&[40, 12]);
    body.extend_from_slice(&3u32.to_le_bytes());
    body.extend_from_slice(&1u32.to_le_bytes());
    for corner in [[-1.0f32; 3], [1.0; 3], [0.0; 3]] {
        for c in corner {
            body.extend_from_slice(&c.to_le_bytes());
        }
        body.extend(std::iter::repeat_n(0u8, 28));
    }
    for index in 0u32..3 {
        body.extend_from_slice(&index.to_le_bytes());
    }
    body
}

fn fetch(id: u64) -> Result<Vec<u8>, String> {
    match id {
        9 => Err("offline".into()),
        _ => Ok(mesh_bytes()),
    }
}

fn uri(id: u64) -> Variant {
    Variant::Content(Content::Uri(format!("rbxassetid://{id}")))
}

/// A character `Model` whose `Head` is 2 studs, with a `FaceControls` when
/// `face` and a `WrapTarget` cage lifted half a stud.
fn character(mesh: u64, face: bool) -> (WeakDom, rbx_dom::Ref, rbx_dom::Ref) {
    let mut dom = WeakDom::new();
    let model = dom.new_instance("Model", "Player", None);
    let head = dom.new_instance("MeshPart", "Head", Some(model));
    dom.set_property(head, "MeshId", uri(mesh)).unwrap();
    let two = Vector3Data {
        x: 2.0,
        y: 2.0,
        z: 2.0,
    };
    dom.set_property(head, "size", Variant::Vector3(two))
        .unwrap();
    if face {
        dom.new_instance("FaceControls", "FaceControls", Some(head));
    }
    let wrap = dom.new_instance("WrapTarget", "HeadWrapTarget", Some(head));
    dom.set_property(wrap, "CageMeshId", uri(5)).unwrap();
    let lifted = CFrameData {
        position: Vector3Data {
            x: 0.0,
            y: 0.5,
            z: 0.0,
        },
        rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    };
    dom.set_property(wrap, "CageOrigin", Variant::CFrame(lifted))
        .unwrap();
    (dom, model, head)
}

#[test]
fn a_model_previews_its_head_part() {
    let (dom, model, head) = character(1, true);
    assert_eq!(head_part(&dom, model), Some(head));
    assert_eq!(head_part(&dom, head), Some(head));
    let database = ReflectionDatabase::embedded();
    let plan = head_plan(&dom, &database, model).unwrap();
    assert_eq!(plan.part, head);
    assert!(plan.face_controls);
    assert_eq!(plan.mesh, Some(1));
    assert_eq!(plan.cage.map(|c| c.0), Some(5));
}

#[test]
fn the_cage_lands_where_its_origin_puts_it() {
    let (dom, model, _) = character(1, true);
    let plan = head_plan(&dom, &ReflectionDatabase::embedded(), model).unwrap();
    let (head, mesh) = resolve_head(&plan, &fetch);
    assert!(mesh.is_some());
    let cage = head.cage.expect("cage fetched");
    assert_eq!(cage.positions[1], [1.0, 1.5, 1.0]);
}

#[test]
fn a_head_without_facs_fails_the_pose_rule() {
    let (dom, model, _) = character(1, false);
    let plan = head_plan(&dom, &ReflectionDatabase::embedded(), model).unwrap();
    let (head, _) = resolve_head(&plan, &fetch);
    assert!(head.facs.is_none());
    let rules: Vec<Rule> = rbx_ugc::head::check(&head)
        .into_iter()
        .map(|f| f.rule)
        .collect();
    assert!(rules.contains(&Rule::Head(HeadRule::FaceControls)));
    assert!(rules.contains(&Rule::Head(HeadRule::FacsPoses)));
    assert!(
        rules.contains(&Rule::Head(HeadRule::Schema)),
        "no AvatarPartScaleType"
    );
}

#[test]
fn a_box_head_saved_by_studio_reads_as_box() {
    let (mut dom, model, head) = character(1, true);
    // The binary reader keeps this BinaryString as raw bytes.
    let marker = Variant::Unknown {
        type_id: 28,
        raw: b"CSGPHS\0\0\0\0BLOCK".to_vec(),
    };
    dom.set_property(head, "PhysicalConfigData", marker)
        .unwrap();
    // What the editor reads back for the unsaved enum: its default.
    dom.set_property(head, "CollisionFidelity", Variant::Enum(0))
        .unwrap();
    let plan = head_plan(&dom, &ReflectionDatabase::embedded(), model).unwrap();
    let (resolved, _) = resolve_head(&plan, &fetch);
    assert_eq!(resolved.collision_fidelity, "Box");
    assert!(!resolved.scale_type);
}

#[test]
fn an_offline_mesh_leaves_nothing_to_preview() {
    let (dom, model, _) = character(9, true);
    let plan = head_plan(&dom, &ReflectionDatabase::embedded(), model).unwrap();
    let (head, mesh) = resolve_head(&plan, &fetch);
    assert!(mesh.is_none() && head.mesh.is_none() && head.facs.is_none());
}
