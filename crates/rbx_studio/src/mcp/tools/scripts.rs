//! `script_read`, `script_search` and `script_grep`, with the arguments
//! Studio's own tools take and the text it answers them with.

#[cfg(test)]
mod tests;

use rbx_dom::{Ref, WeakDom};
use rbx_lua::pattern::Pattern;
use rbx_reflection::ReflectionDatabase;
use serde_json::Value;

use super::{count_argument, flag_argument, path_of, required_text, resolve, Outcome};
use crate::mcp::Session;
use crate::script_editor::source;

/// Studio's own limits for the same two tools.
const MAX_SEARCH_RESULTS: usize = 10;
const MAX_GREP_MATCHES: usize = 50;

/// Between a line's number and its text, as Studio's `script_read` prints it.
const LINE_MARK: char = '\u{2192}';
/// The width Studio right-aligns a line's number to.
const NUMBER_WIDTH: usize = 6;
/// What Studio's `script_search` and `script_grep` say when nothing matches.
const NO_MATCHES: &str = "No matches found for the given query.";

pub(super) fn read(arguments: &Value, session: &Session) -> Outcome {
    let dom = session.dom;
    let path = required_text(arguments, "target_file")?;
    let reference =
        resolve(dom, path).ok_or_else(|| format!("Script not found at path: {path}"))?;
    let class = dom.get(reference).map(|instance| instance.class());
    if !class.is_some_and(|class| source::is_script_class(session.properties.db(), class)) {
        return Err(format!("{} is not a script", path_of(dom, reference)));
    }
    // A script inserted and never written to has no Source yet, which reads
    // as the empty script it is.
    let text = source::read(dom, reference).unwrap_or_default();
    // Split on every newline, so a Source ending in one reads with the empty
    // last line Studio shows for it.
    let lines: Vec<&str> = if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').collect()
    };
    let first = count_argument(arguments, "start_line_one_indexed");
    let last = count_argument(arguments, "end_line_one_indexed_inclusive");
    if flag_argument(arguments, "should_read_entire_file") == Some(false)
        && (first.is_none() || last.is_none())
    {
        return Err("reading part of a script needs start_line_one_indexed and \
             end_line_one_indexed_inclusive"
            .to_owned());
    }
    if lines.is_empty() {
        return Ok(Value::String(String::new()));
    }
    let first = first.unwrap_or(1).max(1);
    let last = last.unwrap_or(lines.len()).min(lines.len());
    if first > last {
        return Err(format!(
            "no lines {first} to {last}: the script has {} lines",
            lines.len()
        ));
    }
    let numbered: Vec<String> = (first..=last)
        .map(|number| {
            let line = lines[number - 1].trim_end_matches('\r');
            format!("{number:>NUMBER_WIDTH$}{LINE_MARK}{line}")
        })
        .collect();
    Ok(Value::String(numbered.join("\n")))
}

pub(super) fn search(arguments: &Value, session: &Session) -> Outcome {
    let keywords: Vec<String> = required_text(arguments, "keywords")?
        .split(',')
        .map(|keyword| keyword.trim().to_lowercase())
        .filter(|keyword| !keyword.is_empty())
        .collect();
    if keywords.is_empty() {
        return Err("no keywords to search for".to_owned());
    }
    let dom = session.dom;
    let mut ranked: Vec<(u32, Ref)> = scripts(dom, session.properties.db())
        .into_iter()
        .filter_map(|reference| {
            let name = dom.get(reference)?.name().to_lowercase();
            let best = keywords.iter().filter_map(|k| rank(&name, k)).min()?;
            Some((best, reference))
        })
        .collect();
    // Stable, so scripts that rank alike stay in Explorer order.
    ranked.sort_by_key(|&(rank, _)| rank);
    let rows: Vec<String> = ranked
        .into_iter()
        .take(MAX_SEARCH_RESULTS)
        .filter_map(|(_, reference)| {
            let instance = dom.get(reference)?;
            Some(format!(
                "Name: {} | Type: {} | Path: {}",
                instance.name(),
                instance.class(),
                path_of(dom, reference)
            ))
        })
        .collect();
    Ok(Value::String(lines_or_none(rows)))
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

/// Matching is left to [`Session::deferred`]: a Luau pattern backtracks, and
/// Luau's matcher cannot be interrupted, so one call can take far longer
/// than the UI thread may be held. Its own answer is a placeholder.
pub(super) fn grep(arguments: &Value, session: &mut Session) -> Outcome {
    let query = required_text(arguments, "query")?;
    if query.is_empty() {
        return Err("the query is empty".to_owned());
    }
    let wildcards = repeated_wildcards(query);
    if wildcards > MAX_REPEATED_WILDCARDS {
        return Err(format!(
            "the pattern repeats `.` {wildcards} times (with *, + or -); more than \
             {MAX_REPEATED_WILDCARDS} can take minutes to match a single long line. Narrow it \
             with a character class such as %w or [^\"], or grep for fewer parts at once"
        ));
    }
    let dom = session.dom;
    let sources: Vec<(String, String)> = scripts(dom, session.properties.db())
        .into_iter()
        .filter_map(|reference| Some((path_of(dom, reference), source::read(dom, reference)?)))
        .collect();
    let query = query.to_owned();
    session.deferred = Some(Box::new(move || grep_sources(&query, &sources)));
    Ok(Value::Null)
}

/// More than this many `.*`, `.+` or `.-` in one pattern is refused. Each one
/// multiplies the ways a line can be tried by about its length: on a 120
/// character line with no match, three took 0.07 s and four 1.9 s, and five
/// would take about a minute. Matching runs off the UI thread, but a call
/// that never ends still holds a core until the editor closes.
const MAX_REPEATED_WILDCARDS: usize = 3;

/// How many times `pattern` repeats `.`, the item that matches anything and
/// so backtracks over everything. A `.` escaped as `%.` or inside a `[set]`
/// is the dot itself.
fn repeated_wildcards(pattern: &str) -> usize {
    let bytes = pattern.as_bytes();
    let mut count = 0;
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'%' => at += 2,
            b'[' => {
                at += 1;
                // A `^` and then a `]` right after the bracket are members.
                if bytes.get(at) == Some(&b'^') {
                    at += 1;
                }
                if bytes.get(at) == Some(&b']') {
                    at += 1;
                }
                while at < bytes.len() && bytes[at] != b']' {
                    at += if bytes[at] == b'%' { 2 } else { 1 };
                }
                at += 1;
            }
            b'.' => {
                if matches!(bytes.get(at + 1), Some(b'*' | b'+' | b'-')) {
                    count += 1;
                }
                at += 1;
            }
            _ => at += 1,
        }
    }
    count
}

/// `script_grep`'s answer over `(path, Source)` pairs, in Explorer order.
fn grep_sources(query: &str, sources: &[(String, String)]) -> Outcome {
    let pattern = Pattern::new(query)?;
    let mut matches = Vec::new();
    'scripts: for (path, text) in sources {
        for (index, line) in text.lines().enumerate() {
            if !pattern.is_in(line)? {
                continue;
            }
            if matches.len() == MAX_GREP_MATCHES {
                matches.push(format!("Note: stopped at {MAX_GREP_MATCHES} matches"));
                break 'scripts;
            }
            matches.push(format!("Path: {path} | Line: {} | {line}", index + 1));
        }
    }
    Ok(Value::String(lines_or_none(matches)))
}

fn lines_or_none(rows: Vec<String>) -> String {
    if rows.is_empty() {
        NO_MATCHES.to_owned()
    } else {
        rows.join("\n")
    }
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
