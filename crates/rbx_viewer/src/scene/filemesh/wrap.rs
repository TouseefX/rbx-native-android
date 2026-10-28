//! Layered clothing, wrapped to the body.
//!
//! A layered garment's `WrapLayer` carries its inner cage (`ReferenceMeshId`,
//! in the garment's own frame, shifted by `ReferenceOrigin`) and its outer
//! cage (`CageMeshId`, shifted by `CageOrigin`); each body part of the
//! character it is worn on carries a `WrapTarget` whose `CageMeshId` is the
//! body as it is now. Roblox fits the garment cage to cage: the inner cage
//! goes onto the body cage vertex for vertex, and a layer above goes onto the
//! outer cage of the layer below. [`cage_map`] does that pairing by template
//! UV and binds each garment vertex to the closest spot on its inner cage,
//! rebuilt at the same spot of the body with the same clearance; layers are fitted lowest `Order` first, each one's outer cage laid over
//! the surface the next one is worn on. Then each vertex rises a little
//! along its normal by the layer's `Order`, so stacked layers do not fight
//! over one surface.
//!
//! An inner cage that is not made from the body's template (no UV pairing)
//! falls back to the nearest-vertex fit: every reference vertex is matched to
//! the nearest body cage vertex in the world, and each garment vertex moves
//! by the inverse-distance blend of the four nearest reference vertices'
//! displacements. The reference cage rests in the garment's frame, which need
//! not be where the body stands (the real Black Detective Trench Coat's sits
//! 0.8 studs high), so for that fit the reference is first slid onto the body
//! ([`register`]); matching vertices across that gap would pick the wrong
//! surface and tear the garment.
//!
//! `Puffiness`, `ShrinkFactor` and `BindOffset` are not applied: Roblox
//! publishes no formula for them.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};
use rbx_assets::AssetRef;
use rbx_dom::{Ref, Variant, WeakDom};
use rbx_reflection::ReflectionDatabase;

use self::cage_map::{CageMap, Surface, WorldCage};
use super::fit::{self, Fit};
use super::{parsed_asset_ref, Entry};
use crate::scene::cframe_matrix;

/// Neighbours blended per garment vertex.
const BLEND: usize = 4;
/// A body cage vertex further than this from its reference vertex is another
/// limb's, not this one's resized.
const MAX_SHIFT: f32 = 1.5;
/// Reference vertices sampled per registration pass.
const SAMPLES: usize = 600;
/// Registration passes per starting offset.
const PASSES: usize = 10;
/// The share of the closest matches that steer registration, so a sleeve the
/// body lacks does not drag the whole reference after it.
const TRIM: f32 = 0.7;
/// Studs of lift per `Order` step.
const LIFT: f32 = 0.004;

#[derive(Clone)]
pub(super) struct Wrap {
    reference: AssetRef,
    /// `ReferenceOrigin`, in the garment's mesh space.
    origin: Mat4,
    /// `CageMeshId` and `CageOrigin`: the outer cage the layer above is
    /// worn over.
    outer: Option<(AssetRef, Mat4)>,
    order: f32,
    targets: Vec<Target>,
    /// The character the garment is worn on: the handle's accessory's parent,
    /// whose parts the wrapped garment is skinned to.
    pub(super) character: Ref,
}

#[derive(Clone)]
struct Target {
    cage: AssetRef,
    origin: Mat4,
    /// The body part's own mesh and fit: the cage shares its space.
    mesh: AssetRef,
    fit: Fit,
    /// The body part's `Name`: the bone its cage's vertices lend the garment.
    part: String,
}

/// The wrap of `handle`, when it carries a `WrapLayer` with a reference cage
/// and sits in a character that has cages to wrap to.
pub(super) fn of(dom: &WeakDom, database: &ReflectionDatabase, handle: Ref) -> Option<Wrap> {
    let layer = dom
        .get(handle)?
        .children()
        .iter()
        .filter_map(|&c| dom.get(c))
        .find(|c| c.class() == "WrapLayer")?;
    let reference = parsed_asset_ref(layer.properties().get("ReferenceMeshId")?)?;
    let origin = match layer.properties().get("ReferenceOrigin") {
        Some(Variant::CFrame(c)) => cframe_matrix(c),
        _ => Mat4::IDENTITY,
    };
    let outer = layer
        .properties()
        .get("CageMeshId")
        .and_then(parsed_asset_ref)
        .map(|cage| {
            let origin = match layer.properties().get("CageOrigin") {
                Some(Variant::CFrame(c)) => cframe_matrix(c),
                _ => Mat4::IDENTITY,
            };
            (cage, origin)
        });
    let order = crate::scene::number(layer.properties().get("Order"));
    let rig = dom.parent(dom.parent(handle)?)?;
    let targets: Vec<Target> = dom
        .get(rig)?
        .children()
        .iter()
        .filter_map(|&part| {
            let part = dom.get(part)?;
            let (mesh, fit) = fit::mesh_part(database, part)?;
            let name = part.name().to_owned();
            let target = part
                .children()
                .iter()
                .filter_map(|&c| dom.get(c))
                .find(|c| c.class() == "WrapTarget")?;
            Some(Target {
                cage: parsed_asset_ref(target.properties().get("CageMeshId")?)?,
                origin: match target.properties().get("CageOrigin") {
                    Some(Variant::CFrame(c)) => cframe_matrix(c),
                    _ => Mat4::IDENTITY,
                },
                mesh,
                fit,
                part: name,
            })
        })
        .collect();
    (!targets.is_empty()).then_some(Wrap {
        reference,
        origin,
        outer,
        order,
        targets,
        character: rig,
    })
}

impl Wrap {
    /// The meshes the wrap needs besides the garment's own.
    pub(super) fn meshes(&self) -> impl Iterator<Item = &AssetRef> {
        std::iter::once(&self.reference)
            .chain(self.targets.iter().flat_map(|t| [&t.cage, &t.mesh]))
            .chain(self.outer.iter().map(|(cage, _)| cage))
    }
}

pub(super) fn key(referent: Ref) -> AssetRef {
    AssetRef::Thumb(format!("wrapped-mesh/{referent:?}"))
}

/// Every wrapped garment's deformed mesh, by referent, for those whose cages
/// have all arrived. Each character's garments are fitted lowest `Order`
/// first, over the surface the ones below left. Each is skinned to the
/// character's parts: through the garment's own skin when its bones name the
/// body parts it wraps, otherwise through one lent by the body (see
/// [`lent_skin`]).
pub(super) fn derive(
    entries: &[Entry],
    meshes: &HashMap<AssetRef, Arc<rbx_mesh::SkinnedMesh>>,
) -> HashMap<Ref, (AssetRef, Arc<rbx_mesh::SkinnedMesh>)> {
    let mut worn: Vec<&Entry> = entries.iter().filter(|e| e.wrap.is_some()).collect();
    worn.sort_by(|a, b| {
        let order = |e: &Entry| e.wrap.as_ref().map_or(0.0, |w| w.order);
        order(a).total_cmp(&order(b))
    });
    let mut surfaces: HashMap<Ref, Surface> = HashMap::new();
    let mut out = HashMap::new();
    for entry in worn {
        let Some(wrap) = entry.wrap.as_ref() else {
            continue;
        };
        let Some(garment) = meshes.get(&entry.mesh) else {
            continue;
        };
        let model = entry.fit.transform(garment);
        let surface = match surfaces.entry(wrap.character) {
            std::collections::hash_map::Entry::Occupied(found) => found.into_mut(),
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(body_surface(wrap, meshes))
            }
        };
        let Some(deformed) = deform_over(wrap, model, garment, meshes, surface) else {
            continue;
        };
        let skin = match &garment.skin {
            Some(own) if names_the_body(wrap, own) => Some(own.clone()),
            _ => lent_skin(wrap, model, &deformed, meshes),
        };
        out.insert(
            entry.referent,
            (
                key(entry.referent),
                Arc::new(rbx_mesh::SkinnedMesh {
                    mesh: deformed,
                    skin,
                }),
            ),
        );
    }
    out
}

/// The body's cages as the surface the first layer is worn over.
fn body_surface(wrap: &Wrap, meshes: &HashMap<AssetRef, Arc<rbx_mesh::SkinnedMesh>>) -> Surface {
    let mut surface = Surface::default();
    for target in &wrap.targets {
        // A part whose mesh or cage didn't load (a head behind
        // authentication) leaves a hole, not a bare body.
        let (Some(cage), Some(part)) = (meshes.get(&target.cage), meshes.get(&target.mesh)) else {
            continue;
        };
        let to_world = target.fit.transform(part) * target.origin;
        surface.lay(&world_cage(cage, to_world));
    }
    surface
}

fn world_cage(cage: &rbx_mesh::Mesh, to_world: Mat4) -> WorldCage<'_> {
    WorldCage {
        positions: cage
            .vertices
            .iter()
            .map(|v| to_world.transform_point3(Vec3::from(v.position)))
            .collect(),
        mesh: cage,
    }
}

/// The garment fitted over `surface` by its cages, then its outer cage laid
/// over `surface` for the layer above. Falls back to [`deform`] when the
/// inner cage does not pair with the surface.
fn deform_over(
    wrap: &Wrap,
    model: Mat4,
    garment: &rbx_mesh::Mesh,
    meshes: &HashMap<AssetRef, Arc<rbx_mesh::SkinnedMesh>>,
    surface: &mut Surface,
) -> Option<rbx_mesh::Mesh> {
    let reference = meshes.get(&wrap.reference)?;
    let inner = world_cage(reference, model * wrap.origin);
    let Some(map) = CageMap::pair(&inner, surface) else {
        return deform(wrap, model, garment, meshes);
    };
    let back = model.inverse();
    let lift = LIFT * (wrap.order + 1.0);
    let mut out = garment.clone();
    for vertex in &mut out.vertices {
        let at = model.transform_point3(Vec3::from(vertex.position));
        let normal = model
            .transform_vector3(Vec3::from(vertex.normal))
            .normalize_or_zero();
        let (moved, turned) = map.carry(at, normal);
        vertex.position = back.transform_point3(moved + turned * lift).to_array();
        vertex.normal = back
            .transform_vector3(turned)
            .normalize_or_zero()
            .to_array();
    }
    if let Some((cage, origin)) = &wrap.outer {
        if let Some(outer) = meshes.get(cage) {
            let mut laid = world_cage(outer, model * *origin);
            for p in &mut laid.positions {
                *p = map.carry(*p, Vec3::ZERO).0;
            }
            surface.lay(&laid);
        }
    }
    Some(out)
}

/// The nearest-vertex fit, for an inner cage the surface does not pair with.
fn deform(
    wrap: &Wrap,
    model: Mat4,
    garment: &rbx_mesh::Mesh,
    meshes: &HashMap<AssetRef, Arc<rbx_mesh::SkinnedMesh>>,
) -> Option<rbx_mesh::Mesh> {
    let reference = meshes.get(&wrap.reference)?;
    let (body, _) = body(wrap, meshes)?;
    let to_world = model * wrap.origin;
    let reference_points: Vec<Vec3> = reference
        .vertices
        .iter()
        .map(|v| to_world.transform_point3(Vec3::from(v.position)))
        .collect();
    let slide = register(&reference_points, &body);
    // Where each reference vertex stands, and how far its body counterpart is.
    let anchors: Vec<(Vec3, Vec3)> = reference_points
        .iter()
        .filter_map(|&point| {
            let at = point + slide;
            let shift = nearest(&body, at)? - at;
            (shift.length() <= MAX_SHIFT).then_some((at, shift))
        })
        .collect();
    if anchors.is_empty() {
        return None;
    }
    let back = model.inverse();
    let mut out = garment.clone();
    for vertex in &mut out.vertices {
        let at = model.transform_point3(Vec3::from(vertex.position)) + slide;
        let normal = model
            .transform_vector3(Vec3::from(vertex.normal))
            .normalize_or_zero();
        let moved = at + blended(&anchors, at) + normal * LIFT * (wrap.order + 1.0);
        vertex.position = back.transform_point3(moved).to_array();
    }
    Some(out)
}

/// Every body cage vertex where the wrap lays it, with the target it is
/// from; `None` until every cage and body mesh has arrived.
fn body(
    wrap: &Wrap,
    meshes: &HashMap<AssetRef, Arc<rbx_mesh::SkinnedMesh>>,
) -> Option<(Vec<Vec3>, Vec<u16>)> {
    let (mut points, mut owners) = (Vec::new(), Vec::new());
    for (index, target) in wrap.targets.iter().enumerate() {
        // A part whose mesh or cage didn't load (a head behind
        // authentication) leaves a hole, not a bare body.
        let (Some(cage), Some(part)) = (meshes.get(&target.cage), meshes.get(&target.mesh)) else {
            continue;
        };
        let to_world = target.fit.transform(part) * target.origin;
        points.extend(
            cage.vertices
                .iter()
                .map(|v| to_world.transform_point3(Vec3::from(v.position))),
        );
        owners.resize(points.len(), index as u16);
    }
    Some((points, owners))
}

/// Whether a bone of `skin` is named after a body part the garment wraps.
fn names_the_body(wrap: &Wrap, skin: &rbx_mesh::Skin) -> bool {
    skin.bones
        .iter()
        .any(|bone| wrap.targets.iter().any(|target| target.part == bone.name))
}

/// A skin for a garment without one of its own: each wrapped vertex takes
/// the parts of the [`BLEND`] body cage vertices nearest it, by inverse
/// square distance, one bone per wrapped body part (bound at the identity,
/// which the rig's binding cancels out anyway).
fn lent_skin(
    wrap: &Wrap,
    model: Mat4,
    wrapped: &rbx_mesh::Mesh,
    meshes: &HashMap<AssetRef, Arc<rbx_mesh::SkinnedMesh>>,
) -> Option<rbx_mesh::Skin> {
    let (body, owners) = body(wrap, meshes)?;
    if body.is_empty() || wrap.targets.len() > usize::from(u16::MAX) {
        return None;
    }
    let influences = wrapped
        .vertices
        .iter()
        .map(|vertex| {
            let at = model.transform_point3(Vec3::from(vertex.position));
            let mut near = [(f32::INFINITY, 0u16); BLEND];
            for (point, &owner) in body.iter().zip(&owners) {
                let d = point.distance_squared(at);
                if d < near[BLEND - 1].0 {
                    near[BLEND - 1] = (d, owner);
                    near.sort_by(|a, b| a.0.total_cmp(&b.0));
                }
            }
            let mut merged: Vec<(u16, f32)> = Vec::with_capacity(BLEND);
            for (d, owner) in near.iter().filter(|(d, _)| d.is_finite()) {
                let w = 1.0 / (d + 1e-6);
                match merged.iter_mut().find(|(bone, _)| bone == owner) {
                    Some((_, weight)) => *weight += w,
                    None => merged.push((*owner, w)),
                }
            }
            let total: f32 = merged.iter().map(|(_, w)| w).sum();
            let mut influence = rbx_mesh::Influences {
                bones: [0; 4],
                weights: [0.0; 4],
            };
            for (slot, (bone, weight)) in merged.into_iter().enumerate() {
                influence.bones[slot] = bone;
                influence.weights[slot] = weight / total;
            }
            influence
        })
        .collect();
    Some(rbx_mesh::Skin {
        bones: wrap
            .targets
            .iter()
            .map(|target| rbx_mesh::Bone {
                name: target.part.clone(),
                parent: None,
                lod_parent: None,
                culling: 0.0,
                rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                position: [0.0; 3],
            })
            .collect(),
        subsets: Vec::new(),
        envelopes: Vec::new(),
        influences,
    })
}

/// The translation that lays the reference over the body: nearest-vertex
/// matching from a few starting offsets, each pass moving by the mean of the
/// closest [`TRIM`] of the gaps, and the start with the least left over wins.
fn register(reference: &[Vec3], body: &[Vec3]) -> Vec3 {
    let stride = (reference.len() / SAMPLES).max(1);
    let sample: Vec<Vec3> = reference.iter().copied().step_by(stride).collect();
    let (rb, bb) = (bounds(reference), bounds(body));
    let starts = [
        Vec3::ZERO,
        (bb.0 + bb.1 - rb.0 - rb.1) * 0.5,
        Vec3::new(0.0, bb.0.y - rb.0.y, 0.0),
        Vec3::new(0.0, bb.1.y - rb.1.y, 0.0),
    ];
    let gaps = |slide: Vec3| -> Vec<Vec3> {
        let mut gaps: Vec<Vec3> = sample
            .iter()
            .filter_map(|&p| nearest(body, p + slide).map(|n| n - p - slide))
            .collect();
        gaps.sort_by(|a, b| a.length_squared().total_cmp(&b.length_squared()));
        gaps.truncate(((gaps.len() as f32 * TRIM).ceil() as usize).max(1));
        gaps
    };
    let mean = |g: &[Vec3]| g.iter().copied().sum::<Vec3>() / g.len() as f32;
    let mut best = (f32::INFINITY, Vec3::ZERO);
    for start in starts {
        let mut slide = start;
        for _ in 0..PASSES {
            let step = mean(&gaps(slide));
            slide += step;
            if step.length_squared() < 1e-8 {
                break;
            }
        }
        let left = gaps(slide);
        let left = left.iter().map(|g| g.length()).sum::<f32>() / left.len() as f32;
        if left < best.0 - 1e-6 {
            best = (left, slide);
        }
    }
    best.1
}

fn bounds(points: &[Vec3]) -> (Vec3, Vec3) {
    points.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), &p| (lo.min(p), hi.max(p)),
    )
}

fn nearest(points: &[Vec3], at: Vec3) -> Option<Vec3> {
    points
        .iter()
        .copied()
        .min_by(|a, b| a.distance_squared(at).total_cmp(&b.distance_squared(at)))
}

/// The inverse-square-distance blend of the [`BLEND`] anchors nearest `at`.
fn blended(anchors: &[(Vec3, Vec3)], at: Vec3) -> Vec3 {
    let mut near = [(f32::INFINITY, Vec3::ZERO); BLEND];
    for &(anchor, shift) in anchors {
        let d = anchor.distance_squared(at);
        if d < near[BLEND - 1].0 {
            near[BLEND - 1] = (d, shift);
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
    }
    let (mut sum, mut weight) = (Vec3::ZERO, 0.0);
    for (d, shift) in near.iter().filter(|(d, _)| d.is_finite()) {
        let w = 1.0 / (d + 1e-6);
        sum += *shift * w;
        weight += w;
    }
    if weight > 0.0 {
        sum / weight
    } else {
        Vec3::ZERO
    }
}

mod cage_map;

#[cfg(test)]
#[path = "wrap/tests.rs"]
mod tests;
