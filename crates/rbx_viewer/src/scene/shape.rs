//! Works out which unit mesh a `BasePart` draws as, and the size/offset to
//! scale and place it with.
//!
//! Precedence, matching the engine: a legacy mesh child (`BlockMesh`,
//! `CylinderMesh`, `SpecialMesh`) replaces the part's own geometry outright;
//! otherwise `WedgePart`/`CornerWedgePart`'s class decides it; otherwise a
//! `Part`'s own `shape` property (note the lowercase wire name, `Shape` with a
//! capital belongs to `ParticleEmitter`, a different property entirely);
//! otherwise it is a plain box.

use std::collections::BTreeSet;

use glam::{Mat4, Vec3};
use rbx_dom::{Instance, Ref, Variant, WeakDom};
use rbx_reflection::ReflectionDatabase;

use crate::shapes::{TrussAxis, TrussStyle};

const WEDGE_PART: &str = "WedgePart";
const CORNER_WEDGE_PART: &str = "CornerWedgePart";
const TRUSS_PART: &str = "TrussPart";
/// `Enum.MeshType.Head` and `Enum.MeshType.FileMesh`.
pub(crate) const HEAD_MESH: u32 = 0;
pub(crate) const FILE_MESH: u32 = 5;
// Roblox restricts a real TrussPart.Size to 2*2*n studs, n a multiple of 2, up
// to 512, n/2 (this cap) is the largest legitimate number of 2-stud repeats.
const MAX_TRUSS_SEGMENTS: u32 = 256;

/// Which unit mesh (from [`crate::shapes`]) a part instances, before its
/// transform is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShapeKind {
    Box,
    Ball,
    CylinderX,
    CylinderY,
    Wedge,
    CornerWedge,
    Truss {
        axis: TrussAxis,
        segments: u32,
        style: TrussStyle,
    },
}

/// A part's resolved shape: which mesh, and the size/offset to place it with.
///
/// `size` already folds in a mesh child's `Scale` (or is clamped to a uniform
/// diameter for `Part.shape == Ball`, which, unlike `SpecialMesh.MeshType ==
/// Sphere` — never stretches into an ellipsoid); `offset` is zero unless a mesh
/// child carries one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Geometry {
    pub(crate) kind: ShapeKind,
    pub(crate) size: Vec3,
    pub(crate) offset: Vec3,
}

impl Geometry {
    /// The matrix the unit mesh is drawn through: the part's CFrame, then the
    /// shape's offset in the part's own frame, then its size as a scale.
    pub(crate) fn model(&self, cframe: Mat4) -> Mat4 {
        cframe * Mat4::from_translation(self.offset) * Mat4::from_scale(self.size)
    }
}

/// Resolves one part's geometry from its mesh children, class, and `shape`
/// property, in that order.
pub(crate) fn resolve(
    dom: &WeakDom,
    database: &ReflectionDatabase,
    instance: &Instance,
    size: Vec3,
) -> Geometry {
    if let Some(geometry) = mesh_child(dom, instance, size) {
        return geometry;
    }

    let class = instance.class();
    if database.is_subclass_of(class, WEDGE_PART) {
        return Geometry {
            kind: ShapeKind::Wedge,
            size,
            offset: Vec3::ZERO,
        };
    }
    if database.is_subclass_of(class, CORNER_WEDGE_PART) {
        return Geometry {
            kind: ShapeKind::CornerWedge,
            size,
            offset: Vec3::ZERO,
        };
    }
    if database.is_subclass_of(class, TRUSS_PART) {
        return Geometry {
            kind: truss_kind(instance, size),
            size,
            offset: Vec3::ZERO,
        };
    }

    let kind = match instance.properties().get("shape") {
        Some(&Variant::Enum(raw)) => part_type(raw),
        // A MeshPart head whose mesh is not at hand (Roblox's stock head mesh
        // is not publicly downloadable): a ball reads as a head, a box does
        // not.
        _ if class == "MeshPart" && instance.name() == "Head" => ShapeKind::Ball,
        // UnionOperation, MeshPart and anything else with no `shape` property:
        // a plain box until CSG/mesh geometry lands (see rbx_mesh).
        _ => ShapeKind::Box,
    };
    let size = match kind {
        // Enum.PartType.Ball is always a true sphere, clamped to the smallest
        // dimension rather than stretched into an ellipsoid.
        ShapeKind::Ball => Vec3::splat(size.x.min(size.y).min(size.z)),
        // A Cylinder part's cross-section is a circle too, as wide as the
        // smaller of Y and Z: Studio's own bake of a union holding one has
        // exactly that extent.
        ShapeKind::CylinderX => {
            let diameter = size.y.min(size.z);
            Vec3::new(size.x, diameter, diameter)
        }
        _ => size,
    };
    Geometry {
        kind,
        size,
        offset: Vec3::ZERO,
    }
}

/// `Enum.PartType`: Ball=0, Block=1, Cylinder=2, Wedge=3, CornerWedge=4.
///
/// Any other value falls back to a box; [`unknown_part_types`] lists those
/// values so the loader can say so in Output rather than drawing them silently.
fn part_type(raw: u32) -> ShapeKind {
    match raw {
        0 => ShapeKind::Ball,
        2 => ShapeKind::CylinderX,
        3 => ShapeKind::Wedge,
        4 => ShapeKind::CornerWedge,
        _ => ShapeKind::Box,
    }
}

/// The `shape` values, outside the `Enum.PartType` values [`part_type`]
/// knows, that any instance in `dom` carries, sorted and without repeats.
pub(crate) fn unknown_part_types(dom: &WeakDom) -> BTreeSet<u32> {
    let mut pending: Vec<Ref> = dom.root_refs().to_vec();
    let mut found = BTreeSet::new();
    while let Some(referent) = pending.pop() {
        let Some(instance) = dom.get(referent) else {
            continue;
        };
        if let Some(&Variant::Enum(raw)) = instance.properties().get("shape") {
            if !(0..=4).contains(&raw) {
                found.insert(raw);
            }
        }
        pending.extend_from_slice(instance.children());
    }
    found
}

/// A `TrussPart`'s shape: its long axis, how many 2-stud lattice repeats fit
/// along it, and its cross-bracing pattern.
fn truss_kind(instance: &Instance, size: Vec3) -> ShapeKind {
    let axis = truss_axis(size);
    let length = match axis {
        TrussAxis::X => size.x,
        TrussAxis::Y => size.y,
        TrussAxis::Z => size.z,
    };
    let style = match instance.properties().get("Style") {
        Some(&Variant::Enum(raw)) => truss_style(raw),
        _ => TrussStyle::default(),
    };
    ShapeKind::Truss {
        axis,
        segments: truss_segments(length),
        style,
    }
}

/// The part's long axis: whichever `size` component is largest. Roblox always
/// forces the other two to 2 studs for a real `TrussPart`, so this is
/// unambiguous; a tie (only possible from a non-conforming scripted size)
/// resolves to X.
fn truss_axis(size: Vec3) -> TrussAxis {
    if size.y > size.x && size.y >= size.z {
        TrussAxis::Y
    } else if size.z > size.x && size.z > size.y {
        TrussAxis::Z
    } else {
        TrussAxis::X
    }
}

/// One lattice repeat per real 2-stud length, matching how Roblox documents
/// `TrussPart.Size` (`2*2*n`, `n` a multiple of 2): `n / 2` is always a whole
/// number for a real truss. Clamped in case a script sets a non-conforming
/// size.
fn truss_segments(long_axis_length: f32) -> u32 {
    if !long_axis_length.is_finite() {
        return 1;
    }
    ((long_axis_length / 2.0).round() as i64).clamp(1, MAX_TRUSS_SEGMENTS as i64) as u32
}

/// `Enum.Style`: AlternatingSupports=0, BridgeStyleSupports=1, NoSupports=2.
fn truss_style(raw: u32) -> TrussStyle {
    match raw {
        1 => TrussStyle::BridgeStyleSupports,
        2 => TrussStyle::NoSupports,
        _ => TrussStyle::AlternatingSupports,
    }
}

/// The legacy mesh child that replaces the part's own shape: the first
/// `BlockMesh`, `CylinderMesh` or non-FileMesh `SpecialMesh`. A `SpecialMesh`
/// FileMesh has no procedural shape (its geometry is the asset `filemesh`
/// draws), and a `DynamicMesh` carries no properties at all (the API dump
/// lists none, its geometry arriving through the runtime editable-mesh API),
/// so neither replaces anything here.
fn legacy_mesh<'a>(dom: &'a WeakDom, instance: &Instance) -> Option<&'a Instance> {
    instance
        .children()
        .iter()
        .filter_map(|&child_ref| dom.get(child_ref))
        .find(|child| match child.class() {
            "BlockMesh" | "CylinderMesh" => true,
            "SpecialMesh" => !matches!(
                child.properties().get("MeshType"),
                Some(&Variant::Enum(FILE_MESH))
            ),
            _ => false,
        })
}

fn mesh_child(dom: &WeakDom, instance: &Instance, size: Vec3) -> Option<Geometry> {
    let child = legacy_mesh(dom, instance)?;
    let (scale, offset) = mesh_scale_offset(child);
    let size = size * scale;
    match child.class() {
        "BlockMesh" => Some(Geometry {
            kind: ShapeKind::Box,
            size,
            offset,
        }),
        // Along Y, and "maintaining a 1:1 ratio for the part's X and Z
        // axis, using the lowest value" (creator-docs `CylinderMesh`).
        "CylinderMesh" => {
            let diameter = size.x.min(size.z);
            Some(Geometry {
                kind: ShapeKind::CylinderY,
                size: Vec3::new(diameter, size.y, diameter),
                offset,
            })
        }
        _ => Some(special_mesh(child, size, offset)),
    }
}

/// `Enum.MeshType`, mapped onto the meshes this viewer draws. Every mapping
/// was measured against Studio's own render (front, side and top silhouettes
/// at several sizes), not taken from the enum's names.
///
/// Torso is a plain box, as sharp-cornered as Brick, not the bevelled
/// `torso.mesh` the docs' "default Humanoid torso" suggests. Head draws
/// Studio's `head.mesh`, a file mesh planned in `filemesh::fit`; the cylinder
/// here is only what shows until that mesh lands, at the same extent. The four
/// deprecated types draw nothing at all, see [`draws_nothing`].
fn special_mesh(mesh: &Instance, size: Vec3, offset: Vec3) -> Geometry {
    let Some(&Variant::Enum(mesh_type)) = mesh.properties().get("MeshType") else {
        return Geometry {
            kind: ShapeKind::Box,
            size,
            offset,
        };
    };
    let (kind, size) = match mesh_type {
        HEAD_MESH => (ShapeKind::CylinderY, head_extent(size)),
        2 => (ShapeKind::Wedge, size),
        3 => (ShapeKind::Ball, size),
        // Along X like a Cylinder part, not Y like a `CylinderMesh`, and as
        // wide as the smaller of Y and Z: a 4x8x4 one is 4 studs round.
        4 => {
            let diameter = size.y.min(size.z);
            (ShapeKind::CylinderX, Vec3::new(size.x, diameter, diameter))
        }
        11 => (ShapeKind::CornerWedge, size),
        _ => (ShapeKind::Box, size), // Torso, Brick, the deprecated four, unknown
    };
    Geometry { kind, size, offset }
}

/// The extent Studio draws a `MeshType.Head` at: Y as given, X and Z both the
/// smaller of the two, so a classic 2x1x1 head part (Scale 1.25) is a 1.25
/// stud round head, and an 8x4x4 one no wider than a 4x4x4 one.
pub(crate) fn head_extent(size: Vec3) -> Vec3 {
    let diameter = size.x.min(size.z);
    Vec3::new(diameter, size.y, diameter)
}

/// Whether `instance` wears a `SpecialMesh` of one of the deprecated types
/// (Prism, Pyramid, ParallelRamp, RightAngleRamp). Studio draws nothing for
/// those, the part keeps only its box to click and select.
pub(crate) fn draws_nothing(dom: &WeakDom, instance: &Instance) -> bool {
    legacy_mesh(dom, instance).is_some_and(|mesh| {
        mesh.class() == "SpecialMesh"
            && matches!(
                mesh.properties().get("MeshType"),
                Some(&Variant::Enum(7..=10))
            )
    })
}

fn mesh_scale_offset(mesh: &Instance) -> (Vec3, Vec3) {
    let vector3 = |name: &str, default: Vec3| match mesh.properties().get(name) {
        Some(&Variant::Vector3(v)) => Vec3::new(v.x, v.y, v.z),
        _ => default,
    };
    (vector3("Scale", Vec3::ONE), vector3("Offset", Vec3::ZERO))
}

#[cfg(test)]
#[path = "shape/tests.rs"]
mod tests;
