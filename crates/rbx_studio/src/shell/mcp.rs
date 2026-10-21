//! Beta Features › MCP server: the shell's end of `crate::mcp`. A timer
//! drains what clients sent and answers it against the shell's own DOM, on
//! the UI thread, so a tool always sees the place as the Explorer does, and
//! applies a `multi_edit` as one undo step.

use std::time::Duration;

use gpui_kit::Context;

use crate::command_bar::Feedback;
use crate::mcp::protocol::Reply;
use crate::mcp::{self, endpoint::Server, PlayState, ScriptEdit, Session, Studio};

use super::play::Phase;
use super::Shell;

pub(crate) const BETA_ID: &str = "mcp-server";

/// How long a client waits at most for the next drain: short enough that a
/// chain of tool calls does not crawl, long enough to cost nothing idle.
const POLL_INTERVAL: Duration = Duration::from_millis(30);

impl Shell {
    /// Starts listening when the beta is on.
    pub(super) fn mcp_start(&mut self, cx: &mut Context<Self>) {
        if !crate::beta::enabled(BETA_ID) {
            return;
        }
        let Some(dir) = mcp::endpoint_dir() else {
            self.output
                .push_warning("MCP server: no configuration directory to write its endpoint in");
            return;
        };
        match Server::start(&dir) {
            Ok(server) => {
                self.output.push(
                    "MCP server",
                    Feedback::Info(format!(
                        "studio {} listening on 127.0.0.1:{}; connect a client with `rbxstudio --mcp`",
                        server.endpoint().studio_id,
                        server.endpoint().port
                    )),
                );
                self.mcp = Some(server);
            }
            Err(err) => {
                self.output
                    .push_warning(&format!("MCP server could not start: {err}"));
                return;
            }
        }
        cx.spawn(async move |shell, cx| loop {
            cx.background_executor().timer(POLL_INTERVAL).await;
            let alive = shell.update(cx, |shell, cx| shell.mcp_answer(cx)).is_ok();
            if !alive {
                break;
            }
        })
        .detach();
    }

    fn mcp_answer(&mut self, cx: &mut Context<Self>) {
        let Some(server) = &self.mcp else {
            return;
        };
        let requests = server.drain();
        if requests.is_empty() {
            return;
        }
        // Typing still waiting on its debounce is part of the script as the
        // user sees it, so a tool reads it and an edit lands on top of it.
        if !self.live_shown() {
            self.flush_script_edits(cx);
        }
        let studio = self.mcp_studio();
        for request in requests {
            let mut session = Session::new(self.edit_dom(), &self.properties, &studio);
            let reply = mcp::protocol::handle(request.line(), &mut session);
            let planned = session.planned.take();
            if let Some(edit) = planned {
                self.mcp_apply(&edit, cx);
            }
            match reply {
                Some(Reply::Later(work)) => {
                    // The connection waits for this answer before reading
                    // its next line, so its calls still answer in order.
                    let spawned = std::thread::Builder::new()
                        .name("mcp-tool".to_owned())
                        .spawn(move || request.answer(Some(work())));
                    if let Err(err) = spawned {
                        self.output
                            .push_warning(&format!("MCP server could not run a tool: {err}"));
                    }
                }
                reply => request.answer(reply.map(Reply::wait)),
            }
        }
    }

    fn mcp_studio(&self) -> Studio {
        let play = if self.live_shown() {
            PlayState::ShowingLive
        } else {
            match self.play.phase {
                Phase::Idle => PlayState::Edit,
                Phase::Starting(_) => PlayState::Starting,
                Phase::Running => PlayState::Playing,
            }
        };
        Studio {
            id: self
                .mcp
                .as_ref()
                .map(|server| server.endpoint().studio_id.clone())
                .unwrap_or_default(),
            name: self
                .path
                .file_name()
                .map(|file| file.to_string_lossy().into_owned())
                .unwrap_or_default(),
            place_id: crate::home::link_of(&self.path).map(|link| link.place_id),
            play,
        }
    }

    /// Writes a planned edit the way `shell::scripts` writes a tab's text,
    /// and an insert the way `shell::keys` does: snapshot, write, log.
    fn mcp_apply(&mut self, edit: &ScriptEdit, cx: &mut Context<Self>) {
        self.dom.take_changes();
        let before = self.dom.clone();
        if mcp::apply_edit(&mut self.dom, edit).is_none() {
            return;
        }
        self.push_history_snapshot(before);
        let changes = self.dom.take_changes();
        if edit.creates() {
            self.rebuild_explorer(cx);
        }
        // An open tab on the script re-seeds from the DOM on its next render
        // (`resync_scripts`).
        self.reflect_changes(&changes, cx);
        self.record_history_change(changes);
        cx.notify();
    }
}
