//! A Roblox model (`.rbxm`/`.rbxmx`) as the one mesh the auto-rigger takes.
//!
//! Every part is placed the way the viewport and the glTF export place it
//! (`rbx_viewer::export`): a `MeshPart`'s mesh fitted to its `Size` and
//! carried through its `CFrame`, a `SpecialMesh` by its own scale and
//! offset, a plain `Part` as its shape. What the parts wear carries over
//! when they all wear the same thing (see [`Look`]).

use std::path::Path;

use glam::{Vec2, Vec3};
use rbx_assets::AssetRef;
use rbx_autorig::{Axis, Mesh};
use rbx_dom::{Content, Ref, Variant, WeakDom};
use rbx_reflection::ReflectionDatabase;
use rbx_viewer::export;
use rbx_viewer::pick::Meshes;

use super::bundle::Surface;

/// The extensions read as models.
pub(super) fn is_model(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("rbxm") || e.eq_ignore_ascii_case("rbxmx"))
}

/// What a model's parts wear, kept for the rig's parts when every source
/// part wears the same: a part only has one texture, and the cut parts mix
/// geometry from several source parts.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Look {
    /// `MeshPart.TextureID`, as an asset number.
    pub(super) texture: Option<u64>,
    /// `SurfaceAppearance` maps.
    pub(super) surface: Option<Surface>,
    /// The parts' colour.
    pub(super) color: Option<[u8; 3]>,
}

/// Reads the model file and gathers its geometry. Blocks on the network for
/// the meshes: call it off the UI thread.
pub(super) fn read(path: &Path) -> Result<(Mesh, Look), String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("couldn\u{2019}t read {}: {e}", path.display()))?;
    let dom = rbx_viewer::parse_place(&bytes, path)?;
    gather(&dom, ReflectionDatabase::shared(), Meshes::fetched)
}

/// The model's geometry as one mesh in studs, and its [`Look`]. `fetch`
/// brings the meshes the parts name.
pub(super) fn gather(
    dom: &WeakDom,
    database: &ReflectionDatabase,
    fetch: impl Fn(&WeakDom, &ReflectionDatabase, &[Ref]) -> Result<Meshes, Vec<String>>,
) -> Result<(Mesh, Look), String> {
    let roots = dom.root_refs();
    let (parts, joints, unions) = survey(dom, database, roots);
    if joints > 0 {
        return Err(format!(
            "The model is already rigged ({joints} Bone or Motor6D instances): the auto-rigger takes an unrigged model"
        ));
    }
    if unions > 0 {
        return Err(format!(
            "The model holds {unions} union operations, which the auto-rigger can\u{2019}t read: convert them to MeshParts first"
        ));
    }
    if parts.is_empty() {
        return Err("The model is empty: it holds no parts to rig".into());
    }
    let meshes = fetch(dom, database, roots).map_err(|failures| {
        format!(
            "Couldn\u{2019}t fetch the model\u{2019}s meshes (are you offline?): {}",
            failures.join("; ")
        )
    })?;
    let solids = export::meshes_of(dom, database, &meshes, roots);
    let mut mesh = Mesh {
        up_hint: Some(Axis::PosY),
        ..Mesh::default()
    };
    let (mut normals, mut uvs) = (true, false);
    for solid in solids.meshes.iter().filter(|s| s.color[3] > 0.0) {
        normals &= solid.normals.len() == solid.positions.len();
        uvs |= !solid.uvs.is_empty();
    }
    for solid in solids.meshes.iter().filter(|s| s.color[3] > 0.0) {
        let base = mesh.positions.len() as u32;
        mesh.positions
            .extend(solid.positions.iter().map(|&p| Vec3::from(p)));
        if normals {
            mesh.normals
                .extend(solid.normals.iter().map(|&n| Vec3::from(n)));
        }
        if uvs {
            let mut own: Vec<Vec2> = solid.uvs.iter().map(|&uv| Vec2::from(uv)).collect();
            own.resize(solid.positions.len(), Vec2::ZERO);
            mesh.uvs.extend(own);
        }
        mesh.triangles.extend(
            solid
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| [base + t[0], base + t[1], base + t[2]]),
        );
    }
    if mesh.triangles.is_empty() {
        return Err("The model has no visible geometry to rig".into());
    }
    let mut look = look_of(dom, &parts, &mut mesh.notes);
    if !uvs {
        look.texture = None;
        look.surface = None;
    }
    Ok((mesh, look))
}

/// The parts, how many `Bone`s and `Motor6D`s and how many unions the model
/// holds.
fn survey(dom: &WeakDom, database: &ReflectionDatabase, roots: &[Ref]) -> (Vec<Ref>, usize, usize) {
    let (mut parts, mut joints, mut unions) = (Vec::new(), 0, 0);
    let mut stack = roots.to_vec();
    while let Some(referent) = stack.pop() {
        let Some(instance) = dom.get(referent) else {
            continue;
        };
        stack.extend_from_slice(instance.children());
        match instance.class() {
            "Bone" | "Motor6D" => joints += 1,
            "UnionOperation" | "NegateOperation" | "IntersectOperation" => unions += 1,
            "Terrain" => {}
            class if database.is_subclass_of(class, "BasePart") => parts.push(referent),
            _ => {}
        }
    }
    (parts, joints, unions)
}

fn asset_number(value: Option<&Variant>) -> Option<u64> {
    let Some(Variant::Content(Content::Uri(uri))) = value else {
        return None;
    };
    match AssetRef::parse(uri).ok()? {
        AssetRef::Id(id) => Some(id),
        _ => None,
    }
}

/// What every value agrees on, or none, noting it when some part has one.
fn shared<T: Copy + PartialEq>(
    what: &str,
    values: &[Option<T>],
    notes: &mut Vec<String>,
) -> Option<T> {
    let first = values.first().copied().flatten();
    if values.iter().all(|v| *v == first) {
        return first;
    }
    notes.push(format!(
        "The model\u{2019}s parts don\u{2019}t all share one {what}, so the rig comes out without it"
    ));
    None
}

/// The texture, `SurfaceAppearance` and colour every part shares. When the
/// parts disagree the rig goes without, with a note saying so.
fn look_of(dom: &WeakDom, parts: &[Ref], notes: &mut Vec<String>) -> Look {
    let mut textures = Vec::new();
    let mut surfaces = Vec::new();
    let mut colors = Vec::new();
    for &referent in parts {
        let Some(part) = dom.get(referent) else {
            continue;
        };
        let child = |class: &str| {
            part.children()
                .iter()
                .filter_map(|&c| dom.get(c))
                .find(|c| c.class() == class)
        };
        textures.push(
            asset_number(part.properties().get("TextureID")).or_else(|| {
                let special = child("SpecialMesh")?;
                asset_number(special.properties().get("TextureId"))
            }),
        );
        surfaces.push(child("SurfaceAppearance").and_then(|appearance| {
            let maps = [
                "ColorMap",
                "MetalnessMap",
                "NormalMap",
                "RoughnessMap",
                "TexturePack",
            ]
            .map(|name| asset_number(appearance.properties().get(name)).unwrap_or(0));
            maps.iter().any(|&id| id != 0).then_some(maps)
        }));
        colors.push(match part.properties().get("Color3uint8") {
            Some(&Variant::Color3uint8 { r, g, b }) => Some([r, g, b]),
            _ => None,
        });
    }
    Look {
        texture: shared("texture", &textures, notes),
        surface: shared("SurfaceAppearance", &surfaces, notes),
        color: shared("colour", &colors, notes),
    }
}

#[cfg(test)]
#[path = "autorig_model/tests.rs"]
mod tests;
