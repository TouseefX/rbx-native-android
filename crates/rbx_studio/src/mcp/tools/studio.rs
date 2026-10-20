//! `get_studio_state` and `list_roblox_studios`: which editor answered, and
//! what it is doing.

use serde_json::{json, Value};

use crate::mcp::{PlayState, Studio};

pub(crate) const STUDIO_STATE: &str = "get_studio_state";
pub(crate) const LIST_STUDIOS: &str = "list_roblox_studios";

/// One row of `list_roblox_studios`. A place with no place ID is listed by
/// name only, as Studio lists it.
pub(crate) fn describe(studio: &Studio) -> Value {
    let mut row = json!({ "studio_id": studio.id, "name": studio.name });
    if let Some(place_id) = studio.place_id {
        row["place_id"] = json!(place_id);
    }
    row
}

pub(super) fn state(studio: &Studio) -> Value {
    let mut state = describe(studio);
    state["play_state"] = json!(studio.play.label());
    // A playtest here runs in a real Roblox client, not in the editor, so
    // the edited place is the only data model a tool can reach.
    state["datamodel_types"] = json!(["Edit"]);
    state["editable"] = json!(studio.play != PlayState::ShowingLive);
    state
}
