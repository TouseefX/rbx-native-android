use std::collections::HashMap;
use std::sync::Arc;

use glam::Vec3;
use rbx_assets::AssetRef;
use rbx_dom::{CFrameData, Instance, Ref, Variant, Vector3Data, WeakDom};
use rbx_mesh::{Bone, Envelope, Facs, Skin, Subset};
use rbx_reflection::ReflectionDatabase;

use crate::scene::filemesh::{plan, resolve, Plan, Resolved};
use crate::scene::material::Catalog;

const HEAD: Ref = Ref::new(2);
const CONTROLS: Ref = Ref::new(3);
const IDENTITY: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

fn vector3(v: Vec3) -> Variant {
    Variant::Vector3(Vector3Data {
        x: v.x,
        y: v.y,
        z: v.z,
    })
}

/// Two vertices: one on the `jaw` bone, which `JawDrop` lowers half a stud,
/// one on `skull`, which nothing moves.
fn head_mesh() -> rbx_mesh::SkinnedMesh {
    let vertex = |y: f32| rbx_mesh::Vertex {
        position: [0.0, y, 0.0],
        normal: [0.0, 0.0, -1.0],
        uv: [0.0, 0.0],
        color: [255; 4],
    };
    let bone = |name: &str| Bone {
        name: name.to_owned(),
        parent: None,
        lod_parent: None,
        culling: 0.0,
        rotation: IDENTITY,
        position: [0.0, 0.0, 0.0],
    };
    let envelope = |slot| Envelope {
        slots: [slot, 0, 0, 0],
        weights: [255, 0, 0, 0],
    };
    let skin = Skin {
        facs: Some(Facs {
            bones: vec!["jaw".into()],
            controls: vec!["c_JD".into()],
            correctives: Vec::new(),
            position: [vec![0.0], vec![-0.5], vec![0.0]],
            rotation: [vec![0.0], vec![0.0], vec![0.0]],
        }),
        ..Skin::new(
            vec![bone("jaw"), bone("skull")],
            vec![Subset {
                faces: 0..1,
                vertices: 0..2,
                bones: vec![0, 1],
            }],
            vec![envelope(0), envelope(1)],
        )
    };
    rbx_mesh::SkinnedMesh {
        mesh: rbx_mesh::Mesh {
            version: (6, 0),
            bounds: rbx_mesh::Aabb {
                min: [-0.5, -0.5, -0.5],
                max: [0.5, 0.5, 0.5],
            },
            vertices: vec![vertex(-0.5), vertex(0.5)],
            indices: vec![0, 1, 1],
            lods: Vec::new(),
        },
        skin: Some(skin),
    }
}

/// A lone `MeshPart` head straight under `Workspace`, with a
/// `FaceControls` whose `JawDrop` is `jaw`.
fn place(jaw: f32) -> WeakDom {
    let mut dom = WeakDom::new();
    let workspace = Ref::new(1);
    dom.insert(Instance::new(workspace, "Workspace", "Workspace"));
    dom.set_parent(workspace, None);
    let mut head = Instance::new(HEAD, "MeshPart", "Head");
    let properties = head.properties_mut();
    properties.insert("MeshId".into(), Variant::String("rbxassetid://9".into()));
    properties.insert("size".into(), vector3(Vec3::ONE));
    properties.insert("InitialSize".into(), vector3(Vec3::ONE));
    properties.insert(
        "CFrame".into(),
        Variant::CFrame(CFrameData {
            position: Vector3Data {
                x: 0.0,
                y: 3.0,
                z: 0.0,
            },
            rotation: IDENTITY,
        }),
    );
    dom.insert(head);
    dom.set_parent(HEAD, Some(workspace));
    dom.insert(Instance::new(CONTROLS, "FaceControls", "FaceControls"));
    dom.set_parent(CONTROLS, Some(HEAD));
    set_jaw(&mut dom, jaw);
    dom
}

fn set_jaw(dom: &mut WeakDom, value: f32) {
    dom.get_mut(CONTROLS)
        .unwrap()
        .properties_mut()
        .insert("JawDrop".into(), Variant::Float32(value));
}

fn resolved(dom: &WeakDom) -> (Plan, Resolved) {
    let database = ReflectionDatabase::embedded();
    let plan = plan(dom, &database, &mut Catalog::new(dom, &database));
    let meshes = HashMap::from([(AssetRef::Id(9), Arc::new(head_mesh()))]);
    let resolved = resolve(&plan, meshes, HashMap::new()).0;
    (plan, resolved)
}

fn jaw_y(mesh: &rbx_mesh::Mesh) -> f32 {
    mesh.vertices[0].position[1]
}

#[test]
fn a_lone_dynamic_head_opens_its_jaw_from_face_controls() {
    let dom = place(1.0);
    let (plan, mut resolved) = resolved(&dom);
    let rig = resolved.rigs.get_mut(&HEAD).expect("rigged for its face");
    let posed = rig.pose(&plan.poses).expect("posed by JawDrop");
    assert!((jaw_y(&posed) - -1.0).abs() < 1e-5, "{}", jaw_y(&posed));
    assert_eq!(posed.vertices[1].position, [0.0, 0.5, 0.0]);
}

#[test]
fn writing_or_removing_face_controls_reposes_the_head() {
    let mut dom = place(0.5);
    let (mut plan, mut resolved) = resolved(&dom);
    let rig = resolved.rigs.get_mut(&HEAD).unwrap();
    assert!((jaw_y(&rig.pose(&plan.poses).unwrap()) - -0.75).abs() < 1e-5);

    set_jaw(&mut dom, 0.5);
    assert!(!plan.poses.face(&dom, HEAD), "the same value is no change");

    set_jaw(&mut dom, 0.0);
    assert!(plan.poses.face(&dom, HEAD));
    assert!(plan.poses.take_dirty());
    let rig = resolved.rigs.get_mut(&HEAD).unwrap();
    assert_eq!(jaw_y(&rig.pose(&plan.poses).unwrap()), -0.5, "back at rest");

    set_jaw(&mut dom, 1.0);
    assert!(plan.poses.face(&dom, HEAD));
    dom.remove(CONTROLS);
    assert!(plan.poses.face(&dom, HEAD), "taken away, the face rests");
    assert!(plan.poses.face_of(HEAD).is_empty());
}
