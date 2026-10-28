//! Other items worn with the one being fitted while testing: two public
//! catalog garments downloaded anonymously, and every accessory already in
//! the place, each switchable, layered clothing in `WrapLayer.Order`.

use std::sync::Arc;

use gpui_kit::*;
use rbx_cloud::{AssetType, AvatarAsset};
use rbx_dom::{Ref, Variant, WeakDom};

use super::preview::{find, FOLDER};
use super::{workspace, FittingWindow, Job};
use crate::shell::clipboard;
use crate::shell::rig::Worn;

/// Public layered catalog items known to load anonymously: id, name and
/// Roblox asset type (67 jacket, 66 pants).
const CATALOG: [(u64, &str, u32); 2] = [
    (8516609675, "Denim Jacket", 67),
    (6984740059, "Cargo Pants", 66),
];

/// Where a layer's model comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum From {
    Catalog(u64),
    Place(Ref),
}

pub(super) struct Layer {
    pub(super) label: String,
    pub(super) from: From,
    /// The Roblox asset type it is worn as.
    kind: u32,
    /// `WrapLayer.Order`, for layered items.
    pub(super) order: Option<i32>,
    pub(super) on: bool,
    pub(super) model: Job<Arc<WeakDom>>,
}

/// The asset type a layered accessory is worn as, by the body attachment it
/// names: it decides the body part its handle sits on.
fn layered_kind(dom: &WeakDom, handle: Ref) -> u32 {
    let has = |name: &str| find(dom, handle, "Attachment", name).is_some();
    if has("WaistCenterAttachment") {
        66
    } else if has("LeftFootAttachment") {
        70
    } else if has("RightFootAttachment") {
        71
    } else {
        67
    }
}

fn order_of(dom: &WeakDom, root: Ref) -> Option<i32> {
    let layer = find(dom, root, "WrapLayer", "HandleWrapLayer")
        .or_else(|| find(dom, root, "WrapLayer", "WrapLayer"))?;
    match dom.get(layer)?.properties().get("Order") {
        Some(Variant::Int32(v)) => Some(*v),
        _ => Some(1),
    }
}

/// Every `Accessory` in `Workspace` outside the preview, as a layer.
fn place_layers(dom: &WeakDom) -> Vec<Layer> {
    let Some(ws) = workspace(dom) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut pending = vec![ws];
    while let Some(next) = pending.pop() {
        let Some(inst) = dom.get(next) else { continue };
        if inst.name() == FOLDER && dom.parent(next) == Some(ws) {
            continue;
        }
        if inst.class() == "Accessory" {
            let handle = find(dom, next, "MeshPart", "Handle");
            let order = order_of(dom, next);
            let kind = match (handle, order) {
                (Some(h), Some(_)) => layered_kind(dom, h),
                _ => 8,
            };
            out.push(Layer {
                label: inst.name().to_string(),
                from: From::Place(next),
                kind,
                order,
                on: false,
                model: Job::Ready(Arc::new(clipboard::detached(dom, &[next]))),
            });
            continue;
        }
        pending.extend(inst.children().iter().rev());
    }
    out
}

/// Blocking: a catalog item's model, through the asset cache.
fn download(id: u64) -> Result<WeakDom, String> {
    let cache = rbx_assets::AssetCache::new(None).map_err(|e| e.to_string())?;
    let bytes = match cache.get_id(id) {
        Some(bytes) => bytes,
        None => {
            let bytes = rbx_cloud::Client::new(None)
                .asset(id)
                .map_err(|e| format!("downloading {id}: {e}"))?
                .bytes;
            let _ = cache.put_id(id, &bytes);
            bytes
        }
    };
    crate::shell::freeze::read_model(&bytes)
}

/// Sets every `WrapLayer.Order` in `dom` to `order`.
fn set_order(dom: &mut WeakDom, order: i32) {
    let layers: Vec<Ref> = dom
        .root_refs()
        .to_vec()
        .into_iter()
        .flat_map(|root| {
            let mut found = Vec::new();
            let mut pending = vec![root];
            while let Some(next) = pending.pop() {
                if let Some(inst) = dom.get(next) {
                    if inst.class() == "WrapLayer" {
                        found.push(next);
                    }
                    pending.extend(inst.children());
                }
            }
            found
        })
        .collect();
    for layer in layers {
        let _ = dom.set_property(layer, "Order", Variant::Int32(order));
    }
}

impl FittingWindow {
    /// The catalog items, then the place's accessories, all off.
    pub(super) fn load_layers(&mut self, cx: &App) {
        let mut layers: Vec<Layer> = CATALOG
            .iter()
            .enumerate()
            .map(|(i, &(id, name, kind))| Layer {
                label: name.to_string(),
                from: From::Catalog(id),
                kind,
                order: Some(2 + i as i32),
                on: false,
                model: Job::Idle,
            })
            .collect();
        layers.extend(place_layers(&self.shell.read(cx).dom));
        self.layers = layers;
    }

    /// Switches layer `index`, downloading a catalog item the first time.
    pub(super) fn toggle_layer(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(layer) = self.layers.get_mut(index) else {
            return;
        };
        layer.on = !layer.on;
        if let (true, Job::Idle | Job::Failed(_), From::Catalog(id)) =
            (layer.on, &layer.model, layer.from)
        {
            layer.model = Job::Busy;
            let task = cx.background_spawn(async move { download(id) });
            cx.spawn(async move |this, cx| {
                let result = task.await;
                let _ = this.update(cx, |view, cx| {
                    if let Some(layer) = view.layers.get_mut(index) {
                        layer.model = match result {
                            Ok(dom) => Job::Ready(Arc::new(dom)),
                            Err(e) => Job::Failed(e),
                        };
                    }
                    view.sync(cx);
                });
            })
            .detach();
        }
        self.sync(cx);
    }

    /// Swaps the order of layer `index` (or the fitted item, `None`) with
    /// the next layered item above (`up`) or below it.
    pub(super) fn reorder(&mut self, index: Option<usize>, up: bool, cx: &mut Context<Self>) {
        let mine = self.settings.wrap.order;
        let current = match index {
            Some(i) => match self.layers.get(i).and_then(|l| l.order) {
                Some(o) => o,
                None => return,
            },
            None => mine,
        };
        let target = if up {
            current + 1
        } else {
            (current - 1).max(1)
        };
        if target == current {
            return;
        }
        // Whoever holds the target order trades places.
        for (i, layer) in self.layers.iter_mut().enumerate() {
            if Some(i) != index && layer.order == Some(target) {
                layer.order = Some(current);
            }
        }
        if index.is_some() && mine == target {
            self.settings.wrap.order = current;
        }
        match index {
            Some(i) => self.layers[i].order = Some(target),
            None => self.settings.wrap.order = target,
        }
        self.sync(cx);
    }

    /// The layers switched on and loaded, as `rig::dress` takes them.
    pub(super) fn worn_layers(&self) -> Vec<Worn> {
        self.layers
            .iter()
            .filter(|l| l.on)
            .filter_map(|l| {
                let model = l.model.ready()?;
                let mut dom = WeakDom::clone(model);
                if let Some(order) = l.order {
                    set_order(&mut dom, order);
                }
                Some(Worn {
                    asset: AvatarAsset {
                        id: match l.from {
                            From::Catalog(id) => id,
                            From::Place(_) => 0,
                        },
                        name: l.label.clone(),
                        asset_type: AssetType {
                            id: l.kind,
                            name: String::new(),
                        },
                    },
                    dom: Ok(dom),
                })
            })
            .collect()
    }
}
