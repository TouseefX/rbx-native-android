//! `Instance` attributes in XML. A binary file keeps one inside
//! `AttributesSerialize` as a referent (see `rbx_dom::attributes`), but XML
//! referents are strings, and Studio writes it outside the blob instead: as
//! a `Ref` property named `__attrRef_<attribute>` beside an
//! `AttributesSerialize` that leaves it out. Evidence: a `.rbxlx` Roblox
//! Studio saved holding a state machine, whose `StateMachineNode` carries
//! `<BinaryString name="AttributesSerialize">AAAAAA==</BinaryString>` (no
//! attributes) and `<Ref name="__attrRef_EntryState">RBX…</Ref>` naming the
//! entry state's `ObjectValue` (`rbx_animation/tests/fixtures/
//! studio_state_machine.rbxmx`).
//!
//! The DOM holds the attribute in the blob whichever format a file came
//! from, so the reader folds each `__attrRef_` property into it and the
//! writer splits them back out.

use std::collections::BTreeMap;

use rbx_dom::{attributes, Ref, Variant};

use crate::value::STRING_TYPE_ID;

const PREFIX: &str = "__attrRef_";
const BLOB: &str = "AttributesSerialize";

fn bytes(value: Option<&Variant>) -> &[u8] {
    match value {
        Some(Variant::String(text)) => text.as_bytes(),
        Some(Variant::Unknown { raw, .. }) => raw,
        _ => &[],
    }
}

/// Moves every `__attrRef_` `Ref` in `properties` into the blob, appended
/// after the entries already there, which are kept byte for byte.
pub(crate) fn fold(properties: &mut BTreeMap<String, Variant>) {
    let refs: Vec<(String, Ref)> = properties
        .iter()
        .filter_map(|(key, value)| match (key.strip_prefix(PREFIX), value) {
            (Some(name), Variant::Ref(target)) => Some((name.to_owned(), *target)),
            _ => None,
        })
        .collect();
    if refs.is_empty() {
        return;
    }
    let old = bytes(properties.get(BLOB)).to_vec();
    let count = old
        .get(..4)
        .map_or(0, |n| u32::from_le_bytes([n[0], n[1], n[2], n[3]]));
    let mut blob = (count + refs.len() as u32).to_le_bytes().to_vec();
    blob.extend(old.get(4..).unwrap_or_default());
    for (name, target) in &refs {
        blob.extend((name.len() as u32).to_le_bytes());
        blob.extend(name.as_bytes());
        blob.push(attributes::INSTANCE_TYPE_ID);
        blob.extend(target.value().to_le_bytes());
        properties.remove(&format!("{PREFIX}{name}"));
    }
    let blob = match String::from_utf8(blob) {
        Ok(text) => Variant::String(text),
        Err(e) => Variant::Unknown {
            type_id: STRING_TYPE_ID,
            raw: e.into_bytes(),
        },
    };
    properties.insert(BLOB.to_owned(), blob);
}

/// For a property about to be written: when it is a blob holding `Instance`
/// attributes, the blob without them and each as its `__attrRef_` property.
pub(crate) fn split(name: &str, value: &Variant) -> Option<(Variant, Vec<(String, Ref)>)> {
    if name != BLOB {
        return None;
    }
    let stripped = attributes::remap_refs(value, |_| None)?;
    let refs = attributes::decode(Some(value))
        .into_iter()
        .filter_map(|(name, v)| match v {
            Variant::Ref(target) => Some((format!("{PREFIX}{name}"), target)),
            _ => None,
        })
        .collect();
    Some((stripped, refs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_and_split_are_inverses_and_keep_other_attributes() {
        let mut map = BTreeMap::new();
        map.insert("Speed".to_owned(), Variant::Float64(2.));
        let blob = attributes::encode(&map, |_| None).unwrap();
        let blob = Variant::String(String::from_utf8(blob).unwrap());
        let mut properties = BTreeMap::new();
        properties.insert(BLOB.to_owned(), blob.clone());
        properties.insert(format!("{PREFIX}Target"), Variant::Ref(Ref::new(7)));

        fold(&mut properties);
        assert_eq!(properties.len(), 1);
        let folded = attributes::decode(properties.get(BLOB));
        assert_eq!(folded["Speed"], Variant::Float64(2.));
        assert_eq!(folded["Target"], Variant::Ref(Ref::new(7)));

        let (stripped, refs) = split(BLOB, &properties[BLOB]).unwrap();
        assert_eq!(stripped, blob);
        assert_eq!(refs, vec![(format!("{PREFIX}Target"), Ref::new(7))]);
    }
}
