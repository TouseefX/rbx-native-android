//! The `Accessory` the Fitting Tool writes, laid out exactly as Studio's
//! Accessory Fitting Tool writes it (measured on its output in Studio):
//!
//! - an `Accessory` named after the source part, `AccessoryType` Unknown
//!   for rigid and layered alike, `AttachmentPoint` identity;
//! - the source part itself as its `Handle`, keeping its mesh, size,
//!   texture and children, unanchored, not massless, queryable and
//!   touchable, colliding when rigid and not when layered, standing where
//!   it sat on the preview body;
//! - rigid: an `AvatarPartScaleType` `StringValue` after the existing
//!   children, then the attachment;
//! - layered: a `HandleWrapLayer` (`WrapLayer`) pointing at the item's
//!   cages, then the attachment;
//! - the attachment named for the type's body attachment, its `CFrame` the
//!   handle's inverse times the body attachment's world `CFrame`, so the fit
//!   lives in it.
//!
//! Studio's own tool uploads the cages it makes for clothing as it writes
//! the item; this writes local cage files and uploads nothing.

use rbx_dom::{Content, Ref, Variant, Vector3Data, WeakDom};
use rbx_ugc::BodyScale;

use super::state::Wrap;
use crate::shell::rig::Cf;

/// The two cage meshes of a layered item, by URI.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CageUris {
    pub(super) inner: String,
    pub(super) outer: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Kind {
    Rigid(BodyScale),
    Layered { wrap: Wrap, cages: Option<CageUris> },
}

/// Where the item goes and what it becomes.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Output<'a> {
    /// The attachment's name, the body attachment it pairs with.
    pub(super) attachment: &'a str,
    /// The handle's world `CFrame`.
    pub(super) handle: Cf,
    /// The body attachment's world `CFrame`.
    pub(super) body_attachment: Cf,
    /// Uniform factor on the handle's size.
    pub(super) scale: f32,
    pub(super) kind: Kind,
}

const SCALE_VALUE: &str = "AvatarPartScaleType";
const WRAP_NAME: &str = "HandleWrapLayer";

fn set(dom: &mut WeakDom, node: Ref, key: &str, value: Variant) {
    let _ = dom.set_property(node, key, value);
}

fn uri(text: &str) -> Variant {
    Variant::Content(Content::Uri(text.to_string()))
}

fn identity() -> Variant {
    Variant::CFrame(Cf::at([0.0; 3]).data())
}

/// Drops `part`'s children a previous fit left, so fitting twice gives one
/// of each.
fn clear_fit(dom: &mut WeakDom, part: Ref, attachment: &str) {
    let stale: Vec<Ref> = dom
        .get(part)
        .map(|p| p.children().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|&c| {
            dom.get(c).is_some_and(|i| {
                matches!(i.class(), "WrapLayer" | "Weld" | "WeldConstraint")
                    || (i.class() == "StringValue" && i.name() == SCALE_VALUE)
                    || (i.class() == "Attachment" && i.name() == attachment)
            })
        })
        .collect();
    for c in stale {
        dom.remove(c);
    }
}

/// The part's size times `scale`, under whichever key it already uses
/// (files store `size`).
fn scale_size(dom: &mut WeakDom, part: Ref, scale: f32) {
    if (scale - 1.0).abs() < f32::EPSILON {
        return;
    }
    let Some(props) = dom.get(part).map(|p| p.properties()) else {
        return;
    };
    let Some((key, v)) = ["size", "Size"].iter().find_map(|k| match props.get(*k) {
        Some(Variant::Vector3(v)) => Some((*k, *v)),
        _ => None,
    }) else {
        return;
    };
    let scaled = Vector3Data {
        x: v.x * scale,
        y: v.y * scale,
        z: v.z * scale,
    };
    set(dom, part, key, Variant::Vector3(scaled));
}

fn add_wrap(dom: &mut WeakDom, handle: Ref, wrap: &Wrap, cages: Option<&CageUris>) {
    let layer = dom.new_instance("WrapLayer", WRAP_NAME, Some(handle));
    set(dom, layer, "Order", Variant::Int32(wrap.order));
    set(dom, layer, "Puffiness", Variant::Float32(wrap.puffiness));
    set(dom, layer, "ShrinkFactor", Variant::Float32(wrap.shrink));
    set(
        dom,
        layer,
        "AutoSkin",
        Variant::Enum(wrap.auto_skin.value()),
    );
    set(dom, layer, "Enabled", Variant::Bool(true));
    set(dom, layer, "BindOffset", identity());
    // The cage files hold their vertices in the handle's own space, so
    // neither needs an origin of its own.
    set(dom, layer, "CageOrigin", identity());
    set(dom, layer, "ReferenceOrigin", identity());
    if let Some(cages) = cages {
        set(dom, layer, "CageMeshId", uri(&cages.outer));
        set(dom, layer, "ReferenceMeshId", uri(&cages.inner));
    }
}

/// Makes `part` the `Handle` of a new `Accessory` under `parent`, as
/// [`Output`] says, and returns the accessory.
pub(super) fn generate(dom: &mut WeakDom, part: Ref, parent: Ref, out: &Output) -> Ref {
    let name = dom
        .get(part)
        .map_or_else(|| "Accessory".to_string(), |p| p.name().to_string());
    let accessory = dom.new_instance("Accessory", &name, Some(parent));
    set(dom, accessory, "AccessoryType", Variant::Enum(0));
    set(dom, accessory, "AttachmentPoint", identity());

    clear_fit(dom, part, out.attachment);
    dom.set_parent(part, Some(accessory));
    let _ = dom.set_name(part, "Handle");
    let layered = matches!(out.kind, Kind::Layered { .. });
    for (key, on) in [
        ("Anchored", false),
        ("Massless", false),
        ("CanQuery", true),
        ("CanTouch", true),
        ("CanCollide", !layered),
    ] {
        set(dom, part, key, Variant::Bool(on));
    }
    set(dom, part, "CFrame", Variant::CFrame(out.handle.data()));
    scale_size(dom, part, out.scale);

    match &out.kind {
        Kind::Rigid(scale) => {
            let value = dom.new_instance("StringValue", SCALE_VALUE, Some(part));
            set(
                dom,
                value,
                "Value",
                Variant::String(scale.part_scale_type().into()),
            );
        }
        Kind::Layered { wrap, cages } => add_wrap(dom, part, wrap, cages.as_ref()),
    }
    let attachment = dom.new_instance("Attachment", out.attachment, Some(part));
    let local = out.handle.inverse().mul(&out.body_attachment);
    set(dom, attachment, "CFrame", Variant::CFrame(local.data()));
    accessory
}

#[cfg(test)]
#[path = "generate/tests.rs"]
mod tests;
