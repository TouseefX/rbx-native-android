//! FACS offsets turned into skinning matrices: each face bone's offset is
//! applied in its own frame and carried down to its children, the way
//! `Bone.Transform` composes.

use super::FacePose;
use crate::skin::{Bone, Skin};

/// A rigid transform: rotation row-major, then position (`Bone`'s layout).
type Rigid = ([f32; 9], [f32; 3]);

fn mul(a: &Rigid, b: &Rigid) -> Rigid {
    let (ra, pa) = a;
    let (rb, pb) = b;
    let mut r = [0.0; 9];
    for i in 0..3 {
        for j in 0..3 {
            r[i * 3 + j] = (0..3).map(|k| ra[i * 3 + k] * rb[k * 3 + j]).sum();
        }
    }
    let p = std::array::from_fn(|i| (0..3).map(|k| ra[i * 3 + k] * pb[k]).sum::<f32>() + pa[i]);
    (r, p)
}

fn inverse((r, p): &Rigid) -> Rigid {
    let t = [r[0], r[3], r[6], r[1], r[4], r[7], r[2], r[5], r[8]];
    let p = std::array::from_fn(|i| -(0..3).map(|k| t[i * 3 + k] * p[k]).sum::<f32>());
    (t, p)
}

/// `CFrame.new(position) * CFrame.Angles(rx, ry, rz)`: `Rx · Ry · Rz`.
fn offset(pose: &FacePose) -> Rigid {
    let [x, y, z] = pose.rotation.map(f32::to_radians);
    let (sx, cx, sy, cy, sz, cz) = (x.sin(), x.cos(), y.sin(), y.cos(), z.sin(), z.cos());
    let r = [
        cy * cz,
        -cy * sz,
        sy,
        cx * sz + sx * sy * cz,
        cx * cz - sx * sy * sz,
        -sx * cy,
        sx * sz - cx * sy * cz,
        sx * cz + cx * sy * sz,
        cx * cy,
    ];
    (r, pose.position)
}

fn bind(bone: &Bone) -> Rigid {
    (bone.rotation, bone.position)
}

impl Skin {
    /// Per bone of [`Skin::bones`], the mesh-space skinning matrix (rotation
    /// row-major, then position) that puts the face in the pose `weight`
    /// gives, with every non-face bone at rest; `None` without FACS data.
    /// A face bone's offset carries its child bones with it.
    pub fn facs_palette(&self, weight: &dyn Fn(&str) -> f32) -> Option<Vec<[f32; 12]>> {
        let facs = self.facs.as_ref()?;
        let mut local: Vec<Option<Rigid>> = vec![None; self.bones.len()];
        for pose in facs.pose(weight) {
            if let Some(index) = self.bone(&facs.bones[pose.bone]) {
                local[index] = Some(offset(&pose));
            }
        }
        let mut world: Vec<Option<Rigid>> = vec![None; self.bones.len()];
        for index in 0..self.bones.len() {
            self.posed(index, &local, &mut world, 0);
        }
        Some(
            world
                .iter()
                .zip(&self.bones)
                .map(|(posed, bone)| {
                    let (r, p) = mul(&posed.unwrap_or_else(|| bind(bone)), &inverse(&bind(bone)));
                    let mut out = [0.0; 12];
                    out[..9].copy_from_slice(&r);
                    out[9..].copy_from_slice(&p);
                    out
                })
                .collect(),
        )
    }

    /// `parent_posed · parent_bind⁻¹ · bind · offset`, memoised in `world`;
    /// `depth` stops a looping parent chain.
    fn posed(
        &self,
        index: usize,
        local: &[Option<Rigid>],
        world: &mut [Option<Rigid>],
        depth: usize,
    ) -> Rigid {
        if let Some(done) = world[index] {
            return done;
        }
        let bone = &self.bones[index];
        let own = match local[index] {
            Some(offset) => mul(&bind(bone), &offset),
            None => bind(bone),
        };
        let parent = bone
            .parent
            .map(usize::from)
            .filter(|&p| p < self.bones.len() && p != index && depth < self.bones.len());
        let result = match parent {
            Some(p) => {
                let parent_posed = self.posed(p, local, world, depth + 1);
                let rest_local = mul(&inverse(&bind(&self.bones[p])), &own);
                mul(&parent_posed, &rest_local)
            }
            None => own,
        };
        world[index] = Some(result);
        result
    }
}

/// Positions of `positions` skinned through `palette` (from
/// [`Skin::facs_palette`]), weights renormalised; a vertex with no influence
/// stays put.
pub fn deform_positions(
    skin: &Skin,
    positions: &[[f32; 3]],
    palette: &[[f32; 12]],
) -> Vec<[f32; 3]> {
    positions
        .iter()
        .zip(
            skin.influences
                .iter()
                .map(Some)
                .chain(std::iter::repeat(None)),
        )
        .map(|(v, influences)| {
            let Some(influences) = influences else {
                return *v;
            };
            let mut total = 0.0;
            let mut out = [0.0f32; 3];
            for (bone, weight) in influences.iter() {
                let Some(m) = palette.get(bone as usize) else {
                    continue;
                };
                total += weight;
                for (i, o) in out.iter_mut().enumerate() {
                    *o += weight
                        * (m[i * 3] * v[0] + m[i * 3 + 1] * v[1] + m[i * 3 + 2] * v[2] + m[9 + i]);
                }
            }
            match total > 0.0 {
                true => out.map(|o| o / total),
                false => *v,
            }
        })
        .collect()
}
