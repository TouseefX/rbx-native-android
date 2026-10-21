//! Shared assets: a `Model`, `Folder` or `ModuleScript` marked with the
//! `RbxNativeSharedId` string attribute is the same asset in every place of
//! the game that holds an instance with that id, and Sync Shared copies the
//! active place's version over the others. The attribute is an ordinary
//! instance attribute, so a marked place is still a plain place file that
//! Roblox Studio and the engine open unchanged.
//!
//! Matching is by id alone, wherever the instance sits: an asset moved to
//! another folder in one place still syncs. An instance carrying the
//! attribute inside another shared instance travels with that outer one and
//! is not a sync root of its own.

use std::collections::hash_map::RandomState;
use std::collections::HashSet;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rbx_dom::diff::{diff, ChangeKind, DiffNode};
use rbx_dom::{Ref, Variant, WeakDom};
use rbx_reflection::ReflectionDatabase;

use super::super::clipboard::{detached, snapshot};
use super::search::full_name;
use crate::properties::attributes;

mod links;
#[cfg(test)]
mod tests;

/// The attribute that marks an instance as shared; its value is the id.
pub(crate) const SHARED_ATTRIBUTE: &str = "RbxNativeSharedId";

/// Whether `reference` may be marked: a `Model` (or a subclass of it, by
/// `db.is_subclass_of`), a `Folder` or a `ModuleScript`.
pub(crate) fn shareable(dom: &WeakDom, db: &ReflectionDatabase, reference: Ref) -> bool {
    dom.get(reference).is_some_and(|instance| {
        let class = instance.class();
        class == "Folder" || class == "ModuleScript" || db.is_subclass_of(class, "Model")
    })
}

/// The instance's shared id: the attribute's value when it is a non-empty
/// `Variant::String`.
pub(crate) fn shared_id(dom: &WeakDom, reference: Ref) -> Option<String> {
    match attributes::attributes(dom, reference).remove(SHARED_ATTRIBUTE) {
        Some(Variant::String(id)) if !id.is_empty() => Some(id),
        _ => None,
    }
}

/// A fresh id: 32 lowercase hex digits, unique per call within a process
/// and across runs (std `RandomState` hashing of the time and a counter; no
/// new dependency).
pub(crate) fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let half = || {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u128(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos()),
        );
        hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
        hasher.finish()
    };
    format!("{:016x}{:016x}", half(), half())
}

/// Sets the attribute to `id`, keeping every other attribute
/// (`crate::properties::attributes::attributes`, insert, then
/// `crate::properties::attributes::write_attributes`). Overwrites an
/// existing id. Errors when `reference` does not resolve.
pub(crate) fn mark(dom: &mut WeakDom, reference: Ref, id: &str) -> Result<(), String> {
    if dom.get(reference).is_none() {
        return Err("the instance is gone".into());
    }
    let mut current = attributes::attributes(dom, reference);
    current.insert(SHARED_ATTRIBUTE.to_owned(), Variant::String(id.to_owned()));
    attributes::write_attributes(dom, reference, &current)
}

/// Drops the attribute; `Ok(false)` when it was not there.
pub(crate) fn unmark(dom: &mut WeakDom, reference: Ref) -> Result<bool, String> {
    let mut current = attributes::attributes(dom, reference);
    if current.remove(SHARED_ATTRIBUTE).is_none() {
        return Ok(false);
    }
    attributes::write_attributes(dom, reference, &current).map(|()| true)
}

/// Every sync root in `dom`, depth-first in child order: instances carrying
/// an id that have no ancestor carrying one.
pub(crate) fn shared_roots(dom: &WeakDom) -> Vec<(String, Ref)> {
    let mut roots = Vec::new();
    let mut stack: Vec<Ref> = dom.root_refs().iter().rev().copied().collect();
    while let Some(reference) = stack.pop() {
        if let Some(id) = shared_id(dom, reference) {
            roots.push((id, reference));
            continue;
        }
        if let Some(instance) = dom.get(reference) {
            stack.extend(instance.children().iter().rev());
        }
    }
    roots
}

/// One shared asset as it stands in a target place, against the source's.
#[derive(Debug, Clone)]
pub(crate) struct Pair {
    /// The source instance's name.
    pub(crate) name: String,
    pub(crate) source: Ref,
    pub(crate) target: Ref,
    /// The target instance's full name (`super::search::full_name`).
    pub(crate) target_path: String,
    /// What a sync would change in the target: `rbx_dom::diff::diff` of the
    /// target's subtree (before) against the source's (after), each first
    /// copied out on its own with `super::super::clipboard::detached`.
    /// Empty when the two already match.
    pub(crate) nodes: Vec<DiffNode>,
}

/// For every sync root of `source` (the first one, when an id repeats),
/// every sync root of `target` with the same id, in `source` order then
/// `target` order. Ids only one side holds produce nothing.
pub(crate) fn plan(source: &WeakDom, target: &WeakDom, db: &ReflectionDatabase) -> Vec<Pair> {
    let targets = shared_roots(target);
    let mut seen = HashSet::new();
    let mut pairs = Vec::new();
    for (id, source_ref) in shared_roots(source) {
        if !seen.insert(id.clone()) {
            continue;
        }
        let before_source = detached(source, &[source_ref]);
        for &(_, target_ref) in targets.iter().filter(|(other, _)| *other == id) {
            pairs.push(Pair {
                name: source
                    .get(source_ref)
                    .map(|instance| instance.name().to_owned())
                    .unwrap_or_default(),
                source: source_ref,
                target: target_ref,
                target_path: full_name(target, target_ref),
                nodes: tidy(diff(&detached(target, &[target_ref]), &before_source), db),
            });
        }
    }
    pairs
}

/// Leaves out what is not a change to the asset: the instances' own
/// identities (`UniqueId`, `HistoryId`, different in every place), and a
/// property neither side really sets: absent on one side and the class
/// default (or, with no default known, an empty value) on the other, or
/// two spellings of empty. The binary format
/// writes a property for every instance of a class once any instance has
/// it, so the same asset reads with more or fewer defaults depending on its
/// neighbours. An update left with nothing is dropped; an addition lists
/// only what was set, as presence diffs do.
fn tidy(mut nodes: Vec<DiffNode>, db: &ReflectionDatabase) -> Vec<DiffNode> {
    super::super::team_presence::drop_defaults(&mut nodes, db);
    for node in &mut nodes {
        if node.kind != ChangeKind::Updated {
            continue;
        }
        let class = node.class.clone();
        node.properties.retain(|change| {
            if IDENTITY.contains(&change.name.as_str()) {
                return false;
            }
            let default = db.stored_default(&class, &change.name);
            // Absent, the class default, or (with no default known) an empty
            // blob such as `Tags` or a mesh's data: all read as never set.
            let unset = |value: &Option<Variant>| match value {
                None => true,
                Some(value) => match default {
                    Some(default) => value == default,
                    None => {
                        matches!(value, Variant::String(text) if text.is_empty())
                            || matches!(value, Variant::Unknown { raw, .. } if raw.is_empty())
                    }
                },
            };
            !(unset(&change.before) && unset(&change.after))
        });
    }
    nodes.retain(|node| {
        node.kind != ChangeKind::Updated || !node.properties.is_empty() || node.source.is_some()
    });
    nodes
}

/// Per-instance ids the engine gives every instance of every place.
const IDENTITY: [&str; 2] = ["UniqueId", "HistoryId"];

/// Replaces each pair's target subtree in `target` by a fresh copy of its
/// source subtree under the same parent, at the same position among its
/// siblings, with references across its edge and its `UniqueId`s put right
/// (`links::swap`). Pairs with no `nodes`, or whose refs no longer resolve,
/// are skipped. Returns the new roots, in pair order. Every write goes
/// through `WeakDom`'s logged mutators so `take_changes` reports it.
pub(crate) fn apply(target: &mut WeakDom, source: &WeakDom, pairs: &[Pair]) -> Vec<Ref> {
    let mut roots = Vec::new();
    for pair in pairs.iter().filter(|pair| !pair.nodes.is_empty()) {
        let (Some(node), Some(_)) = (snapshot(source, pair.source), target.get(pair.target)) else {
            continue;
        };
        roots.push(links::swap(target, source, &node, pair.target));
    }
    roots
}
