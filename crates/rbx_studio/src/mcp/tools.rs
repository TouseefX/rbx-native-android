//! The tools a client can call, named and shaped after Studio's own MCP
//! server's, and the dotted paths they name instances by.

mod edit;
mod scripts;
mod studio;
mod tree;

#[cfg(test)]
mod tests;

use rbx_dom::{Ref, WeakDom};
use serde_json::{json, Value};

use super::protocol::{Failure, INVALID_PARAMS};
use super::Session;

pub(crate) use edit::{apply as apply_edit, ScriptEdit};
pub(crate) use studio::{describe, LIST_STUDIOS, STUDIO_STATE};

/// What every path starts with, as `Instance:GetFullName()` would print it
/// from a script: the services are the DataModel's children.
const ROOT: &str = "game";

/// A tool's own failure (a path that names nothing): the client is told in
/// the result rather than as a protocol error, so the model can read it and
/// try again.
type Outcome = Result<Value, String>;

/// `tools/list`'s answer. Every tool but `list_roblox_studios` also takes
/// a `studio_id`, as Studio's do.
pub(super) fn list() -> Value {
    let mut tools = json!([
        tool(
            "search_game_tree",
            "Lists the instance hierarchy as a flat JSON array of {path, class, children}. \
             Filters by a starting path, a class (matching subclasses, as IsA does) and \
             keywords in the name, down to a depth limit.",
            json!({
                "path": string("Where to start, such as game.Workspace. Defaults to game."),
                "instance_type": string("Only instances that are this class or derive from it."),
                "keywords": string("Only instances whose name contains this, ignoring case."),
                "max_depth": integer("How many levels below the start to go. Defaults to 3."),
            }),
            &[],
        ),
        tool(
            "inspect_instance",
            "Returns one instance's class, every property it carries, its attributes and \
             tags, its children and how many descendants it has.",
            json!({ "path": string("The instance, such as game.Workspace.Baseplate.") }),
            &["path"],
        ),
        tool(
            "script_read",
            "Reads a script's Source, each line numbered. A line range reads part of it.",
            json!({
                "path": string("The script, such as game.ServerScriptService.Main."),
                "start_line": integer("The first line to read, counting from 1."),
                "end_line": integer("The last line to read, included."),
            }),
            &["path"],
        ),
        tool(
            "script_search",
            "Finds scripts by name, fuzzily: the letters typed in order. Up to 10 results, \
             best first.",
            json!({ "query": string("Part of the script's name.") }),
            &["query"],
        ),
        tool(
            "script_grep",
            "Finds a piece of text in every script's Source. The text is matched literally, \
             not as a Luau pattern. Up to 50 matches.",
            json!({
                "pattern": string("The text to find."),
                "case_sensitive": json!({ "type": "boolean", "description": "Defaults to false." }),
            }),
            &["pattern"],
        ),
        tool(
            "multi_edit",
            "Applies several edits to one script's Source in order, all or none, as one              undo step. Each old_string must appear exactly once unless replace_all is set.              If nothing is at the path yet, a new script is created under its parent and the              edits apply to its empty Source, so the first one's old_string is empty.",
            json!({
                "path": string("The script, such as game.ServerScriptService.Main."),
                "edits": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "old_string": string("The exact text to replace."),
                            "new_string": string("What replaces it."),
                            "replace_all": json!({ "type": "boolean", "description": "Replace every occurrence. Defaults to false." }),
                        },
                        "required": ["old_string", "new_string"],
                    },
                },
                "datamodel_type": json!({ "type": "string", "enum": ["Edit"], "description": "Only the Edit data model can be edited." }),
                "class_name": json!({ "type": "string", "enum": edit::SCRIPT_CLASSES, "description": "The class of a script this creates. Defaults to Script." }),
            }),
            &["path", "edits"],
        ),
        tool(
            STUDIO_STATE,
            "Returns this editor's studio_id, place name and place ID, its play state and              the data model types tools can reach.",
            json!({}),
            &[],
        ),
    ]);
    if let Value::Array(tools) = &mut tools {
        for tool in tools.iter_mut() {
            tool["inputSchema"]["properties"]["studio_id"] = string(
                "The editor to act on, from list_roblox_studios. May be left out when only one                  editor is open.",
            );
        }
        tools.push(tool(
            LIST_STUDIOS,
            "Lists every running editor serving MCP: its studio_id, its place's name and, for              a place linked to Roblox, its place ID.",
            json!({}),
            &[],
        ));
    }
    tools
}

/// `tools/call`'s answer.
pub(super) fn call(params: &Value, session: &mut Session) -> Result<Value, Failure> {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return Err((INVALID_PARAMS, "tools/call needs a tool name".to_owned()));
    };
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
    let outcome = match text_argument(&arguments, "studio_id") {
        // Only a client that connected to this editor by hand gets here with
        // another editor's id: the broker routes by it.
        Some(id) if name != LIST_STUDIOS && id != session.studio.id => Err(format!(
            "this editor is studio {}, not {id}; call {LIST_STUDIOS} for the others",
            session.studio.id
        )),
        _ => run(name, &arguments, session)?,
    };
    Ok(match outcome {
        Ok(value) => result(&value, false),
        Err(message) => result(&Value::String(message), true),
    })
}

fn run(name: &str, arguments: &Value, session: &mut Session) -> Result<Outcome, Failure> {
    Ok(match name {
        "search_game_tree" => tree::search(arguments, session),
        "inspect_instance" => tree::inspect(arguments, session),
        "script_read" => scripts::read(arguments, session),
        "script_search" => scripts::search(arguments, session),
        "script_grep" => scripts::grep(arguments, session),
        "multi_edit" => edit::multi_edit(arguments, session),
        STUDIO_STATE => Ok(studio::state(session.studio)),
        LIST_STUDIOS => Ok(json!([describe(session.studio)])),
        _ => return Err((INVALID_PARAMS, format!("unknown tool: {name}"))),
    })
}

/// A tool's result: its value as text, pretty JSON for anything but a
/// string. `pub(super)`: the broker answers `list_roblox_studios` with it.
pub(super) fn result(value: &Value, is_error: bool) -> Value {
    let text = match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    };
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required },
    })
}

fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn integer(description: &str) -> Value {
    json!({ "type": "integer", "minimum": 0, "description": description })
}

fn text_argument<'a>(arguments: &'a Value, name: &str) -> Option<&'a str> {
    arguments.get(name).and_then(Value::as_str)
}

fn required_text<'a>(arguments: &'a Value, name: &str) -> Result<&'a str, String> {
    text_argument(arguments, name).ok_or_else(|| format!("missing the `{name}` argument"))
}

fn count_argument(arguments: &Value, name: &str) -> Option<usize> {
    arguments
        .get(name)
        .and_then(Value::as_u64)
        .map(|count| usize::try_from(count).unwrap_or(usize::MAX))
}

/// `game.Workspace.Part` for an instance.
fn path_of(dom: &WeakDom, reference: Ref) -> String {
    let mut names = Vec::new();
    let mut at = Some(reference);
    while let Some(current) = at {
        let Some(instance) = dom.get(current) else {
            break;
        };
        names.push(instance.name());
        at = dom.parent(current);
    }
    names.push(ROOT);
    names.reverse();
    names.join(".")
}

/// The instance a dotted path names; `game.` in front is optional, so a
/// path copied from the Explorer works as well. `None` for `game` itself,
/// which is not an instance this DOM holds.
fn resolve(dom: &WeakDom, path: &str) -> Option<Ref> {
    let path = path.trim();
    let path = path.strip_prefix("game.").unwrap_or(path);
    let segments: Vec<&str> = path.split('.').collect();
    descend(dom, dom.root_refs(), &segments)
}

/// A name may itself hold a dot (`Config.v2`), so each level tries every
/// run of segments as one name, shortest first.
fn descend(dom: &WeakDom, candidates: &[Ref], segments: &[&str]) -> Option<Ref> {
    for taken in 1..=segments.len() {
        let name = segments[..taken].join(".");
        for &candidate in candidates {
            let Some(instance) = dom.get(candidate) else {
                continue;
            };
            if instance.name() != name {
                continue;
            }
            if taken == segments.len() {
                return Some(candidate);
            }
            if let Some(found) = descend(dom, instance.children(), &segments[taken..]) {
                return Some(found);
            }
        }
    }
    None
}

fn resolve_argument(dom: &WeakDom, arguments: &Value) -> Result<Ref, String> {
    let path = required_text(arguments, "path")?;
    resolve(dom, path).ok_or_else(|| format!("nothing in the place is at {path}"))
}
