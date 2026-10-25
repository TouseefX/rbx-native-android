//! The tools a client can call, named and shaped after Studio's own MCP
//! server's, and the dotted paths they name instances by.

mod datamodel;
mod assets;
mod docs;
mod edit;
mod input;
mod luau;
mod scripts;
mod skills;
mod studio;
mod tree;
mod values;

#[cfg(test)]
mod tests;

use rbx_dom::{Ref, WeakDom};
use serde_json::{json, Value};

use super::protocol::{Failure, INVALID_PARAMS};
use super::Session;
use datamodel::{any_datamodel_type, datamodel_dom, datamodel_type, edit_datamodel};

pub(crate) use assets::{inserted, Insertion};
pub(crate) use edit::{apply as apply_edit, ScriptEdit};
pub(crate) use studio::{LIST_STUDIOS, STUDIOS_KEY, STUDIO_STATE};

/// What every path starts with, as `Instance:GetFullName()` would print it
/// from a script: the services are the DataModel's children.
const ROOT: &str = "game";

/// A tool's own failure (a path that names nothing): the client is told in
/// the result rather than as a protocol error, so the model can read it and
/// try again.
type Outcome = Result<Value, String>;

/// `tools/list`'s answer. Names, arguments and their meaning follow the
/// schemas Studio's own server lists (`StudioMCP`'s `tools/list`), so a
/// client or prompt written for Studio works unchanged. Studio requires
/// `studio_id` on every call; here it may be left out with one editor open.
pub(super) fn list() -> Value {
    let mut tools = json!([
        tool(
            "search_game_tree",
            "Lists the instance hierarchy as a flat JSON array of {name, className, fullPath, \
             parentName}, depth-first as the Explorer shows it. Filters by a starting path, a \
             class (matching subclasses, as IsA does) and name keywords. Past max_depth, an \
             instance's children are summarized by class instead of listed.",
            json!({
                "path": string("Where to start, such as Workspace or Workspace.Models. Defaults to the whole place."),
                "instance_type": string("Only instances that are this class or derive from it, such as BasePart."),
                "keywords": string("Only instances whose name contains one of these, ignoring case. Separate several with commas or spaces."),
                "max_depth": integer("How many levels below the start to go. Defaults to 3, at most 10."),
                "head_limit": integer("The most instances to return. Defaults to 200."),
                "datamodel_type": any_datamodel_type(),
            }),
            &[],
        ),
        tool(
            "inspect_instance",
            "Returns an instance's class, every property it carries, its attributes and tags, \
             and its children and descendant count. The path ignores case; when siblings share \
             a name, every match is returned, up to 20.",
            json!({
                "path": string("The instance, such as Workspace.Baseplate. game. in front is optional."),
                "datamodel_type": any_datamodel_type(),
            }),
            &["path"],
        ),
        tool(
            "script_read",
            "Reads a script's Source, each line numbered as LINE_NUMBER\u{2192}LINE_CONTENT. \
             Reads the whole script unless should_read_entire_file is false.",
            json!({
                "target_file": string("The script, such as game.ServerScriptService.Main."),
                "should_read_entire_file": boolean("Defaults to true. When false, both line numbers are needed."),
                "start_line_one_indexed": integer("The first line to read, counting from 1."),
                "end_line_one_indexed_inclusive": integer("The last line to read, included."),
            }),
            &["target_file"],
        ),
        tool(
            "script_search",
            "Finds scripts by name, fuzzily: a keyword matches a name that holds its letters \
             in order. Up to 10 results, best first.",
            json!({ "keywords": string("Comma-separated parts of the script's name, each ignoring case.") }),
            &["keywords"],
        ),
        tool(
            "script_grep",
            "Finds a string or Luau pattern in every script's Source, line by line, matched by \
             Luau's own string.find. Up to 50 matches.",
            json!({ "query": string("The string or Luau pattern to find, such as print or ^local%s.") }),
            &["query"],
        ),
        tool(
            "multi_edit",
            "Applies several edits to one script's Source in order, each on what the last left, \
             all or none, as one undo step. Each old_string must appear exactly once unless \
             replace_all is set, and must differ from its new_string. If nothing is at file_path \
             yet, a script of className is created under its parent and the edits apply to its \
             empty Source, so the first one's old_string is empty.",
            json!({
                "file_path": string("The script, such as game.ServerScriptService.Main."),
                "edits": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "old_string": string("The exact text to replace; empty to fill a new script."),
                            "new_string": string("What replaces it."),
                            "replace_all": boolean("Replace every occurrence. Defaults to false."),
                        },
                        "required": ["old_string", "new_string"],
                    },
                },
                "className": json!({
                    "type": "string",
                    "enum": edit::SCRIPT_CLASSES,
                    "description": "The class of the script to create. Needed only when nothing is at file_path.",
                }),
                "datamodel_type": datamodel_type(),
            }),
            &["file_path", "edits"],
        ),
        tool(
            STUDIO_STATE,
            "Returns this editor's studio id, place name and place ID, its play state and the \
             data model types tools can reach.",
            json!({}),
            &[],
        ),
        docs::schema(),
        skills::schema(),
    ]);
    if let Value::Array(tools) = &mut tools {
        tools.extend(luau::schemas());
        tools.push(super::broker::subagent_schema());
        tools.extend(input::schemas());
        tools.extend(assets::list());
        for tool in tools.iter_mut() {
            let id = &mut tool["inputSchema"]["properties"]["studio_id"];
            if id.is_null() {
                *id = string(
                    "The editor to act on, from list_roblox_studios. May be left out when only \
                     one editor is open.",
                );
            }
        }
        tools.push(tool(
            LIST_STUDIOS,
            "Lists every running editor serving MCP, each with the id to pass as studio_id and \
             a name: its place file's name and, for a place linked to Roblox, its place ID.",
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
    Ok(finished(outcome))
}

/// A tool's outcome as `tools/call` answers it. `pub(super)`: a deferred
/// tool's outcome is only known once its work has run.
pub(super) fn finished(outcome: Outcome) -> Value {
    match outcome {
        Ok(value) => result(&value, false),
        Err(message) => result(&Value::String(message), true),
    }
}

fn run(name: &str, arguments: &Value, session: &mut Session) -> Result<Outcome, Failure> {
    Ok(match name {
        "search_game_tree" => tree::search(arguments, session),
        "inspect_instance" => tree::inspect(arguments, session),
        "script_read" => scripts::read(arguments, session),
        "script_search" => scripts::search(arguments, session),
        "script_grep" => scripts::grep(arguments, session),
        "multi_edit" => edit::multi_edit(arguments, session),
        input::NAVIGATION => input::navigation(arguments, session),
        input::KEYBOARD => input::keyboard(arguments, session),
        input::MOUSE => input::mouse(arguments, session),
        STUDIO_STATE => Ok(studio::state(session.studio)),
        skills::NAME => skills::call(arguments),
        docs::NAME => {
            let arguments = arguments.clone();
            session.deferred = Some(Box::new(move || docs::call(&arguments)));
            Ok(Value::Null)
        }
        LIST_STUDIOS => Ok(studio::list(session.studio)),
        luau::EXECUTE_LUAU => luau::execute(arguments, session),
        luau::START_STOP_PLAY => luau::start_stop(arguments, session),
        luau::CONSOLE_OUTPUT => luau::console(session),
        luau::SCREEN_CAPTURE => luau::capture(arguments, session),
        _ => match assets::run(name, arguments, session) {
            Some(outcome) => outcome,
            None => return Err((INVALID_PARAMS, format!("unknown tool: {name}"))),
        },
    })
}

/// The answer to a tool that reads nothing of an editor's (`skill`,
/// `http_get`), which the broker gives itself, so documentation is reachable
/// with no editor open and a slow fetch never waits on a UI thread.
pub(super) fn without_editor(name: &str, arguments: &Value) -> Option<Value> {
    let outcome = match name {
        skills::NAME => skills::call(arguments),
        docs::NAME => docs::call(arguments),
        _ => return None,
    };
    Some(finished(outcome))
}

/// A tool's result: its value as text, pretty JSON for anything but a
/// string. `pub(super)`: the broker answers `list_roblox_studios` with it.
pub(super) fn result(value: &Value, is_error: bool) -> Value {
    let text = match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    };
    json!({ "content": [text_content(&text)], "isError": is_error })
}

/// An MCP text content block.
pub(crate) fn text_content(text: &str) -> Value {
    json!({ "type": "text", "text": text })
}

/// An MCP image content block holding a JPEG, as Studio's screen_capture
/// answers.
pub(crate) fn image_content(jpeg: &[u8]) -> Value {
    use base64::Engine;
    json!({
        "type": "image",
        "data": base64::engine::general_purpose::STANDARD.encode(jpeg),
        "mimeType": "image/jpeg",
    })
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

fn boolean(description: &str) -> Value {
    json!({ "type": "boolean", "description": description })
}

fn text_argument<'a>(arguments: &'a Value, name: &str) -> Option<&'a str> {
    arguments.get(name).and_then(Value::as_str)
}

fn required_text<'a>(arguments: &'a Value, name: &str) -> Result<&'a str, String> {
    text_argument(arguments, name).ok_or_else(|| format!("missing the `{name}` argument"))
}

fn flag_argument(arguments: &Value, name: &str) -> Option<bool> {
    arguments.get(name).and_then(Value::as_bool)
}

fn count_argument(arguments: &Value, name: &str) -> Option<usize> {
    arguments
        .get(name)
        .and_then(Value::as_u64)
        .map(|count| usize::try_from(count).unwrap_or(usize::MAX))
}

/// `Workspace.Part` for an instance, as Studio's tools print paths: from
/// the services down, without `game.` in front.
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
    names.reverse();
    names.join(".")
}

/// `path` without the optional `game.` in front, in either case.
fn relative(path: &str) -> &str {
    let path = path.trim();
    match path.get(..ROOT.len() + 1) {
        Some(prefix) if prefix.eq_ignore_ascii_case("game.") => &path[ROOT.len() + 1..],
        _ => path,
    }
}

/// The instance a dotted path names; `game.` in front is optional, so a
/// path copied from the Explorer works as well. `None` for `game` itself,
/// which is not an instance this DOM holds.
fn resolve(dom: &WeakDom, path: &str) -> Option<Ref> {
    let segments: Vec<&str> = relative(path).split('.').collect();
    descend(dom, dom.root_refs(), &segments)
}

/// Every instance a dotted path names, ignoring case, in Explorer order:
/// siblings that share a name are all of them named by it.
fn resolve_all(dom: &WeakDom, path: &str) -> Vec<Ref> {
    let segments: Vec<&str> = relative(path).split('.').collect();
    let mut found = Vec::new();
    collect(dom, dom.root_refs(), &segments, &mut found);
    found
}

/// [`descend`] for every match rather than the first.
fn collect(dom: &WeakDom, candidates: &[Ref], segments: &[&str], found: &mut Vec<Ref>) {
    for taken in 1..=segments.len() {
        let name = segments[..taken].join(".").to_lowercase();
        for &candidate in candidates {
            let Some(instance) = dom.get(candidate) else {
                continue;
            };
            if instance.name().to_lowercase() != name {
                continue;
            }
            if taken == segments.len() {
                if !found.contains(&candidate) {
                    found.push(candidate);
                }
            } else {
                collect(dom, instance.children(), &segments[taken..], found);
            }
        }
    }
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
