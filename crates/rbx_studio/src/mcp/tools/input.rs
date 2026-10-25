//! `character_navigation`, `user_keyboard_input` and `user_mouse_input`:
//! playing the running playtest as its player would, with Studio's
//! arguments, checks and answers (compared side by side with its
//! `StudioMCP`).
//!
//! Studio's own versions act on its in-process client, so each tool here
//! does what that one does, to the real Roblox client:
//! - mouse buttons, the wheel and keys are real OS input into the embedded
//!   client's window (XTest on Linux, `SendInput` on Windows): Studio's are
//!   engine-level input events that games read through `UserInputService`
//!   and GUI objects exactly like a person's;
//! - walking is the client probe's `Humanoid:MoveTo` along a computed path at
//!   a scaled `WalkSpeed`, which is what Studio's does (no key is held: the
//!   humanoid's `MoveDirection` stays zero);
//! - `textInput` puts text into the focused `TextBox` without key events,
//!   as Studio's does, through the probe;
//! - where a `GuiObject` is on screen comes from the probe.
//!
//! Each call checks its arguments and the play state at once, then runs off
//! the UI thread ([`Session::deferred`]): a walk takes seconds, and every
//! step waits on the client.

mod run;

#[cfg(test)]
mod tests;

use serde_json::{json, Value};

use super::{string, text_argument, tool, Outcome};
use crate::mcp::{PlayState, Session};

pub(crate) const NAVIGATION: &str = "character_navigation";
pub(crate) const KEYBOARD: &str = "user_keyboard_input";
pub(crate) const MOUSE: &str = "user_mouse_input";

/// The one data model these tools act on: the playtest's client.
const CLIENT_DATAMODEL: &str = "Client";

/// Studio's longest `wait_time_ms`.
const MAX_WAIT_MS: f64 = 10_000.0;

/// The three tools' `tools/list` entries.
pub(super) fn schemas() -> Vec<Value> {
    let path = |what: &str| {
        string(&format!(
            "The instance {what}, such as LocalPlayer.PlayerGui.Menu.Play or \
             game.Workspace.Door. Starts with game, LocalPlayer or Workspace."
        ))
    };
    let wait = number(&format!(
        "Milliseconds to wait before the next step; required for wait, 0 to {MAX_WAIT_MS}."
    ));
    vec![
        tool(
            NAVIGATION,
            "Walks the player character to a position or an instance (a BasePart, or a Model's \
             pivot) along a computed path, and answers once it arrives.",
            json!({
                "instance_path": path("to walk to"),
                "x": number("The x coordinate to walk to, when instance_path is not given."),
                "y": number("The y coordinate to walk to, when instance_path is not given."),
                "z": number("The z coordinate to walk to, when instance_path is not given."),
                "speed_multiplier": number(
                    "Times the character's walk speed: 1.0 by default, from 0.1 to 10.0."
                ),
                "datamodel_type": client_datamodel(),
            }),
            &["datamodel_type"],
        ),
        tool(
            KEYBOARD,
            "Sends keyboard actions to the game in order: keyDown, keyUp, keyPress (down then \
             up), textInput (text into the focused TextBox, clicking instance_path first when \
             given) or wait.",
            actions(json!({
                "action": {
                    "type": "string",
                    "enum": ["keyDown", "keyUp", "keyPress", "textInput", "wait"],
                    "description": "What this step does.",
                },
                "key_code": {
                    "type": "string",
                    "enum": rbx_play::keycode::names().collect::<Vec<_>>(),
                    "description": "The Enum.KeyCode name; required for keyDown, keyUp and keyPress.",
                },
                "text_inputs": string("The text to type; required for textInput."),
                "instance_path": path("to click before typing, such as a TextBox"),
                "wait_time_ms": wait.clone(),
            })),
            &["actions", "datamodel_type"],
        ),
        tool(
            MOUSE,
            "Sends mouse actions to the game in order: moveTo, mouseButtonDown, mouseButtonUp, \
             mouseButtonClick, scrollUp, scrollDown or wait. A step aims at instance_path (the \
             centre of a GuiObject) or at x and y; later steps may leave both out and reuse the \
             last position.",
            actions(json!({
                "action": {
                    "type": "string",
                    "enum": [
                        "moveTo",
                        "mouseButtonDown",
                        "mouseButtonUp",
                        "mouseButtonClick",
                        "scrollUp",
                        "scrollDown",
                        "wait",
                    ],
                    "description": "What this step does.",
                },
                "instance_path": path("to aim at; x and y are ignored when it is given"),
                "x": number("Screen x in GUI coordinates, as InputObject.Position measures it."),
                "y": number("Screen y in GUI coordinates, below the top bar inset."),
                "mouse_button": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Required for mouseButtonDown, mouseButtonUp and mouseButtonClick.",
                },
                "wait_time_ms": wait,
            })),
            &["actions", "datamodel_type"],
        ),
    ]
}

fn number(description: &str) -> Value {
    json!({ "type": "number", "description": description })
}

fn client_datamodel() -> Value {
    json!({
        "type": "string",
        "enum": [CLIENT_DATAMODEL],
        "description": "The data model to act on: the running playtest's Client.",
    })
}

fn actions(step: Value) -> Value {
    json!({
        "actions": {
            "type": "array",
            "description": "The steps, run in order.",
            "items": { "type": "object", "properties": step, "required": ["action"] },
        },
        "datamodel_type": client_datamodel(),
    })
}

pub(super) fn navigation(arguments: &Value, session: &mut Session) -> Outcome {
    let play = playtest(arguments, session)?;
    let arguments = arguments.clone();
    session.deferred = Some(Box::new(move || {
        run::navigate(&mut run::Live::new(play), &arguments).map(Value::String)
    }));
    Ok(Value::Null)
}

pub(super) fn keyboard(arguments: &Value, session: &mut Session) -> Outcome {
    let steps = steps(arguments)?;
    let play = playtest(arguments, session)?;
    session.deferred = Some(Box::new(move || {
        run::keyboard(&mut run::Live::new(play), &steps).map(Value::String)
    }));
    Ok(Value::Null)
}

pub(super) fn mouse(arguments: &Value, session: &mut Session) -> Outcome {
    let steps = steps(arguments)?;
    let play = playtest(arguments, session)?;
    session.deferred = Some(Box::new(move || {
        run::mouse(&mut run::Live::new(play), &steps).map(Value::String)
    }));
    Ok(Value::Null)
}

/// `actions`, which Studio requires before anything else.
fn steps(arguments: &Value) -> Result<Vec<Value>, String> {
    match arguments.get("actions") {
        Some(Value::Array(steps)) => Ok(steps.clone()),
        Some(_) => Err("actions must be an array".to_owned()),
        None => Err("Missing required argument: actions".to_owned()),
    }
}

/// The running playtest, once `datamodel_type` names its client.
fn playtest(arguments: &Value, session: &Session) -> Result<crate::mcp::PlayInput, String> {
    match text_argument(arguments, "datamodel_type") {
        None => return Err("datamodel_type is required".to_owned()),
        Some(CLIENT_DATAMODEL) => {}
        Some(other) => {
            return Err(format!(
                "{other} datamodel is not reachable: these tools act on the {CLIENT_DATAMODEL} \
                 of a running playtest"
            ))
        }
    }
    match (&session.play, session.studio.play) {
        (Some(play), PlayState::Playing | PlayState::ShowingLive) => Ok(play.clone()),
        (_, state) => Err(format!(
            "{CLIENT_DATAMODEL} datamodel is not available in {} mode",
            state.label()
        )),
    }
}
