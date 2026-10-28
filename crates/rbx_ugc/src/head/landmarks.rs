//! The eye and mouth landmarks: found in the outer cage by their template
//! UVs, projected onto the head mesh, then watched through the 5 actions.
//!
//! The pages say which cage vertices mark the face but not their numbers.
//! Cage UVs never change from Roblox's template (the layered accessory
//! specification forbids touching them), so a landmark is the set of cage
//! vertices at fixed UVs:
//! - each eye is its eyelid loop, the 8 "inner boundary" UVs of the eye
//!   reference Studio's own validator ships for eyelashes
//!   (`EyelashCageUVReferenceValues` in its UGCValidation package);
//! - the mouth is the second loop of vertices out from the lips, as the
//!   specification says, read off the green and purple lip vertices of
//!   Roblox's own dynamic head cage (asset 76523574292282, the head of
//!   Studio's `characterR15DynamicHeadV2`).
//!
//! Roblox scores the projection and the movement with a model it does not
//! publish. The two limits below are ours: a landmark may sit at most
//! [`PROJECT_LIMIT`] of the head's height off the mesh, and an action passes
//! when its landmark's projected points move on average at least
//! [`MOVE_FLOOR`] of it. On Roblox's own head its landmarks sit at most 1.3%
//! of its height off the mesh and the weakest action (sad, on the mouth)
//! moves its landmark 3.7%.

use super::{finding, measured, Head, HeadRule, ACTIONS};
use crate::{Finding, Geometry, Value};

/// How close a cage UV has to be to a template UV to be that vertex.
const UV_MATCH: f32 = 2e-3;
/// Furthest a landmark vertex may sit from the mesh, as a share of the
/// head's height.
pub const PROJECT_LIMIT: f32 = 0.08;
/// Least average movement of a landmark in its action, as a share of the
/// head's height.
pub const MOVE_FLOOR: f32 = 0.01;

pub(super) const RIGHT_EYE: [[f32; 2]; 8] = [
    [3.4079, 0.4220],
    [3.4214, 0.4103],
    [3.4343, 0.4103],
    [3.4472, 0.4103],
    [3.4629, 0.4216],
    [3.4472, 0.4278],
    [3.4343, 0.4279],
    [3.4214, 0.4279],
];
pub(super) const LEFT_EYE: [[f32; 2]; 8] = [
    [3.5371, 0.4216],
    [3.5528, 0.4103],
    [3.5657, 0.4103],
    [3.5786, 0.4103],
    [3.5921, 0.4220],
    [3.5786, 0.4279],
    [3.5657, 0.4279],
    [3.5528, 0.4278],
];
pub(super) const MOUTH: [[f32; 2]; 12] = [
    [3.4417, 0.5771],
    [3.4682, 0.5594],
    [3.4871, 0.5498],
    [3.5000, 0.5521],
    [3.5129, 0.5498],
    [3.5318, 0.5594],
    [3.5583, 0.5771],
    [3.5320, 0.5963],
    [3.5160, 0.6014],
    [3.5000, 0.6035],
    [3.4840, 0.6014],
    [3.4680, 0.5963],
];

/// The three landmarks the page names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Landmark {
    LeftEye,
    RightEye,
    Mouth,
}

impl Landmark {
    pub const ALL: [Landmark; 3] = [Landmark::LeftEye, Landmark::RightEye, Landmark::Mouth];

    pub fn label(self) -> &'static str {
        match self {
            Landmark::LeftEye => "left eye",
            Landmark::RightEye => "right eye",
            Landmark::Mouth => "mouth",
        }
    }

    fn uvs(self) -> &'static [[f32; 2]] {
        match self {
            Landmark::LeftEye => &LEFT_EYE,
            Landmark::RightEye => &RIGHT_EYE,
            Landmark::Mouth => &MOUTH,
        }
    }
}

/// Where a point landed on the mesh: triangle and barycentric weights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub triangle: usize,
    pub weights: [f32; 3],
    pub distance: f32,
}

/// One landmark as found: its cage vertices (in the head part's space) and
/// where each projects onto the mesh, `None` past the limit.
#[derive(Debug, Clone, PartialEq)]
pub struct LandmarkPoints {
    pub landmark: Landmark,
    pub cage: Vec<[f32; 3]>,
    /// Template UVs with no cage vertex.
    pub missing: usize,
    pub hits: Vec<Option<Hit>>,
}

impl LandmarkPoints {
    /// The projected points on `positions` (the rest mesh or a posed copy).
    pub fn on(&self, mesh: &Geometry, positions: &[[f32; 3]]) -> Vec<[f32; 3]> {
        self.hits
            .iter()
            .flatten()
            .filter_map(|hit| {
                let tri = mesh.triangles.get(hit.triangle)?;
                let corner = |i: usize| positions.get(tri[i] as usize).copied();
                let (a, b, c) = (corner(0)?, corner(1)?, corner(2)?);
                let w = hit.weights;
                Some(std::array::from_fn(|k| {
                    w[0] * a[k] + w[1] * b[k] + w[2] * c[k]
                }))
            })
            .collect()
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

/// Closest point on triangle `abc` to `p` as barycentric weights (Ericson,
/// Real-Time Collision Detection, 5.1.5).
fn closest(p: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return [1.0, 0.0, 0.0];
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return [0.0, 1.0, 0.0];
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return [1.0 - v, v, 0.0];
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return [0.0, 0.0, 1.0];
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return [1.0 - w, 0.0, w];
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return [0.0, 1.0 - w, w];
    }
    let denom = 1.0 / (va + vb + vc);
    let (v, w) = (vb * denom, vc * denom);
    [1.0 - v - w, v, w]
}

fn project(p: [f32; 3], mesh: &Geometry) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    for (triangle, tri) in mesh.triangles.iter().enumerate() {
        let corner = |i: usize| mesh.positions.get(tri[i] as usize).copied();
        let (Some(a), Some(b), Some(c)) = (corner(0), corner(1), corner(2)) else {
            continue;
        };
        let weights = closest(p, a, b, c);
        let q: [f32; 3] =
            std::array::from_fn(|k| weights[0] * a[k] + weights[1] * b[k] + weights[2] * c[k]);
        let distance = length(sub(p, q));
        if best.is_none_or(|b| distance < b.distance) {
            best = Some(Hit {
                triangle,
                weights,
                distance,
            });
        }
    }
    best
}

fn height(mesh: &Geometry) -> f32 {
    let (lo, hi) = mesh
        .positions
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), p| {
            (lo.min(p[1]), hi.max(p[1]))
        });
    (hi - lo).max(f32::EPSILON)
}

/// The three landmarks of `cage` projected onto `mesh`.
pub fn find(cage: &Geometry, mesh: &Geometry) -> Vec<LandmarkPoints> {
    let limit = PROJECT_LIMIT * height(mesh);
    Landmark::ALL
        .into_iter()
        .map(|landmark| {
            let points: Vec<[f32; 3]> = landmark
                .uvs()
                .iter()
                .filter_map(|uv| {
                    let at = cage.uvs.iter().position(|c| {
                        (c[0] - uv[0]).abs() < UV_MATCH && (c[1] - uv[1]).abs() < UV_MATCH
                    })?;
                    cage.positions.get(at).copied()
                })
                .collect();
            let hits = points
                .iter()
                .map(|p| project(*p, mesh).filter(|h| h.distance <= limit))
                .collect();
            LandmarkPoints {
                landmark,
                missing: landmark.uvs().len() - points.len(),
                cage: points,
                hits,
            }
        })
        .collect()
}

pub(super) fn check(head: &Head, out: &mut Vec<Finding>) {
    let Some(mesh) = &head.mesh else { return };
    let Some(cage) = &head.cage else {
        out.push(finding(
            HeadRule::Landmarks,
            &head.name,
            format!(
                "{} has no outer cage. The WrapTarget\u{2019}s cage marks the eyes and mouth that validation looks for.",
                head.name
            ),
        ));
        return;
    };
    let found = find(cage, mesh);
    let mut projected = Vec::new();
    for points in &found {
        let label = points.landmark.label();
        if points.missing > 0 {
            out.push(measured(
                finding(
                    HeadRule::Landmarks,
                    &head.name,
                    format!(
                        "The cage has no {label} landmark: {} of its template vertices are gone or their UVs moved. Start the cage from Roblox\u{2019}s template and keep its UVs.",
                        points.missing
                    ),
                ),
                Value::Count(points.cage.len()),
                Value::Count(points.landmark.uvs().len()),
            ));
        } else if points.hits.iter().any(Option::is_none) {
            out.push(finding(
                HeadRule::Landmarks,
                &head.name,
                format!(
                    "The {label} landmark can\u{2019}t be projected onto the head: the cage sits too far from the mesh there. Bring the cage\u{2019}s {label} vertices right over the face, without gaps under them."
                ),
            ));
        } else {
            projected.push(points);
        }
    }
    let Some(facs) = &head.facs else { return };
    let floor = MOVE_FLOOR * height(mesh);
    let rest = &mesh.positions;
    for action in ACTIONS {
        let Some(points) = projected.iter().find(|p| p.landmark == action.landmark) else {
            continue;
        };
        let Some((_, posed)) = facs.poses.iter().find(|(n, _)| n == action.name) else {
            continue;
        };
        let (before, after) = (points.on(mesh, rest), points.on(mesh, posed));
        let moved = before
            .iter()
            .zip(&after)
            .map(|(a, b)| length(sub(*a, *b)))
            .sum::<f32>()
            / before.len().max(1) as f32;
        if moved < floor {
            out.push(finding(
                HeadRule::FacialAction,
                &head.name,
                format!(
                    "{}: the {} landmark barely moves ({moved:.3} studs on average). Make sure the {} poses move the face right under the cage\u{2019}s {} vertices.",
                    action.name,
                    action.landmark.label(),
                    action.controls.iter().map(|(c, _)| *c).collect::<Vec<_>>().join(", "),
                    action.landmark.label()
                ),
            ));
        }
    }
}
