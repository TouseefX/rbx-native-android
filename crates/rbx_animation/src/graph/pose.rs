//! A pose flowing along a graph's wires: each joint's `Transform` with how
//! much of it applies. A joint missing from a pose is at rest (weight 0).

use std::collections::BTreeMap;

use glam::{Quat, Vec3};

use crate::frame::Frame;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Posed {
    pub frame: Frame,
    pub weight: f32,
}

pub type Pose = BTreeMap<String, Posed>;

/// The rest pose's stand-in for a joint one side of a blend lacks.
const REST: Posed = Posed {
    frame: Frame::IDENTITY,
    weight: 0.,
};

/// Crossfades `a` into `b` by `t` (0 is all `a`).
pub fn mix(a: &Pose, b: &Pose, t: f32) -> Pose {
    let mut out = Pose::new();
    for joint in a.keys().chain(b.keys()) {
        if out.contains_key(joint) {
            continue;
        }
        let (x, y) = (
            a.get(joint).copied().unwrap_or(REST),
            b.get(joint).copied().unwrap_or(REST),
        );
        // A side that does not pose the joint should not drag the other's
        // rotation towards rest, only its weight.
        let frame = match (a.contains_key(joint), b.contains_key(joint)) {
            (true, true) => x.frame.lerp(&y.frame, t),
            (true, false) => x.frame,
            _ => y.frame,
        };
        out.insert(
            joint.clone(),
            Posed {
                frame,
                weight: x.weight + (y.weight - x.weight) * t,
            },
        );
    }
    out
}

/// A weighted blend of several poses. Weights at or below zero are left
/// out; the rest are normalised. Each joint's rotation is the normalised
/// weighted sum of the quaternions that pose it (aligned to the first), and
/// its position the weighted mean: what real Studio's Blend2D gives
/// (`tests/studio_graph.rs`, within 2.4e-4 on four 2D samples).
pub fn blend(parts: &[(Pose, f32)]) -> Pose {
    let parts: Vec<&(Pose, f32)> = parts.iter().filter(|(_, w)| *w > 0.).collect();
    let total: f32 = parts.iter().map(|(_, w)| w).sum();
    let mut out = Pose::new();
    for (pose, _) in &parts {
        for joint in pose.keys() {
            if out.contains_key(joint) {
                continue;
            }
            let posing: Vec<(&Posed, f32)> = parts
                .iter()
                .filter_map(|(p, w)| p.get(joint).map(|posed| (posed, *w)))
                .collect();
            let first = posing[0].0.frame.rotation;
            let (mut rotation, mut position, mut share, mut weight) =
                (Quat::from_xyzw(0., 0., 0., 0.), Vec3::ZERO, 0., 0.);
            for (posed, w) in &posing {
                let q = posed.frame.rotation;
                let aligned = if first.dot(q) < 0. { -q } else { q };
                rotation += aligned * *w;
                position += posed.frame.position * *w;
                share += w;
                weight += posed.weight * w;
            }
            out.insert(
                joint.clone(),
                Posed {
                    frame: Frame::new(position / share, rotation.normalize()),
                    weight: weight / total,
                },
            );
        }
    }
    out
}

/// Normalised lerp of two rotations along the shorter arc; `t` outside
/// 0..1 extrapolates.
fn nlerp(a: Quat, b: Quat, t: f32) -> Quat {
    let b = if a.dot(b) < 0. { -b } else { b };
    (a * (1. - t) + b * t).normalize()
}

/// `Over`: `over` laid on `base` at `weight`, joint by joint
/// (`nlerp(base, over, weight · w)` with `w` the joint's own weight, so a
/// joint `over` leaves out or masks off shows `base`). Roblox leaves Weight
/// unclamped, so past 1 (or below 0) the rotation and position extrapolate.
/// The weight's meaning is documented (creator-docs `AnimationNodeType.yaml`, OverNode: Weight
/// is "the blend weight used to attenuate the Over pose. Defaults to 1.0
/// (full override) and is unclamped"); the rotation is a normalised lerp,
/// not a slerp, which real Studio matches within 5e-5 at weights 0.3, 0.7
/// and 1.5 (`tests/studio_graph.rs`).
pub fn over(base: &Pose, over: &Pose, weight: f32) -> Pose {
    let mut out = base.clone();
    for (joint, top) in over {
        let under = base.get(joint).copied().unwrap_or(REST);
        let t = top.weight * weight;
        let frame = if base.contains_key(joint) {
            Frame::new(
                under.frame.position.lerp(top.frame.position, t),
                nlerp(under.frame.rotation, top.frame.rotation, t),
            )
        } else {
            top.frame
        };
        out.insert(
            joint.clone(),
            Posed {
                frame,
                weight: under.weight + (1. - under.weight) * t.clamp(0., 1.),
            },
        );
    }
    out
}

/// `Add`: `additive` applied on top of `base`, scaled by `weight`, as creator-docs `AnimationNodeType.yaml`
/// documents AddNode ("adds the Additive pose to the Base pose, attenuated
/// by a specific Weight (unclamped)").
pub fn add(base: &Pose, additive: &Pose, weight: f32) -> Pose {
    let mut out = base.clone();
    for (joint, delta) in additive {
        let scaled = Frame::IDENTITY.lerp(&delta.frame, weight * delta.weight);
        let under = base.get(joint).copied().unwrap_or(Posed {
            frame: Frame::IDENTITY,
            weight: 1.,
        });
        out.insert(
            joint.clone(),
            Posed {
                frame: under.frame.mul(&scaled),
                weight: under.weight.max(1.),
            },
        );
    }
    out
}

/// `Subtract`: `a − weight·b`, which turns a clip into an additive one. The
/// formula is creator-docs `AnimationNodeType.yaml`'s own for SubtractNode (`A - Weight * B`).
/// The rotation takes `b`'s off the front, `slerp(I, b, weight)⁻¹ · a`, so
/// that `Add` (which applies an additive after the base) gives `a` back:
/// real Studio matches that within 7e-5 at weights 0.3 to 1.5
/// (`tests/studio_graph.rs`). The position subtracts in `a`'s frame, which
/// that test cannot tell apart from the alternatives.
pub fn subtract(a: &Pose, b: &Pose, weight: f32) -> Pose {
    let mut out = a.clone();
    for (joint, minus) in b {
        let scaled = Frame::IDENTITY.lerp(&minus.frame, weight * minus.weight);
        let from = a.get(joint).copied().unwrap_or(Posed {
            frame: Frame::IDENTITY,
            weight: 1.,
        });
        out.insert(
            joint.clone(),
            Posed {
                frame: Frame::new(
                    from.frame.mul(&scaled.inverse()).position,
                    (scaled.rotation.conjugate() * from.frame.rotation).normalize(),
                ),
                weight: from.weight.max(1.),
            },
        );
    }
    out
}

/// Every joint's final `Transform`: its pose scaled by its weight from rest.
pub fn resolve(pose: &Pose) -> BTreeMap<String, Frame> {
    pose.iter()
        .filter(|(_, p)| p.weight > 0.)
        .map(|(j, p)| (j.clone(), Frame::IDENTITY.lerp(&p.frame, p.weight.min(1.))))
        .collect()
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;

    fn at(joint: &str, x: f32) -> Pose {
        Pose::from([(
            joint.to_owned(),
            Posed {
                frame: Frame::at(Vec3::new(x, 0., 0.)),
                weight: 1.,
            },
        )])
    }

    fn x(p: &Pose, joint: &str) -> f32 {
        p[joint].frame.position.x
    }

    #[test]
    fn mix_keeps_one_sided_joints_in_place() {
        let m = mix(&at("Head", 2.), &at("Torso", 4.), 0.25);
        assert_eq!(x(&m, "Head"), 2.);
        assert!((m["Head"].weight - 0.75).abs() < 1e-6);
        assert!((m["Torso"].weight - 0.25).abs() < 1e-6);
        let both = mix(&at("Head", 0.), &at("Head", 4.), 0.5);
        assert!((x(&both, "Head") - 2.).abs() < 1e-6);
    }

    #[test]
    fn blend_weights() {
        let b = blend(&[(at("Head", 0.), 0.25), (at("Head", 4.), 0.75)]);
        assert!((x(&b, "Head") - 3.).abs() < 1e-5);
        assert!(blend(&[]).is_empty());
    }

    #[test]
    fn over_add_subtract() {
        let o = over(&at("Head", 0.), &at("Head", 4.), 0.5);
        assert!((x(&o, "Head") - 2.).abs() < 1e-5);
        let past = over(&at("Head", 0.), &at("Head", 4.), 1.5);
        assert!((x(&past, "Head") - 6.).abs() < 1e-5, "unclamped");
        let a = add(&at("Head", 1.), &at("Head", 2.), 0.5);
        assert!((x(&a, "Head") - 2.).abs() < 1e-5);
        let s = subtract(&at("Head", 3.), &at("Head", 2.), 1.);
        assert!((x(&s, "Head") - 1.).abs() < 1e-5);
    }

    fn turned(joint: &str, frame: Frame) -> Pose {
        Pose::from([(joint.to_owned(), Posed { frame, weight: 1. })])
    }

    /// Row-major rotation of `p[joint]`, as `CFrame:GetComponents()` lists it.
    fn rows(p: &Pose, joint: &str) -> [f32; 9] {
        p[joint].frame.to_data().rotation
    }

    fn near(a: [f32; 9], b: [f32; 9]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
    }

    // The expected rotations are what real Studio's Animator gave for the
    // same graphs (`tests/studio_graph.rs`, 2026-10-21).
    #[test]
    fn over_and_subtract_turn_as_studio_does() {
        let y90 = turned("A", Frame::angles(0., std::f32::consts::FRAC_PI_2, 0.));
        let z45 = turned("A", Frame::angles(0., 0., std::f32::consts::FRAC_PI_4));
        let o = over(&y90, &z45, 0.3);
        let studio = [0.39558, -0.2075, 0.89468, 0.2075, 0.96915, 0.13303, -0.89468, 0.13303, 0.42644];
        assert!(near(rows(&o, "A"), studio), "{:?}", rows(&o, "A"));
        let s = subtract(&z45, &y90, 0.7);
        let r = std::f32::consts::FRAC_1_SQRT_2;
        let studio = [0.32098, -0.321, -0.89103, r, r, 0., 0.63006, -0.63004, 0.45395];
        assert!(near(rows(&s, "A"), studio), "{:?}", rows(&s, "A"));
    }

    #[test]
    fn blend_sums_quaternions_as_studio_does() {
        let b = blend(&[
            (turned("A", Frame::angles(0., std::f32::consts::FRAC_PI_2, 0.)), 0.75),
            (turned("A", Frame::angles(std::f32::consts::FRAC_PI_2, 0., 0.)), 0.25),
        ]);
        let studio = [0.30764, 0.23079, 0.92309, 0.23079, 0.92307, -0.3077, -0.92309, 0.3077, 0.23072];
        assert!(near(rows(&b, "A"), studio), "{:?}", rows(&b, "A"));
    }

    #[test]
    fn resolve_scales_from_rest() {
        let mut p = at("Head", 4.);
        p.get_mut("Head").unwrap().weight = 0.5;
        p.insert("Torso".into(), REST);
        let r = resolve(&p);
        assert!((r["Head"].position.x - 2.).abs() < 1e-5);
        assert!(!r.contains_key("Torso"));
    }
}
