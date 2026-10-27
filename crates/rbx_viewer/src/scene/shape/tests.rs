use super::*;
use rbx_dom::{Ref, Vector3Data};

fn database() -> ReflectionDatabase {
    ReflectionDatabase::embedded()
}

fn part(dom: &mut WeakDom, class: &str) -> Ref {
    let referent = Ref::new(dom.root_refs().len() as u32 + 1);
    let instance = Instance::new(referent, class, class);
    dom.insert(instance);
    dom.set_parent(referent, None);
    referent
}

fn set_vector3(dom: &mut WeakDom, referent: Ref, property: &str, v: [f32; 3]) {
    dom.get_mut(referent).unwrap().properties_mut().insert(
        property.to_string(),
        Variant::Vector3(Vector3Data {
            x: v[0],
            y: v[1],
            z: v[2],
        }),
    );
}

#[test]
fn a_plain_part_with_no_shape_property_is_a_box() {
    let mut dom = WeakDom::new();
    let referent = part(&mut dom, "Part");

    let geometry = resolve(
        &dom,
        &database(),
        dom.get(referent).unwrap(),
        Vec3::new(4.0, 2.0, 6.0),
    );

    assert_eq!(geometry.kind, ShapeKind::Box);
    assert_eq!(geometry.size, Vec3::new(4.0, 2.0, 6.0));
}

#[test]
fn part_shape_ball_clamps_to_the_smallest_dimension() {
    let mut dom = WeakDom::new();
    let referent = part(&mut dom, "Part");
    dom.get_mut(referent)
        .unwrap()
        .properties_mut()
        .insert("shape".to_string(), Variant::Enum(0));

    let geometry = resolve(
        &dom,
        &database(),
        dom.get(referent).unwrap(),
        Vec3::new(4.0, 2.0, 6.0),
    );

    assert_eq!(geometry.kind, ShapeKind::Ball);
    assert_eq!(geometry.size, Vec3::splat(2.0));
}

#[test]
fn part_shape_cylinder_is_cylinder_x_with_the_parts_own_size() {
    let mut dom = WeakDom::new();
    let referent = part(&mut dom, "Part");
    dom.get_mut(referent)
        .unwrap()
        .properties_mut()
        .insert("shape".to_string(), Variant::Enum(2));

    let geometry = resolve(
        &dom,
        &database(),
        dom.get(referent).unwrap(),
        Vec3::new(4.0, 2.0, 2.0),
    );

    assert_eq!(geometry.kind, ShapeKind::CylinderX);
    assert_eq!(geometry.size, Vec3::new(4.0, 2.0, 2.0));
}

#[test]
fn a_particle_emitter_shape_property_is_never_confused_for_a_part_shape() {
    // A different Shape enum (ParticleEmitterShape) on an unrelated class
    // must never be read as if it were Part's lowercase `shape`.
    let mut dom = WeakDom::new();
    let referent = part(&mut dom, "ParticleEmitter");
    dom.get_mut(referent)
        .unwrap()
        .properties_mut()
        .insert("Shape".to_string(), Variant::Enum(0));

    let geometry = resolve(
        &dom,
        &database(),
        dom.get(referent).unwrap(),
        Vec3::splat(4.0),
    );

    assert_eq!(geometry.kind, ShapeKind::Box);
}

#[test]
fn wedge_part_is_a_wedge_regardless_of_any_shape_property() {
    let mut dom = WeakDom::new();
    let referent = part(&mut dom, "WedgePart");

    let geometry = resolve(
        &dom,
        &database(),
        dom.get(referent).unwrap(),
        Vec3::splat(4.0),
    );

    assert_eq!(geometry.kind, ShapeKind::Wedge);
}

#[test]
fn corner_wedge_part_is_a_corner_wedge() {
    let mut dom = WeakDom::new();
    let referent = part(&mut dom, "CornerWedgePart");

    let geometry = resolve(
        &dom,
        &database(),
        dom.get(referent).unwrap(),
        Vec3::splat(4.0),
    );

    assert_eq!(geometry.kind, ShapeKind::CornerWedge);
}

#[test]
fn a_cylinder_mesh_child_replaces_the_parent_with_a_y_axis_cylinder() {
    let mut dom = WeakDom::new();
    let part_ref = part(&mut dom, "Part");
    let mesh_ref = part(&mut dom, "CylinderMesh");
    dom.set_parent(mesh_ref, Some(part_ref));
    set_vector3(&mut dom, mesh_ref, "Scale", [2.0, 3.0, 2.0]);
    set_vector3(&mut dom, mesh_ref, "Offset", [0.0, 1.0, 0.0]);

    let geometry = resolve(
        &dom,
        &database(),
        dom.get(part_ref).unwrap(),
        Vec3::new(1.0, 4.0, 1.0),
    );

    assert_eq!(geometry.kind, ShapeKind::CylinderY);
    assert_eq!(geometry.size, Vec3::new(2.0, 12.0, 2.0));
    assert_eq!(geometry.offset, Vec3::new(0.0, 1.0, 0.0));
}

#[test]
fn a_block_mesh_child_keeps_a_box_but_still_applies_scale() {
    let mut dom = WeakDom::new();
    let part_ref = part(&mut dom, "Part");
    let mesh_ref = part(&mut dom, "BlockMesh");
    dom.set_parent(mesh_ref, Some(part_ref));
    set_vector3(&mut dom, mesh_ref, "Scale", [2.0, 2.0, 2.0]);

    let geometry = resolve(
        &dom,
        &database(),
        dom.get(part_ref).unwrap(),
        Vec3::splat(1.0),
    );

    assert_eq!(geometry.kind, ShapeKind::Box);
    assert_eq!(geometry.size, Vec3::splat(2.0));
}

#[test]
fn a_special_mesh_sphere_is_an_ellipsoid_not_clamped_like_part_shape_ball() {
    let mut dom = WeakDom::new();
    let part_ref = part(&mut dom, "Part");
    let mesh_ref = part(&mut dom, "SpecialMesh");
    dom.set_parent(mesh_ref, Some(part_ref));
    dom.get_mut(mesh_ref)
        .unwrap()
        .properties_mut()
        .insert("MeshType".to_string(), Variant::Enum(3));

    let geometry = resolve(
        &dom,
        &database(),
        dom.get(part_ref).unwrap(),
        Vec3::new(4.0, 2.0, 6.0),
    );

    assert_eq!(geometry.kind, ShapeKind::Ball);
    assert_eq!(geometry.size, Vec3::new(4.0, 2.0, 6.0));
}

#[test]
fn a_special_mesh_file_mesh_leaves_the_box_untouched() {
    let mut dom = WeakDom::new();
    let part_ref = part(&mut dom, "Part");
    let mesh_ref = part(&mut dom, "SpecialMesh");
    dom.set_parent(mesh_ref, Some(part_ref));
    dom.get_mut(mesh_ref)
        .unwrap()
        .properties_mut()
        .insert("MeshType".to_string(), Variant::Enum(5));

    let geometry = resolve(
        &dom,
        &database(),
        dom.get(part_ref).unwrap(),
        Vec3::splat(4.0),
    );

    assert_eq!(geometry.kind, ShapeKind::Box);
    assert_eq!(geometry.size, Vec3::splat(4.0));
}

#[test]
fn a_corner_wedge_mesh_type_maps_to_the_corner_wedge_shape() {
    let mut dom = WeakDom::new();
    let part_ref = part(&mut dom, "Part");
    let mesh_ref = part(&mut dom, "SpecialMesh");
    dom.set_parent(mesh_ref, Some(part_ref));
    dom.get_mut(mesh_ref)
        .unwrap()
        .properties_mut()
        .insert("MeshType".to_string(), Variant::Enum(11));

    let geometry = resolve(
        &dom,
        &database(),
        dom.get(part_ref).unwrap(),
        Vec3::splat(4.0),
    );

    assert_eq!(geometry.kind, ShapeKind::CornerWedge);
}

/// A Cylinder part's cross-section is a circle as wide as the smaller of Y
/// and Z, the extent Studio's own bake gives a union holding one.
#[test]
fn part_shape_cylinder_keeps_a_circular_cross_section() {
    let mut dom = WeakDom::new();
    let referent = part(&mut dom, "Part");
    dom.get_mut(referent)
        .unwrap()
        .properties_mut()
        .insert("shape".to_string(), Variant::Enum(2));

    let geometry = resolve(
        &dom,
        &database(),
        dom.get(referent).unwrap(),
        Vec3::new(1.8, 1.0, 0.5),
    );

    assert_eq!(geometry.kind, ShapeKind::CylinderX);
    assert_eq!(geometry.size, Vec3::new(1.8, 0.5, 0.5));
}

/// creator-docs `CylinderMesh`: "maintaining a 1:1 ratio for the part's X and
/// Z axis, using the lowest value", after its `Scale`.
#[test]
fn a_cylinder_mesh_keeps_a_circular_cross_section() {
    let mut dom = WeakDom::new();
    let part_ref = part(&mut dom, "Part");
    let mesh_ref = part(&mut dom, "CylinderMesh");
    dom.set_parent(mesh_ref, Some(part_ref));
    set_vector3(&mut dom, mesh_ref, "Scale", [1.0, 0.6, 1.0]);

    let geometry = resolve(
        &dom,
        &database(),
        dom.get(part_ref).unwrap(),
        Vec3::new(0.4, 0.2, 0.3),
    );

    assert_eq!(geometry.kind, ShapeKind::CylinderY);
    assert!(
        (geometry.size - Vec3::new(0.3, 0.12, 0.3))
            .abs()
            .max_element()
            < 1e-6
    );
}

/// The same docs contrast a `CylinderMesh` (along Y) with a `SpecialMesh`
/// cylinder, which lies along X like a Cylinder part.
#[test]
fn a_special_mesh_cylinder_lies_along_x() {
    let mut dom = WeakDom::new();
    let part_ref = part(&mut dom, "Part");
    let mesh_ref = part(&mut dom, "SpecialMesh");
    dom.set_parent(mesh_ref, Some(part_ref));
    dom.get_mut(mesh_ref)
        .unwrap()
        .properties_mut()
        .insert("MeshType".to_string(), Variant::Enum(4));

    let geometry = resolve(
        &dom,
        &database(),
        dom.get(part_ref).unwrap(),
        Vec3::new(0.4, 4.0, 4.0),
    );

    assert_eq!(geometry.kind, ShapeKind::CylinderX);
    assert_eq!(geometry.size, Vec3::new(0.4, 4.0, 4.0));
}

#[test]
fn an_unresolved_mesh_part_head_reads_as_a_ball_but_other_mesh_parts_stay_boxes() {
    let mut dom = WeakDom::new();
    let head = part(&mut dom, "MeshPart");
    dom.get_mut(head).unwrap().set_name("Head");
    let torso = part(&mut dom, "MeshPart");
    let kind = |dom: &WeakDom, referent| {
        resolve(
            dom,
            &database(),
            dom.get(referent).unwrap(),
            Vec3::new(2.0, 1.0, 1.0),
        )
        .kind
    };
    assert_eq!(kind(&dom, head), ShapeKind::Ball);
    assert_eq!(kind(&dom, torso), ShapeKind::Box);
}

fn set_enum(dom: &mut WeakDom, referent: Ref, property: &str, raw: u32) {
    dom.get_mut(referent)
        .unwrap()
        .properties_mut()
        .insert(property.to_string(), Variant::Enum(raw));
}

#[test]
fn an_out_of_range_part_shape_is_reported_once_per_value() {
    let mut dom = WeakDom::new();
    let unknown = part(&mut dom, "Part");
    set_enum(&mut dom, unknown, "shape", 7);
    let known = part(&mut dom, "Part");
    set_enum(&mut dom, known, "shape", 2);

    assert_eq!(unknown_part_types(&dom), BTreeSet::from([7]));
}

#[test]
fn a_dom_without_any_shape_property_reports_no_unknown_part_types() {
    let mut dom = WeakDom::new();
    part(&mut dom, "Part");

    assert!(unknown_part_types(&dom).is_empty());
}

fn special_mesh_part(dom: &mut WeakDom, mesh_type: u32) -> Ref {
    let part_ref = part(dom, "Part");
    let mesh_ref = part(dom, "SpecialMesh");
    dom.set_parent(mesh_ref, Some(part_ref));
    set_enum(dom, mesh_ref, "MeshType", mesh_type);
    part_ref
}

/// Measured in Studio: a 4x8x4 `SpecialMesh` cylinder is 4 studs long and 4
/// round, not stretched to 8.
#[test]
fn a_special_mesh_cylinder_is_as_round_as_the_smaller_of_y_and_z() {
    let mut dom = WeakDom::new();
    let part_ref = special_mesh_part(&mut dom, 4);
    let geometry = resolve(
        &dom,
        &database(),
        dom.get(part_ref).unwrap(),
        Vec3::new(6.0, 3.0, 2.0),
    );
    assert_eq!(geometry.kind, ShapeKind::CylinderX);
    assert_eq!(geometry.size, Vec3::new(6.0, 2.0, 2.0));
}

/// Until `head.mesh` lands, a Head stands in as a Y cylinder at the head's
/// own extent: X and Z both the smaller of the two, Y as given.
#[test]
fn a_special_mesh_head_takes_the_smaller_of_x_and_z_for_both() {
    let mut dom = WeakDom::new();
    let part_ref = special_mesh_part(&mut dom, HEAD_MESH);
    let geometry = resolve(
        &dom,
        &database(),
        dom.get(part_ref).unwrap(),
        Vec3::new(8.0, 4.0, 6.0),
    );
    assert_eq!(geometry.kind, ShapeKind::CylinderY);
    assert_eq!(geometry.size, Vec3::new(6.0, 4.0, 6.0));
    assert_eq!(head_extent(Vec3::new(2.5, 1.25, 1.25)), Vec3::splat(1.25));
}

#[test]
fn a_special_mesh_torso_is_a_plain_box() {
    let mut dom = WeakDom::new();
    let part_ref = special_mesh_part(&mut dom, 1);
    let geometry = resolve(
        &dom,
        &database(),
        dom.get(part_ref).unwrap(),
        Vec3::new(6.0, 3.0, 2.0),
    );
    assert_eq!(geometry.kind, ShapeKind::Box);
    assert_eq!(geometry.size, Vec3::new(6.0, 3.0, 2.0));
}

/// Prism, Pyramid, ParallelRamp and RightAngleRamp render nothing in Studio.
#[test]
fn only_the_deprecated_mesh_types_draw_nothing() {
    let hidden: Vec<u32> = (0..=11)
        .filter(|&mesh_type| {
            let mut dom = WeakDom::new();
            let part_ref = special_mesh_part(&mut dom, mesh_type);
            draws_nothing(&dom, dom.get(part_ref).unwrap())
        })
        .collect();
    assert_eq!(hidden, vec![7, 8, 9, 10]);

    let mut dom = WeakDom::new();
    let plain = part(&mut dom, "Part");
    assert!(!draws_nothing(&dom, dom.get(plain).unwrap()));
}
