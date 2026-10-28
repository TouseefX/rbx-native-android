//! The head rules that need no landmarks.

use super::{finding, measured, Head, HeadRule, REQUIRED_POSES};
use crate::spec::{body_bounds, BodyGroup};
use crate::{Finding, Value};

pub(super) fn face_controls(head: &Head, out: &mut Vec<Finding>) {
    if !head.face_controls {
        out.push(finding(
            HeadRule::FaceControls,
            &head.name,
            format!(
                "{} has no FaceControls. Add a FaceControls named FaceControls under the head so its FACS poses can be driven.",
                head.name
            ),
        ));
    }
}

pub(super) fn poses(head: &Head, out: &mut Vec<Finding>) {
    if head.mesh.is_none() {
        return;
    }
    let Some(facs) = &head.facs else {
        out.push(finding(
            HeadRule::FacsPoses,
            &head.name,
            format!(
                "{} has no FACS data. A dynamic head needs at least the 17 FACS poses the specification lists.",
                head.name
            ),
        ));
        return;
    };
    let missing: Vec<&str> = REQUIRED_POSES
        .into_iter()
        .filter(|p| !facs.controls.iter().any(|c| c == p))
        .collect();
    if !missing.is_empty() {
        out.push(measured(
            finding(
                HeadRule::FacsPoses,
                &head.name,
                format!(
                    "{} is missing FACS poses: {}. All 17 are needed for avatar chat.",
                    head.name,
                    missing.join(", ")
                ),
            ),
            Value::Count(REQUIRED_POSES.len() - missing.len()),
            Value::Count(REQUIRED_POSES.len()),
        ));
    }
    let inactive: Vec<&str> = REQUIRED_POSES
        .into_iter()
        .filter(|p| facs.controls.iter().any(|c| c == p) && !facs.active.iter().any(|c| c == p))
        .collect();
    if !inactive.is_empty() {
        out.push(finding(
            HeadRule::FacsPoses,
            &head.name,
            format!(
                "{} has FACS poses that move nothing: {}. Pose the face for each of them.",
                head.name,
                inactive.join(", ")
            ),
        ));
    }
}

pub(super) fn part_settings(head: &Head, out: &mut Vec<Finding>) {
    let mut wrong = Vec::new();
    if head.material != "Plastic" {
        wrong.push(format!("Material is {}, not Plastic", head.material));
    }
    if head.transparency != 0.0 {
        wrong.push(format!("Transparency is {}, not 0", head.transparency));
    }
    if head.vertex_color != [1.0, 1.0, 1.0] {
        let [r, g, b] = head.vertex_color;
        wrong.push(format!("VertexColor is {r}, {g}, {b}, not 1, 1, 1"));
    }
    for issue in wrong {
        out.push(finding(
            HeadRule::PartSettings,
            &head.name,
            format!("{issue}. The Marketplace takes dynamic heads with the default look only."),
        ));
    }
    if !head.extraneous.is_empty() {
        out.push(finding(
            HeadRule::Extraneous,
            &head.name,
            format!(
                "Remove {} from the model; a body with a dynamic head may not carry scripts or extra parts.",
                head.extraneous.join(", ")
            ),
        ));
    }
}

/// Unique UVs, to 1e-4, in Roblox's head cage template (Studio's own count,
/// 2026-10-28; the template cage itself has 352 bit-exact ones).
pub(super) const TEMPLATE_UVS: usize = 343;

/// Studio's checks no page states, seen in its Save to Roblox on Roblox's
/// own head (see `HeadRule::Schema`).
pub(super) fn schema(head: &Head, out: &mut Vec<Finding>) {
    if !head.scale_type {
        out.push(finding(
            HeadRule::Schema,
            &head.name,
            "Add a StringValue named AvatarPartScaleType under the head part; Studio refuses a head without one.".into(),
        ));
    }
    if head.collision_fidelity != "Default" {
        out.push(finding(
            HeadRule::Schema,
            &head.name,
            format!(
                "CollisionFidelity is {}, not Default. Studio refuses any other value on a head.",
                head.collision_fidelity
            ),
        ));
    }
    let Some(cage) = &head.cage else { return };
    let unique: std::collections::BTreeSet<(i64, i64)> = cage
        .uvs
        .iter()
        .map(|uv| ((uv[0] * 1e4).round() as i64, (uv[1] * 1e4).round() as i64))
        .collect();
    if unique.len() != TEMPLATE_UVS {
        out.push(measured(
            finding(
                HeadRule::CageTemplate,
                &head.name,
                format!(
                    "The outer cage has {} unique UVs, Roblox's head template has {TEMPLATE_UVS}. Start the cage from the template and only move its vertices.",
                    unique.len()
                ),
            ),
            Value::Count(unique.len()),
            Value::Count(TEMPLATE_UVS),
        ));
    }
}

fn extent(points: &[[f32; 3]]) -> [f32; 3] {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in points {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    std::array::from_fn(|a| (hi[a] - lo[a]).max(0.0))
}

/// Every pose has to stay inside the head's bounding box limit, not just
/// the neutral face.
pub(super) fn emote_bounds(head: &Head, out: &mut Vec<Finding>) {
    let Some(facs) = &head.facs else { return };
    let (_, max) = body_bounds(Some(BodyGroup::Head));
    let over = |size: &[f32; 3]| (0..3).map(|a| size[a] / max[a]).fold(0.0, f32::max);
    let worst = facs
        .poses
        .iter()
        .map(|(name, points)| (name, extent(points)))
        .filter(|(_, size)| over(size) > 1.0)
        .max_by(|a, b| over(&a.1).total_cmp(&over(&b.1)));
    if let Some((pose, size)) = worst {
        out.push(measured(
            finding(
                HeadRule::EmoteBounds,
                &head.name,
                format!(
                    "{} grows past the head's bounding box limit when posed as {pose}. Keep every FACS pose inside the box.",
                    head.name
                ),
            ),
            Value::Studs(size),
            Value::Studs(max),
        ));
    }
}
