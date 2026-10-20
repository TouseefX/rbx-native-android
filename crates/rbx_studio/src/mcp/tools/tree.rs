//! `search_game_tree` and `inspect_instance`.

use rbx_dom::{Ref, WeakDom};
use serde_json::{json, Map, Value};

use super::{count_argument, path_of, resolve, resolve_argument, text_argument, Outcome, ROOT};
use crate::mcp::Session;

const DEFAULT_DEPTH: usize = 3;
/// Enough to see a large place's shape; past it the client is asked to
/// narrow the search rather than handed a reply too long to read.
const MAX_RESULTS: usize = 500;
/// `inspect_instance` names this many children at most and counts the rest.
const MAX_CHILDREN: usize = 100;

pub(super) fn search(arguments: &Value, session: &Session) -> Outcome {
    let dom = session.dom;
    let db = session.properties.db();
    let start = match text_argument(arguments, "path").map(str::trim) {
        None | Some("") | Some(ROOT) => dom.root_refs().to_vec(),
        Some(path) => {
            vec![resolve(dom, path).ok_or_else(|| format!("nothing in the place is at {path}"))?]
        }
    };
    let class = text_argument(arguments, "instance_type").filter(|class| !class.is_empty());
    let keywords = text_argument(arguments, "keywords")
        .map(str::to_lowercase)
        .filter(|keywords| !keywords.is_empty());
    let max_depth = count_argument(arguments, "max_depth").unwrap_or(DEFAULT_DEPTH);

    let mut found = Vec::new();
    let mut truncated = false;
    // Depth-first in file order, so the array reads like the Explorer.
    let mut pending: Vec<(Ref, usize)> = start.iter().rev().map(|&r| (r, 0)).collect();
    while let Some((reference, depth)) = pending.pop() {
        let Some(instance) = dom.get(reference) else {
            continue;
        };
        let class_matches = class.is_none_or(|class| db.is_subclass_of(instance.class(), class));
        let name_matches = keywords
            .as_deref()
            .is_none_or(|keywords| instance.name().to_lowercase().contains(keywords));
        if class_matches && name_matches {
            if found.len() == MAX_RESULTS {
                truncated = true;
                break;
            }
            found.push(json!({
                "path": path_of(dom, reference),
                "class": instance.class(),
                "children": instance.children().len(),
            }));
        }
        if depth < max_depth {
            pending.extend(instance.children().iter().rev().map(|&c| (c, depth + 1)));
        }
    }
    if truncated {
        found.push(json!({
            "truncated": format!(
                "stopped at {MAX_RESULTS} instances; narrow the path, class or keywords"
            ),
        }));
    }
    Ok(Value::Array(found))
}

pub(super) fn inspect(arguments: &Value, session: &Session) -> Outcome {
    let dom = session.dom;
    let reference = resolve_argument(dom, arguments)?;
    let instance = dom
        .get(reference)
        .ok_or_else(|| "that instance is gone".to_owned())?;
    let class = instance.class();
    let format = |name: &str, value| session.properties.format(dom, class, name, value);

    let properties: Map<String, Value> = instance
        .properties()
        .iter()
        .map(|(name, value)| (name.clone(), Value::String(format(name, value))))
        .collect();
    let attributes: Map<String, Value> = instance
        .attributes()
        .iter()
        .map(|(name, value)| (name.clone(), Value::String(format(name, value))))
        .collect();
    let children: Vec<Value> = instance
        .children()
        .iter()
        .take(MAX_CHILDREN)
        .filter_map(|&child| dom.get(child))
        .map(|child| json!({ "name": child.name(), "class": child.class() }))
        .collect();

    let mut report = json!({
        "path": path_of(dom, reference),
        "name": instance.name(),
        "class": class,
        "properties": properties,
        "attributes": attributes,
        "tags": instance.tags(),
        "children": children,
        "child_count": instance.children().len(),
        "descendant_count": descendants(dom, reference),
    });
    if crate::script_editor::source::is_script_class(session.properties.db(), class) {
        report["source"] = json!("read it with script_read");
    }
    Ok(report)
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
