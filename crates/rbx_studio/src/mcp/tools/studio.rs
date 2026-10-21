//! `get_studio_state` and `list_roblox_studios`: which editor answered, and
//! what it is doing, in the shapes real Studio answers them with.

use serde_json::{json, Value};

use crate::mcp::{PlayState, Studio};

pub(crate) const STUDIO_STATE: &str = "get_studio_state";
pub(crate) const LIST_STUDIOS: &str = "list_roblox_studios";

/// The key Studio's `list_roblox_studios` answer holds its rows under.
pub(crate) const STUDIOS_KEY: &str = "studios";

/// This editor's row of `list_roblox_studios`: the session's `id` to pass as
/// `studio_id`, and a name to pick it by. Studio names a row by its place
/// file (`TestPlace.rbxl`); a place linked to Roblox adds its place ID,
/// which is what tells two copies of one place apart.
pub(crate) fn describe(studio: &Studio) -> Value {
    let name = match studio.place_id {
        Some(place_id) => format!("{} (place {place_id})", studio.name),
        None => studio.name.clone(),
    };
    json!({ "id": studio.id, "name": name })
}

/// `list_roblox_studios` as one editor answers it: only itself, as compact
/// JSON the way Studio sends it.
pub(super) fn list(studio: &Studio) -> Value {
    Value::String(json!({ STUDIOS_KEY: [describe(studio)] }).to_string())
}

/// Studio's own three lines. A playtest here runs in a real Roblox client,
/// not in the editor, so the edited place is the only data model a tool can
/// reach. While the viewport shows a running playtest's tree, tools still
/// read the edited place but `multi_edit` waits for it to come back.
pub(super) fn state(studio: &Studio) -> Value {
    let focused = match studio.play {
        PlayState::ShowingLive => {
            "a running playtest (Edit is read-only until it is switched back)"
        }
        _ => "Edit",
    };
    Value::String(format!(
        "- Current Studio Mode: {}\n- Available DataModels: Edit\n- Focused DataModel in the \
         viewport: {focused}",
        studio.play.label()
    ))
}
