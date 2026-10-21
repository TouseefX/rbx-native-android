//! The editor-only data a place carries for this editor, taken out of a
//! copy before it leaves: layer membership and the layer table
//! (`crate::layers`) and the notes folder (`crate::notes`). It happens on
//! Save/Publish to Roblox while Studio Settings' "Strip editor-only data
//! on publish" is on (the default), and on File › Export Clean Copy…. The open place and its local
//! file keep everything.

use rbx_dom::{Ref, WeakDom};

use crate::properties::attributes::{attributes, put_attribute};

/// Removes every attribute whose name starts with `crate::layers::PREFIX`
/// from every instance (the blob itself when nothing else is left, see
/// `properties::attributes::put_attribute`), and every `Folder` named
/// `crate::notes::FOLDER_NAME` with its contents. Returns how many
/// attributes and instances it removed.
pub(crate) fn strip_editor_data(dom: &mut WeakDom) -> usize {
    let mut removed = 0;

    let folders: Vec<Ref> = descendants(dom)
        .into_iter()
        .filter(|&r| {
            dom.get(r)
                .is_some_and(|i| i.class() == "Folder" && i.name() == crate::notes::FOLDER_NAME)
        })
        .collect();
    for folder in folders {
        if dom.get(folder).is_none() {
            continue;
        }
        removed += subtree_size(dom, folder);
        dom.remove(folder);
    }

    for r in descendants(dom) {
        let names: Vec<String> = attributes(dom, r)
            .into_keys()
            .filter(|name| name.starts_with(crate::layers::PREFIX))
            .collect();
        for name in names {
            if put_attribute(dom, r, &name, None).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

/// A stripped copy of `dom`, `dom` itself untouched.
pub(crate) fn stripped(dom: &WeakDom) -> WeakDom {
    let mut copy = dom.clone();
    strip_editor_data(&mut copy);
    copy
}

fn descendants(dom: &WeakDom) -> Vec<Ref> {
    let mut out = Vec::new();
    let mut stack: Vec<Ref> = dom.root_refs().to_vec();
    while let Some(r) = stack.pop() {
        if let Some(instance) = dom.get(r) {
            out.push(r);
            stack.extend(instance.children().iter().copied());
        }
    }
    out
}

fn subtree_size(dom: &WeakDom, root: Ref) -> usize {
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(r) = stack.pop() {
        if let Some(instance) = dom.get(r) {
            count += 1;
            stack.extend(instance.children().iter().copied());
        }
    }
    count
}

#[cfg(test)]
#[path = "strip/tests.rs"]
mod tests;
