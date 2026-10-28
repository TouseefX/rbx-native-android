//! An `Accessory`, `MeshPart` or `Model` from the place as an [`rbx_ugc::Item`]:
//! [`plan`] reads the tree on the UI thread, [`resolve`] downloads and
//! parses the meshes and textures it names off it.

use std::collections::BTreeMap;

use rbx_dom::{Content, Ref, Variant, WeakDom};
use rbx_ugc::spec::R15_PARTS;
use rbx_ugc::{
    AssetType, CFrame, Cage, CageKind, Finding, Geometry, Item, Part, Placement, Rule, Severity,
    Texture, Value, IDENTITY,
};

use super::{Collected, ItemKind};

const SURFACE_SLOTS: [&str; 4] = ["ColorMap", "NormalMap", "MetalnessMap", "RoughnessMap"];

#[derive(Debug, Clone, PartialEq)]
struct PartPlan {
    name: String,
    mesh: Option<u64>,
    size: [f32; 3],
    cframe: CFrame,
    attachments: Vec<Placement>,
}

#[derive(Debug, Clone, PartialEq)]
struct TexturePlan {
    owner: String,
    slot: String,
    id: u64,
}

#[derive(Debug, Clone, PartialEq)]
struct CagePlan {
    kind: CageKind,
    name: String,
    mesh: u64,
}

/// What [`resolve`] needs from the tree, with asset ids still unfetched.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Plan {
    name: String,
    kind: ItemKind,
    parts: Vec<PartPlan>,
    textures: Vec<TexturePlan>,
    cages: Vec<CagePlan>,
    emissive: bool,
}

fn uri<'a>(props: &'a BTreeMap<String, Variant>, key: &str) -> Option<&'a str> {
    let text = match props.get(key)? {
        Variant::String(text) => text.as_str(),
        Variant::Content(Content::Uri(uri)) => uri.as_str(),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

fn asset_id(text: &str) -> Option<u64> {
    match rbx_assets::AssetRef::parse(text).ok()? {
        rbx_assets::AssetRef::Id(id) => Some(id),
        _ => None,
    }
}

fn cframe_of(props: &BTreeMap<String, Variant>) -> CFrame {
    match props.get("CFrame") {
        Some(Variant::CFrame(c)) => {
            let r = c.rotation;
            [
                c.position.x,
                c.position.y,
                c.position.z,
                r[0],
                r[1],
                r[2],
                r[3],
                r[4],
                r[5],
                r[6],
                r[7],
                r[8],
            ]
        }
        _ => IDENTITY,
    }
}

fn is_base_part(class: &str) -> bool {
    matches!(class, "MeshPart" | "Part")
}

fn descendants(dom: &WeakDom, root: Ref, out: &mut Vec<Ref>) {
    let Some(inst) = dom.get(root) else { return };
    for &child in inst.children() {
        out.push(child);
        descendants(dom, child, out);
    }
}

fn part_plan(dom: &WeakDom, part: Ref) -> Option<PartPlan> {
    let inst = dom.get(part)?;
    let props = inst.properties();
    let size = match props.get("Size") {
        Some(Variant::Vector3(v)) => [v.x, v.y, v.z],
        _ => [1.0; 3],
    };
    let attachments = inst
        .children()
        .iter()
        .filter_map(|&c| dom.get(c))
        .filter(|c| c.class() == "Attachment")
        .map(|c| Placement {
            name: c.name().to_string(),
            cframe: cframe_of(c.properties()),
        })
        .collect();
    Some(PartPlan {
        name: inst.name().to_string(),
        mesh: ["MeshId", "MeshContent"]
            .iter()
            .find_map(|k| uri(props, k))
            .and_then(asset_id),
        size,
        cframe: cframe_of(props),
        attachments,
    })
}

fn collect_textures(dom: &WeakDom, part: Ref, plan: &mut Plan) {
    let Some(inst) = dom.get(part) else { return };
    let props = inst.properties();
    if let Some((slot, id)) = ["TextureID", "TextureContent"]
        .into_iter()
        .find_map(|k| uri(props, k).and_then(asset_id).map(|id| (k, id)))
    {
        plan.textures.push(TexturePlan {
            owner: inst.name().to_string(),
            slot: slot.to_string(),
            id,
        });
    }
    for &child in inst.children() {
        let Some(sa) = dom.get(child).filter(|c| c.class() == "SurfaceAppearance") else {
            continue;
        };
        let sp = sa.properties();
        for slot in SURFACE_SLOTS {
            let content = format!("{slot}Content");
            if let Some(id) = [slot, content.as_str()]
                .iter()
                .find_map(|k| uri(sp, k))
                .and_then(asset_id)
            {
                plan.textures.push(TexturePlan {
                    owner: sa.name().to_string(),
                    slot: slot.to_string(),
                    id,
                });
            }
        }
        if uri(sp, "EmissiveMaskContent").is_some() || uri(sp, "EmissiveMask").is_some() {
            plan.emissive = true;
        }
    }
}

fn wrap_cages(dom: &WeakDom, part: Ref, item: &str, plan: &mut Plan) {
    let Some(inst) = dom.get(part) else { return };
    for &child in inst.children() {
        let Some(wrap) = dom.get(child).filter(|c| c.class() == "WrapLayer") else {
            continue;
        };
        let props = wrap.properties();
        for (key, kind, suffix) in [
            ("ReferenceMeshId", CageKind::Inner, "InnerCage"),
            ("CageMeshId", CageKind::Outer, "OuterCage"),
        ] {
            if let Some(mesh) = uri(props, key).and_then(asset_id) {
                plan.cages.push(CagePlan {
                    kind,
                    name: format!("{item}_{suffix}"),
                    mesh,
                });
            }
        }
    }
}

pub(super) fn plan(dom: &WeakDom, root: Ref, kind: ItemKind) -> Result<Plan, String> {
    let inst = dom
        .get(root)
        .ok_or_else(|| "That item is no longer in the place.".to_string())?;
    let mut plan = Plan {
        name: inst.name().to_string(),
        kind,
        parts: Vec::new(),
        textures: Vec::new(),
        cages: Vec::new(),
        emissive: false,
    };
    let single = match inst.class() {
        "Accessory" => {
            let kids = || inst.children().iter().copied();
            let is_part = |r: &Ref| dom.get(*r).is_some_and(|c| is_base_part(c.class()));
            let handle = kids()
                .find(|r| is_part(r) && dom.get(*r).is_some_and(|c| c.name() == "Handle"))
                .or_else(|| kids().find(is_part))
                .ok_or_else(|| {
                    "The accessory has no Handle part. Add a MeshPart named Handle.".to_string()
                })?;
            Some(handle)
        }
        "MeshPart" | "Part" => Some(root),
        "Model" => None,
        _ => return Err("Pick an Accessory, a MeshPart or a Model".to_string()),
    };
    if let Some(part) = single {
        plan.parts.extend(part_plan(dom, part));
        collect_textures(dom, part, &mut plan);
        let name = plan.name.clone();
        wrap_cages(dom, part, &name, &mut plan);
        return Ok(plan);
    }
    let mut all = Vec::new();
    descendants(dom, root, &mut all);
    let body = matches!(kind.asset, AssetType::Body | AssetType::DynamicHead);
    for r in all {
        let Some(child) = dom.get(r).filter(|c| c.class() == "MeshPart") else {
            continue;
        };
        let name = child.name();
        if body {
            if !R15_PARTS.contains(&name) {
                continue;
            }
        } else if let Some((kind, mesh)) = cage_of(name, child.properties()) {
            plan.cages.push(CagePlan {
                kind,
                name: name.to_string(),
                mesh,
            });
            continue;
        }
        plan.parts.extend(part_plan(dom, r));
        collect_textures(dom, r, &mut plan);
    }
    Ok(plan)
}

fn cage_of(name: &str, props: &BTreeMap<String, Variant>) -> Option<(CageKind, u64)> {
    let kind = if name.ends_with("_InnerCage") {
        CageKind::Inner
    } else if name.ends_with("_OuterCage") {
        CageKind::Outer
    } else {
        return None;
    };
    let mesh = ["MeshId", "MeshContent"]
        .iter()
        .find_map(|k| uri(props, k))
        .and_then(asset_id)?;
    Some((kind, mesh))
}

fn note(subject: String, message: String) -> Finding {
    Finding {
        rule: Rule::Unreadable,
        severity: Severity::Note,
        subject,
        measured: None,
        limit: Some(Value::Text("readable".into())),
        message,
    }
}

fn geometry(bytes: &[u8], scale_to: Option<[f32; 3]>) -> Result<Geometry, rbx_mesh::MeshError> {
    let sm = rbx_mesh::parse_skinned(bytes)?;
    let mesh = &sm.mesh;
    let center = mesh.bounds.center();
    let bsize = mesh.bounds.size();
    let factor: [f32; 3] = std::array::from_fn(|a| match scale_to {
        Some(size) if bsize[a] != 0.0 => size[a] / bsize[a],
        _ => 1.0,
    });
    let positions = mesh
        .vertices
        .iter()
        .map(|v| {
            if scale_to.is_some() {
                std::array::from_fn(|a| (v.position[a] - center[a]) * factor[a])
            } else {
                v.position
            }
        })
        .collect();
    let triangles = sm.lod0().as_chunks::<3>().0.to_vec();
    let influences = sm.skin.as_ref().map_or_else(Vec::new, |skin| {
        skin.influences
            .iter()
            .map(|i| i.iter().count().min(255) as u8)
            .collect()
    });
    Ok(Geometry {
        positions,
        triangles,
        uvs: mesh.vertices.iter().map(|v| v.uv).collect(),
        influences,
        in_studs: scale_to.is_some(),
    })
}

/// Blocking. `fetch` downloads an asset's bytes by id.
pub(super) fn resolve(plan: &Plan, fetch: &dyn Fn(u64) -> Result<Vec<u8>, String>) -> Collected {
    let mut notes = Vec::new();
    let parts = plan
        .parts
        .iter()
        .map(|p| {
            let geometry = p.mesh.and_then(|id| {
                match fetch(id).and_then(|b| geometry(&b, Some(p.size)).map_err(|e| e.to_string()))
                {
                    Ok(g) => Some(g),
                    Err(e) => {
                        notes.push(note(
                            p.name.clone(),
                            format!("The mesh could not be read: {e}"),
                        ));
                        None
                    }
                }
            });
            Part {
                name: p.name.clone(),
                geometry,
                cframe: p.cframe,
                attachments: p.attachments.clone(),
            }
        })
        .collect();
    let mut textures = Vec::new();
    for t in &plan.textures {
        let dims = fetch(t.id).and_then(|bytes| {
            image::ImageReader::new(std::io::Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|e| e.to_string())?
                .into_dimensions()
                .map_err(|e| e.to_string())
        });
        match dims {
            Ok((width, height)) => textures.push(Texture {
                owner: t.owner.clone(),
                slot: t.slot.clone(),
                width,
                height,
            }),
            Err(e) => notes.push(note(
                format!("{}.{}", t.owner, t.slot),
                format!("The texture could not be read: {e}"),
            )),
        }
    }
    let mut cages: Vec<Cage> = Vec::new();
    let mut inner: Option<Geometry> = None;
    let mut ordered: Vec<&CagePlan> = plan.cages.iter().collect();
    ordered.sort_by_key(|c| c.kind == CageKind::Outer);
    for c in ordered {
        match fetch(c.mesh).and_then(|b| geometry(&b, None).map_err(|e| e.to_string())) {
            Ok(g) => {
                let template = match c.kind {
                    CageKind::Inner => {
                        inner.get_or_insert_with(|| g.clone());
                        None
                    }
                    CageKind::Outer => inner.clone(),
                };
                cages.push(Cage {
                    kind: c.kind,
                    name: c.name.clone(),
                    geometry: g,
                    template,
                });
            }
            Err(e) => notes.push(note(
                c.name.clone(),
                format!("The cage mesh could not be read: {e}"),
            )),
        }
    }
    Collected {
        item: Item {
            name: plan.name.clone(),
            asset_type: plan.kind.asset,
            scale: plan.kind.scale,
            parts,
            textures,
            cages,
        },
        emissive: plan.emissive,
        notes,
    }
}

/// An asset's bytes through the editor's asset cache, downloading what it
/// lacks. Blocking.
pub(super) fn fetch_cached(id: u64) -> Result<Vec<u8>, String> {
    let cache = rbx_assets::AssetCache::new(None).map_err(|e| e.to_string())?;
    if let Some(bytes) = cache.get_id(id) {
        return Ok(bytes);
    }
    let client = rbx_cloud::Client::new(rbx_cloud::ApiKey::from_env_or_config());
    let bytes = client
        .asset(id)
        .map_err(|e| format!("downloading asset {id}: {e}"))?
        .bytes;
    let _ = cache.put_id(id, &bytes);
    Ok(bytes)
}

#[cfg(test)]
#[path = "collect/tests.rs"]
mod tests;
