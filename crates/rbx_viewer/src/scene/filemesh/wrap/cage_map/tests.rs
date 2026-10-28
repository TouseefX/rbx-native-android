use super::*;

/// An 8 x 8 sheet in the XY plane facing +Z, every vertex its own UV.
fn sheet(place: impl Fn(Vec3) -> Vec3) -> rbx_mesh::Mesh {
    let n = 8;
    let mut vertices = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let at = place(Vec3::new(i as f32 * 0.25, j as f32 * 0.25, 0.0));
            vertices.push(rbx_mesh::Vertex {
                position: at.to_array(),
                normal: [0.0, 0.0, 1.0],
                uv: [i as f32 * 0.1, j as f32 * 0.1],
                color: [255; 4],
            });
        }
    }
    let mut indices = Vec::new();
    for j in 0..n - 1 {
        for i in 0..n - 1 {
            let a = (j * n + i) as u32;
            indices.extend([
                a,
                a + 1,
                a + n as u32,
                a + 1,
                a + n as u32 + 1,
                a + n as u32,
            ]);
        }
    }
    rbx_mesh::Mesh {
        version: (4, 1),
        vertices,
        indices,
        lods: Vec::new(),
        bounds: rbx_mesh::Aabb {
            min: [0.0; 3],
            max: [2.0; 3],
        },
    }
}

fn world(mesh: &rbx_mesh::Mesh) -> WorldCage<'_> {
    WorldCage {
        positions: mesh
            .vertices
            .iter()
            .map(|v| Vec3::from(v.position))
            .collect(),
        mesh,
    }
}

fn surface_of(mesh: &rbx_mesh::Mesh) -> Surface {
    let mut surface = Surface::default();
    surface.lay(&world(mesh));
    surface
}

#[test]
fn an_inner_cage_on_its_own_body_leaves_the_garment_alone() {
    let body = sheet(|p| p);
    let map = CageMap::pair(&world(&body), &surface_of(&body)).unwrap();
    let at = Vec3::new(0.9, 0.8, 0.1);
    let (moved, normal) = map.carry(at, Vec3::Z);
    assert!(moved.distance(at) < 1e-5, "{moved}");
    assert!(normal.distance(Vec3::Z) < 1e-5);
}

#[test]
fn the_garment_goes_where_the_paired_body_vertices_go() {
    // The body stands 3 studs away and twice as wide: pairing is by UV, not
    // by distance, so the garment follows across the gap.
    let inner = sheet(|p| p);
    let body = sheet(|p| Vec3::new(p.x * 2.0 + 3.0, p.y, p.z));
    let map = CageMap::pair(&world(&inner), &surface_of(&body)).unwrap();
    let (moved, _) = map.carry(Vec3::new(1.0, 1.0, 0.1), Vec3::Z);
    assert!(moved.distance(Vec3::new(5.0, 1.0, 0.1)) < 0.05, "{moved}");
}

#[test]
fn a_turned_surface_turns_the_offset_with_it() {
    // The body faces +X instead of +Z: a point 0.1 in front of the inner
    // cage stays 0.1 in front of the body.
    let inner = sheet(|p| p);
    let body = sheet(|p| Vec3::new(0.0, p.y, -p.x));
    let map = CageMap::pair(&world(&inner), &surface_of(&body)).unwrap();
    let (moved, normal) = map.carry(Vec3::new(0.75, 0.75, 0.1), Vec3::Z);
    assert!(
        moved.distance(Vec3::new(0.1, 0.75, -0.75)) < 0.02,
        "{moved}"
    );
    assert!(normal.distance(Vec3::X) < 0.02, "{normal}");
}

#[test]
fn a_cage_of_another_topology_does_not_pair() {
    let body = sheet(|p| p);
    let mut flat = sheet(|p| p);
    for v in &mut flat.vertices {
        v.uv = [0.0; 2];
    }
    assert!(CageMap::pair(&world(&flat), &surface_of(&body)).is_none());
}

#[test]
fn a_layer_laid_over_the_body_answers_for_its_uvs() {
    let body = sheet(|p| p);
    let outer = sheet(|p| p + Vec3::Z * 0.2);
    let mut surface = surface_of(&body);
    surface.lay(&world(&outer));
    // A second garment made on the bare body is worn over the first one.
    let map = CageMap::pair(&world(&body), &surface).unwrap();
    let (moved, _) = map.carry(Vec3::new(1.0, 1.0, 0.05), Vec3::Z);
    assert!((moved.z - 0.25).abs() < 1e-4, "{moved}");
}
