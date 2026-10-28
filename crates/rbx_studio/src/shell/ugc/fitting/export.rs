//! Generate Accessory, and what Upload Item gets for it. An item whose
//! meshes all live on Roblox uploads as the accessory itself; one with a
//! mesh only this editor has (its cages, a local source mesh) uploads as a
//! glTF holding the item mesh and `<Name>_InnerCage` / `<Name>_OuterCage`,
//! the names Roblox's importer pairs cages to a layered item by.

use std::path::PathBuf;

use glam::Vec3;
use gpui_kit::*;
use rbx_autorig::{ItemCages, Mesh};
use rbx_dom::{Content, Ref, Variant, WeakDom};
use rbx_viewer::export::{Export, ExportMesh, ExportNode};

use super::super::collect::{fetch_cached, mesh_key, Key};
use super::super::{ItemKind, ItemSource};
use super::preview::{self, find};
use super::{cages, generate, workspace, FittingWindow, Generated, Job};

/// A point or direction as an uploaded glTF holds it: turned half round
/// about Y, because Roblox's importer turns what it reads half round (see
/// `rbx_autorig`'s glTF writer, which measured it on live uploads).
fn file_space(v: Vec3) -> [f32; 3] {
    [-v.x, v.y, -v.z]
}

fn surface(name: &str, mesh: &Mesh, texture: Option<usize>) -> ExportMesh {
    ExportMesh {
        name: name.to_string(),
        positions: mesh.positions.iter().map(|&p| file_space(p)).collect(),
        normals: mesh.normals.iter().map(|&n| file_space(n)).collect(),
        tangents: Vec::new(),
        indices: mesh.triangles.iter().flatten().copied().collect(),
        color: [1.0; 4],
        uvs: mesh.uvs.iter().map(|v| v.to_array()).collect(),
        maps: rbx_viewer::export::Maps {
            color: texture,
            ..Default::default()
        },
        blend: false,
        finish: Default::default(),
    }
}

/// The item `name` as a glTF scene: its mesh, then its cages, each a node of
/// its own name at the origin (all three are in the handle's space already).
pub(super) fn scene(
    name: &str,
    item: &Mesh,
    cages: Option<&ItemCages>,
    png: Option<Vec<u8>>,
) -> Export {
    let texture = png.as_ref().map(|_| 0);
    let mut meshes = vec![surface(name, item, texture)];
    if let Some(c) = cages {
        meshes.push(surface(&format!("{name}_InnerCage"), &c.inner, None));
        meshes.push(surface(&format!("{name}_OuterCage"), &c.outer, None));
    }
    let nodes = meshes
        .iter()
        .enumerate()
        .map(|(i, m)| ExportNode {
            name: m.name.clone(),
            class: "MeshPart".into(),
            parent: None,
            placement: None,
            meshes: vec![i],
            material: None,
        })
        .collect();
    Export {
        meshes,
        textures: png.into_iter().collect(),
        nodes,
    }
}

const PNG: &[u8] = b"\x89PNG";

fn uri_of(dom: &WeakDom, node: Ref, keys: &[&str]) -> Option<String> {
    let props = dom.get(node)?.properties();
    keys.iter().find_map(|k| match props.get(*k) {
        Some(Variant::Content(Content::Uri(u))) if !u.is_empty() => Some(u.clone()),
        Some(Variant::String(u)) if !u.is_empty() => Some(u.clone()),
        _ => None,
    })
}

/// The handle's colour map, when it has an uploaded one.
fn colour_map(dom: &WeakDom, handle: Ref) -> Option<u64> {
    let sa = find(dom, handle, "SurfaceAppearance", "SurfaceAppearance");
    let uri = sa
        .and_then(|sa| uri_of(dom, sa, &["ColorMap", "ColorMapContent"]))
        .or_else(|| uri_of(dom, handle, &["TextureID", "TextureContent"]))?;
    match mesh_key(&uri)? {
        Key::Id(id) => Some(id),
        Key::Local(_) => None,
    }
}

/// Blocking: writes the glTF for the generated item and returns its path.
fn write_file(
    name: &str,
    mesh: Key,
    size: [f32; 3],
    cages: Option<ItemCages>,
    texture: Option<u64>,
) -> Result<PathBuf, String> {
    let item = cages::item_mesh(&fetch_cached(&mesh)?, size)?;
    let png = texture
        .and_then(|id| fetch_cached(&Key::Id(id)).ok())
        .filter(|b| b.starts_with(PNG));
    let text = rbx_viewer::export::gltf(&scene(name, &item, cages.as_ref(), png));
    let dir = rbx_assets::cache_root()
        .ok_or("No cache folder to write the upload into")?
        .join("fitting");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("{name}.gltf"));
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

impl FittingWindow {
    pub(super) fn can_generate(&self) -> bool {
        self.source.is_some()
            && self.live.as_ref().is_some_and(|l| l.item.is_some())
            && (!self.settings.layered() || self.cages.ready().is_some())
    }

    /// Writes the real accessory into `Workspace` as one undo step, from
    /// where the working copy sits on the fitting body, and drops the
    /// preview.
    pub(super) fn generate(&mut self, cx: &mut Context<Self>) {
        if !self.can_generate() {
            return;
        }
        let (Some(source), Some(live)) = (self.source.clone(), self.live.as_ref()) else {
            return;
        };
        let rig = live.rig;
        self.stop_motion(cx);
        let settings = self.settings.clone();
        let uris = self.cages.ready().map(|c| c.uris.clone());
        let made = self.shell.update(cx, |shell, cx| -> Result<Ref, String> {
            let (out, _) = preview::output(&shell.dom, rig, &settings, uris.as_ref())?;
            let ws = workspace(&shell.dom).ok_or("The place has no Workspace")?;
            if shell.dom.get(source.part).is_none() {
                return Err(format!("{} is no longer in the place", source.name));
            }
            preview::clear(&mut shell.dom, ws);
            shell.commit_preview(true, cx);
            shell.push_history();
            let accessory = generate::generate(&mut shell.dom, source.part, ws, &out);
            let changes = shell.dom.take_changes();
            shell.rebuild_explorer(cx);
            shell.reselect(vec![accessory], cx);
            shell.reflect_changes(&changes, cx);
            shell.record_history_change(changes);
            Ok(accessory)
        });
        match made {
            Ok(accessory) => {
                self.live = None;
                self.generated = Some(Generated {
                    accessory,
                    name: source.name,
                    upload: Job::Idle,
                });
                self.prepare_upload(cx);
                self.restart_check(cx);
            }
            Err(e) => self.wear_error = Some(e),
        }
        cx.notify();
    }

    /// Works out what Upload Item will send for the generated item.
    fn prepare_upload(&mut self, cx: &mut Context<Self>) {
        let Some(g) = &self.generated else { return };
        let (accessory, name) = (g.accessory, g.name.clone());
        let read = {
            let dom = &self.shell.read(cx).dom;
            find(dom, accessory, "MeshPart", "Handle").map(|handle| {
                let mesh =
                    uri_of(dom, handle, &["MeshId", "MeshContent"]).and_then(|u| mesh_key(&u));
                (mesh, preview::size(dom, handle), colour_map(dom, handle))
            })
        };
        let Some((mesh, size, texture)) = read else {
            return;
        };
        let cages = self.cages.ready().map(|c| c.now.clone());
        let upload = match (mesh, cages.is_some()) {
            (Some(Key::Id(_)), false) => Job::Ready(ItemSource::Instance {
                root: accessory,
                name,
                class: "Accessory".into(),
            }),
            (None, _) => Job::Failed(format!("{name}'s handle has no mesh to upload")),
            (Some(key), _) => {
                let task = cx
                    .background_spawn(async move { write_file(&name, key, size, cages, texture) });
                cx.spawn(async move |this, cx| {
                    let result = task.await;
                    let _ = this.update(cx, |view, cx| {
                        if let Some(g) = &mut view.generated {
                            g.upload = match result {
                                Ok(path) => Job::Ready(ItemSource::File(path)),
                                Err(e) => Job::Failed(e),
                            };
                        }
                        cx.notify();
                    });
                })
                .detach();
                Job::Busy
            }
        };
        if let Some(g) = &mut self.generated {
            g.upload = upload;
        }
    }

    pub(super) fn open_upload(&mut self, cx: &mut Context<Self>) {
        let Some(Job::Ready(source)) = self.generated.as_ref().map(|g| &g.upload) else {
            return;
        };
        let source = source.clone();
        let kind = ItemKind {
            asset: self.settings.asset,
            scale: self.settings.scale,
        };
        self.shell.update(cx, |shell, cx| {
            shell.open_ugc_upload(Some(source), Some(kind), cx)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{scene, ItemCages, Mesh, Vec3};
    use glam::Vec2;

    fn tri() -> Mesh {
        Mesh {
            positions: vec![Vec3::new(1.0, 2.0, 3.0), Vec3::X, Vec3::Y],
            normals: vec![Vec3::Z; 3],
            uvs: vec![Vec2::ZERO, Vec2::X, Vec2::Y],
            triangles: vec![[0, 1, 2]],
            ..Mesh::default()
        }
    }

    #[test]
    fn the_scene_names_the_cages_after_the_item_and_turns_it_for_the_importer() {
        let cages = ItemCages {
            inner: tri(),
            outer: tri(),
        };
        let e = scene("Coat", &tri(), Some(&cages), None);
        let names: Vec<&str> = e.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["Coat", "Coat_InnerCage", "Coat_OuterCage"]);
        assert_eq!(e.meshes[1].name, "Coat_InnerCage");
        assert_eq!(e.meshes[0].positions[0], [-1.0, 2.0, -3.0]);
        assert_eq!(e.meshes[0].normals[0], [0.0, 0.0, -1.0]);
        assert_eq!(e.meshes[2].indices, vec![0, 1, 2]);
        assert!(e.textures.is_empty() && e.meshes[0].maps.color.is_none());
    }

    #[test]
    fn a_rigid_item_is_one_node_with_its_texture() {
        let e = scene("Bow", &tri(), None, Some(b"\x89PNG".to_vec()));
        assert_eq!(e.nodes.len(), 1);
        assert_eq!(e.meshes[0].maps.color, Some(0));
        assert_eq!(e.textures.len(), 1);
    }
}
