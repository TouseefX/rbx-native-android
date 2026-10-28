use glam::{Vec2, Vec3};
use rbx_autorig::{CageTemplate, Mesh, Part};

use super::*;

fn idx(part: Part) -> usize {
    Part::ALL.iter().position(|&p| p == part).unwrap()
}

fn triangle(at: f32) -> Mesh {
    Mesh {
        positions: vec![Vec3::new(at, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::Y],
        normals: vec![Vec3::Z; 3],
        uvs: vec![Vec2::new(at, 0.0), Vec2::X, Vec2::Y],
        triangles: vec![[0, 1, 2]],
        ..Mesh::default()
    }
}

fn templates() -> Vec<CageTemplate> {
    // Reversed, to show the merge goes by part, not by list order.
    Part::ALL
        .iter()
        .rev()
        .map(|&part| CageTemplate {
            part,
            mesh: triangle(idx(part) as f32),
            size: Vec3::splat(2.0),
            attachments: Vec::new(),
        })
        .collect()
}

fn placed() -> Vec<Placed> {
    Part::ALL
        .iter()
        .map(|&part| Placed {
            part,
            frame: Cf::at([0.0, idx(part) as f32 * 10.0, 0.0]),
            size: Vec3::new(4.0, 2.0, 2.0),
        })
        .collect()
}

#[test]
fn the_template_keeps_every_vertex_in_part_order_in_the_handles_space() {
    let handle = Cf::at([0.0, 5.0, 0.0]);
    let mesh = template(&templates(), &placed(), handle).unwrap();
    assert_eq!(mesh.positions.len(), 45);
    assert_eq!(mesh.triangles[1], [3, 4, 5]);
    // UpperTorso (index 1): its first vertex is (1, 0, 0) in the template,
    // stretched x2 along X, stood 10 studs up, read from 5 up.
    let upper = idx(Part::UpperTorso) * 3;
    assert_eq!(mesh.positions[upper], Vec3::new(2.0, 5.0, 0.0));
    assert_eq!(mesh.uvs[upper], Vec2::new(1.0, 0.0));
    assert_eq!(mesh.normals[upper], Vec3::Z);
}

#[test]
fn a_missing_part_is_said() {
    let mut few = placed();
    few.retain(|p| p.part != Part::Head);
    let err = template(&templates(), &few, Cf::at([0.0; 3])).unwrap_err();
    assert!(err.contains("Head"), "{err}");
}

#[test]
fn the_item_mesh_is_centred_and_sized_like_its_handle() {
    let bytes = rbx_autorig::mesh_v2(&triangle(0.0));
    let mesh = item_mesh(&bytes, [2.0, 4.0, 1.0]).unwrap();
    let (centre, size) = bounds(&mesh);
    assert_eq!(centre, [0.0, 0.0, 0.0]);
    assert_eq!([size[0], size[1]], [2.0, 4.0]);
    assert_eq!(mesh.triangles, vec![[0, 1, 2]]);
}

#[test]
fn a_region_centre_is_its_part_in_the_handles_space() {
    let at = Region::LeftLeg
        .centre(&placed(), Cf::at([0.0, 10.0, 0.0]))
        .unwrap();
    let want = idx(Part::LeftLowerLeg) as f32 * 10.0 - 10.0;
    assert_eq!(at, Vec3::new(0.0, want, 0.0));
}

#[test]
fn geometry_carries_the_topology_the_validator_compares() {
    let g = geometry(&triangle(0.5));
    assert_eq!(g.positions.len(), 3);
    assert_eq!(g.uvs[0], [0.5, 0.0]);
    assert_eq!(g.triangles, vec![[0, 1, 2]]);
}
