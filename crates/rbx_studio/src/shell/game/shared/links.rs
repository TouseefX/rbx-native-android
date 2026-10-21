//! Swapping a target's copy of a shared asset for the source's without
//! breaking what points at it or giving the place ids it already holds.
//!
//! Referents only mean something inside the DOM they came from, so every
//! reference that crosses the swapped subtree's edge is looked up again by
//! path: a step is a child's name and which of its same-named siblings it
//! is. One that no longer resolves is cleared, which is how this DOM stores
//! a nil reference.

use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

use rbx_dom::{Content, Ref, UniqueId, Variant, WeakDom};

use super::super::super::clipboard::{materialize_mapped, Clipped};

type Step = (String, usize);

const UNIQUE_ID: &str = "UniqueId";

/// Replaces `old` in `target` by a copy of `node` (snapshotted from
/// `source`) at the same place among its siblings, and returns the copy.
///
/// - A reference from outside `old` into it now points at the instance at
///   the same path in the copy.
/// - A reference in the copy to something outside the source's subtree
///   points at whatever sits at that path in `target`.
/// - Each copied instance takes the `UniqueId` of the instance it replaces
///   at the same path, else a new one, so the place never holds the
///   source's ids and an unchanged instance keeps its own across syncs.
pub(super) fn swap(target: &mut WeakDom, source: &WeakDom, node: &Clipped, old: Ref) -> Ref {
    let inbound: Vec<(Ref, String, Option<Vec<Step>>)> = subtree(target, None)
        .into_iter()
        .filter(|&holder| !within(target, old, holder))
        .flat_map(|holder| {
            references(target, holder)
                .filter(|&(_, pointee)| within(target, old, pointee))
                .map(|(name, pointee)| (holder, name, path(target, Some(old), pointee)))
                .collect::<Vec<_>>()
        })
        .collect();
    let ids: HashMap<Vec<Step>, Variant> = subtree(target, Some(old))
        .into_iter()
        .filter_map(|r| {
            let id = target.get(r)?.properties().get(UNIQUE_ID)?.clone();
            Some((path(target, Some(old), r)?, id))
        })
        .collect();

    let (fresh, copies) = materialize_mapped(target, node, target.parent(old));
    target.place_after(fresh, old);
    target.remove(old);

    for (&origin, &copy) in &copies {
        let outbound: Vec<(String, Option<Ref>)> = references(source, origin)
            .filter(|(_, pointee)| !copies.contains_key(pointee))
            .map(|(name, pointee)| {
                let found = path(source, None, pointee).and_then(|p| resolve(target, None, &p));
                (name, found)
            })
            .collect();
        for (name, found) in outbound {
            relink(target, copy, &name, found);
        }
    }
    for (holder, name, steps) in inbound {
        let found = steps.and_then(|steps| resolve(target, Some(fresh), &steps));
        relink(target, holder, &name, found);
    }

    let mut new_id = id_maker();
    for copy in subtree(target, Some(fresh)) {
        let kept = path(target, Some(fresh), copy).and_then(|p| ids.get(&p).cloned());
        let has_id = target
            .get(copy)
            .is_some_and(|i| i.properties().contains_key(UNIQUE_ID));
        if let Some(id) = kept.or_else(|| has_id.then(|| Variant::UniqueId(new_id()))) {
            let _ = target.set_property(copy, UNIQUE_ID, id);
        }
    }
    fresh
}

/// Points `holder`'s reference `name` at `to`, keeping the property's kind
/// (`Ref` or `Content::Object`), or clears it.
fn relink(dom: &mut WeakDom, holder: Ref, name: &str, to: Option<Ref>) {
    let Some(value) = dom.get(holder).and_then(|i| i.properties().get(name)) else {
        return;
    };
    let relinked = match (value, to) {
        (Variant::Ref(_), Some(to)) => Variant::Ref(to),
        (Variant::Content(Content::Object(_)), Some(to)) => Variant::Content(Content::Object(to)),
        _ => {
            let _ = dom.remove_property(holder, name);
            return;
        }
    };
    let _ = dom.set_property(holder, name, relinked);
}

/// `holder`'s reference properties and what each points at.
fn references(dom: &WeakDom, holder: Ref) -> impl Iterator<Item = (String, Ref)> + '_ {
    dom.get(holder)
        .into_iter()
        .flat_map(|i| i.properties())
        .filter_map(|(name, value)| match value {
            Variant::Ref(r) | Variant::Content(Content::Object(r)) => Some((name.clone(), *r)),
            _ => None,
        })
}

/// `top` and everything under it, or the whole DOM for `None`.
fn subtree(dom: &WeakDom, top: Option<Ref>) -> Vec<Ref> {
    let mut stack: Vec<Ref> = match top {
        Some(top) => vec![top],
        None => dom.root_refs().to_vec(),
    };
    let mut all = Vec::new();
    while let Some(r) = stack.pop() {
        if let Some(instance) = dom.get(r) {
            stack.extend(instance.children());
            all.push(r);
        }
    }
    all
}

fn within(dom: &WeakDom, top: Ref, reference: Ref) -> bool {
    std::iter::successors(Some(reference), |&up| dom.parent(up)).any(|at| at == top)
}

fn siblings(dom: &WeakDom, parent: Option<Ref>) -> &[Ref] {
    match parent {
        Some(parent) => dom.get(parent).map_or(&[], |i| i.children()),
        None => dom.root_refs(),
    }
}

/// The steps from `top` (the DOM's roots for `None`) down to `reference`,
/// or `None` when it is not under `top`.
fn path(dom: &WeakDom, top: Option<Ref>, reference: Ref) -> Option<Vec<Step>> {
    let mut steps = Vec::new();
    let mut at = reference;
    while Some(at) != top {
        let name = dom.get(at)?.name();
        let parent = dom.parent(at);
        let nth = siblings(dom, parent)
            .iter()
            .take_while(|&&sibling| sibling != at)
            .filter(|&&sibling| dom.get(sibling).is_some_and(|i| i.name() == name))
            .count();
        steps.push((name.to_owned(), nth));
        match parent {
            Some(up) => at = up,
            None if top.is_none() => break,
            None => return None,
        }
    }
    steps.reverse();
    Some(steps)
}

fn resolve(dom: &WeakDom, top: Option<Ref>, steps: &[Step]) -> Option<Ref> {
    steps
        .iter()
        .try_fold(top, |at, (name, nth)| {
            siblings(dom, at)
                .iter()
                .copied()
                .filter(|&child| dom.get(child).is_some_and(|i| i.name() == name))
                .nth(*nth)
                .map(Some)
        })
        .flatten()
}

/// Ids shaped as Studio makes them: one random number per run, the run's
/// start in seconds since 2021-01-01, and a counter.
fn id_maker() -> impl FnMut() -> UniqueId {
    const EPOCH_2021: u64 = 1_609_459_200;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(now.as_nanos());
    let random = hasher.finish() as i64;
    let time = now.as_secs().saturating_sub(EPOCH_2021) as u32;
    let mut index = 0;
    move || {
        index += 1;
        UniqueId {
            index,
            time,
            random,
        }
    }
}

#[cfg(test)]
mod tests;
