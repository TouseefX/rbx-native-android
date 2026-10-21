//! 3D notes: a short text pinned at a world position or to an instance,
//! drawn as a billboard in the edit viewport and listed in the Layers dock.
//!
//! Each note is an `ObjectValue` in a `Folder` named [`FOLDER_NAME`] under
//! `ServerStorage`, a container no game script reads by convention, and
//! that the publish path removes (see `crate::strip`). The `ObjectValue`'s
//! own `Value` is the instance a note is pinned to, so a pin follows its
//! instance through renames, reparenting, merges and both file formats as
//! any `Ref` property does. The rest are attributes, all prefixed
//! `RbxNative` so a note dragged elsewhere is still stripped:
//! [`TEXT`] and [`AUTHOR`] (`String`), [`COLOR`] (`Color3`) and [`OFFSET`]
//! (`Vector3`: the world position of an unpinned note, or the offset from
//! its instance's [`anchor`] for a pinned one).

use std::collections::BTreeMap;

use rbx_dom::{Color3Data, Ref, Variant, Vector3Data, WeakDom};

use crate::folder_colors::Rgb;
use crate::properties::attributes::{attributes, put_attribute};

pub(crate) const FOLDER_NAME: &str = "RbxNativeNotes";
pub(crate) const NOTE_CLASS: &str = "ObjectValue";
/// The text a freshly pinned note starts with, selected in its editor.
pub(crate) const DEFAULT_TEXT: &str = "Note";
pub(crate) const TEXT: &str = "RbxNativeNoteText";
pub(crate) const AUTHOR: &str = "RbxNativeNoteAuthor";
pub(crate) const COLOR: &str = "RbxNativeNoteColor";
pub(crate) const OFFSET: &str = "RbxNativeNoteOffset";

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Note {
    /// The `ObjectValue` holding it.
    pub(crate) referent: Ref,
    pub(crate) text: String,
    pub(crate) author: String,
    pub(crate) color: Rgb,
    /// The instance it is pinned to, while that still exists.
    pub(crate) target: Option<Ref>,
    /// Where it stands in the world now.
    pub(crate) position: [f32; 3],
}

/// The notes folder, when the place has one.
pub(crate) fn folder(dom: &WeakDom) -> Option<Ref> {
    dom.root_refs()
        .iter()
        .filter_map(|root| dom.get(*root))
        .filter(|root| root.class() == "ServerStorage")
        .flat_map(|root| root.children().iter().copied())
        .find(|child| {
            dom.get(*child)
                .is_some_and(|c| c.class() == "Folder" && c.name() == FOLDER_NAME)
        })
}

fn is_note(dom: &WeakDom, referent: Ref) -> bool {
    dom.get(referent).is_some_and(|instance| {
        instance.class() == NOTE_CLASS && attributes(dom, referent).contains_key(TEXT)
    })
}

fn string_attribute(map: &BTreeMap<String, Variant>, name: &str) -> String {
    match map.get(name) {
        Some(Variant::String(value)) => value.clone(),
        _ => String::new(),
    }
}

fn to_byte(channel: f32) -> u8 {
    (channel * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Every note in the folder, in child order. A child that is not a
/// [`NOTE_CLASS`] carrying a [`TEXT`] attribute is not a note. A note whose
/// target is gone, or has no [`anchor`], stands at its offset as a world
/// position.
pub(crate) fn notes(dom: &WeakDom) -> Vec<Note> {
    let Some(folder) = folder(dom) else {
        return Vec::new();
    };
    let Some(folder) = dom.get(folder) else {
        return Vec::new();
    };
    folder
        .children()
        .iter()
        .copied()
        .filter(|child| is_note(dom, *child))
        .filter_map(|referent| {
            let instance = dom.get(referent)?;
            let map = attributes(dom, referent);
            let color = match map.get(COLOR) {
                Some(Variant::Color3(c)) => (to_byte(c.r), to_byte(c.g), to_byte(c.b)),
                _ => (255, 255, 255),
            };
            let offset = match map.get(OFFSET) {
                Some(Variant::Vector3(v)) => [v.x, v.y, v.z],
                _ => [0.0; 3],
            };
            let target = match instance.properties().get("Value") {
                Some(Variant::Ref(target)) if dom.get(*target).is_some() => Some(*target),
                _ => None,
            };
            let position = target
                .and_then(|target| anchor(dom, target))
                .map_or(offset, |a| {
                    [a[0] + offset[0], a[1] + offset[1], a[2] + offset[2]]
                });
            Some(Note {
                referent,
                text: string_attribute(&map, TEXT),
                author: string_attribute(&map, AUTHOR),
                color,
                target,
                position,
            })
        })
        .collect()
}

fn storage_folder(dom: &mut WeakDom) -> Ref {
    if let Some(folder) = folder(dom) {
        return folder;
    }
    let existing = dom
        .root_refs()
        .iter()
        .copied()
        .find(|root| dom.get(*root).is_some_and(|r| r.class() == "ServerStorage"));
    let storage =
        existing.unwrap_or_else(|| dom.new_instance("ServerStorage", "ServerStorage", None));
    dom.new_instance("Folder", FOLDER_NAME, Some(storage))
}

/// Pins a new note named `Note` at world position `at`, to `target` when
/// given (its offset then measured from the target's [`anchor`]). Creates
/// `ServerStorage` (as a root) and the folder when the place lacks them.
pub(crate) fn add_note(
    dom: &mut WeakDom,
    text: &str,
    author: &str,
    color: Rgb,
    at: [f32; 3],
    target: Option<Ref>,
) -> Result<Ref, String> {
    if target.is_some_and(|target| dom.get(target).is_none()) {
        return Err("The instance to pin the note to no longer exists".to_owned());
    }
    let base = target.and_then(|target| anchor(dom, target));
    let offset = base.map_or(at, |a| [at[0] - a[0], at[1] - a[1], at[2] - a[2]]);
    let folder = storage_folder(dom);
    let note = dom.new_instance(NOTE_CLASS, "Note", Some(folder));
    if let Some(target) = target {
        dom.set_property(note, "Value", Variant::Ref(target))
            .map_err(|err| err.to_string())?;
    }
    let channel = |value: u8| f32::from(value) / 255.0;
    put_attribute(dom, note, TEXT, Some(Variant::String(text.to_owned())))?;
    put_attribute(dom, note, AUTHOR, Some(Variant::String(author.to_owned())))?;
    put_attribute(
        dom,
        note,
        COLOR,
        Some(Variant::Color3(Color3Data {
            r: channel(color.0),
            g: channel(color.1),
            b: channel(color.2),
        })),
    )?;
    put_attribute(
        dom,
        note,
        OFFSET,
        Some(Variant::Vector3(Vector3Data {
            x: offset[0],
            y: offset[1],
            z: offset[2],
        })),
    )?;
    Ok(note)
}

/// Rewrites a note's text, trimmed. Refused for a referent that is not a
/// note, and for blank text.
pub(crate) fn set_text(dom: &mut WeakDom, note: Ref, text: &str) -> Result<(), String> {
    if !is_note(dom, note) {
        return Err("Not a note".to_owned());
    }
    let text = text.trim();
    if text.is_empty() {
        return Err("A note needs text".to_owned());
    }
    put_attribute(dom, note, TEXT, Some(Variant::String(text.to_owned())))
}

/// Deletes a note. Refused for a referent that is not a note.
pub(crate) fn remove_note(dom: &mut WeakDom, note: Ref) -> Result<(), String> {
    if !is_note(dom, note) {
        return Err("Not a note".to_owned());
    }
    dom.remove(note);
    Ok(())
}

/// Where an instance stands, for pinning: its `CFrame`'s position, else
/// its `WorldPivot`'s, else the first descendant's (depth first) that has a
/// `CFrame`. `None` for an instance with none of those.
pub(crate) fn anchor(dom: &WeakDom, referent: Ref) -> Option<[f32; 3]> {
    let instance = dom.get(referent)?;
    let position_of = |name: &str| match instance.properties().get(name) {
        Some(Variant::CFrame(cf)) => Some([cf.position.x, cf.position.y, cf.position.z]),
        _ => None,
    };
    position_of("CFrame")
        .or_else(|| position_of("WorldPivot"))
        .or_else(|| first_cframe_below(dom, referent))
}

/// Where Pin Note puts a note on `referent`: its anchor, lifted to the top
/// of its world box when it is a part, so the billboard (and the camera
/// framing it) is not buried inside the part.
pub(crate) fn pin_point(dom: &WeakDom, referent: Ref) -> Option<[f32; 3]> {
    let [x, y, z] = anchor(dom, referent)?;
    let properties = dom.get(referent)?.properties();
    let size = properties.get("size").or_else(|| properties.get("Size"));
    let lift = match (properties.get("CFrame"), size) {
        (Some(Variant::CFrame(cf)), Some(Variant::Vector3(size))) => {
            let up = &cf.rotation[3..6];
            (up[0].abs() * size.x + up[1].abs() * size.y + up[2].abs() * size.z) / 2.0
        }
        _ => 0.0,
    };
    Some([x, y + lift, z])
}

fn first_cframe_below(dom: &WeakDom, referent: Ref) -> Option<[f32; 3]> {
    dom.get(referent)?.children().iter().find_map(|child| {
        match dom.get(*child)?.properties().get("CFrame") {
            Some(Variant::CFrame(cf)) => Some([cf.position.x, cf.position.y, cf.position.z]),
            _ => first_cframe_below(dom, *child),
        }
    })
}

#[cfg(test)]
#[path = "notes/tests.rs"]
mod tests;
