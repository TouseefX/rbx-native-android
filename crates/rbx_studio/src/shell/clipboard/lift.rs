//! Where a plain Paste lands. Studio puts the copy where the original was,
//! unless that is inside another part, and then lifts it straight up until
//! it rests on top. Measured in real Studio: a 1.2-stud-tall part at
//! y = 0.5 pasted twice lands at 1.7, then 2.9, each on the one before;
//! "Paste at Original Location" leaves it at 0.5.

use std::collections::HashSet;

use glam::{Mat4, Vec3};
use rbx_dom::{Ref, Variant, WeakDom};
use rbx_reflection::ReflectionDatabase;
use rbx_viewer::pick;

/// A world-aligned box, `min` and `max` corners.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Box3 {
    pub(super) min: Vec3,
    pub(super) max: Vec3,
}

impl Box3 {
    fn of(model: Mat4) -> Self {
        let half = (model.x_axis.truncate().abs()
            + model.y_axis.truncate().abs()
            + model.z_axis.truncate().abs())
            * 0.5;
        let centre = model.w_axis.truncate();
        Box3 {
            min: centre - half,
            max: centre + half,
        }
    }

    fn union(self, other: Box3) -> Self {
        Box3 {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    /// Overlapping by more than a hair: resting face to face is clear.
    fn overlaps(self, other: Box3) -> bool {
        const SLACK: f32 = 1e-3;
        (self.min + SLACK).cmplt(other.max).all() && (other.min + SLACK).cmplt(self.max).all()
    }
}

/// How far up `copy` has to move to stop overlapping any of `others`, each
/// step resting it on the top of the highest one it is inside, until it is
/// inside none.
pub(super) fn clearance(copy: Box3, others: &[Box3]) -> f32 {
    let mut lifted = 0.0;
    let mut at = copy;
    // Each step lands strictly higher, so this ends; the bound is a guard.
    for _ in 0..=others.len() {
        let Some(top) = others
            .iter()
            .filter(|other| at.overlaps(**other))
            .map(|other| other.max.y)
            .reduce(f32::max)
        else {
            break;
        };
        let rise = top - at.min.y;
        lifted += rise;
        at.min.y += rise;
        at.max.y += rise;
    }
    lifted
}

/// Lifts the parts under `pasted` (each a just-pasted root) off whatever
/// else they landed inside, as one block.
pub(super) fn lift(dom: &mut WeakDom, database: &ReflectionDatabase, pasted: &[Ref]) {
    let parts: Vec<Ref> = pasted
        .iter()
        .flat_map(|&root| pick::parts_of(dom, database, root).collect::<Vec<_>>())
        .collect();
    let Some(copy) = parts
        .iter()
        .filter_map(|&part| pick::model_of(dom, part).map(Box3::of))
        .reduce(Box3::union)
    else {
        return;
    };
    let mine: HashSet<Ref> = parts.iter().copied().collect();
    let others: Vec<Box3> = pick::drawable_parts(dom, database)
        .filter(|part| !mine.contains(part))
        .filter_map(|part| pick::model_of(dom, part).map(Box3::of))
        .collect();
    let rise = clearance(copy, &others);
    if rise == 0.0 {
        return;
    }
    for part in parts {
        let Some(Variant::CFrame(mut cframe)) = dom
            .get(part)
            .and_then(|instance| instance.properties().get("CFrame").cloned())
        else {
            continue;
        };
        cframe.position.y += rise;
        let _ = dom.set_property(part, "CFrame", Variant::CFrame(cframe));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(y: f32, height: f32) -> Box3 {
        Box3 {
            min: Vec3::new(-2.0, y - height / 2.0, -1.0),
            max: Vec3::new(2.0, y + height / 2.0, 1.0),
        }
    }

    #[test]
    fn a_copy_inside_its_original_rests_on_top_of_it() {
        // Studio: 0.5 -> 1.7 for a 1.2-tall part.
        let rise = clearance(block(0.5, 1.2), &[block(0.5, 1.2)]);
        assert!((0.5 + rise - 1.7).abs() < 1e-5, "{rise}");
    }

    #[test]
    fn a_second_copy_climbs_onto_the_first() {
        // Studio: the next paste lands at 2.9.
        let rise = clearance(block(0.5, 1.2), &[block(0.5, 1.2), block(1.7, 1.2)]);
        assert!((0.5 + rise - 2.9).abs() < 1e-5, "{rise}");
    }

    #[test]
    fn a_copy_in_open_space_stays_put() {
        let far = Box3 {
            min: Vec3::new(10.0, 0.0, 10.0),
            max: Vec3::new(12.0, 1.0, 12.0),
        };
        assert_eq!(clearance(block(0.5, 1.2), &[far]), 0.0);
    }
}
