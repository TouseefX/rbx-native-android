//! XML place/model format writing: the encode counterpart of `deserialize`.
//!
//! Entry point: [`serialize`]. Walks the DOM tree once, writing `Item`s as it
//! goes; each property is delegated to `serializer::value`, mirroring the
//! reader's own `value` module one `Variant` kind at a time.

mod item;
mod value;
mod writer;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rbx_dom::WeakDom;

use crate::error::XmlError;
use item::write_item;
use writer::Writer;

pub(crate) use item::referent;

// Fixed rather than derived from the DOM: this crate has no reflection database
// to tell whether a given tree actually has (or needs) explicit joint instances,
// and Roblox's own Studio always writes this Meta tag with this value.
const EXPLICIT_AUTO_JOINTS: (&str, &str) = ("ExplicitAutoJoints", "true");

/// Serializes a DOM tree into a Roblox XML place/model file (`.rbxlx`/`.rbxmx`).
///
/// The output is a fresh encoding, not a byte-for-byte replica of any file the DOM
/// might have been read from: referents are derived from the DOM's own `Ref`
/// values (`RBX` + the numeric id) rather than reusing whatever string a source
/// file used. `deserialize(&serialize(&dom)?)` reconstructs an equivalent tree,
/// though not necessarily with the same numeric `Ref` values (the reader assigns
/// its own referents in file order on every parse; see its `assign_referents`).
///
/// A SharedString (`Variant::Unknown { 0x1C }`, how both readers hand one
/// back) is written as a `SharedString` reference into a `SharedStrings`
/// table after the last `Item`, keyed by its MD5 like Studio's own files.
pub fn serialize(dom: &WeakDom) -> Result<String, XmlError> {
    let mut writer = Writer::new();

    writer.open("roblox", &[("version", "4")]);
    writer.leaf(
        "Meta",
        &[("name", EXPLICIT_AUTO_JOINTS.0)],
        EXPLICIT_AUTO_JOINTS.1,
    );
    let mut shared = BTreeMap::new();
    for &root in dom.root_refs() {
        write_item(&mut writer, dom, root, &mut shared)?;
    }
    if !shared.is_empty() {
        writer.open("SharedStrings", &[]);
        for (key, raw) in &shared {
            writer.leaf("SharedString", &[("md5", key)], &STANDARD.encode(raw));
        }
        writer.close("SharedStrings");
    }
    writer.close("roblox");

    Ok(writer.into_string())
}
