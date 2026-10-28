//! Cage-to-cage mapping: a garment's inner cage paired with the surface it
//! is worn over, vertex for vertex, by UV.
//!
//! Roblox's body cages and every layered item's cages are made from one
//! template, and the pairing key is the template UV: the layered
//! specification says the cage "vertexes and UVs ... are used to match
//! coordinates between other cages". Checked on real assets: every UV of
//! the stock Mannequin's UpperTorso cage (8650495410) is in the Denim
//! Jacket's inner cage (8917459593), and within either cage one UV is always
//! one position (repeats are render splits of a vertex). So a garment's
//! inner cage vertex goes exactly where the body cage vertex with its UV
//! is. Roblox's own deformer between those pairs is not published; here each
//! garment vertex follows the eight inner cage vertices nearest it, each
//! carrying the garment's offset turned the way the surface turned there, by
//! inverse square distance.

use std::collections::HashMap;

use glam::{Quat, Vec3};

/// Inner cage vertices blended per garment vertex.
const NEAREST: usize = 8;
/// The UV grid a key snaps to: the template's UVs are exact copies, far
/// closer than this, and distinct template vertices sit far wider apart.
const UV_STEP: f32 = 1e-4;
/// The share of an inner cage's UVs the surface must answer for the pairing
/// to be the template's; a cage of some other topology falls back to
/// nearest-vertex fitting.
const MIN_MATCHED: f32 = 0.6;
/// Fewer distinct UVs than this is no template cage (an unwrapped or
/// UV-less mesh puts every vertex on a handful of keys).
const MIN_KEYS: usize = 32;

type Key = (i32, i32);

fn key(uv: [f32; 2]) -> Key {
    (
        (uv[0] / UV_STEP).round() as i32,
        (uv[1] / UV_STEP).round() as i32,
    )
}

/// One cage in the world: positions, UVs and triangles.
pub(super) struct WorldCage<'a> {
    pub(super) positions: Vec<Vec3>,
    pub(super) mesh: &'a rbx_mesh::Mesh,
}

impl WorldCage<'_> {
    fn uv(&self, i: usize) -> [f32; 2] {
        self.mesh.vertices[i].uv
    }

    fn normals(&self) -> Vec<Vec3> {
        let mut normals = vec![Vec3::ZERO; self.positions.len()];
        for &[a, b, c] in self.mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = [a, b, c].map(|i| i as usize);
            let (Some(&pa), Some(&pb), Some(&pc)) = (
                self.positions.get(a),
                self.positions.get(b),
                self.positions.get(c),
            ) else {
                continue;
            };
            let n = (pb - pa).cross(pc - pa);
            for i in [a, b, c] {
                normals[i] += n;
            }
        }
        normals.into_iter().map(Vec3::normalize_or_zero).collect()
    }
}

/// The surface a layer is worn over, by template UV: the body's cages, then
/// each lower layer's outer cage laid over them.
#[derive(Default)]
pub(super) struct Surface {
    points: HashMap<Key, (Vec3, Vec3)>,
}

impl Surface {
    /// Lays `cage` over what is there: its UVs now answer with its own
    /// positions and normals. Seam copies of one UV (a vertex shared by two
    /// body parts' cages) are averaged.
    pub(super) fn lay(&mut self, cage: &WorldCage) {
        let normals = cage.normals();
        let mut sums: HashMap<Key, (Vec3, Vec3, f32)> = HashMap::new();
        for (i, &p) in cage.positions.iter().enumerate() {
            let entry = sums
                .entry(key(cage.uv(i)))
                .or_insert((Vec3::ZERO, Vec3::ZERO, 0.0));
            entry.0 += p;
            entry.1 += normals[i];
            entry.2 += 1.0;
        }
        for (k, (p, n, count)) in sums {
            self.points.insert(k, (p / count, n.normalize_or_zero()));
        }
    }
}

/// One inner cage vertex and where it is worn: rest position, and the
/// target position and turn of the surface there.
struct Anchor {
    rest: Vec3,
    target: Vec3,
    turn: Quat,
}

/// A garment's inner cage paired with the surface it is worn over.
pub(super) struct CageMap {
    anchors: Vec<Anchor>,
}

impl CageMap {
    /// Pairs `inner` with `surface` by UV, or `None` when the inner cage is
    /// not made from the surface's template.
    pub(super) fn pair(inner: &WorldCage, surface: &Surface) -> Option<CageMap> {
        let normals = inner.normals();
        let mut seen: HashMap<Key, ()> = HashMap::new();
        let mut anchors = Vec::new();
        for (i, &rest) in inner.positions.iter().enumerate() {
            let k = key(inner.uv(i));
            if seen.insert(k, ()).is_some() {
                continue;
            }
            if let Some(&(target, target_normal)) = surface.points.get(&k) {
                let turn = if normals[i] == Vec3::ZERO || target_normal == Vec3::ZERO {
                    Quat::IDENTITY
                } else {
                    Quat::from_rotation_arc(normals[i], target_normal)
                };
                anchors.push(Anchor { rest, target, turn });
            }
        }
        let template =
            seen.len() >= MIN_KEYS && anchors.len() as f32 >= seen.len() as f32 * MIN_MATCHED;
        template.then_some(CageMap { anchors })
    }

    /// Where a garment point at `at` (world) with normal `normal` is worn.
    pub(super) fn carry(&self, at: Vec3, normal: Vec3) -> (Vec3, Vec3) {
        let mut near = [(f32::INFINITY, usize::MAX); NEAREST];
        for (i, anchor) in self.anchors.iter().enumerate() {
            let d = anchor.rest.distance_squared(at);
            if d < near[NEAREST - 1].0 {
                near[NEAREST - 1] = (d, i);
                near.sort_by(|a, b| a.0.total_cmp(&b.0));
            }
        }
        let (mut sum, mut turned, mut weight) = (Vec3::ZERO, Vec3::ZERO, 0.0);
        for &(d, i) in near.iter().filter(|(d, _)| d.is_finite()) {
            let anchor = &self.anchors[i];
            let w = 1.0 / (d + 1e-6);
            sum += (anchor.target + anchor.turn * (at - anchor.rest)) * w;
            turned += anchor.turn * normal * w;
            weight += w;
        }
        if weight > 0.0 {
            (sum / weight, turned.normalize_or_zero())
        } else {
            (at, normal)
        }
    }
}

#[cfg(test)]
#[path = "cage_map/tests.rs"]
mod tests;
