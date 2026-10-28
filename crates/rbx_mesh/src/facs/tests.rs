use super::*;
use crate::skin::{Bone, Envelope, Skin, Subset};

fn names(list: &[&str]) -> Vec<u8> {
    list.iter().flat_map(|n| n.bytes().chain([0])).collect()
}

/// A version 1 matrix of `values` (`rows × cols` row-major).
fn raw(rows: u32, cols: u32, values: &[f32]) -> Vec<u8> {
    let mut out = 1u16.to_le_bytes().to_vec();
    out.extend(rows.to_le_bytes());
    out.extend(cols.to_le_bytes());
    out.extend(values.iter().flat_map(|v| v.to_le_bytes()));
    out
}

/// A version 2 matrix: every value quantized between `min` and `max`.
fn quantized(rows: u32, cols: u32, min: f32, max: f32, steps: &[u16]) -> Vec<u8> {
    let mut out = 2u16.to_le_bytes().to_vec();
    out.extend(rows.to_le_bytes());
    out.extend(cols.to_le_bytes());
    out.extend(min.to_le_bytes());
    out.extend(max.to_le_bytes());
    out.extend(steps.iter().flat_map(|v| v.to_le_bytes()));
    out
}

/// Bones `jaw`, `lip`; controls `c_JD`, `c_PK`; one two-control corrective.
/// `JawDrop` lowers the jaw 0.5 and turns it 90° about X; the corrective
/// moves the lip 1 in X.
pub(crate) fn block() -> Vec<u8> {
    let bones = names(&["jaw", "lip"]);
    let controls = names(&["c_JD", "c_PK"]);
    let zero = [0.0; 6];
    let mut transforms = raw(2, 3, &[0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    transforms.extend(raw(2, 3, &[-0.5, 0.0, 0.0, 0.0, 0.0, 0.0]));
    transforms.extend(raw(2, 3, &zero));
    transforms.extend(raw(2, 3, &[90.0, 0.0, 0.0, 0.0, 0.0, 0.0]));
    transforms.extend(raw(2, 3, &zero));
    transforms.extend(quantized(2, 3, 0.0, 1.0, &[0; 6]));
    let mut out = Vec::new();
    out.extend((bones.len() as u32).to_le_bytes());
    out.extend((controls.len() as u32).to_le_bytes());
    out.extend((transforms.len() as u64).to_le_bytes());
    out.extend(4u32.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(bones);
    out.extend(controls);
    out.extend(transforms);
    out.extend([0, 0, 1, 0]);
    out
}

#[test]
fn reads_names_matrices_and_correctives() {
    let facs = read(&block()).unwrap();
    assert_eq!(facs.bones, ["jaw", "lip"]);
    assert_eq!(facs.controls, ["c_JD", "c_PK"]);
    assert_eq!(facs.correctives, vec![vec![0, 1]]);
    assert!(facs.has("JawDrop") && facs.has("Pucker") && !facs.has("JawLeft"));
    assert!(facs.is_active("JawDrop"));
    // Pucker alone moves nothing: only its corrective with JawDrop does.
    assert!(!facs.is_active("Pucker"));
}

#[test]
fn a_corrective_weighs_the_product_of_its_controls() {
    let facs = read(&block()).unwrap();
    let pose = facs.pose(&|p| match p {
        "JawDrop" => 0.5,
        "Pucker" => 0.5,
        _ => 0.0,
    });
    assert_eq!(pose[0].position, [0.0, -0.25, 0.0]);
    assert_eq!(pose[0].rotation, [45.0, 0.0, 0.0]);
    assert_eq!(pose[1].position, [0.25, 0.0, 0.0]);
}

#[test]
fn a_block_that_does_not_balance_is_refused() {
    let mut bytes = block();
    bytes.push(0);
    assert!(read(&bytes).is_err());
    assert!(read(&block()[..20]).is_err());
}

#[test]
fn every_short_name_maps_to_one_property() {
    assert_eq!(control_property("c_JD"), "JawDrop");
    assert_eq!(control_property("l_EC"), "LeftEyeClosed");
    assert_eq!(control_property("JawDrop"), "JawDrop");
    let mut shorts: Vec<_> = CONTROL_NAMES.iter().map(|(_, s)| *s).collect();
    shorts.sort();
    shorts.dedup();
    assert_eq!(shorts.len(), 50);
}

fn bone(name: &str, parent: Option<u16>, position: [f32; 3]) -> Bone {
    Bone {
        name: name.into(),
        parent,
        lod_parent: parent,
        culling: 0.0,
        rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        position,
    }
}

/// `jaw` at the origin with `lip` one stud in front (-Z) as its child; vertex
/// 0 follows the jaw, vertex 1 the lip.
fn skin() -> Skin {
    let envelope = |slot| Envelope {
        slots: [slot, 0, 0, 0],
        weights: [255, 0, 0, 0],
    };
    Skin {
        facs: Some(read(&block()).unwrap()),
        ..Skin::new(
            vec![
                bone("jaw", None, [0.0, 0.0, 0.0]),
                bone("lip", Some(0), [0.0, 0.0, -1.0]),
            ],
            vec![Subset {
                faces: 0..0,
                vertices: 0..2,
                bones: vec![0, 1],
            }],
            vec![envelope(0), envelope(1)],
        )
    }
}

#[test]
fn a_child_bone_rides_on_its_parents_offset() {
    let skin = skin();
    let palette = skin
        .facs_palette(&|p| if p == "JawDrop" { 1.0 } else { 0.0 })
        .unwrap();
    let moved = deform_positions(&skin, &[[0.0, 0.0, 0.0], [0.0, 0.0, -1.0]], &palette);
    let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
    assert!(close(moved[0], [0.0, -0.5, 0.0]), "{:?}", moved[0]);
    // Turned 90° about X around the dropped jaw: -Z swings to +Y.
    assert!(close(moved[1], [0.0, 0.5, 0.0]), "{:?}", moved[1]);
}

#[test]
fn at_rest_the_palette_is_identity() {
    let skin = skin();
    let palette = skin.facs_palette(&|_| 0.0).unwrap();
    for m in palette {
        assert_eq!(
            m,
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]
        );
    }
    let without = Skin { facs: None, ..skin };
    assert!(without.facs_palette(&|_| 1.0).is_none());
}
