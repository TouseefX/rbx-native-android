//! Hidden surface removal: the body and lower layers hidden under layered
//! clothing, from the HSR data Roblox ships with every cage.
//!
//! <https://create.roblox.com/docs/reference/engine/classes/HiddenSurfaceRemovalAsset>
//! says the data lets the engine "identify the faces of a wrap's mesh that are
//! fully occluded by clothing layers worn above it" and strip them before
//! rendering. Each `WrapLayer` and `WrapTarget` carries it inline as
//! `HSRData`, or names a `HiddenSurfaceRemovalAsset` model by `HSRAssetId`
//! (Studio's default blocky body does the latter, the Mannequin's packages
//! and the catalog Denim Jacket 8516609675 the former). The format is not
//! published; read off those assets (versions 16 and 18, both laid out the
//! same past the header):
//!
//! - `u8` version, `u32` cage vertex count, `u32` face count, `u32` index
//!   count, then for version 18 a `u32` of extra records (0 except on a head);
//! - one bit per inner cage vertex, 0 where the layer covers what is under
//!   it: on the Cargo Pants (6984740059) the 0 bits are the legs and hips, on
//!   the jacket the torso and arms, on a body none at all;
//! - per mesh face (LOD 0 order), a `u16` start and `u16` count into
//! - the `u16` indices of the cage vertices the face sits under: the outer
//!   cage for a layer (0.14 studs from the face on average on the pants, 1.9
//!   on the inner cage), the only cage for a body part.
//!
//! What follows (a `u16` per face, 0xFFFF everywhere but on a head, the
//! head's extra records and a 32 character hash) is not needed to hide a
//! face. A face is hidden when every cage vertex it sits under is covered,
//! matched across cages by template UV like the fit (see [`super::cage_map`]),
//! by some layer worn over it: the body by any layer, a layer only by those
//! of a higher `Order`. A face with no cage vertices is never hidden.

use std::collections::HashSet;

use rbx_dom::{Variant, WeakDom};

use super::cage_map::{key, Key};

/// One wrap's HSR data.
#[derive(Debug, PartialEq)]
pub(crate) struct Hsr {
    /// Per inner cage vertex: whether this layer covers what is under it.
    covers: Vec<bool>,
    /// Per mesh face, its range in `under`.
    faces: Vec<(usize, usize)>,
    /// Cage vertex indices, see the module doc.
    under: Vec<u16>,
}

impl Hsr {
    /// Reads `HSRData`, `None` for an empty or unknown blob.
    pub(crate) fn parse(bytes: &[u8]) -> Option<Hsr> {
        let u32_at = |at: usize| -> Option<usize> {
            Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize)
        };
        let u16_at = |at: usize| -> Option<u16> {
            Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
        };
        let start = match bytes.first()? {
            16 => 13,
            18 => 17,
            _ => return None,
        };
        let (vertices, face_count, index_count) = (u32_at(1)?, u32_at(5)?, u32_at(9)?);
        let bits = bytes.get(start..start + vertices.div_ceil(8))?;
        let covers = (0..vertices)
            .map(|i| bits[i / 8] >> (i % 8) & 1 == 0)
            .collect();
        let ranges = start + bits.len();
        let indices = ranges + 4 * face_count;
        let faces = (0..face_count)
            .map(|f| {
                let (from, count) = (u16_at(ranges + 4 * f)?, u16_at(ranges + 4 * f + 2)?);
                let (from, count) = (usize::from(from), usize::from(count));
                Some(if from + count <= index_count {
                    (from, count)
                } else {
                    (0, 0)
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let under = (0..index_count)
            .map(|i| u16_at(indices + 2 * i))
            .collect::<Option<Vec<_>>>()?;
        Some(Hsr {
            covers,
            faces,
            under,
        })
    }

    /// Reads the `HSRData` of the `HiddenSurfaceRemovalAsset` an
    /// `HSRAssetId` names, a binary model.
    pub(crate) fn from_asset(bytes: &[u8]) -> Option<Hsr> {
        let dom = rbx_binary::deserialize(bytes).ok()?;
        find_data(&dom).and_then(Hsr::parse)
    }

    /// The template UVs this layer covers, looked up on its inner cage.
    pub(super) fn covered<'a>(
        &'a self,
        inner: &'a rbx_mesh::Mesh,
    ) -> impl Iterator<Item = Key> + 'a {
        self.covers
            .iter()
            .zip(&inner.vertices)
            .filter(|(covers, _)| **covers)
            .map(|(_, vertex)| key(vertex.uv))
    }

    /// Per face of the wrapped mesh, whether it is hidden: every cage vertex
    /// it sits under is in `covered`.
    pub(super) fn hidden(&self, cage: &rbx_mesh::Mesh, covered: &HashSet<Key>) -> Vec<bool> {
        self.faces
            .iter()
            .map(|&(from, count)| {
                count > 0
                    && self.under[from..from + count].iter().all(|&i| {
                        cage.vertices
                            .get(usize::from(i))
                            .is_some_and(|v| covered.contains(&key(v.uv)))
                    })
            })
            .collect()
    }
}

/// The non-empty `HSRData` of the first `HiddenSurfaceRemovalAsset` in `dom`.
fn find_data(dom: &WeakDom) -> Option<&[u8]> {
    let mut pending = dom.root_refs().to_vec();
    while let Some(next) = pending.pop() {
        let instance = dom.get(next)?;
        if instance.class() == "HiddenSurfaceRemovalAsset" {
            return data(instance.properties().get("HSRData"));
        }
        pending.extend(instance.children());
    }
    None
}

/// The bytes of an `HSRData` value: a `BinaryString` in an asset, a
/// `SharedString` in a saved place, both read back as raw bytes.
pub(super) fn data(value: Option<&Variant>) -> Option<&[u8]> {
    match value? {
        Variant::Unknown { raw, .. } if !raw.is_empty() => Some(raw),
        _ => None,
    }
}

/// `mesh` without the faces `hidden` flags. Vertices and bounds stay, so a
/// skin still lines up and the part's fit is unchanged; the LOD table (which
/// nothing draws from) keeps the file's extents.
pub(crate) fn cull(mesh: &rbx_mesh::SkinnedMesh, hidden: &[bool]) -> rbx_mesh::SkinnedMesh {
    let mut out = mesh.clone();
    out.mesh.indices = mesh
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .enumerate()
        .filter(|(face, _)| !hidden.get(*face).copied().unwrap_or(false))
        .flat_map(|(_, corners)| *corners)
        .collect();
    out
}

#[cfg(test)]
#[path = "hsr/tests.rs"]
mod tests;
