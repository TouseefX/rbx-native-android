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
//! garment vertex is bound to the closest spot on its inner cage and rebuilt
//! at the same spot of the paired surface with the same clearance, which is
//! what keeps a blocky body's corners under the cloth.

use std::collections::HashMap;

use glam::{Mat3, Quat, Vec3};

/// Inner cage vertices whose triangles are searched per garment vertex.
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

/// One inner cage vertex and where it is worn: rest position and normal,
/// and the target position and normal of the surface there.
struct Anchor {
    rest: Vec3,
    rest_normal: Vec3,
    target: Vec3,
    target_normal: Vec3,
}

/// A garment's inner cage paired with the surface it is worn over.
pub(super) struct CageMap {
    anchors: Vec<Anchor>,
    /// Inner cage triangles whose three corners all paired, by anchor.
    triangles: Vec<[usize; 3]>,
    /// The triangles each anchor is a corner of.
    around: Vec<Vec<usize>>,
}

impl CageMap {
    /// Pairs `inner` with `surface` by UV, or `None` when the inner cage is
    /// not made from the surface's template.
    pub(super) fn pair(inner: &WorldCage, surface: &Surface) -> Option<CageMap> {
        // Render splits of one vertex each see only their own triangles.
        let mut summed: HashMap<Key, Vec3> = HashMap::new();
        for (i, n) in inner.normals().into_iter().enumerate() {
            *summed.entry(key(inner.uv(i))).or_default() += n;
        }
        let mut seen: HashMap<Key, Option<usize>> = HashMap::new();
        let mut anchor_of = vec![None; inner.positions.len()];
        let mut anchors = Vec::new();
        for (i, &rest) in inner.positions.iter().enumerate() {
            let k = key(inner.uv(i));
            if let Some(&found) = seen.get(&k) {
                anchor_of[i] = found;
                continue;
            }
            let found = surface.points.get(&k).map(|&(target, target_normal)| {
                anchors.push(Anchor {
                    rest,
                    rest_normal: summed[&k].normalize_or_zero(),
                    target,
                    target_normal,
                });
                anchors.len() - 1
            });
            seen.insert(k, found);
            anchor_of[i] = found;
        }
        let template =
            seen.len() >= MIN_KEYS && anchors.len() as f32 >= seen.len() as f32 * MIN_MATCHED;
        if !template {
            return None;
        }
        let mut triangles = Vec::new();
        let mut around = vec![Vec::new(); anchors.len()];
        for corners in inner.mesh.indices.as_chunks::<3>().0 {
            let Some([a, b, c]) = corners
                .iter()
                .map(|&i| anchor_of.get(i as usize).copied().flatten())
                .collect::<Option<Vec<_>>>()
                .and_then(|v| <[usize; 3]>::try_from(v).ok())
            else {
                continue;
            };
            if a == b || b == c || a == c {
                continue;
            }
            for corner in [a, b, c] {
                around[corner].push(triangles.len());
            }
            triangles.push([a, b, c]);
        }
        Some(CageMap {
            anchors,
            triangles,
            around,
        })
    }

    /// Where a garment point at `at` (world) with normal `normal` is worn.
    ///
    /// The point is bound to the closest spot on the inner cage: which
    /// triangle, where in it, and its offset in the frame of the cage's
    /// normal there. It is rebuilt at the same spot of the paired surface,
    /// so it keeps its clearance over whatever shape the surface has (a
    /// blocky body's square corners included). Points with no paired
    /// triangle near them fall back to [`Self::blend`].
    pub(super) fn carry(&self, at: Vec3, normal: Vec3) -> (Vec3, Vec3) {
        let near = self.nearest(at);
        let mut best: Option<(f32, usize, Vec3)> = None;
        for &(_, i) in near.iter().filter(|(d, _)| d.is_finite()) {
            for &t in &self.around[i] {
                let [a, b, c] = self.triangles[t].map(|k| self.anchors[k].rest);
                let bary = closest_on_triangle(at, a, b, c);
                let d = (a * bary.x + b * bary.y + c * bary.z).distance_squared(at);
                if best.is_none_or(|(bd, _, _)| d < bd) {
                    best = Some((d, t, bary));
                }
            }
        }
        let Some((_, t, bary)) = best else {
            return self.blend(&near, at, normal);
        };
        let [a, b, c] = self.triangles[t].map(|k| &self.anchors[k]);
        let spot = |f: fn(&Anchor) -> Vec3| f(a) * bary.x + f(b) * bary.y + f(c) * bary.z;
        let (Some(rest), Some(target)) = (
            frame(spot(|a| a.rest_normal), b.rest - a.rest),
            frame(spot(|a| a.target_normal), b.target - a.target),
        ) else {
            return self.blend(&near, at, normal);
        };
        let local = rest.transpose() * (at - spot(|a| a.rest));
        let moved = spot(|a| a.target) + target * local;
        let turned = (target * (rest.transpose() * normal)).normalize_or_zero();
        (moved, turned)
    }

    /// The inner cage anchors nearest `at`, closest first.
    fn nearest(&self, at: Vec3) -> [(f32, usize); NEAREST] {
        let mut near = [(f32::INFINITY, usize::MAX); NEAREST];
        for (i, anchor) in self.anchors.iter().enumerate() {
            let d = anchor.rest.distance_squared(at);
            if d < near[NEAREST - 1].0 {
                near[NEAREST - 1] = (d, i);
                near.sort_by(|a, b| a.0.total_cmp(&b.0));
            }
        }
        near
    }

    /// Each near anchor carrying the offset turned the way the surface
    /// turned there, blended by inverse square distance.
    fn blend(&self, near: &[(f32, usize)], at: Vec3, normal: Vec3) -> (Vec3, Vec3) {
        let (mut sum, mut turned, mut weight) = (Vec3::ZERO, Vec3::ZERO, 0.0);
        for &(d, i) in near.iter().filter(|(d, _)| d.is_finite()) {
            let anchor = &self.anchors[i];
            let turn = if anchor.rest_normal == Vec3::ZERO || anchor.target_normal == Vec3::ZERO {
                Quat::IDENTITY
            } else {
                Quat::from_rotation_arc(anchor.rest_normal, anchor.target_normal)
            };
            let w = 1.0 / (d + 1e-6);
            sum += (anchor.target + turn * (at - anchor.rest)) * w;
            turned += turn * normal * w;
            weight += w;
        }
        if weight > 0.0 {
            (sum / weight, turned.normalize_or_zero())
        } else {
            (at, normal)
        }
    }
}

/// An orthonormal frame with `normal` as its third axis and `edge` (made
/// square to it) as its first, or `None` when either is degenerate.
fn frame(normal: Vec3, edge: Vec3) -> Option<Mat3> {
    let n = normal.try_normalize()?;
    let t = (edge - n * edge.dot(n)).try_normalize()?;
    Some(Mat3::from_cols(t, n.cross(t), n))
}

/// Barycentric weights of the point on triangle `abc` closest to `p`
/// (Ericson, Real-Time Collision Detection 5.1.5).
fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return Vec3::X;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return Vec3::Y;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return Vec3::new(1.0 - v, v, 0.0);
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return Vec3::Z;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return Vec3::new(1.0 - w, 0.0, w);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return Vec3::new(0.0, 1.0 - w, w);
    }
    let denom = 1.0 / (va + vb + vc);
    let (v, w) = (vb * denom, vc * denom);
    Vec3::new(1.0 - v - w, v, w)
}

#[cfg(test)]
#[path = "cage_map/tests.rs"]
mod tests;
