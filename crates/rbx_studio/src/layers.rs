//! Editor-only layers: named, coloured groups an instance can belong to,
//! at most one, so a part of a large place can be hidden, locked, soloed
//! or isolated in the edit viewport the way every 3D tool's layers can.
//!
//! What a layer *is* travels with the place, so co-editing, merge and a
//! teammate opening the file all see the same layers: membership is an
//! attribute on the instance ([`MEMBER_ATTRIBUTE`] = the layer's id), and
//! each layer's name and colour is one attribute on `Workspace`
//! ([`DEF_PREFIX`]`<id>` = `"rrggbb name"`). One attribute per layer rather
//! than one table: `rbx_dom::merge` merges attributes by name, so two people
//! adding a layer each merge cleanly instead of conflicting on one blob.
//! Attributes are plain engine data, both file formats round-trip them
//! and Roblox ignores them, and the publish path strips them anyway (see
//! `crate::strip`).
//!
//! What a layer *shows* ([`LayerView`]: hidden, locked, solo, isolate) is
//! the viewer's own state, kept per place in the editor's config rather
//! than in the file: hiding a layer is a way of looking, not an edit, so it
//! must not dirty the file, fight a co-editor's toggles or reach a publish.
//! None of it ever writes `Transparency`, `Locked` or `Parent`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use rbx_dom::{Ref, Variant, WeakDom};
use serde::{Deserialize, Serialize};

use crate::folder_colors::Rgb;
use crate::properties::attributes::{attributes, put_attribute};

/// The prefix every attribute this editor stores for itself starts with;
/// `crate::strip` removes all of them.
pub(crate) const PREFIX: &str = "RbxNative";
/// On any instance: the id of the layer it belongs to (a `String`).
pub(crate) const MEMBER_ATTRIBUTE: &str = "RbxNativeLayer";
/// On `Workspace`: `RbxNativeLayerDef_<id>` = `"rrggbb name"` (a `String`).
pub(crate) const DEF_PREFIX: &str = "RbxNativeLayerDef_";

/// The swatches a new layer cycles through, in order.
pub(crate) const PALETTE: [Rgb; 8] = [
    (0x4F, 0x8C, 0xFF),
    (0xFF, 0x8A, 0x3D),
    (0x3D, 0xC4, 0x7E),
    (0xE5, 0x4B, 0x6B),
    (0xA8, 0x6B, 0xFF),
    (0xF2, 0xC9, 0x4C),
    (0x2E, 0xC4, 0xC4),
    (0x9A, 0xA4, 0xB2),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layer {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) color: Rgb,
}

/// The place's `Workspace`, where the layer table lives.
pub(crate) fn workspace(dom: &WeakDom) -> Option<Ref> {
    dom.root_refs()
        .iter()
        .copied()
        .find(|&r| dom.get(r).is_some_and(|i| i.class() == "Workspace"))
}

/// Every instance, parents before children, siblings in child order.
fn tree_order(dom: &WeakDom) -> Vec<Ref> {
    let mut out = Vec::new();
    let mut stack: Vec<Ref> = dom.root_refs().iter().rev().copied().collect();
    while let Some(r) = stack.pop() {
        let Some(instance) = dom.get(r) else {
            continue;
        };
        out.push(r);
        stack.extend(instance.children().iter().rev().copied());
    }
    out
}

fn parse_def(value: &Variant) -> Option<(Rgb, String)> {
    let Variant::String(text) = value else {
        return None;
    };
    let (hex, name) = text.split_once(' ')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(((channel(0)?, channel(2)?, channel(4)?), name.to_owned()))
}

fn encode_def(name: &str, (r, g, b): Rgb) -> Variant {
    Variant::String(format!("{r:02x}{g:02x}{b:02x} {name}"))
}

fn def_key(id: &str) -> String {
    format!("{DEF_PREFIX}{id}")
}

fn definition(dom: &WeakDom, id: &str) -> Option<Layer> {
    layers(dom).into_iter().find(|layer| layer.id == id)
}

/// Every layer defined on `Workspace`, sorted by name then id. A definition
/// that does not parse (not a `String`, no 6-hex-digit colour) is skipped.
pub(crate) fn layers(dom: &WeakDom) -> Vec<Layer> {
    let Some(ws) = workspace(dom) else {
        return Vec::new();
    };
    let mut out: Vec<Layer> = attributes(dom, ws)
        .iter()
        .filter_map(|(key, value)| {
            let id = key.strip_prefix(DEF_PREFIX)?;
            let (color, name) = parse_def(value)?;
            Some(Layer {
                id: id.to_owned(),
                name,
                color,
            })
        })
        .collect();
    out.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
    out
}

/// Defines a new layer and returns its id: `L1`, `L2`, … the first not
/// already defined. Refused without a `Workspace` or with a blank name.
pub(crate) fn add_layer(dom: &mut WeakDom, name: &str, color: Rgb) -> Result<String, String> {
    let ws = workspace(dom).ok_or("The place has no Workspace")?;
    let name = name.trim();
    if name.is_empty() {
        return Err("A layer needs a name".to_owned());
    }
    let taken = attributes(dom, ws);
    let id = (1..)
        .map(|n| format!("L{n}"))
        .find(|id| !taken.contains_key(&def_key(id)))
        .expect("an unbounded range always yields a free id");
    put_attribute(dom, ws, &def_key(&id), Some(encode_def(name, color)))?;
    Ok(id)
}

/// Refused for an unknown id or a blank name.
pub(crate) fn rename_layer(dom: &mut WeakDom, id: &str, name: &str) -> Result<(), String> {
    let layer = definition(dom, id).ok_or_else(|| format!("No layer {id}"))?;
    let name = name.trim();
    if name.is_empty() {
        return Err("A layer needs a name".to_owned());
    }
    let ws = workspace(dom).ok_or("The place has no Workspace")?;
    put_attribute(dom, ws, &def_key(id), Some(encode_def(name, layer.color)))
}

/// Refused for an unknown id.
pub(crate) fn recolor_layer(dom: &mut WeakDom, id: &str, color: Rgb) -> Result<(), String> {
    let layer = definition(dom, id).ok_or_else(|| format!("No layer {id}"))?;
    let ws = workspace(dom).ok_or("The place has no Workspace")?;
    put_attribute(dom, ws, &def_key(id), Some(encode_def(&layer.name, color)))
}

/// Removes the definition and every instance's membership of it. Refused
/// for an unknown id.
pub(crate) fn delete_layer(dom: &mut WeakDom, id: &str) -> Result<(), String> {
    definition(dom, id).ok_or_else(|| format!("No layer {id}"))?;
    let ws = workspace(dom).ok_or("The place has no Workspace")?;
    for member in members(dom, id) {
        put_attribute(dom, member, MEMBER_ATTRIBUTE, None)?;
    }
    put_attribute(dom, ws, &def_key(id), None)
}

/// The layer `referent` itself is assigned to.
pub(crate) fn own_layer(dom: &WeakDom, referent: Ref) -> Option<String> {
    match attributes(dom, referent).remove(MEMBER_ATTRIBUTE)? {
        Variant::String(id) => Some(id),
        _ => None,
    }
}

/// The layer `referent` is drawn as part of: its own, or else the nearest
/// ancestor's, a `Model` put on a layer takes its parts with it.
pub(crate) fn layer_of(dom: &WeakDom, referent: Ref) -> Option<String> {
    let mut current = Some(referent);
    while let Some(r) = current {
        if let Some(id) = own_layer(dom, r) {
            return Some(id);
        }
        current = dom.parent(r);
    }
    None
}

/// Puts every one of `refs` on layer `id`, or takes them off any layer
/// with `None`. Refused (nothing written) when `id` is not defined or a
/// referent is gone; services and `Workspace` itself are refused too.
pub(crate) fn assign(dom: &mut WeakDom, refs: &[Ref], id: Option<&str>) -> Result<(), String> {
    if let Some(id) = id {
        definition(dom, id).ok_or_else(|| format!("No layer {id}"))?;
    }
    for &r in refs {
        let instance = dom.get(r).ok_or("The instance is gone")?;
        if dom.parent(r).is_none() || instance.class() == "Workspace" {
            return Err(format!("{} cannot be put on a layer", instance.name()));
        }
    }
    for &r in refs {
        put_attribute(
            dom,
            r,
            MEMBER_ATTRIBUTE,
            id.map(|id| Variant::String(id.to_owned())),
        )?;
    }
    Ok(())
}

/// The instances assigned to `id` themselves, in tree order.
pub(crate) fn members(dom: &WeakDom, id: &str) -> Vec<Ref> {
    tree_order(dom)
        .into_iter()
        .filter(|&r| own_layer(dom, r).as_deref() == Some(id))
        .collect()
}

/// How many instances are assigned to each layer id.
pub(crate) fn counts(dom: &WeakDom) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    for r in tree_order(dom) {
        if let Some(id) = own_layer(dom, r) {
            *out.entry(id).or_insert(0) += 1;
        }
    }
    out
}

/// One viewer's view of the layers: per-user editor state, never saved
/// into the place (see the module doc).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LayerView {
    pub(crate) hidden: BTreeSet<String>,
    pub(crate) locked: BTreeSet<String>,
    /// While any is soloed, layers not soloed are hidden (instances on no
    /// layer stay drawn).
    pub(crate) solo: BTreeSet<String>,
    /// Draws only this layer's instances, everything else left out.
    pub(crate) isolate: Option<String>,
}

impl LayerView {
    /// Whether layer `id` draws as far as hide and solo go.
    pub(crate) fn is_shown(&self, id: &str) -> bool {
        !self.hidden.contains(id) && (self.solo.is_empty() || self.solo.contains(id))
    }

    /// The roots the viewport leaves undrawn: every instance assigned to a
    /// layer that is not [`LayerView::is_shown`], in tree order.
    pub(crate) fn hidden_roots(&self, dom: &WeakDom) -> Vec<Ref> {
        let defined: BTreeSet<String> = layers(dom).into_iter().map(|l| l.id).collect();
        tree_order(dom)
            .into_iter()
            .filter(|&r| {
                own_layer(dom, r).is_some_and(|id| defined.contains(&id) && !self.is_shown(&id))
            })
            .collect()
    }

    /// The isolated layer's members, the only roots the viewport draws, or
    /// `None` while nothing is isolated or the isolated id is not defined.
    pub(crate) fn isolate_roots(&self, dom: &WeakDom) -> Option<Vec<Ref>> {
        let id = self.isolate.as_deref()?;
        definition(dom, id)?;
        Some(members(dom, id))
    }

    /// Whether a viewport click or drag passes through `referent`: its
    /// [`layer_of`] is locked.
    pub(crate) fn is_locked(&self, dom: &WeakDom, referent: Ref) -> bool {
        layer_of(dom, referent).is_some_and(|id| self.locked.contains(&id))
    }

    /// Whether the Explorer dims `referent`'s row: its [`layer_of`] is
    /// locked or not shown.
    pub(crate) fn dims(&self, dom: &WeakDom, referent: Ref) -> bool {
        layer_of(dom, referent).is_some_and(|id| self.locked.contains(&id) || !self.is_shown(&id))
    }

    /// Drops every id `layers` no longer defines.
    pub(crate) fn prune(&mut self, layers: &[Layer]) {
        let known = |id: &String| layers.iter().any(|l| &l.id == id);
        self.hidden.retain(known);
        self.locked.retain(known);
        self.solo.retain(known);
        if self.isolate.as_ref().is_some_and(|id| !known(id)) {
            self.isolate = None;
        }
    }
}

/// Every place's [`LayerView`], keyed by the place file's path, in
/// `layer_view.json` beside `folder_colors.json` (same load/save discipline
/// as `crate::folder_colors::FolderColors`).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LayerViews(BTreeMap<String, LayerView>);

impl LayerViews {
    /// A missing or malformed file is an empty store.
    pub(crate) fn load() -> LayerViews {
        let Some(path) = store_path() else {
            return LayerViews::default();
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return LayerViews::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    pub(crate) fn save(&self) -> Result<(), crate::settings::SettingsError> {
        let path = store_path().ok_or(crate::settings::SettingsError::NoConfigDir)?;
        let bytes = serde_json::to_vec_pretty(self).expect("LayerViews always serializes");
        crate::settings::write_atomic(&path, &bytes)
    }

    pub(crate) fn get(&self, place: &Path) -> LayerView {
        self.0
            .get(place.to_string_lossy().as_ref())
            .cloned()
            .unwrap_or_default()
    }

    /// Stores `view` for `place`; a default view removes the entry.
    pub(crate) fn set(&mut self, place: &Path, view: LayerView) {
        let key = place.to_string_lossy().into_owned();
        if view == LayerView::default() {
            self.0.remove(&key);
        } else {
            self.0.insert(key, view);
        }
    }
}

fn store_path() -> Option<std::path::PathBuf> {
    crate::settings::default_config_dir().map(|dir| dir.join("layer_view.json"))
}

#[cfg(test)]
#[path = "layers/tests.rs"]
mod tests;
