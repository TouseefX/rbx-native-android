//! The fitting preview in the place: one `Model` in `Workspace` holding the
//! test body, a working copy of the item worn on it, the size box and the
//! cage overlays. Every function here edits a `WeakDom` and nothing else;
//! the window commits the edits outside the undo history.

use rbx_dom::{Content, Ref, Variant, Vector3Data, WeakDom};
use rbx_ugc::spec::item_bounds;

use super::generate::{self, CageUris, Kind, Output};
use super::state::{layered_part, Settings};
use crate::shell::clipboard;
use crate::shell::rig::{build_rig, Cf, RigOptions};

/// The preview's `Model`; one is all there ever is.
pub(super) const FOLDER: &str = "AccessoryFittingPreview";

fn set(dom: &mut WeakDom, node: Ref, key: &str, value: Variant) {
    let _ = dom.set_property(node, key, value);
}

fn v3(v: [f32; 3]) -> Variant {
    Variant::Vector3(Vector3Data {
        x: v[0],
        y: v[1],
        z: v[2],
    })
}

/// Removes every preview under `workspace`, a stale one an undo brought
/// back included.
pub(super) fn clear(dom: &mut WeakDom, workspace: Ref) {
    let stale: Vec<Ref> = dom
        .get(workspace)
        .map(|w| w.children().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|&c| dom.get(c).is_some_and(|i| i.name() == FOLDER))
        .collect();
    for c in stale {
        dom.remove(c);
    }
}

/// The preview's body, built fresh in a new preview `Model`.
pub(super) fn build(dom: &mut WeakDom, workspace: Ref, options: &RigOptions) -> (Ref, Ref) {
    clear(dom, workspace);
    let folder = dom.new_instance("Model", FOLDER, Some(workspace));
    let rig = build_rig(dom, options, folder);
    (folder, rig)
}

/// The first descendant of `root` named `name` of class `class`.
pub(super) fn find(dom: &WeakDom, root: Ref, class: &str, name: &str) -> Option<Ref> {
    let mut pending = vec![root];
    while let Some(next) = pending.pop() {
        let inst = dom.get(next)?;
        for &c in inst.children().iter().rev() {
            pending.push(c);
        }
        if next != root && inst.class() == class && inst.name() == name {
            return Some(next);
        }
    }
    None
}

pub(super) fn cframe(dom: &WeakDom, node: Ref, key: &str) -> Cf {
    match dom.get(node).and_then(|i| i.properties().get(key)) {
        Some(Variant::CFrame(c)) => Cf::from_data(c),
        _ => Cf::at([0.0; 3]),
    }
}

pub(super) fn size(dom: &WeakDom, part: Ref) -> [f32; 3] {
    let props = dom.get(part).map(|p| p.properties());
    match props.and_then(|p| p.get("size").or_else(|| p.get("Size"))) {
        Some(Variant::Vector3(v)) => [v.x, v.y, v.z],
        _ => [1.0; 3],
    }
}

/// A body part of `rig` and its world `CFrame`.
pub(super) fn part(dom: &WeakDom, rig: Ref, name: &str) -> Option<(Ref, Cf)> {
    let part = find(dom, rig, "MeshPart", name).or_else(|| find(dom, rig, "Part", name))?;
    Some((part, cframe(dom, part, "CFrame")))
}

/// The body attachment `name` of `rig`: its part and its world `CFrame`.
pub(super) fn attachment(dom: &WeakDom, rig: Ref, name: &str) -> Option<(Ref, Cf)> {
    let att = find(dom, rig, "Attachment", name)?;
    let part = dom.parent(att)?;
    let world = cframe(dom, part, "CFrame").mul(&cframe(dom, att, "CFrame"));
    Some((part, world))
}

/// Where the item's handle and its body attachment stand on `rig`, and the
/// part it is welded to. A rigid handle sits on the attachment moved by the
/// placement; a layered one on its body part with no offset.
pub(super) fn frames(
    dom: &WeakDom,
    rig: Ref,
    settings: &Settings,
) -> Result<(Cf, Cf, Ref), String> {
    let (att_part, body_att) = attachment(dom, rig, settings.attachment)
        .ok_or_else(|| format!("The test body has no {}", settings.attachment))?;
    if settings.layered() {
        let name = layered_part(settings.asset);
        let (part, at) =
            part(dom, rig, name).ok_or_else(|| format!("The test body has no {name}"))?;
        return Ok((at, body_att, part));
    }
    Ok((
        body_att.mul(&settings.placement.frame()),
        body_att,
        att_part,
    ))
}

/// What [`generate::generate`] writes for `settings` on `rig`.
pub(super) fn output<'a>(
    dom: &WeakDom,
    rig: Ref,
    settings: &'a Settings,
    cages: Option<&CageUris>,
) -> Result<(Output<'a>, Ref), String> {
    let (handle, body_attachment, part) = frames(dom, rig, settings)?;
    let kind = if settings.layered() {
        Kind::Layered {
            wrap: settings.wrap,
            cages: cages.cloned(),
        }
    } else {
        Kind::Rigid(settings.scale)
    };
    let scale = if settings.layered() {
        1.0
    } else {
        settings.placement.scale
    };
    Ok((
        Output {
            attachment: settings.attachment,
            handle,
            body_attachment,
            scale,
            kind,
        },
        part,
    ))
}

/// Welds `accessory`'s handle to `part` where it stands, the way a worn
/// accessory is held: `part = handle * C0 * C1^-1`.
pub(super) fn weld(dom: &mut WeakDom, accessory: Ref, part: Ref) {
    let Some(handle) = find(dom, accessory, "MeshPart", "Handle") else {
        return;
    };
    let c0 = cframe(dom, handle, "CFrame")
        .inverse()
        .mul(&cframe(dom, part, "CFrame"));
    let weld = dom.new_instance("Weld", "AccessoryWeld", Some(handle));
    set(dom, weld, "Part0", Variant::Ref(handle));
    set(dom, weld, "Part1", Variant::Ref(part));
    set(dom, weld, "C0", Variant::CFrame(c0.data()));
    set(dom, weld, "C1", Variant::CFrame(Cf::at([0.0; 3]).data()));
}

/// A copy of `source` under `folder`, made into the accessory `settings`
/// describe and worn on `rig`.
pub(super) fn wear(
    dom: &mut WeakDom,
    folder: Ref,
    rig: Ref,
    source: Ref,
    settings: &Settings,
    cages: Option<&CageUris>,
) -> Result<Ref, String> {
    let (out, part) = output(dom, rig, settings, cages)?;
    let copy = clipboard::detached(dom, &[source]);
    let root = *copy
        .root_refs()
        .first()
        .ok_or("The item is no longer in the place")?;
    let working = clipboard::graft(dom, &copy, root, folder).ok_or("Could not copy the item")?;
    let accessory = generate::generate(dom, working, folder, &out);
    weld(dom, accessory, part);
    Ok(accessory)
}

/// The spec's size box for `settings` on `rig`, outlined green, or red
/// when `over` (the item is too big): an invisible part, and the
/// `SelectionBox` on it.
pub(super) fn size_box(
    dom: &mut WeakDom,
    folder: Ref,
    rig: Ref,
    settings: &Settings,
    over: bool,
) -> Option<(Ref, Ref)> {
    let bounds = item_bounds(settings.asset, settings.scale, settings.attachment)?;
    let (_, at) = attachment(dom, rig, settings.attachment)?;
    let centre = at.mul(&Cf::at(bounds.offset));
    let part = dom.new_instance("Part", "FitBox", Some(folder));
    set(dom, part, "Anchored", Variant::Bool(true));
    set(dom, part, "CanCollide", Variant::Bool(false));
    set(dom, part, "CanQuery", Variant::Bool(false));
    set(dom, part, "CanTouch", Variant::Bool(false));
    set(dom, part, "Locked", Variant::Bool(true));
    set(dom, part, "Transparency", Variant::Float32(1.0));
    set(dom, part, "size", v3(bounds.size));
    set(dom, part, "CFrame", Variant::CFrame(centre.data()));
    let outline = dom.new_instance("SelectionBox", "FitBoxOutline", Some(folder));
    set(dom, outline, "Adornee", Variant::Ref(part));
    set(dom, outline, "SurfaceTransparency", Variant::Float32(0.88));
    set(dom, outline, "LineThickness", Variant::Float32(0.03));
    paint_box(dom, outline, over);
    Some((part, outline))
}

/// Colours the size box's outline: green when the item fits, red when not.
pub(super) fn paint_box(dom: &mut WeakDom, outline: Ref, over: bool) {
    let [r, g, b] = if over {
        [0.93, 0.33, 0.31]
    } else {
        [0.30, 0.82, 0.47]
    };
    let rgb = Variant::Color3(rbx_dom::Color3Data { r, g, b });
    set(dom, outline, "Color3", rgb.clone());
    set(dom, outline, "SurfaceColor3", rgb);
}

/// A cage drawn over the item.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Overlay {
    pub(super) name: String,
    pub(super) uri: String,
    /// The cage's extent in the handle's space, as (centre, size).
    pub(super) bounds: ([f32; 3], [f32; 3]),
    pub(super) color: [u8; 3],
}

/// A see-through copy of a cage mesh over the item standing at `handle`.
pub(super) fn overlay(
    dom: &mut WeakDom,
    folder: Ref,
    handle: Cf,
    cage: &Overlay,
    transparency: f32,
) -> Ref {
    let Overlay {
        name,
        uri,
        bounds,
        color,
    } = cage;
    let part = dom.new_instance("MeshPart", name, Some(folder));
    set(
        dom,
        part,
        "MeshId",
        Variant::Content(Content::Uri(uri.clone())),
    );
    for (key, on) in [
        ("Anchored", true),
        ("CanCollide", false),
        ("CanQuery", false),
        ("CanTouch", false),
        ("Locked", true),
        ("CastShadow", false),
    ] {
        set(dom, part, key, Variant::Bool(on));
    }
    set(dom, part, "size", v3(bounds.1));
    set(dom, part, "InitialSize", v3(bounds.1));
    let at = handle.mul(&Cf::at(bounds.0));
    set(dom, part, "CFrame", Variant::CFrame(at.data()));
    let [r, g, b] = *color;
    set(dom, part, "Color3uint8", Variant::Color3uint8 { r, g, b });
    set(dom, part, "Transparency", Variant::Float32(transparency));
    part
}

#[cfg(test)]
#[path = "preview/tests.rs"]
mod tests;
