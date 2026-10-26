//! Where a canvas' rectangle is in the world and how many pixels it gets:
//! the face of a part a `SurfaceGui` covers, and the stud size a
//! `BillboardGui` asks for.

use std::collections::BTreeMap;

use glam::Vec3;
use rbx_dom::Variant;

use super::super::plan::{float, vector2, Span};
use super::{DEFAULT_CANVAS, FIXED_SIZE, MAX_CANVAS, PIXELS_PER_STUD};
use crate::scene::Placement;
use crate::textures::NormalId;

/// `Face`, Front by default like Roblox itself.
pub(in crate::scene::gui) fn face(properties: &BTreeMap<String, Variant>) -> NormalId {
    match properties.get("Face") {
        Some(&Variant::Enum(raw)) => NormalId::from_ordinal(raw).unwrap_or(NormalId::Front),
        _ => NormalId::Front,
    }
}

/// The canvas' pixel size for a billboard's `Size`: the stud half at
/// [`PIXELS_PER_STUD`] plus the offset half, which is pixels already. An
/// offset-only billboard then lays its contents out one canvas pixel per
/// screen pixel, the size it is drawn at whatever the distance.
pub(in crate::scene::gui) fn billboard_canvas(size: Span) -> [f32; 2] {
    let (studs, pixels) = (studs(size), pixels(size));
    [0, 1].map(|axis| {
        (studs[axis] * PIXELS_PER_STUD + pixels[axis])
            .clamp(0.0, MAX_CANVAS)
            .round()
    })
}

/// The pixel size of a `SurfaceGui`'s canvas: the face's stud size at
/// `PixelsPerStud` by default, `CanvasSize` under `SizingMode.FixedSize`.
/// Either way the canvas is stretched over the whole face, so the two only
/// differ in how many pixels a `UDim2` offset comes to.
///
/// Falls back to [`DEFAULT_CANVAS`] per axis where the result is degenerate,
/// a zero axis would ask for a texture no adapter will allocate.
pub(in crate::scene::gui) fn surface_canvas(
    properties: &BTreeMap<String, Variant>,
    face_studs: [f32; 2],
) -> [f32; 2] {
    let raw = match properties.get("SizingMode") {
        Some(&Variant::Enum(FIXED_SIZE)) => vector2(properties, "CanvasSize"),
        _ => {
            let density = float(properties, "PixelsPerStud", PIXELS_PER_STUD);
            face_studs.map(|studs| studs * density)
        }
    };
    let axis = |value: f32, default: f32| match value.is_finite() && value >= 1.0 {
        true => value.min(MAX_CANVAS).round(),
        false => default,
    };
    [
        axis(raw[0], DEFAULT_CANVAS[0]),
        axis(raw[1], DEFAULT_CANVAS[1]),
    ]
}

/// The scale half of a `BillboardGui.Size`: its world width and height in
/// studs, which shrink with distance like any other geometry.
///
/// Roblox gives the two halves of that `UDim2` different units: this one is
/// studs in 3D, the other ([`pixels`]) a screen size that does not shrink.
pub(in crate::scene::gui) fn studs(size: Span) -> [f32; 2] {
    size.scale.map(|scale| scale.max(0.0))
}

/// The offset half of a `BillboardGui.Size`, in screen pixels. Only the
/// renderer can turn it into studs: one pixel covers more of the world the
/// further the billboard is from the eye.
pub(in crate::scene::gui) fn pixels(size: Span) -> [f32; 2] {
    size.offset.map(|offset| offset.max(0.0))
}

/// The four world corners of `face` on a part, in image order.
///
/// The same rectangle a stretched `Decal` covers: [`NormalId::axes`] is what
/// `crate::textures::face` builds its own projection from, so the canvas and a
/// decal on the very same face land on exactly the same quad.
pub(in crate::scene::gui) fn face_corners(
    face: NormalId,
    placement: &Placement,
    z_offset: f32,
) -> [Vec3; 4] {
    let (normal, u, v) = face.axes();
    // The unit mesh spans [-0.5, 0.5]³, so the face plane sits half a unit
    // along its own normal and the image axes span the other two.
    let centre = normal * 0.5;
    let model = &placement.model;
    let push = model.transform_vector3(normal).normalize_or_zero() * z_offset;
    let corner = |right: f32, down: f32| {
        model.transform_point3(centre + u * (right * 0.5) + v * (down * 0.5)) + push
    };
    [
        corner(-1.0, -1.0),
        corner(1.0, -1.0),
        corner(1.0, 1.0),
        corner(-1.0, 1.0),
    ]
}

/// Width and height in studs of a face quad in image order, as the part is
/// actually placed, so a scaled `Placement` sizes the canvas like Roblox
/// sizes it off the part's own `Size`.
pub(in crate::scene::gui) fn face_studs(corners: &[Vec3; 4]) -> [f32; 2] {
    [
        corners[1].distance(corners[0]),
        corners[3].distance(corners[0]),
    ]
}
