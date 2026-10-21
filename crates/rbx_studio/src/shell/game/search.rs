//! Find in Game's text search: every script's `Source` in one place,
//! matched the way the script editor's own Find All matches
//! (`script_editor::find`: plain text, ASCII case-insensitive), grouped by
//! script with one row per matching line, plus Replace's preview and its
//! write. Pure functions over a `WeakDom`, so a place that is not loaded
//! in the window can be searched from its file alone.
//!
//! Text only. An instance query (selecting by class, property or tag) would
//! be a second producer of the same per-place rows: `search`'s shape
//! (one `WeakDom` in, rows grouped by instance out) is the seam it plugs
//! into, with the Find window unchanged.

use std::ops::Range;

use rbx_dom::{Ref, WeakDom};
use rbx_reflection::ReflectionDatabase;

use crate::script_editor::{find, source};

#[cfg(test)]
mod tests;

/// One matching line of a script.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LineHit {
    /// 1-based.
    pub(crate) line: usize,
    /// The first match on this line, as a byte range into the whole `Source`
    /// (what the script editor selects on a jump).
    pub(crate) range: Range<usize>,
    /// The line's text with leading indentation dropped (`find::line_at`).
    pub(crate) text: String,
    /// How many matches this line holds.
    pub(crate) count: usize,
}

/// One script with at least one match.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScriptHits {
    pub(crate) script: Ref,
    /// Its full name, e.g. `ServerScriptService.Game.Main` (see [`full_name`]).
    pub(crate) path: String,
    pub(crate) class: String,
    pub(crate) lines: Vec<LineHit>,
}

/// One line a Replace would change, before and after (whole line, no
/// trailing newline, indentation kept).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Preview {
    pub(crate) line: usize,
    pub(crate) before: String,
    pub(crate) after: String,
}

/// Every script in `dom` (any `LuaSourceContainer`, `source::is_script`)
/// whose `Source` matches `query`, depth-first in child order from the
/// roots. `budget` is the number of line rows still allowed: each `LineHit`
/// added takes one, and once it reaches 0 the walk stops (the script being
/// filled keeps the lines it already got). An empty query finds nothing.
pub(crate) fn search(
    dom: &WeakDom,
    db: &ReflectionDatabase,
    query: &str,
    budget: &mut usize,
) -> Vec<ScriptHits> {
    let mut out = Vec::new();
    if query.is_empty() {
        return out;
    }
    for reference in walk(dom) {
        if *budget == 0 {
            break;
        }
        if !source::is_script(dom, db, reference) {
            continue;
        }
        let Some(text) = source::read(dom, reference) else {
            continue;
        };
        let mut lines: Vec<LineHit> = Vec::new();
        let mut full = false;
        for range in find::matches(&text, query) {
            let (line, shown) = find::line_at(&text, range.start);
            if let Some(last) = lines.last_mut().filter(|hit| hit.line == line) {
                last.count += 1;
                continue;
            }
            if *budget == 0 {
                full = true;
                break;
            }
            *budget -= 1;
            lines.push(LineHit {
                line,
                range,
                text: shown.to_string(),
                count: 1,
            });
        }
        if !lines.is_empty() {
            let class = dom
                .get(reference)
                .map(|instance| instance.class().to_string())
                .unwrap_or_default();
            out.push(ScriptHits {
                script: reference,
                path: full_name(dom, reference),
                class,
                lines,
            });
        }
        if full {
            break;
        }
    }
    out
}

/// Every instance, depth-first in child order from the roots.
fn walk(dom: &WeakDom) -> Vec<Ref> {
    let mut order = Vec::new();
    let mut stack: Vec<Ref> = dom.root_refs().iter().rev().copied().collect();
    while let Some(reference) = stack.pop() {
        order.push(reference);
        if let Some(instance) = dom.get(reference) {
            stack.extend(instance.children().iter().rev().copied());
        }
    }
    order
}

/// `reference`'s ancestors' names and its own joined by `.`, leaving out the
/// `DataModel` root (a root instance whose class is `DataModel`), e.g.
/// `Workspace.Model.Script`. Empty when it does not resolve.
pub(crate) fn full_name(dom: &WeakDom, reference: Ref) -> String {
    let mut names = Vec::new();
    let mut at = Some(reference);
    while let Some(current) = at {
        let Some(instance) = dom.get(current) else {
            return String::new();
        };
        let parent = dom.parent(current);
        if !(parent.is_none() && instance.class() == "DataModel") {
            names.push(instance.name().to_owned());
        }
        at = parent;
    }
    names.reverse();
    names.join(".")
}

/// The lines of `source` a Replace of `query` by `replacement` changes, in
/// order: each line holding at least one match, as it reads before and after
/// every match on it is replaced.
pub(crate) fn preview(source: &str, query: &str, replacement: &str) -> Vec<Preview> {
    source
        .split('\n')
        .enumerate()
        .filter_map(|(index, line)| {
            find::replace_all(line, query, replacement).map(|after| Preview {
                line: index + 1,
                before: line.to_string(),
                after,
            })
        })
        .collect()
}

/// Replaces every match of `query` by `replacement` in every script of
/// `dom` (through `source::write`, so the DOM's change log records each
/// write), returning the scripts whose `Source` changed, in walk order.
/// An empty query changes nothing.
pub(crate) fn replace(
    dom: &mut WeakDom,
    db: &ReflectionDatabase,
    query: &str,
    replacement: &str,
) -> Vec<Ref> {
    let mut changed = Vec::new();
    if query.is_empty() {
        return changed;
    }
    for reference in walk(dom) {
        if !source::is_script(dom, db, reference) {
            continue;
        }
        let Some(text) = source::read(dom, reference) else {
            continue;
        };
        let Some(next) = find::replace_all(&text, query, replacement) else {
            continue;
        };
        if source::write(dom, reference, &next) {
            changed.push(reference);
        }
    }
    changed
}
