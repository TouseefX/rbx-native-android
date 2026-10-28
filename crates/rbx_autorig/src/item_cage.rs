//! Inner and outer cages for a layered item, made from the body template
//! cage it was modelled on.
//!
//! Layered clothing pairs its cages with the body's and with other items'
//! by vertex and UV (creator-docs, `avatar/layered-accessories/specifications.md`,
//! "Cage meshes": "The vertexes and UVs of the inner and outer cage meshes
//! should not be deleted or removed, as they are used to match coordinates
//! between other cages"). So both cages here are the template moved, never
//! remeshed: the inner cage is the template as it stands (the body surface
//! the item was made on), and the outer cage pushes each template vertex out
//! along its normal until it clears the part of the item nearest it, then
//! evens the push out over its neighbours so the shell has no spikes. Each
//! item vertex pushes only the template vertex it is closest to: going by
//! what lies along the normal instead, a torso vertex would reach through
//! the gap to the sleeve and the shell would tent between arm and body.

use glam::Vec3;

use crate::mesh::{smooth_normals, weld, Mesh};

/// Farthest (studs) out the outer cage is pushed.
const REACH: f32 = 1.5;
/// Clearance (studs) left between the outer cage and the item surface so
/// the item is strictly inside the shell.
const MARGIN: f32 = 0.04;
/// Neighbour-averaging passes that smooth the push into a spike-free shell.
const SMOOTH_PASSES: usize = 8;

/// A layered item's two cages, in the item's space, each with the
/// template's vertex order, UVs and triangles.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemCages {
    pub inner: Mesh,
    pub outer: Mesh,
}

/// Template vertices closer than this are one vertex: the template is the
/// body's part cages put together, and their seam copies must move as one or
/// the shell opens at every joint.
const SEAM: f32 = 1e-3;

/// The cages of `item`, which was modelled around `template` (both in the
/// same space, studs).
pub fn item_cages(item: &Mesh, template: &Mesh) -> ItemCages {
    let mut inner = template.clone();
    inner.normals = smooth_normals(&template.positions, &template.triangles);

    let welded = weld(&template.positions, SEAM);
    let triangles: Vec<[u32; 3]> = template
        .triangles
        .iter()
        .map(|t| t.map(|i| welded.of[i as usize]))
        .collect();
    let normals = smooth_normals(&welded.positions, &triangles);
    let mut required = vec![0.0f32; welded.positions.len()];
    for &p in &item.positions {
        let Some(w) = closest(&welded.positions, p) else {
            break;
        };
        let along = (p - welded.positions[w]).dot(normals[w]);
        if along > 0.0 {
            required[w] = required[w].max((along + MARGIN).min(REACH + MARGIN));
        }
    }

    let mut neighbours: Vec<Vec<u32>> = vec![Vec::new(); welded.positions.len()];
    for t in &triangles {
        for k in 0..3 {
            neighbours[t[k] as usize].extend([t[(k + 1) % 3], t[(k + 2) % 3]]);
        }
    }
    for list in &mut neighbours {
        list.sort_unstable();
        list.dedup();
    }
    let cap = REACH + MARGIN;
    let mut push = required.clone();
    for _ in 0..SMOOTH_PASSES {
        push = (0..push.len())
            .map(|i| {
                let list = &neighbours[i];
                if list.is_empty() {
                    return push[i];
                }
                let avg = list.iter().map(|&j| push[j as usize]).sum::<f32>() / list.len() as f32;
                required[i].max(avg).min(cap)
            })
            .collect();
    }

    let mut outer = template.clone();
    for (i, p) in outer.positions.iter_mut().enumerate() {
        let w = welded.of[i] as usize;
        *p = welded.positions[w] + normals[w] * push[w];
    }
    outer.normals = smooth_normals(&outer.positions, &outer.triangles);
    ItemCages { inner, outer }
}

/// The index of the point in `points` closest to `at`.
fn closest(points: &[Vec3], at: Vec3) -> Option<usize> {
    points
        .iter()
        .enumerate()
        .min_by(|a, b| {
            a.1.distance_squared(at)
                .total_cmp(&b.1.distance_squared(at))
        })
        .map(|(i, _)| i)
}

/// Moves every vertex of `cage` within `radius` of `centre` along its own
/// normal by up to `amount` studs (negative pulls in), fading smoothly to
/// nothing at `radius`, and recomputes the normals. Vertex order, UVs and
/// triangles are untouched.
pub fn nudge(cage: &mut Mesh, centre: Vec3, radius: f32, amount: f32) {
    if radius <= 0.0 || amount == 0.0 {
        return;
    }
    let normals = smooth_normals(&cage.positions, &cage.triangles);
    for (p, n) in cage.positions.iter_mut().zip(&normals) {
        let d = p.distance(centre);
        if d < radius {
            let w = 0.5 * (1.0 + (std::f32::consts::PI * d / radius).cos());
            *p += *n * amount * w;
        }
    }
    cage.normals = smooth_normals(&cage.positions, &cage.triangles);
}

#[cfg(test)]
#[path = "item_cage/tests.rs"]
mod tests;
