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
//! along its normal until it clears the item, then evens the push out over
//! its neighbours so the shell has no spikes.

use std::collections::HashMap;

use glam::Vec3;

use crate::mesh::{smooth_normals, weld, Mesh};

/// Lateral distance (studs) within which an item vertex counts as sitting
/// over a template vertex; wider would let far-off fabric push the cage.
const RADIUS: f32 = 0.3;
/// Farthest (studs) along the normal an item vertex can be and still be
/// treated as this vertex's clothing rather than another body part.
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
    let grid = Grid::of(&item.positions);
    let required: Vec<f32> = welded
        .positions
        .iter()
        .zip(&normals)
        .map(|(&v, &n)| grid.clearance(v, n))
        .collect();

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

/// The item's vertices bucketed by [`RADIUS`]-sized cells.
struct Grid(HashMap<(i32, i32, i32), Vec<Vec3>>);

fn cell_of(p: Vec3) -> (i32, i32, i32) {
    let q = (p / RADIUS).floor();
    (q.x as i32, q.y as i32, q.z as i32)
}

impl Grid {
    fn of(points: &[Vec3]) -> Grid {
        let mut cells: HashMap<(i32, i32, i32), Vec<Vec3>> = HashMap::new();
        for &p in points {
            cells.entry(cell_of(p)).or_default().push(p);
        }
        Grid(cells)
    }

    /// How far out along `n` the cage vertex at `v` must go to clear every
    /// item vertex over it (within [`RADIUS`] of the normal line, at most
    /// [`REACH`] out), plus [`MARGIN`]; 0 when nothing is over it. The cells
    /// are visited by stepping along the normal, so the search covers the
    /// whole reach without scanning a reach-sized cube.
    fn clearance(&self, v: Vec3, n: Vec3) -> f32 {
        let mut visited = std::collections::HashSet::new();
        let mut best = 0.0f32;
        let steps = (REACH / RADIUS).ceil() as i32;
        for step in 0..=steps {
            let (cx, cy, cz) = cell_of(v + n * (step as f32 * RADIUS));
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        let cell = (cx + dx, cy + dy, cz + dz);
                        if !visited.insert(cell) {
                            continue;
                        }
                        for &p in self.0.get(&cell).into_iter().flatten() {
                            let d = p - v;
                            let along = d.dot(n);
                            if along > 0.0 && along <= REACH && (d - n * along).length() <= RADIUS {
                                best = best.max(along + MARGIN);
                            }
                        }
                    }
                }
            }
        }
        best
    }
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
