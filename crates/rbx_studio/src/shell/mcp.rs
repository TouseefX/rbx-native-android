//! Beta Features › MCP server: the shell's end of `crate::mcp`. A timer
//! drains what clients sent and answers it against the shell's own DOM, on
//! the UI thread, so a tool always sees the place as the Explorer does.

use std::time::Duration;

use gpui_kit::Context;

use crate::command_bar::Feedback;
use crate::mcp::{self, endpoint::Server, Session};

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
        let Some(file) = mcp::endpoint_file() else {
            self.output
                .push_warning("MCP server: no configuration directory to write mcp.json in");
            return;
        };
        match Server::start(file) {
            Ok(server) => {
                self.output.push(
                    "MCP server",
                    Feedback::Info(format!(
                        "listening on 127.0.0.1:{}; connect a client with `rbxstudio --mcp`",
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
            let alive = shell.update(cx, |shell, _| shell.mcp_answer()).is_ok();
            if !alive {
                break;
            }
        })
        .detach();
    }

    fn mcp_answer(&mut self) {
        let Some(server) = &self.mcp else {
            return;
        };
        let session = Session {
            dom: &self.dom,
            properties: &self.properties,
        };
        for request in server.drain() {
            let reply = mcp::protocol::handle(request.line(), &session);
            request.answer(reply);
        }
    }
}
