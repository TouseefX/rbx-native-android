//! Dynamic heads: a `FaceControls` under a skinned `MeshPart` poses the
//! face bones of its mesh's FACS block, the way Roblox does.
//!
//! Each control's value (0 to 1) weighs its column of the mesh's FACS
//! matrices (see `rbx_mesh::Facs`); the resulting per-bone offsets become
//! one more native-space matrix per bone, applied before the body pose, so
//! a face keeps its expression while the head turns.

use std::collections::HashMap;

use glam::{Mat4, Vec4};
use rbx_dom::{Ref, Variant, WeakDom};

use super::Poses;

/// The `FaceControls` values under `part`, `None` when it has none.
fn read(dom: &WeakDom, part: Ref) -> Option<Vec<(String, f32)>> {
    let controls = dom.get(part)?.children().iter().find_map(|child| {
        dom.get(*child)
            .filter(|instance| instance.class() == "FaceControls")
    })?;
    let mut values: Vec<(String, f32)> = rbx_mesh::CONTROL_NAMES
        .iter()
        .filter_map(|(name, _)| match controls.properties().get(*name) {
            Some(Variant::Float32(v)) if *v != 0.0 => Some((name.to_string(), v.clamp(0.0, 1.0))),
            _ => None,
        })
        .collect();
    values.sort_by(|a, b| a.0.cmp(&b.0));
    Some(values)
}

impl Poses {
    /// Re-reads the `FaceControls` under `part`; whether its face changed.
    pub(crate) fn face(&mut self, dom: &WeakDom, part: Ref) -> bool {
        let now = read(dom, part).filter(|values| !values.is_empty());
        let changed = match &now {
            Some(values) => self.faces.insert(part, values.clone()).as_ref() != Some(values),
            None => self.faces.remove(&part).is_some(),
        };
        self.dirty |= changed;
        changed
    }

    pub(super) fn face_of(&self, part: Ref) -> HashMap<&str, f32> {
        self.faces
            .get(&part)
            .map(|values| values.iter().map(|(n, v)| (n.as_str(), *v)).collect())
            .unwrap_or_default()
    }
}

/// Per bone, the native-space matrix of the face `values` set, or `None`
/// when the mesh has no FACS block or every control is at 0.
pub(super) fn palette(skin: &rbx_mesh::Skin, values: &HashMap<&str, f32>) -> Option<Vec<Mat4>> {
    if values.is_empty() {
        return None;
    }
    let weight = |name: &str| values.get(name).copied().unwrap_or(0.0);
    let matrices = skin.facs_palette(&weight)?;
    Some(
        matrices
            .iter()
            .map(|m| {
                Mat4::from_cols(
                    Vec4::new(m[0], m[3], m[6], 0.0),
                    Vec4::new(m[1], m[4], m[7], 0.0),
                    Vec4::new(m[2], m[5], m[8], 0.0),
                    Vec4::new(m[9], m[10], m[11], 1.0),
                )
            })
            .collect(),
    )
}

#[cfg(test)]
#[path = "face/tests.rs"]
mod tests;
