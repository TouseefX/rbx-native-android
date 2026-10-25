//! The `datamodel_type` argument: which tree a tool reads, the edited place
//! or, during a playtest, the game's Client or Server tree.

use rbx_dom::WeakDom;
use serde_json::{json, Value};

use super::text_argument;
use crate::mcp::{DataModel, PlayState, Session, Studio};

pub(super) fn datamodel_type() -> Value {
    json!({
        "type": "string",
        "enum": [EDIT_DATAMODEL],
        "description": "The data model to act on. Only Edit can be edited.",
    })
}

pub(super) fn any_datamodel_type() -> Value {
    json!({
        "type": "string",
        "enum": ["Edit", "Client", "Server"],
        "description": "The data model to read. Defaults to Edit, the edited place; Client and \
            Server exist only during a playtest.",
    })
}

/// A `datamodel_type` argument, read; `None` when none is given.
pub(super) fn datamodel_argument(arguments: &Value) -> Result<Option<DataModel>, String> {
    match text_argument(arguments, "datamodel_type") {
        None => Ok(None),
        Some(text) => DataModel::parse(text)
            .map(Some)
            .ok_or_else(|| format!("Invalid datamodel_type: {text}")),
    }
}

/// Studio's words for a data model the current mode does not have.
pub(super) fn unavailable(model: DataModel, studio: &Studio) -> String {
    format!(
        "{} datamodel is not available in {} mode",
        model.label(),
        studio.play.label()
    )
}

/// Refuses Client and Server while no playtest is live, and Edit while the
/// editor shows the playtest's own tree (execute_luau could not write the
/// edited place then).
pub(super) fn require_live(model: DataModel, studio: &Studio) -> Result<(), String> {
    match (model, studio.play) {
        (DataModel::Edit, PlayState::ShowingLive) => Err(unavailable(model, studio)),
        (DataModel::Edit, _) | (_, PlayState::Playing | PlayState::ShowingLive) => Ok(()),
        _ => Err(unavailable(model, studio)),
    }
}

/// The tree `datamodel_type` names; none given is the edited place.
pub(super) fn datamodel_dom<'a>(
    arguments: &Value,
    session: &Session<'a>,
) -> Result<&'a WeakDom, String> {
    let model = datamodel_argument(arguments)?.unwrap_or(DataModel::Edit);
    let live = match model {
        DataModel::Edit => return Ok(session.dom),
        DataModel::Client => session.client,
        DataModel::Server => session.server,
    };
    live.ok_or_else(|| unavailable(model, session.studio))
}

/// The one data model tools reach: see `studio::state`.
pub(super) const EDIT_DATAMODEL: &str = "Edit";

/// Refuses a `datamodel_type` other than [`EDIT_DATAMODEL`]; none given is
/// the edited place.
pub(super) fn edit_datamodel(arguments: &Value) -> Result<(), String> {
    match text_argument(arguments, "datamodel_type") {
        Some(other) if other != EDIT_DATAMODEL => Err(format!(
            "only the {EDIT_DATAMODEL} data model can be edited, not {other}: a playtest's \
             trees are rebuilt from the edited place each run"
        )),
        _ => Ok(()),
    }
}
