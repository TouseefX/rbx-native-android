//! `search_game_tree` and `inspect_instance`, answering in the shape real
//! Studio answers them: the same fields (`className`, `fullPath`,
//! `childSummary`, `immediateChildren` ...), compact JSON, and the same
//! sentences for nothing found and for a cut-off result.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use rbx_dom::{Instance, Ref, Variant, WeakDom};
use rbx_reflection::ReflectionDatabase;
use serde_json::{json, Map, Value};

use super::{
    count_argument, datamodel_dom, path_of, required_text, resolve, resolve_all, text_argument,
    values, Outcome, ROOT,
};
use crate::mcp::Session;

/// Studio's defaults and caps for the same arguments.
const DEFAULT_DEPTH: usize = 3;
const MAX_DEPTH: usize = 10;
const DEFAULT_HEAD_LIMIT: usize = 200;
/// `inspect_instance` returns this many matches at most and counts the rest.
const MAX_MATCHES: usize = 20;
const UNIQUE_ID: &str = "UniqueId";

/// The DataModel's class: the place itself, which this DOM does not hold
/// as an instance (its children, the services, are the DOM's roots).
const DATA_MODEL: &str = "DataModel";

/// What a search asks for, applied to every instance it walks.
struct Filter<'a> {
    db: &'a ReflectionDatabase,
    class: Option<&'a str>,
    keywords: Vec<String>,
}

impl Filter<'_> {
    fn matches(&self, class: &str, name: &str) -> bool {
        let name = name.to_lowercase();
        self.class
            .is_none_or(|wanted| self.db.is_subclass_of(class, wanted))
            && (self.keywords.is_empty() || self.keywords.iter().any(|k| name.contains(k)))
    }
}

pub(super) fn search(arguments: &Value, session: &Session) -> Outcome {
    let dom = datamodel_dom(arguments, session)?;
    let place = session.studio.name.as_str();
    // `None` is the DataModel: a search from the top starts at the place.
    let start: Option<Ref> = match text_argument(arguments, "path").map(str::trim) {
        None | Some("") | Some(ROOT) => None,
        Some(path) => Some(
            resolve(dom, path)
                .ok_or_else(|| format!("Could not find instance at path '{path}'"))?,
        ),
    };
    let class = text_argument(arguments, "instance_type").filter(|class| !class.is_empty());
    let filter = Filter {
        db: session.properties.db(),
        class,
        keywords: keywords(text_argument(arguments, "keywords").unwrap_or("")),
    };
    let max_depth = count_argument(arguments, "max_depth")
        .unwrap_or(DEFAULT_DEPTH)
        .min(MAX_DEPTH);
    let head_limit = count_argument(arguments, "head_limit").unwrap_or(DEFAULT_HEAD_LIMIT);

    let mut found = Vec::new();
    let mut truncated = false;
    // Depth-first in file order, so the array reads like the Explorer.
    let mut pending: Vec<(Option<Ref>, usize)> = vec![(start, 0)];
    while let Some((node, depth)) = pending.pop() {
        let (name, class) = match node {
            None => (place, DATA_MODEL),
            Some(reference) => match dom.get(reference) {
                Some(instance) => (instance.name(), instance.class()),
                None => continue,
            },
        };
        let at_limit = depth >= max_depth;
        let below = children(dom, node);
        if filter.matches(class, name) {
            if found.len() == head_limit {
                truncated = true;
                break;
            }
            let mut row = json!({
                "name": name,
                "className": class,
                "fullPath": node.map_or_else(|| place.to_owned(), |r| path_of(dom, r)),
            });
            if let Some(reference) = node {
                let parent = dom.parent(reference).and_then(|parent| dom.get(parent));
                row["parentName"] = json!(parent.map_or(place, Instance::name));
            }
            if at_limit && !below.is_empty() {
                row["childSummary"] = json!(child_summary(dom, below));
                let unexplored = below
                    .iter()
                    .filter_map(|&child| dom.get(child))
                    .filter(|child| filter.matches(child.class(), child.name()))
                    .count();
                if unexplored > 0 {
                    row["unexploredChildCount"] = json!(unexplored);
                }
            }
            found.push(row);
        }
        if !at_limit {
            pending.extend(below.iter().rev().map(|&c| (Some(c), depth + 1)));
        }
    }
    if found.is_empty() {
        return Ok(Value::String(match class {
            Some(class) => format!("No instances found matching type '{class}'"),
            None => "No instances found.".to_owned(),
        }));
    }
    let array = Value::Array(found).to_string();
    Ok(Value::String(if truncated {
        format!("Note: Output limited to {head_limit} nodes (results truncated)\n\n{array}")
    } else {
        array
    }))
}

/// A node's children; the DataModel's are the services.
fn children(dom: &WeakDom, node: Option<Ref>) -> &[Ref] {
    match node {
        None => dom.root_refs(),
        Some(reference) => dom.get(reference).map_or(&[], Instance::children),
    }
}

/// The keywords a client typed, split on commas and spaces as Studio's
/// `search_game_tree` splits them, and lowered to match without case.
fn keywords(text: &str) -> Vec<String> {
    text.split([',', ' '])
        .map(str::trim)
        .filter(|keyword| !keyword.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// `4 children (1 Camera, 1 Part, 1 SpawnLocation, 1 Terrain)`, as Studio
/// words it (`children` even for one), classes in name order.
fn child_summary(dom: &WeakDom, children: &[Ref]) -> String {
    let mut classes: BTreeMap<&str, usize> = BTreeMap::new();
    for child in children.iter().filter_map(|&c| dom.get(c)) {
        *classes.entry(child.class()).or_default() += 1;
    }
    let parts: Vec<String> = classes
        .iter()
        .map(|(class, n)| format!("{n} {class}"))
        .collect();
    format!("{} children ({})", children.len(), parts.join(", "))
}

pub(super) fn inspect(arguments: &Value, session: &Session) -> Outcome {
    let path = required_text(arguments, "path")?;
    let dom = datamodel_dom(arguments, session)?;
    let matches = resolve_all(dom, path);
    let answer = match matches.as_slice() {
        [] => return Err(format!("Could not find any instances at path '{path}'")),
        [only] => report(dom, *only, session),
        all => {
            let shown = all.len().min(MAX_MATCHES);
            let mut answer = json!({
                "matches": all.iter().take(MAX_MATCHES)
                    .map(|&reference| report(dom, reference, session))
                    .collect::<Vec<_>>(),
                "count": shown,
                "totalFound": all.len(),
                "note": format!(
                    "Multiple instances found with path '{path}'. All {shown} matches are \
                     included below."
                ),
            });
            if all.len() > MAX_MATCHES {
                answer["warning"] = json!(format!(
                    "Found {} total matches, but only showing first {MAX_MATCHES} due to output \
                     limits. Consider using a more specific path to narrow down results.",
                    all.len()
                ));
            }
            answer
        }
    };
    Ok(Value::String(answer.to_string()))
}

/// One instance, as `inspect_instance` reports it.
fn report(dom: &WeakDom, reference: Ref, session: &Session) -> Value {
    let Some(instance) = dom.get(reference) else {
        return Value::Null;
    };
    let class = instance.class();
    let panel = session.properties;
    // The properties a script can read, by the names it reads them by, as
    // the Properties panel lists them, not the fields the file stores.
    let settable = panel.settable(dom, reference);
    let mut properties = Map::new();
    for (name, value) in &settable {
        values::insert(&mut properties, panel, dom, class, name, value);
    }
    let mut attributes = Map::new();
    for (name, value) in instance.attributes() {
        values::insert(&mut attributes, panel, dom, class, &name, &value);
    }
    let immediate: Vec<Value> = instance
        .children()
        .iter()
        .filter_map(|&child| {
            let class = dom.get(child)?.class();
            Some(json!({ "className": class, "path": path_of(dom, child) }))
        })
        .collect();

    let mut report = json!({
        "name": instance.name(),
        "className": class,
        "path": path_of(dom, reference),
        "properties": properties,
        "children": {
            "immediateChildren": immediate,
            "childrenCount": instance.children().len(),
            "totalDescendants": descendants(dom, reference),
        },
    });
    // Studio has no entry for either when there is nothing in it.
    if !attributes.is_empty() {
        report["attributes"] = Value::Object(attributes);
    }
    if !instance.tags().is_empty() {
        report["tags"] = json!(instance.tags());
    }
    // A place saved by Studio carries one; an instance made here does not.
    // It cannot be set, so it is not among `settable`, but Studio lists it
    // with the properties as well as on its own.
    if let Some(Variant::UniqueId(id)) = instance.properties().get(UNIQUE_ID) {
        let id = json!(values::unique_id(id));
        report["properties"][UNIQUE_ID] = id.clone();
        report["uniqueId"] = id;
    }
    report
}

fn descendants(dom: &WeakDom, reference: Ref) -> usize {
    let mut count = 0;
    let mut pending = vec![reference];
    while let Some(current) = pending.pop() {
        if let Some(instance) = dom.get(current) {
            count += instance.children().len();
            pending.extend_from_slice(instance.children());
        }
    }
    count
}
