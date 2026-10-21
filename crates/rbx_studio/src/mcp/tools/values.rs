//! A property's value as Studio's `inspect_instance` writes it, checked
//! against what real Studio answered for the same place: booleans and
//! numbers as JSON values, `Enum.SurfaceType.Smooth`, `0, -8, 0`, a CFrame
//! split into `.Position` and `.Rotation`, text without quotes, and a
//! `UniqueId` as the UUID Studio prints. Anything else reads as the
//! Properties panel shows it.
//!
//! What Studio leaves out is left out too: the identity a report already
//! gives at its top, and a property holding nothing.

use rbx_dom::{PhysicalProperties, UniqueId, Variant, WeakDom};
use serde_json::{json, Map, Value};

use crate::properties::Properties;

/// Text longer than this is summarized by the panel's own formatting (its
/// size) rather than repeated whole: a script's `Source` is read with
/// `script_read`.
const MAX_TEXT: usize = 200;

/// What `inspect_instance` reports outside `properties`, and `Origin`, the
/// Properties panel's own row for an instance's pivot, which no script reads
/// by that name and Studio's report does not list.
const LEFT_OUT: [&str; 4] = ["Name", "ClassName", "Parent", "Origin"];

/// Adds `name`'s value to `out`, as one entry or, for a CFrame, two, or
/// nothing for a property Studio does not list.
pub(super) fn insert(
    out: &mut Map<String, Value>,
    properties: &Properties,
    dom: &WeakDom,
    class: &str,
    name: &str,
    value: &Variant,
) {
    if LEFT_OUT.contains(&name) || holds_nothing(dom, value) {
        return;
    }
    let text = match value {
        Variant::Bool(flag) => json!(flag),
        Variant::Int32(n) => json!(n),
        Variant::Int64(n) => json!(n),
        Variant::Float32(n) => json!(f64::from(*n)),
        Variant::Float64(n) => json!(n),
        Variant::String(text) if text.len() <= MAX_TEXT => json!(text),
        Variant::Enum(raw) => json!(enumeration(properties, class, name, *raw)),
        Variant::Vector2(v) => json!(numbers(&[v.x, v.y])),
        Variant::Vector3(v) => json!(numbers(&[v.x, v.y, v.z])),
        Variant::Color3(c) => json!(numbers(&[c.r, c.g, c.b])),
        Variant::Color3uint8 { r, g, b } => json!(numbers(&[
            f32::from(*r) / 255.0,
            f32::from(*g) / 255.0,
            f32::from(*b) / 255.0
        ])),
        Variant::CFrame(frame) => {
            let p = &frame.position;
            out.insert(format!("{name}.Position"), json!(numbers(&[p.x, p.y, p.z])));
            out.insert(format!("{name}.Rotation"), json!(numbers(&frame.rotation)));
            return;
        }
        Variant::UniqueId(id) => json!(unique_id(id)),
        // No capabilities is what Studio prints as nothing at all.
        Variant::SecurityCapabilities(0) => json!(""),
        other => json!(properties.format(dom, class, name, other)),
    };
    out.insert(name.to_owned(), text);
}

/// A reference to nothing, default physics, an id never assigned, a number
/// that is not one: Studio's report has no entry for any of them.
fn holds_nothing(dom: &WeakDom, value: &Variant) -> bool {
    match value {
        Variant::Ref(reference) => dom.get(*reference).is_none(),
        Variant::OptionalCFrame(None) => true,
        Variant::PhysicalProperties(PhysicalProperties::Default) => true,
        Variant::UniqueId(id) => id.index == 0 && id.time == 0 && id.random == 0,
        Variant::Float32(n) => !n.is_finite(),
        Variant::Float64(n) => !n.is_finite(),
        _ => false,
    }
}

/// `Enum.SurfaceType.Smooth`, or the ordinal when the dump does not name it.
fn enumeration(properties: &Properties, class: &str, name: &str, raw: u32) -> String {
    let db = properties.db();
    db.resolve_property(class, name)
        .and_then(|property| {
            let item = db.enum_name(&property.value_type, raw)?;
            Some(format!("Enum.{}.{item}", property.value_type))
        })
        .unwrap_or_else(|| raw.to_string())
}

/// `0, -8, 0`: each component widened to `f64` first, which is how Studio
/// prints a float (`0.35686275362968445` for a colour's 91/255), with whole
/// numbers bare.
fn numbers(components: &[f32]) -> String {
    components
        .iter()
        .map(|&component| f64::from(component).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Studio's `00382dbe-99a6-3c35-09e5-50b2000003a5`: the random part, then
/// the time, then the index, as one 128-bit UUID.
pub(super) fn unique_id(id: &UniqueId) -> String {
    // The bits as stored, not the number they spell.
    let random = u64::from_ne_bytes(id.random.to_ne_bytes());
    let hex = format!("{random:016x}{:08x}{:08x}", id.time, id.index);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

#[cfg(test)]
mod tests {
    use rbx_dom::UniqueId;

    use super::{numbers, unique_id};

    #[test]
    fn numbers_read_as_studio_prints_them() {
        assert_eq!(numbers(&[0.0, -8.0, 0.0]), "0, -8, 0");
        assert_eq!(
            numbers(&[91.0 / 255.0]),
            "0.35686275362968445",
            "Baseplate's colour, as real Studio answered it"
        );
    }

    #[test]
    fn a_unique_id_reads_as_studios_uuid() {
        // TestPlace's Baseplate: the file's parts, and what Studio printed.
        let id = UniqueId {
            index: 0x0000_03a5,
            time: 0x09e5_50b2,
            random: 0x0038_2dbe_99a6_3c35,
        };
        assert_eq!(unique_id(&id), "00382dbe-99a6-3c35-09e5-50b2000003a5");
    }
}
