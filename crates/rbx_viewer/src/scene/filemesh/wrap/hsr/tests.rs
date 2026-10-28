use super::*;

/// A version 18 blob over `covers` (one per cage vertex) with `faces`, each
/// the cage vertices one mesh face sits under.
fn blob(covers: &[bool], faces: &[&[u16]]) -> Vec<u8> {
    let under: Vec<u16> = faces.iter().flat_map(|f| f.iter().copied()).collect();
    let mut out = vec![18];
    for n in [covers.len(), faces.len(), under.len(), 0] {
        out.extend((n as u32).to_le_bytes());
    }
    let mut bits = vec![0u8; covers.len().div_ceil(8)];
    for (i, covers) in covers.iter().enumerate() {
        if !covers {
            bits[i / 8] |= 1 << (i % 8);
        }
    }
    out.extend(bits);
    let mut from = 0u16;
    for face in faces {
        out.extend(from.to_le_bytes());
        out.extend((face.len() as u16).to_le_bytes());
        from += face.len() as u16;
    }
    for i in under {
        out.extend(i.to_le_bytes());
    }
    out.extend(vec![0xFF; 2 * faces.len()]);
    out.extend(b"0123456789abcdef0123456789abcdef");
    out
}

fn cage(uvs: &[[f32; 2]]) -> rbx_mesh::Mesh {
    rbx_mesh::Mesh {
        version: (4, 0),
        vertices: uvs
            .iter()
            .map(|&uv| rbx_mesh::Vertex {
                position: [0.0; 3],
                normal: [0.0, 1.0, 0.0],
                uv,
                color: [255; 4],
            })
            .collect(),
        indices: Vec::new(),
        lods: Vec::new(),
        bounds: rbx_mesh::Aabb {
            min: [0.0; 3],
            max: [0.0; 3],
        },
    }
}

#[test]
fn reads_the_layout_of_version_16_and_18() {
    let v18 = blob(&[true, false, true], &[&[0, 2], &[1]]);
    let hsr = Hsr::parse(&v18).unwrap();
    assert_eq!(hsr.covers, vec![true, false, true]);
    assert_eq!(hsr.faces, vec![(0, 2), (2, 1)]);
    assert_eq!(hsr.under, vec![0, 2, 1]);
    // Version 16 is the same without the extra record count.
    let mut v16 = v18.clone();
    v16[0] = 16;
    v16.drain(13..17);
    assert_eq!(Hsr::parse(&v16), Some(hsr));
    assert_eq!(Hsr::parse(&[17, 0, 0]), None);
    assert_eq!(Hsr::parse(&v18[..20]), None);
}

#[test]
fn a_face_hides_only_when_every_vertex_it_sits_under_is_covered() {
    // The layer covers the first two template UVs of its inner cage.
    let layer = Hsr::parse(&blob(&[true, true, false], &[])).unwrap();
    let inner = cage(&[[0.1, 0.1], [0.2, 0.2], [0.3, 0.3]]);
    let covered: HashSet<Key> = layer.covered(&inner).collect();
    // A body cage with its own vertex order, paired by UV.
    let body_cage = cage(&[[0.3, 0.3], [0.2, 0.2], [0.1, 0.1]]);
    let body = Hsr::parse(&blob(&[], &[&[1, 2], &[0, 1], &[]])).unwrap();
    assert_eq!(body.hidden(&body_cage, &covered), vec![true, false, false]);
}

#[test]
fn culling_keeps_the_vertices_and_drops_hidden_faces() {
    let mut mesh = cage(&[[0.0; 2]; 4]);
    mesh.indices = vec![0, 1, 2, 0, 2, 3];
    let skinned = rbx_mesh::SkinnedMesh { mesh, skin: None };
    let culled = cull(&skinned, &[true, false]);
    assert_eq!(culled.indices, vec![0, 2, 3]);
    assert_eq!(culled.vertices.len(), 4);
}

#[test]
fn an_hsr_asset_is_read_from_its_model() {
    let mut dom = WeakDom::new();
    let asset = dom.new_instance("HiddenSurfaceRemovalAsset", "Instance", None);
    let raw = blob(&[false], &[&[0]]);
    let data = Variant::Unknown { type_id: 0x01, raw };
    dom.set_property(asset, "HSRData", data).unwrap();
    let bytes = rbx_binary::serialize(&dom).unwrap();
    let hsr = Hsr::from_asset(&bytes).unwrap();
    assert_eq!(hsr.covers, vec![false]);
}
