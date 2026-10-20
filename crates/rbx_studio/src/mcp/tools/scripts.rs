//! `script_read`, `script_search` and `script_grep`.

use rbx_dom::{Ref, WeakDom};
use rbx_reflection::ReflectionDatabase;
use serde_json::{json, Value};

use super::{count_argument, path_of, required_text, resolve_argument, Outcome};
use crate::mcp::Session;
use crate::script_editor::source;

/// Studio's own limits for the same two tools.
const MAX_SEARCH_RESULTS: usize = 10;
const MAX_GREP_MATCHES: usize = 50;

pub(super) fn read(arguments: &Value, session: &Session) -> Outcome {
    let dom = session.dom;
    let reference = resolve_argument(dom, arguments)?;
    let class = dom.get(reference).map(|instance| instance.class());
    if !class.is_some_and(|class| source::is_script_class(session.properties.db(), class)) {
        return Err(format!("{} is not a script", path_of(dom, reference)));
    }
    // A script inserted and never written to has no Source yet, which reads
    // as the empty script it is.
    let text = source::read(dom, reference).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let first = count_argument(arguments, "start_line").unwrap_or(1).max(1);
    let last = count_argument(arguments, "end_line")
        .unwrap_or(lines.len())
        .min(lines.len());
    if lines.is_empty() {
        return Ok(Value::String(String::new()));
    }
    if first > last {
        return Err(format!(
            "no lines {first} to {last}: the script has {} lines",
            lines.len()
        ));
    }
    let numbered: Vec<String> = (first..=last)
        .map(|number| format!("{number}\t{}", lines[number - 1]))
        .collect();
    Ok(Value::String(numbered.join("\n")))
}

pub(super) fn search(arguments: &Value, session: &Session) -> Outcome {
    let query = required_text(arguments, "query")?.to_lowercase();
    let dom = session.dom;
    let mut ranked: Vec<(u32, Ref)> = scripts(dom, session.properties.db())
        .into_iter()
        .filter_map(|reference| {
            let name = dom.get(reference)?.name().to_lowercase();
            Some((rank(&name, &query)?, reference))
        })
        .collect();
    // Stable, so scripts that rank alike stay in Explorer order.
    ranked.sort_by_key(|&(rank, _)| rank);
    Ok(Value::Array(
        ranked
            .into_iter()
            .take(MAX_SEARCH_RESULTS)
            .filter_map(|(_, reference)| {
                let instance = dom.get(reference)?;
                Some(json!({ "path": path_of(dom, reference), "class": instance.class() }))
            })
            .collect(),
    ))
}

/// Lower is better: the whole name, then its start, then anywhere in it,
/// then its letters in order with others between. `None` is no match.
fn rank(name: &str, query: &str) -> Option<u32> {
    if name == query {
        Some(0)
    } else if name.starts_with(query) {
        Some(1)
    } else if name.contains(query) {
        Some(2)
    } else {
        let mut letters = name.chars();
        query
            .chars()
            .all(|wanted| letters.any(|letter| letter == wanted))
            .then_some(3)
    }
}

pub(super) fn grep(arguments: &Value, session: &Session) -> Outcome {
    let pattern = required_text(arguments, "pattern")?;
    if pattern.is_empty() {
        return Err("the pattern is empty".to_owned());
    }
    let case_sensitive = arguments
        .get("case_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let wanted = if case_sensitive {
        pattern.to_owned()
    } else {
        pattern.to_lowercase()
    };
    let dom = session.dom;
    let mut matches = Vec::new();
    'scripts: for reference in scripts(dom, session.properties.db()) {
        let Some(text) = source::read(dom, reference) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            let found = if case_sensitive {
                line.contains(&wanted)
            } else {
                line.to_lowercase().contains(&wanted)
            };
            if !found {
                continue;
            }
            if matches.len() == MAX_GREP_MATCHES {
                matches.push(json!({
                    "truncated": format!("stopped at {MAX_GREP_MATCHES} matches"),
                }));
                break 'scripts;
            }
            matches.push(json!({
                "path": path_of(dom, reference),
                "line": index + 1,
                "text": line,
            }));
        }
    }
    Ok(Value::Array(matches))
}

/// Every script in the place, depth-first in file order.
fn scripts(dom: &WeakDom, db: &ReflectionDatabase) -> Vec<Ref> {
    let mut found = Vec::new();
    let mut pending: Vec<Ref> = dom.root_refs().iter().rev().copied().collect();
    while let Some(reference) = pending.pop() {
        let Some(instance) = dom.get(reference) else {
            continue;
        };
        if source::is_script_class(db, instance.class()) {
            found.push(reference);
        }
        pending.extend(instance.children().iter().rev());
    }
    found
}
