//! Where a canvas' four corners end up in the world.
//!
//! A `SurfaceGui`'s are fixed and were resolved when the place was read; a
//! `BillboardGui`'s only exist once there is an eye to turn towards, so they
//! are rebuilt every frame here, the same split `renderer::beam::ribbon`
//! makes for `FaceCamera`.

use glam::{Mat4, Vec3};

use super::pipeline::VertexRaw;
use crate::scene::GuiAnchor;

/// Whether the eye is near enough for `MaxDistance` to let the canvas draw.
///
/// Measured to the canvas' own centre, the only point a `BillboardGui` has;
/// the property is a pop-in threshold rather than a precise cut, and the docs
/// describe it as a distance "from the camera".
pub(super) fn within(anchor: &GuiAnchor, eye: Vec3, max_distance: f32) -> bool {
    let centre = match *anchor {
        GuiAnchor::Surface { corners } => corners.iter().sum::<Vec3>() / 4.0,
        GuiAnchor::Billboard { origin, .. } => origin,
    };
    eye.distance(centre) <= max_distance
}

/// How many studs one screen pixel covers at a point, what turns the offset
/// half of a billboard's `Size` into studs every frame so it keeps its pixel
/// size at any distance.
#[derive(Debug, Clone, Copy)]
pub(super) struct PixelScale {
    view_projection: Mat4,
    /// The screen's height in the pixels a `UDim2` offset counts.
    height: f32,
}

impl PixelScale {
    pub(super) fn new(view_projection: Mat4, height: u32) -> Self {
        PixelScale {
            view_projection,
            height: height as f32,
        }
    }

    /// Studs per pixel at `point`. The view-projection's second row is the
    /// projection's vertical focal factor times the camera's unit up axis, so
    /// its length is that factor; its last row gives the point's clip `w`,
    /// its depth under a perspective camera and 1 under an orthographic one,
    /// so both come out right.
    fn at(&self, point: Vec3) -> f32 {
        let focal = self.view_projection.row(1).truncate().length();
        let depth = self.view_projection.row(3).dot(point.extend(1.0));
        if focal <= 0.0 || self.height <= 0.0 || depth <= 0.0 {
            return 0.0;
        }
        2.0 * depth / (focal * self.height)
    }
}

/// The canvas rectangle in image order: top-left, top-right, bottom-right,
/// bottom-left.
pub(super) fn corners(anchor: &GuiAnchor, eye: Vec3, scale: PixelScale) -> [Vec3; 4] {
    match *anchor {
        GuiAnchor::Surface { corners } => corners,
        GuiAnchor::Billboard {
            origin,
            size,
            pixels,
            view_offset,
            world_offset,
            size_offset,
        } => {
            let (right, up, forward) = basis(origin, eye);
            let studs_per_pixel = scale.at(origin + world_offset);
            let size = [0, 1].map(|axis| size[axis] + pixels[axis] * studs_per_pixel);
            // `SizeOffset` shifts the quad by that fraction of its own size,
            // which is what makes `0.5, 0.5` anchor the billboard at its
            // bottom left rather than its centre.
            let centre = origin
                + world_offset
                + right * (view_offset.x + size_offset[0] * size[0])
                + up * (view_offset.y + size_offset[1] * size[1])
                + forward * view_offset.z;
            let half_x = right * (size[0] * 0.5);
            let half_y = up * (size[1] * 0.5);
            [
                centre - half_x + half_y,
                centre + half_x + half_y,
                centre + half_x - half_y,
                centre - half_x - half_y,
            ]
        }
    }
}

/// Camera-facing `(right, up, forward)` for a billboard at `origin`.
///
/// Built off world up rather than off the view matrix, which is the same thing
/// here: this viewer's camera never rolls (see `camera::look_at_mat4`), so its
/// own right axis is exactly `Y × forward`. The degenerate cases, the eye
/// inside the billboard, or straight above it, fall back to fixed axes rather
/// than collapsing the quad to nothing.
fn basis(origin: Vec3, eye: Vec3) -> (Vec3, Vec3, Vec3) {
    let forward = non_zero((eye - origin).normalize_or_zero(), Vec3::Z);
    let right = non_zero(Vec3::Y.cross(forward).normalize_or_zero(), Vec3::X);
    (right, forward.cross(right), forward)
}

fn non_zero(axis: Vec3, fallback: Vec3) -> Vec3 {
    match axis == Vec3::ZERO {
        true => fallback,
        false => axis,
    }
}

/// Two triangles covering `corners`, wound the same way a screen-space GUI
/// quad is (see `renderer::gui::quads::quad`) so the canvas is not mirrored.
pub(super) fn vertices(corners: [Vec3; 4], brightness: f32, into: &mut Vec<VertexRaw>) {
    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let corner = |index: usize| VertexRaw {
        position: corners[index].to_array(),
        uv: uvs[index],
        brightness,
    };
    into.extend([
        corner(0),
        corner(1),
        corner(3),
        corner(1),
        corner(2),
        corner(3),
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;

    const EYE: Vec3 = Vec3::new(0.0, 0.0, 10.0);

    /// No pixel half, so the scale is never read.
    const FLAT: PixelScale = PixelScale {
        view_projection: Mat4::IDENTITY,
        height: 1.0,
    };

    fn billboard(size: [f32; 2], view_offset: Vec3, world_offset: Vec3) -> GuiAnchor {
        GuiAnchor::Billboard {
            origin: Vec3::ZERO,
            size,
            pixels: [0.0, 0.0],
            view_offset,
            world_offset,
            size_offset: [0.0, 0.0],
        }
    }

    #[test]
    fn a_billboard_faces_the_eye_and_is_centred_on_its_origin() {
        let corners = corners(&billboard([4.0, 2.0], Vec3::ZERO, Vec3::ZERO), EYE, FLAT);
        // Eye on +Z: the quad lies in the XY plane, image right along +X.
        assert_eq!(corners[0], Vec3::new(-2.0, 1.0, 0.0));
        assert_eq!(corners[1], Vec3::new(2.0, 1.0, 0.0));
        assert_eq!(corners[2], Vec3::new(2.0, -1.0, 0.0));
        assert_eq!(corners[3], Vec3::new(-2.0, -1.0, 0.0));
    }

    #[test]
    fn the_quad_turns_with_the_eye() {
        let corners = corners(
            &billboard([2.0, 2.0], Vec3::ZERO, Vec3::ZERO),
            Vec3::new(10.0, 0.0, 0.0),
            FLAT,
        );
        // Eye on +X: the quad now lies in the YZ plane, image right along -Z.
        assert!(corners.iter().all(|corner| corner.x.abs() < 1e-6));
        assert!((corners[1].z + 1.0).abs() < 1e-6);
    }

    #[test]
    fn studs_offset_is_read_in_the_cameras_own_basis() {
        let corners = corners(
            &billboard([0.0, 0.0], Vec3::new(1.0, 2.0, 3.0), Vec3::ZERO),
            EYE,
            FLAT,
        );
        // Right is +X, up is +Y and forward points back at the eye (+Z).
        assert_eq!(corners[0], Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn studs_offset_world_space_is_read_along_the_global_axes() {
        let corners = corners(
            &billboard([0.0, 0.0], Vec3::ZERO, Vec3::new(0.0, 5.0, 0.0)),
            Vec3::new(10.0, 0.0, 0.0),
            FLAT,
        );
        assert_eq!(corners[0], Vec3::new(0.0, 5.0, 0.0));
    }

    #[test]
    fn an_eye_directly_above_a_billboard_still_gives_it_a_width() {
        let corners = corners(
            &billboard([4.0, 2.0], Vec3::ZERO, Vec3::ZERO),
            Vec3::new(0.0, 10.0, 0.0),
            FLAT,
        );
        assert!((corners[1] - corners[0]).length() > 3.9);
    }

    #[test]
    fn a_surface_quad_is_handed_back_untouched() {
        let fixed = [
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::new(7.0, 8.0, 9.0),
            Vec3::new(10.0, 11.0, 12.0),
        ];
        assert_eq!(
            corners(&GuiAnchor::Surface { corners: fixed }, EYE, FLAT),
            fixed
        );
    }

    #[test]
    fn the_two_triangles_cover_the_quad_in_image_order() {
        let fixed = [Vec3::X, Vec3::Y, Vec3::Z, Vec3::ZERO];
        let mut built = Vec::new();
        vertices(fixed, 1.0, &mut built);
        assert_eq!(built.len(), 6);
        assert_eq!(built[0].uv, [0.0, 0.0]);
        assert_eq!(built[1].uv, [1.0, 0.0]);
        assert_eq!(built[2].uv, [0.0, 1.0]);
        assert_eq!(built[4].uv, [1.0, 1.0]);
        assert_eq!(built[0].position, Vec3::X.to_array());
    }

    #[test]
    fn size_offset_shifts_the_quad_by_a_fraction_of_its_own_size() {
        // `0.5, 0.5` "will anchor at the bottom left", so the origin has to
        // land on the quad's bottom-left corner.
        let anchor = GuiAnchor::Billboard {
            origin: Vec3::ZERO,
            size: [4.0, 2.0],
            pixels: [0.0, 0.0],
            view_offset: Vec3::ZERO,
            world_offset: Vec3::ZERO,
            size_offset: [0.5, 0.5],
        };
        assert_eq!(corners(&anchor, EYE, FLAT)[3], Vec3::ZERO);
    }

    /// A 100px-tall billboard on a 1000px screen under a 90° vertical field of
    /// view: a tenth of the screen's height, at any distance.
    #[test]
    fn the_pixel_half_of_a_size_stays_the_same_size_on_screen() {
        let height = |distance: f32| {
            let eye = Vec3::new(0.0, 0.0, distance);
            let view = glam::camera::rh::view::look_at_mat4(eye, Vec3::ZERO, Vec3::Y);
            let projection = glam::camera::rh::proj::directx::perspective(
                std::f32::consts::FRAC_PI_2,
                1.0,
                0.1,
                1e4,
            );
            let anchor = GuiAnchor::Billboard {
                origin: Vec3::ZERO,
                size: [0.0, 0.0],
                pixels: [200.0, 100.0],
                view_offset: Vec3::ZERO,
                world_offset: Vec3::ZERO,
                size_offset: [0.0, 0.0],
            };
            let corners = corners(&anchor, eye, PixelScale::new(projection * view, 1000));
            // The view at `distance` spans 2 * distance studs (tan 45° = 1).
            (corners[0].y - corners[3].y) / (2.0 * distance)
        };
        assert!((height(10.0) - 0.1).abs() < 1e-4);
        assert!((height(300.0) - 0.1).abs() < 1e-4);
    }

    #[test]
    fn max_distance_cuts_a_canvas_off_beyond_its_limit() {
        let anchor = billboard([1.0, 1.0], Vec3::ZERO, Vec3::ZERO);
        assert!(within(&anchor, EYE, 10.0));
        assert!(!within(&anchor, EYE, 9.0));
        assert!(within(&anchor, EYE, f32::INFINITY));
    }

    #[test]
    fn brightness_rides_on_every_vertex_of_the_quad() {
        let mut built = Vec::new();
        vertices([Vec3::X, Vec3::Y, Vec3::Z, Vec3::ZERO], 4.0, &mut built);
        assert!(built.iter().all(|vertex| vertex.brightness == 4.0));
    }
}
