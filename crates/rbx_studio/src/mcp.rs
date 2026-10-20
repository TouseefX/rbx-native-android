//! An MCP server over the live editor session, shaped after the one built
//! into Roblox Studio (`studio/mcp.md`): an AI client reads the open place
//! through tools named as Studio names them.
//!
//! An MCP client speaks to a server it launches, over that process's stdin
//! and stdout. The editor is not that process: it is already running, with
//! the place open. So `rbxstudio --mcp` is a small [`bridge`] the client
//! launches, which forwards each line to the editor over a loopback socket.
//! The editor's side ([`endpoint`]) listens on a port the system picks and
//! writes it, with a random token, to `<config>/mcp.json`; a connection that
//! does not open with that token is closed unread, so another user on the
//! same machine cannot reach the session through the port alone.
//!
//! Every message is answered on the UI thread ([`protocol::handle`]), with
//! the DOM exactly as the Explorer shows it at that moment. The tools only
//! read for now; anything that edits will have to go through the same DOM
//! mutation and undo path every other edit does.
//!
//! One editor at a time is reachable: the last one started owns
//! `mcp.json`, and removes it on the way out only while it is still its own.

pub(crate) mod bridge;
pub(crate) mod endpoint;
pub(crate) mod protocol;
mod tools;

use std::path::PathBuf;

use rbx_dom::WeakDom;

use crate::properties::Properties;

/// The command-line flag that turns `rbxstudio` into the [`bridge`].
pub(crate) const BRIDGE_FLAG: &str = "--mcp";

/// What a tool reads: the place as it stands, and the Properties panel's
/// own formatting, so a value reads the same to the client as in the panel.
pub(crate) struct Session<'a> {
    pub(crate) dom: &'a WeakDom,
    pub(crate) properties: &'a Properties,
}

/// Where the running editor says how to reach it.
pub(crate) fn endpoint_file() -> Option<PathBuf> {
    crate::settings::default_config_dir().map(|dir| dir.join("mcp.json"))
}
