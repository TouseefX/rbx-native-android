use std::collections::HashMap;

use rbx_dom::{CFrameData, Content, Variant, Vector3Data, WeakDom};
use rbx_mesh::{Aabb, Mesh as FileMesh, SkinnedMesh, Vertex};
use rbx_reflection::ReflectionDatabase;
use rbx_viewer::pick::Meshes;

use super::*;

/// A box centred on the origin, `native` studs across, as a mesh file holds it.
fn cube(native: [f32; 3]) -> SkinnedMesh {
    let h = native.map(|n| n / 2.0);
    let sign = |i: u32, bit: u32| if i & bit == 0 { -1.0 } else { 1.0 };
    let vertices = (0..8)
        .map(|i| Vertex {
            position: [sign(i, 1) * h[0], sign(i, 2) * h[1], sign(i, 4) * h[2]],
            normal: [0.0, 1.0, 0.0],
            uv: [(i & 1) as f32, ((i >> 1) & 1) as f32],
            color: [255; 4],
        })
        .collect();
    let faces: [[u32; 3]; 12] = [
        [0, 2, 1],
        [1, 2, 3],
        [4, 5, 6],
        [5, 7, 6],
        [0, 1, 4],
        [1, 5, 4],
        [2, 6, 3],
        [3, 6, 7],
        [0, 4, 2],
        [2, 4, 6],
        [1, 3, 5],
        [3, 7, 5],
    ];
    let indices: Vec<u32> = faces.iter().flatten().copied().collect();
    SkinnedMesh::from(FileMesh {
        version: (2, 0),
        vertices,
        #[allow(clippy::single_range_in_vec_init)]
        lods: vec![0..indices.len() as u32],
        indices,
        bounds: Aabb {
            min: h.map(|v| -v),
            max: h,
        },
    })
}

const IDENTITY: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

fn at(x: f32, y: f32, z: f32) -> CFrameData {
    CFrameData {
        position: Vector3Data { x, y, z },
        rotation: IDENTITY,
    }
}

fn size(x: f32, y: f32, z: f32) -> Variant {
    Variant::Vector3(Vector3Data { x, y, z })
}

fn uri(id: u64) -> Variant {
    Variant::Content(Content::Uri(format!("rbxassetid://{id}")))
}

/// A model holding one instance per `(class, name, properties)`.
type Node<'a> = (&'a str, &'a str, Vec<(&'a str, Variant)>);

fn model(children: Vec<Node>) -> (WeakDom, Vec<Ref>) {
    let mut dom = WeakDom::new();
    let root = dom.new_instance("Model", "Model", None);
    let made = children
        .into_iter()
        .map(|(class, name, properties)| {
            let child = dom.new_instance(class, name, Some(root));
            for (property, value) in properties {
                dom.set_property(child, property, value).unwrap();
            }
            child
        })
        .collect();
    (dom, made)
}

fn mesh_part(
    name: &str,
    id: u64,
    cframe: CFrameData,
    extent: Variant,
) -> (&'static str, &str, Vec<(&'static str, Variant)>) {
    (
        "MeshPart",
        name,
        vec![
            ("CFrame", Variant::CFrame(cframe)),
            ("size", extent),
            ("MeshId", uri(id)),
        ],
    )
}

/// Serves `native` for each asset id, and fails the rest the way an
/// offline fetch would.
fn serve(
    native: HashMap<u64, [f32; 3]>,
) -> impl Fn(&WeakDom, &ReflectionDatabase, &[Ref]) -> Result<Meshes, Vec<String>> {
    move |dom, database, roots| {
        Meshes::fetched_with(dom, database, roots, |wanted| {
            wanted
                .iter()
                .map(|asset| {
                    let result = match asset {
                        AssetRef::Id(id) => native
                            .get(id)
                            .map(|n| cube(*n))
                            .ok_or(format!("asset {id}: no network")),
                        other => Err(format!("{other:?}")),
                    };
                    (asset.clone(), result)
                })
                .collect()
        })
    }
}

fn bounds(mesh: &Mesh) -> (Vec3, Vec3) {
    mesh.positions.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), &p| (lo.min(p), hi.max(p)),
    )
}

fn near(a: Vec3, b: Vec3) {
    assert!(a.distance(b) < 1e-4, "{a} is not {b}");
}

fn rigged(dom: &mut WeakDom, under: Ref) {
    dom.new_instance("Motor6D", "Root", Some(under));
}

fn gather_with(dom: &WeakDom, served: &[(u64, [f32; 3])]) -> Result<(Mesh, Look), String> {
    gather(
        dom,
        ReflectionDatabase::shared(),
        serve(served.iter().copied().collect()),
    )
}

#[test]
fn a_mesh_part_is_fitted_to_its_size_and_carried_by_its_cframe() {
    let (dom, _) = model(vec![mesh_part(
        "Torso",
        1,
        at(10.0, 5.0, -3.0),
        size(2.0, 4.0, 1.0),
    )]);
    let (mesh, _) = gather_with(&dom, &[(1, [1.0; 3])]).unwrap();
    let (lo, hi) = bounds(&mesh);
    near(lo, Vec3::new(9.0, 3.0, -3.5));
    near(hi, Vec3::new(11.0, 7.0, -2.5));
    assert_eq!(mesh.triangles.len(), 12);
    assert_eq!(mesh.up_hint, Some(Axis::PosY));
}

#[test]
fn the_size_is_scaled_against_the_meshs_own_bounds() {
    let (dom, _) = model(vec![mesh_part(
        "Arm",
        1,
        at(0.0, 0.0, 0.0),
        size(1.0, 3.0, 1.0),
    )]);
    let (mesh, _) = gather_with(&dom, &[(1, [0.5, 6.0, 0.5])]).unwrap();
    let (lo, hi) = bounds(&mesh);
    near(hi - lo, Vec3::new(1.0, 3.0, 1.0));
}

#[test]
fn a_rotated_part_is_rotated_in_the_merge() {
    let turned = CFrameData {
        position: Vector3Data {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        // A quarter turn about Y: the part's long X axis ends up along Z.
        rotation: [0.0, 0.0, 1.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0],
    };
    let (dom, _) = model(vec![mesh_part("Leg", 1, turned, size(4.0, 1.0, 1.0))]);
    let (mesh, _) = gather_with(&dom, &[(1, [1.0; 3])]).unwrap();
    let (lo, hi) = bounds(&mesh);
    near(hi - lo, Vec3::new(1.0, 1.0, 4.0));
}

#[test]
fn several_parts_merge_into_one_mesh() {
    let (dom, _) = model(vec![
        mesh_part("A", 1, at(-5.0, 0.0, 0.0), size(1.0, 1.0, 1.0)),
        mesh_part("B", 1, at(5.0, 0.0, 0.0), size(1.0, 1.0, 1.0)),
        (
            "Part",
            "Block",
            vec![
                ("CFrame", Variant::CFrame(at(0.0, 9.0, 0.0))),
                ("size", size(2.0, 2.0, 2.0)),
            ],
        ),
    ]);
    let (mesh, _) = gather_with(&dom, &[(1, [1.0; 3])]).unwrap();
    assert_eq!(mesh.triangles.len(), 12 + 12 + 12);
    let (lo, hi) = bounds(&mesh);
    near(lo, Vec3::new(-5.5, -0.5, -1.0));
    near(hi, Vec3::new(5.5, 10.0, 1.0));
    assert!(mesh
        .triangles
        .iter()
        .flatten()
        .all(|&i| (i as usize) < mesh.positions.len()));
    assert_eq!(mesh.uvs.len(), mesh.positions.len());
}

#[test]
fn an_empty_model_is_refused() {
    let (dom, _) = model(vec![("Folder", "Nothing", vec![])]);
    let err = gather_with(&dom, &[]).unwrap_err();
    assert!(err.contains("empty"), "{err}");
}

#[test]
fn a_rigged_model_is_refused_by_name() {
    let (mut dom, parts) = model(vec![mesh_part(
        "Torso",
        1,
        at(0.0, 0.0, 0.0),
        size(1.0, 1.0, 1.0),
    )]);
    rigged(&mut dom, parts[0]);
    let err = gather_with(&dom, &[(1, [1.0; 3])]).unwrap_err();
    assert!(err.contains("already rigged"), "{err}");
}

#[test]
fn a_union_is_refused_rather_than_dropped() {
    let (dom, _) = model(vec![(
        "UnionOperation",
        "Union",
        vec![("CFrame", Variant::CFrame(at(0.0, 0.0, 0.0)))],
    )]);
    let err = gather_with(&dom, &[]).unwrap_err();
    assert!(err.contains("union"), "{err}");
}

#[test]
fn a_mesh_that_will_not_fetch_is_reported() {
    let (dom, _) = model(vec![mesh_part(
        "Head",
        77,
        at(0.0, 0.0, 0.0),
        size(1.0, 1.0, 1.0),
    )]);
    let err = gather_with(&dom, &[]).unwrap_err();
    assert!(err.contains("offline") && err.contains("asset 77"), "{err}");
}

#[test]
fn a_fully_transparent_part_adds_no_geometry() {
    let (dom, _) = model(vec![
        mesh_part("Body", 1, at(0.0, 0.0, 0.0), size(1.0, 1.0, 1.0)),
        (
            "Part",
            "HumanoidRootPart",
            vec![
                ("CFrame", Variant::CFrame(at(0.0, 50.0, 0.0))),
                ("size", size(2.0, 2.0, 1.0)),
                ("Transparency", Variant::Float32(1.0)),
            ],
        ),
    ]);
    let (mesh, _) = gather_with(&dom, &[(1, [1.0; 3])]).unwrap();
    assert_eq!(mesh.triangles.len(), 12);
}

fn textured(
    name: &str,
    id: u64,
    texture: u64,
    color: [u8; 3],
) -> (&'static str, &str, Vec<(&'static str, Variant)>) {
    let (class, name, mut properties) =
        mesh_part(name, id, at(id as f32 * 3.0, 0.0, 0.0), size(1.0, 1.0, 1.0));
    properties.push(("TextureID", uri(texture)));
    properties.push((
        "Color3uint8",
        Variant::Color3uint8 {
            r: color[0],
            g: color[1],
            b: color[2],
        },
    ));
    (class, name, properties)
}

#[test]
fn a_texture_and_colour_every_part_shares_carry_over() {
    let (dom, _) = model(vec![
        textured("A", 1, 55, [10, 20, 30]),
        textured("B", 2, 55, [10, 20, 30]),
    ]);
    let (mesh, look) = gather_with(&dom, &[(1, [1.0; 3]), (2, [1.0; 3])]).unwrap();
    assert_eq!(look.texture, Some(55));
    assert_eq!(look.color, Some([10, 20, 30]));
    assert!(mesh.notes.is_empty());
}

#[test]
fn parts_that_disagree_leave_the_rig_plain_and_say_so() {
    let (dom, _) = model(vec![
        textured("A", 1, 55, [10, 20, 30]),
        textured("B", 2, 56, [10, 20, 31]),
    ]);
    let (mesh, look) = gather_with(&dom, &[(1, [1.0; 3]), (2, [1.0; 3])]).unwrap();
    assert_eq!(look, Look::default());
    assert_eq!(mesh.notes.len(), 2, "{:?}", mesh.notes);
}

#[test]
fn only_model_extensions_are_read_as_models() {
    assert!(is_model(Path::new("a/dummy.rbxm")));
    assert!(is_model(Path::new("a/dummy.RBXMX")));
    assert!(!is_model(Path::new("a/dummy.rbxl")));
    assert!(!is_model(Path::new("a/dummy.glb")));
}
