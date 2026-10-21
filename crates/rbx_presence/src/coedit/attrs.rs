//! Attributes and tags on the wire. A place file packs them into two blobs
//! (`AttributesSerialize`, `Tags`) that Studio scripts can't read or write,
//! so each travels as its own property instead:
//! - `@<name>`: one attribute, its value as [`super::value`] JSON (an
//!   `EnumItem` as `{"t": "EnumItem", "v": ["<Enum>", ordinal]}`), `null`
//!   when removed;
//! - `#<tag>`: one tag, `{"t": "Bool", "v": true}`, `null` when removed.
//!
//! One property per attribute and per tag means two editors changing
//! different ones never overwrite each other (last-writer-wins is per
//! property).

use std::collections::BTreeMap;

use rbx_dom::attributes;
use rbx_dom::{Instance, Ref, Variant, WeakDom};
use serde_json::{json, Value};

use super::value;

pub const ATTRIBUTES: &str = "AttributesSerialize";
pub const TAGS: &str = "Tags";

/// Whether `stored` is one of the two blobs sent entry by entry.
pub fn is_blob(stored: &str) -> bool {
    stored == ATTRIBUTES || stored == TAGS
}

fn encode_attribute(v: &Variant, enum_type: Option<&String>) -> Option<Value> {
    match (v, enum_type) {
        (Variant::Enum(n), Some(name)) => Some(json!({"t": "EnumItem", "v": [name, n]})),
        (Variant::Ref(_), _) => None,
        _ => value::encode(v, &|_| None),
    }
}

/// Every attribute and tag of `inst` as wire properties.
pub fn wire_entries(inst: &Instance) -> BTreeMap<String, Value> {
    let blob = inst.properties().get(ATTRIBUTES);
    let enums = attributes::enum_types(blob);
    let mut out: BTreeMap<String, Value> = attributes::decode(blob)
        .iter()
        .filter_map(|(name, v)| Some((format!("@{name}"), encode_attribute(v, enums.get(name))?)))
        .collect();
    for tag in attributes::tags(inst.properties().get(TAGS)) {
        out.insert(format!("#{tag}"), json!({"t": "Bool", "v": true}));
    }
    out
}

/// Writes wire property `n` = `v` on `r` when `n` is an attribute or a tag:
/// `Some(changed)`, `None` for any other property.
pub fn write(dom: &mut WeakDom, r: Ref, n: &str, v: &Value) -> Option<bool> {
    if let Some(name) = n.strip_prefix('@') {
        let inst = dom.get(r)?;
        let blob = inst.properties().get(ATTRIBUTES);
        let (new, enum_type) = match v.get("t").and_then(Value::as_str) {
            None => (None, None),
            Some("EnumItem") => {
                let pair = v.get("v")?.as_array()?;
                let ordinal = u32::try_from(pair.get(1)?.as_u64()?).ok()?;
                (
                    Some(Variant::Enum(ordinal)),
                    Some(pair.first()?.as_str()?.to_string()),
                )
            }
            Some(_) => (Some(value::decode(v, &|_| None)?), None),
        };
        if attributes::decode(blob).get(name) == new.as_ref() {
            return Some(false);
        }
        let blob = attributes::set_attribute(blob, name, new.as_ref(), enum_type.as_deref())?;
        return Some(dom.set_property(r, ATTRIBUTES, blob).is_ok());
    }
    let tag = n.strip_prefix('#')?;
    let inst = dom.get(r)?;
    let mut tags = attributes::tags(inst.properties().get(TAGS));
    let had = tags.contains(&tag);
    let want = !v.is_null();
    if had == want {
        return Some(false);
    }
    if want {
        tags.push(tag);
    } else {
        tags.retain(|t| *t != tag);
    }
    let text = attributes::encode_tags(&tags);
    Some(dom.set_property(r, TAGS, Variant::String(text)).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_round_trip_through_write() {
        let mut dom = WeakDom::new();
        let a = dom.new_instance("Part", "A", None);
        let b = dom.new_instance("Part", "B", None);
        assert_eq!(
            write(&mut dom, a, "@Speed", &json!({"t": "Float64", "v": 16.0})),
            Some(true)
        );
        let kind = json!({"t": "EnumItem", "v": ["Material", 256]});
        assert_eq!(write(&mut dom, a, "@Kind", &kind), Some(true));
        assert_eq!(write(&mut dom, a, "@Kind", &kind), Some(false));
        assert_eq!(
            write(&mut dom, a, "#Lava", &json!({"t": "Bool", "v": true})),
            Some(true)
        );
        assert_eq!(
            write(&mut dom, a, "#Kill", &json!({"t": "Bool", "v": true})),
            Some(true)
        );
        assert_eq!(
            write(&mut dom, a, "Anchored", &json!({"t": "Bool", "v": true})),
            None
        );

        let entries = wire_entries(dom.get(a).unwrap());
        assert_eq!(entries["@Kind"], kind);
        assert_eq!(entries["@Speed"], json!({"t": "Float64", "v": 16.0}));
        assert_eq!(entries.keys().filter(|k| k.starts_with('#')).count(), 2);
        for (n, v) in &entries {
            write(&mut dom, b, n, v);
        }
        assert_eq!(wire_entries(dom.get(b).unwrap()), entries);

        assert_eq!(write(&mut dom, a, "@Speed", &Value::Null), Some(true));
        assert_eq!(write(&mut dom, a, "#Lava", &Value::Null), Some(true));
        let left = wire_entries(dom.get(a).unwrap());
        assert_eq!(left.keys().collect::<Vec<_>>(), ["#Kill", "@Kind"]);
    }
}
