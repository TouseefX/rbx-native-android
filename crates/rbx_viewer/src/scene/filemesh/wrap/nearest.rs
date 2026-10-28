//! The nearest-vertex fit, for an inner cage that is not made from the
//! body's template (see the module doc of [`super`]).

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};
use rbx_assets::AssetRef;

use super::{body, Wrap, BLEND, LIFT};

/// A body cage vertex further than this from its reference vertex is another
/// limb's, not this one's resized.
const MAX_SHIFT: f32 = 1.5;
/// Reference vertices sampled per registration pass.
const SAMPLES: usize = 600;
/// Registration passes per starting offset.
const PASSES: usize = 10;
/// The share of the closest matches that steer registration, so a sleeve the
/// body lacks does not drag the whole reference after it.
const TRIM: f32 = 0.7;
/// The nearest-vertex fit, for an inner cage the surface does not pair with.
pub(super) fn deform(
    wrap: &Wrap,
    model: Mat4,
    garment: &rbx_mesh::Mesh,
    meshes: &HashMap<AssetRef, Arc<rbx_mesh::SkinnedMesh>>,
) -> Option<rbx_mesh::Mesh> {
    let reference = meshes.get(&wrap.reference)?;
    let (body, _) = body(wrap, meshes)?;
    let to_world = model * wrap.origin;
    let reference_points: Vec<Vec3> = reference
        .vertices
        .iter()
        .map(|v| to_world.transform_point3(Vec3::from(v.position)))
        .collect();
    let slide = register(&reference_points, &body);
    // Where each reference vertex stands, and how far its body counterpart is.
    let anchors: Vec<(Vec3, Vec3)> = reference_points
        .iter()
        .filter_map(|&point| {
            let at = point + slide;
            let shift = nearest(&body, at)? - at;
            (shift.length() <= MAX_SHIFT).then_some((at, shift))
        })
        .collect();
    if anchors.is_empty() {
        return None;
    }
    let back = model.inverse();
    let mut out = garment.clone();
    for vertex in &mut out.vertices {
        let at = model.transform_point3(Vec3::from(vertex.position)) + slide;
        let normal = model
            .transform_vector3(Vec3::from(vertex.normal))
            .normalize_or_zero();
        let moved = at + blended(&anchors, at) + normal * LIFT * (wrap.order + 1.0);
        vertex.position = back.transform_point3(moved).to_array();
    }
    Some(out)
}

/// The translation that lays the reference over the body: nearest-vertex
/// matching from a few starting offsets, each pass moving by the mean of the
/// closest [`TRIM`] of the gaps, and the start with the least left over wins.
fn register(reference: &[Vec3], body: &[Vec3]) -> Vec3 {
    let stride = (reference.len() / SAMPLES).max(1);
    let sample: Vec<Vec3> = reference.iter().copied().step_by(stride).collect();
    let (rb, bb) = (bounds(reference), bounds(body));
    let starts = [
        Vec3::ZERO,
        (bb.0 + bb.1 - rb.0 - rb.1) * 0.5,
        Vec3::new(0.0, bb.0.y - rb.0.y, 0.0),
        Vec3::new(0.0, bb.1.y - rb.1.y, 0.0),
    ];
    let gaps = |slide: Vec3| -> Vec<Vec3> {
        let mut gaps: Vec<Vec3> = sample
            .iter()
            .filter_map(|&p| nearest(body, p + slide).map(|n| n - p - slide))
            .collect();
        gaps.sort_by(|a, b| a.length_squared().total_cmp(&b.length_squared()));
        gaps.truncate(((gaps.len() as f32 * TRIM).ceil() as usize).max(1));
        gaps
    };
    let mean = |g: &[Vec3]| g.iter().copied().sum::<Vec3>() / g.len() as f32;
    let mut best = (f32::INFINITY, Vec3::ZERO);
    for start in starts {
        let mut slide = start;
        for _ in 0..PASSES {
            let step = mean(&gaps(slide));
            slide += step;
            if step.length_squared() < 1e-8 {
                break;
            }
        }
        let left = gaps(slide);
        let left = left.iter().map(|g| g.length()).sum::<f32>() / left.len() as f32;
        if left < best.0 - 1e-6 {
            best = (left, slide);
        }
    }
    best.1
}

fn bounds(points: &[Vec3]) -> (Vec3, Vec3) {
    points.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), &p| (lo.min(p), hi.max(p)),
    )
}

fn nearest(points: &[Vec3], at: Vec3) -> Option<Vec3> {
    points
        .iter()
        .copied()
        .min_by(|a, b| a.distance_squared(at).total_cmp(&b.distance_squared(at)))
}

/// The inverse-square-distance blend of the [`BLEND`] anchors nearest `at`.
fn blended(anchors: &[(Vec3, Vec3)], at: Vec3) -> Vec3 {
    let mut near = [(f32::INFINITY, Vec3::ZERO); BLEND];
    for &(anchor, shift) in anchors {
        let d = anchor.distance_squared(at);
        if d < near[BLEND - 1].0 {
            near[BLEND - 1] = (d, shift);
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
    }
    let (mut sum, mut weight) = (Vec3::ZERO, 0.0);
    for (d, shift) in near.iter().filter(|(d, _)| d.is_finite()) {
        let w = 1.0 / (d + 1e-6);
        sum += *shift * w;
        weight += w;
    }
    if weight > 0.0 {
        sum / weight
    } else {
        Vec3::ZERO
    }
}
