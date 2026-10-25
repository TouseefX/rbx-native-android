//! An MCP server over the live editor session, shaped after the one built
//! into Roblox Studio (`studio/mcp.md`): an AI client reads and edits the
//! open place through tools named as Studio names them.
//!
//! An MCP client speaks to a server it launches, over that process's stdin
//! and stdout. The editor is not that process: it is already running, with
//! the place open, and there may be several of them. So `rbxstudio --mcp` is
//! a small [`bridge`] the client launches, which hands each line to a
//! [`broker`] that knows every running editor. Each editor's side
//! ([`endpoint`]) listens on a port the system picks and writes it, with a
//! random token and the editor's `studio_id`, to its own file under
//! `<config>/mcp/`, which only its owner can read; a connection that does not
//! open with that token is closed unread, so another user on the same machine
//! cannot reach the session through the port alone.
//!
//! As in Studio, every tool call names the editor it is for by `studio_id`
//! (`list_roblox_studios` lists them); with one editor running the id may be
//! left out.
//!
//! Every message is answered on the UI thread ([`protocol::handle`]), with
//! the DOM exactly as the Explorer shows it at that moment. An edit is only
//! planned there ([`Session::planned`]); the shell applies it through the
//! same DOM write and undo history a script tab's own edits take. Only a
//! grep, whose Luau pattern has no bound on how long it takes, runs off that
//! thread, over a copy of the scripts ([`Session::deferred`]).

pub(crate) mod bridge;
pub(crate) mod broker;
pub(crate) mod endpoint;
pub(crate) mod protocol;
mod tools;

use std::path::PathBuf;

use rbx_dom::WeakDom;

use crate::properties::Properties;

pub(crate) use tools::{apply_edit, ScriptEdit};

/// The command-line flag that turns `rbxstudio` into the [`bridge`].
pub(crate) const BRIDGE_FLAG: &str = "--mcp";

/// What a tool works on: the place as it stands, the Properties panel's own
/// formatting (so a value reads the same to the client as in the panel),
/// and which editor this is.
pub(crate) struct Session<'a> {
    pub(crate) dom: &'a WeakDom,
    pub(crate) properties: &'a Properties,
    pub(crate) studio: &'a Studio,
    /// The edit a `multi_edit` call asked for, for the shell to apply once
    /// the call has been answered. Tools never write to `dom` themselves.
    pub(crate) planned: Option<ScriptEdit>,
    /// The work a tool with no bound on how long it takes (`script_grep`)
    /// left to run off the UI thread; what it returns is the tool's answer.
    pub(crate) deferred: Option<Deferred>,
    /// The running playtest the input tools act on; `None` outside Play.
    pub(crate) play: Option<PlayInput>,
}

/// A running playtest as the input tools reach it, from any thread.
#[derive(Clone)]
pub(crate) struct PlayInput {
    /// The client's probe: where a GuiObject is, walking the character,
    /// typing into a TextBox.
    pub(crate) probe: rbx_play::channel::Requester,
    /// The embedded client's window, for real mouse and key events; `None`
    /// while it is not embedded or another document hides the 3D view.
    #[cfg(any(target_os = "linux", windows))]
    pub(crate) window: Option<rbx_play::embed::WindowId>,
}

/// Runs over a copy of what it reads, so the editor stays responsive however
/// long it takes, and the place can change meanwhile.
pub(crate) type Deferred = Box<dyn FnOnce() -> Result<serde_json::Value, String> + Send>;

impl<'a> Session<'a> {
    pub(crate) fn new(dom: &'a WeakDom, properties: &'a Properties, studio: &'a Studio) -> Self {
        Session {
            dom,
            properties,
            studio,
            planned: None,
            deferred: None,
            play: None,
        }
    }
}

/// One running editor, as `list_roblox_studios` and `get_studio_state`
/// describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Studio {
    pub(crate) id: String,
    /// The place file's name, such as `Lobby.rbxl`, as the window title
    /// shows it.
    pub(crate) name: String,
    /// Only a place linked to Roblox has one.
    pub(crate) place_id: Option<u64>,
    pub(crate) play: PlayState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlayState {
    Edit,
    Starting,
    Playing,
    /// A running playtest's own tree is shown in the editor. Tools still read
    /// the edited place, but nothing may write to it until the view returns.
    ShowingLive,
}

impl PlayState {
    pub(crate) fn label(self) -> &'static str {
        match self {
            PlayState::Edit => "Edit",
            PlayState::Starting => "Starting",
            PlayState::Playing | PlayState::ShowingLive => "Play",
        }
    }
}

/// Where every running editor says how to reach it, one file each.
pub(crate) fn endpoint_dir() -> Option<PathBuf> {
    crate::settings::default_config_dir().map(|dir| dir.join("mcp"))
}
