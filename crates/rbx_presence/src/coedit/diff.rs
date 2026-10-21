//! What changed locally: the ops that take `shadow` (the DOM as last synced)
//! to `current`, found by comparing the two by referent. Every way the
//! editor mutates the DOM, undo and redo included, shows up here, so nothing
//! has to hook each mutation site.

use std::collections::HashSet;

use rbx_dom::{Ref, Variant, WeakDom};
use rbx_reflection::ReflectionDatabase;
use serde_json::{json, Map, Value};

use super::attrs;
use super::engine::Engine;
use super::ids::IdMap;
use super::names;
use super::value;
use super::{Op, Target};
use crate::paths;

const SCRIPT_CLASSES: [&str; 3] = ["Script", "LocalScript", "ModuleScript"];

fn target_of(dom: &mut WeakDom, ids: &mut IdMap, r: Ref) -> Option<Target> {
    let id = ids.id_of(dom, r)?;
    let p = paths::path_of(dom, r)?;
    Some(Target { id, p })
}

fn encode_prop(current: &mut WeakDom, ids: &mut IdMap, v: &Variant) -> Option<Value> {
    if let Variant::Ref(t) = v {
        target_of(current, ids, *t)?;
    }
    let (dom, ids) = (&*current, &*ids);
    value::encode(v, &|t| {
        Some(Target {
            id: ids.known(t)?.to_string(),
            p: paths::path_of(dom, t)?,
        })
    })
}

fn string_value(s: &str) -> Value {
    json!({"t": "String", "v": s})
}

fn parent_value(current: &mut WeakDom, ids: &mut IdMap, r: Ref) -> Option<Value> {
    match current.parent(r) {
        Some(p) => {
            let t = target_of(current, ids, p)?;
            Some(json!({"t": "Ref", "v": t}))
        }
        None => Some(json!({"t": "Ref", "v": Value::Null})),
    }
}

/// Shadow referents to delete (top-most only) and every shadow referent
/// that a delete takes with it.
fn collect_deletes(
    shadow: &WeakDom,
    current: &WeakDom,
    r: Ref,
    under_deleted: bool,
    deleted: &mut HashSet<Ref>,
    dels: &mut Vec<Ref>,
) {
    let Some(old) = shadow.get(r) else { return };
    let mut inside = under_deleted;
    if under_deleted {
        deleted.insert(r);
    } else if current.get(r).is_none_or(|c| c.class() != old.class()) {
        deleted.insert(r);
        dels.push(r);
        inside = true;
    }
    for &child in old.children() {
        collect_deletes(shadow, current, child, inside, deleted, dels);
    }
}

fn tree_order(dom: &WeakDom, refs: &[Ref], out: &mut Vec<Ref>) {
    for &r in refs {
        out.push(r);
        if let Some(i) = dom.get(r) {
            tree_order(dom, i.children(), out);
        }
    }
}

/// Ops for every difference, parents before children:
/// - removed referents → `Del` (only the top-most of a removed subtree;
///   id from `ids.known`, path from `shadow`);
/// - added referents → `Add` with all encodable properties plus `Name`
///   (`Source` included as a String);
/// - class changed → `Del` then `Add`;
/// - name / parent changed → `Set` `Name` / `Parent`;
/// - a property changed or added → `Set` (removed → `Set` with `null`),
///   except `Source` on a script, which becomes `Src` against
///   `engine.log_text_hash(id)`.
///
/// Unsendable types ([`super::value`]) are skipped. Clocks come from
/// `engine.tick()`. Paths are `crate::paths::path_of` in `current` (in
/// `shadow` for `Del`).
pub fn local_ops(
    shadow: &WeakDom,
    current: &mut WeakDom,
    ids: &mut IdMap,
    engine: &mut Engine,
) -> Vec<Op> {
    let mut deleted = HashSet::new();
    let mut dels = Vec::new();
    for &r in shadow.root_refs() {
        collect_deletes(shadow, current, r, false, &mut deleted, &mut dels);
    }
    let mut ops = Vec::new();
    for r in dels {
        let id = ids.known(r).unwrap_or_default().to_string();
        let p = paths::path_of(shadow, r).unwrap_or_default();
        ops.push(Op::Del {
            at: Target { id, p },
            c: engine.tick(),
        });
    }

    let mut order = Vec::new();
    tree_order(current, current.root_refs(), &mut order);
    for r in order {
        let old = shadow.get(r).filter(|_| !deleted.contains(&r));
        let Some(inst) = current.get(r) else { continue };
        // Most of a large place is unchanged: skip it before naming it.
        let unchanged = old.is_some_and(|old| {
            old.name() == inst.name()
                && shadow.parent(r) == current.parent(r)
                && old.properties() == inst.properties()
        });
        if unchanged {
            continue;
        }
        let Some(at) = target_of(current, ids, r) else {
            continue;
        };
        let Some(inst) = current.get(r) else { continue };
        let class = inst.class().to_string();
        let name = inst.name().to_string();
        let props: Vec<(String, Variant)> = inst
            .properties()
            .iter()
            .filter(|(k, _)| k.as_str() != "UniqueId" && !attrs::is_blob(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let blobs_changed = old.is_none_or(|old| {
            [attrs::ATTRIBUTES, attrs::TAGS]
                .iter()
                .any(|k| old.properties().get(*k) != inst.properties().get(*k))
        });
        let entries = if blobs_changed {
            attrs::wire_entries(inst)
        } else {
            Default::default()
        };
        let Some(old) = old else {
            let parent = match current.parent(r) {
                Some(p) => target_of(current, ids, p),
                None => None,
            };
            let mut map = Map::new();
            map.insert("Name".to_string(), string_value(&name));
            // A new instance starts at its class defaults on every peer, so
            // only what differs is sent (thousands of parts at once stay
            // small).
            let db = ReflectionDatabase::shared();
            for (k, v) in &props {
                if db.default_value(&class, k) == Some(v) {
                    continue;
                }
                if let Some(e) = encode_prop(current, ids, v) {
                    map.insert(names::wire_name(&class, k), e);
                }
            }
            map.extend(entries);
            ops.push(Op::Add {
                at,
                cls: class,
                parent,
                props: map,
                c: engine.tick(),
            });
            continue;
        };

        // A peer that finds this instance by path (the Studio plugin, which
        // cannot read ids) still has it where it was: the rename goes to the
        // old path, the move to the old parent under the new name.
        let mut before = Target {
            id: at.id.clone(),
            p: paths::path_of(shadow, r).unwrap_or_else(|| at.p.clone()),
        };
        if name != old.name() {
            ops.push(Op::Set {
                at: before.clone(),
                n: "Name".to_string(),
                v: string_value(&name),
                c: engine.tick(),
            });
            if let Some(mut names) = paths::split(&before.p) {
                if let Some(last) = names.last_mut() {
                    last.clone_from(&name);
                }
                before.p = paths::join(&names);
            }
        }
        if current.parent(r) != shadow.parent(r) {
            if let Some(v) = parent_value(current, ids, r) {
                ops.push(Op::Set {
                    at: before,
                    n: "Parent".to_string(),
                    v,
                    c: engine.tick(),
                });
            }
        }
        let is_script = SCRIPT_CLASSES.contains(&class.as_str());
        for (k, v) in &props {
            if old.properties().get(k) == Some(v) {
                continue;
            }
            if is_script && k == "Source" {
                if let Variant::String(text) = v {
                    let before = match old.properties().get("Source") {
                        Some(Variant::String(s)) => s.as_str(),
                        _ => "",
                    };
                    let base = engine.log_text_hash(&at.id, before);
                    ops.push(Op::Src {
                        at: at.clone(),
                        base,
                        text: text.clone(),
                        c: engine.tick(),
                    });
                    continue;
                }
            }
            if let Some(e) = encode_prop(current, ids, v) {
                ops.push(Op::Set {
                    at: at.clone(),
                    n: names::wire_name(&class, k),
                    v: e,
                    c: engine.tick(),
                });
            }
        }
        if blobs_changed {
            let (before, after) = (attrs::wire_entries(old), entries);
            for (n, v) in &after {
                if before.get(n) != Some(v) {
                    ops.push(Op::Set {
                        at: at.clone(),
                        n: n.clone(),
                        v: v.clone(),
                        c: engine.tick(),
                    });
                }
            }
            for n in before.keys().filter(|n| !after.contains_key(*n)) {
                ops.push(Op::Set {
                    at: at.clone(),
                    n: n.clone(),
                    v: Value::Null,
                    c: engine.tick(),
                });
            }
        }
        for (k, v) in old.properties() {
            if k == "UniqueId" || attrs::is_blob(k) || props.iter().any(|(n, _)| n == k) {
                continue;
            }
            let sendable = value::encode(v, &|_| {
                Some(Target {
                    id: String::new(),
                    p: String::new(),
                })
            })
            .is_some();
            if sendable {
                ops.push(Op::Set {
                    at: at.clone(),
                    n: names::wire_name(&class, k),
                    v: Value::Null,
                    c: engine.tick(),
                });
            }
        }
    }
    ops
}

#[cfg(test)]
mod tests;
