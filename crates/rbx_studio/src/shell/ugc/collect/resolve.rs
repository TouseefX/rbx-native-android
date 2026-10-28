//! The download half of `collect`: meshes and textures fetched and parsed
//! into the item.

use super::*;

fn note(subject: String, message: String) -> Finding {
    Finding {
        rule: Rule::Unreadable,
        severity: Severity::Note,
        subject,
        measured: None,
        limit: None,
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
pub(in crate::shell::ugc) fn resolve(
    plan: &Plan,
    fetch: &dyn Fn(u64) -> Result<Vec<u8>, String>,
) -> Collected {
    let mut notes = plan.notes.clone();
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
        from_file: false,
    }
}

/// An asset's bytes through the editor's asset cache, downloading what it
/// lacks. Blocking.
pub(in crate::shell::ugc) fn fetch_cached(id: u64) -> Result<Vec<u8>, String> {
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
