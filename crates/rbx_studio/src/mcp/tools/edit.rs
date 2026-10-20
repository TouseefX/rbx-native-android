//! `multi_edit`: several replacements in one script's `Source`, checked
//! against the text in full before anything is written, so a call either
//! lands whole or leaves the script as it was.
//!
//! The tool only plans the write ([`ScriptEdit`]); the shell applies it with
//! [`apply`] between an undo snapshot and its change log, as it does a
//! script tab's own edits.

#[cfg(test)]
mod tests;

use rbx_dom::{Ref, WeakDom};
use serde_json::{json, Value};

use super::{path_of, required_text, resolve, text_argument, Outcome, ROOT};
use crate::mcp::{PlayState, Session};
use crate::script_editor::source;

/// What a new script can be created as, `Script` first as the default.
pub(super) const SCRIPT_CLASSES: [&str; 3] = ["Script", "LocalScript", "ModuleScript"];

/// The one data model `multi_edit` reaches: see `studio::state`.
const EDIT_DATAMODEL: &str = "Edit";

/// A `Source` write a `multi_edit` call asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScriptEdit {
    pub(crate) target: Target,
    pub(crate) source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    Existing(Ref),
    New {
        parent: Ref,
        name: String,
        class: &'static str,
    },
}

impl ScriptEdit {
    /// Whether applying it adds an instance, which the Explorer has to show.
    pub(crate) fn creates(&self) -> bool {
        matches!(self.target, Target::New { .. })
    }
}

/// Writes `edit` into `dom`, creating its script first when it is new. The
/// script written to, or `None` when what it names is gone or `Source`
/// already held the text.
pub(crate) fn apply(dom: &mut WeakDom, edit: &ScriptEdit) -> Option<Ref> {
    match &edit.target {
        Target::Existing(reference) => {
            source::write(dom, *reference, &edit.source).then_some(*reference)
        }
        Target::New {
            parent,
            name,
            class,
        } => {
            dom.get(*parent)?;
            let reference = dom.new_instance(class, name, Some(*parent));
            source::write(dom, reference, &edit.source);
            Some(reference)
        }
    }
}

struct Replacement {
    old: String,
    new: String,
    all: bool,
}

pub(super) fn multi_edit(arguments: &Value, session: &mut Session) -> Outcome {
    if let Some(other) =
        text_argument(arguments, "datamodel_type").filter(|kind| *kind != EDIT_DATAMODEL)
    {
        return Err(format!(
            "only the {EDIT_DATAMODEL} data model can be edited, not {other}"
        ));
    }
    if session.studio.play == PlayState::ShowingLive {
        return Err(
            "the editor is showing a running playtest; switch back to the edited place first"
                .to_owned(),
        );
    }
    let path = required_text(arguments, "path")?;
    let replacements = replacements(arguments)?;
    let dom = session.dom;

    let (target, before, shown) = match resolve(dom, path) {
        Some(reference) => {
            let class = dom.get(reference).map(|instance| instance.class());
            if !class.is_some_and(|class| source::is_script_class(session.properties.db(), class)) {
                return Err(format!("{} is not a script", path_of(dom, reference)));
            }
            let before = source::read(dom, reference).unwrap_or_default();
            (Target::Existing(reference), before, path_of(dom, reference))
        }
        None => {
            let (parent, name) = new_place(dom, path)?;
            let class = new_class(arguments)?;
            let shown = format!("{}.{name}", path_of(dom, parent));
            let target = Target::New {
                parent,
                name,
                class,
            };
            (target, String::new(), shown)
        }
    };

    let after = replace(&before, &replacements)?;
    let created = matches!(target, Target::New { .. });
    let changed = created || after != before;
    let lines = after.lines().count();
    if changed {
        session.planned = Some(ScriptEdit {
            target,
            source: after,
        });
    }
    Ok(json!({ "path": shown, "created": created, "changed": changed, "lines": lines }))
}

fn replacements(arguments: &Value) -> Result<Vec<Replacement>, String> {
    let Some(edits) = arguments
        .get("edits")
        .and_then(Value::as_array)
        .filter(|edits| !edits.is_empty())
    else {
        return Err("missing the `edits` argument: a list of {old_string, new_string}".to_owned());
    };
    edits
        .iter()
        .enumerate()
        .map(|(index, edit)| {
            let field = |name: &str| {
                text_argument(edit, name)
                    .map(str::to_owned)
                    .ok_or_else(|| format!("edit {}: missing `{name}`", index + 1))
            };
            Ok(Replacement {
                old: field("old_string")?,
                new: field("new_string")?,
                all: edit
                    .get("replace_all")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
}

/// Applies every replacement in turn to `text`, each against what the ones
/// before it left, as an editor applying them one by one would.
fn replace(text: &str, replacements: &[Replacement]) -> Result<String, String> {
    let mut text = text.to_owned();
    for (index, replacement) in replacements.iter().enumerate() {
        let number = index + 1;
        if replacement.old.is_empty() {
            // Anywhere at all would match an empty string; only an empty
            // script leaves no doubt about where the text goes.
            if !text.is_empty() {
                return Err(format!(
                    "edit {number}: an empty old_string only fills an empty script"
                ));
            }
            text.clone_from(&replacement.new);
            continue;
        }
        match text.matches(replacement.old.as_str()).count() {
            0 => return Err(format!("edit {number}: old_string was not found")),
            1 => text = text.replacen(&replacement.old, &replacement.new, 1),
            _ if replacement.all => text = text.replace(&replacement.old, &replacement.new),
            count => {
                return Err(format!(
                    "edit {number}: old_string appears {count} times; give more of the \
                     surrounding text or set replace_all"
                ))
            }
        }
    }
    Ok(text)
}

/// Where a script at `path`, which names nothing yet, would go: the last
/// segment is its name, everything before it its parent. Only the last level
/// may be new, so a typo higher up is reported rather than turned into a
/// script whose name holds a dot.
fn new_place(dom: &WeakDom, path: &str) -> Result<(Ref, String), String> {
    let trimmed = path.trim();
    let relative = trimmed.strip_prefix(&format!("{ROOT}.")).unwrap_or(trimmed);
    let parent = relative
        .rsplit_once('.')
        .filter(|(_, name)| !name.is_empty())
        .and_then(|(parent, name)| Some((resolve(dom, parent)?, name.to_owned())));
    parent.ok_or_else(|| {
        format!("nothing in the place is at {path}, nor at the parent a new script would need")
    })
}

fn new_class(arguments: &Value) -> Result<&'static str, String> {
    let Some(asked) = text_argument(arguments, "class_name") else {
        return Ok(SCRIPT_CLASSES[0]);
    };
    SCRIPT_CLASSES
        .iter()
        .copied()
        .find(|class| *class == asked)
        .ok_or_else(|| {
            format!(
                "class_name must be one of {}, not {asked}",
                SCRIPT_CLASSES.join(", ")
            )
        })
}
