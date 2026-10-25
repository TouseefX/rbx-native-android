//! `rbxstudio --mcp`: the process an MCP client launches. It reads one
//! message per line from stdin and writes each answer to stdout, through a
//! [`Broker`] that reaches every running editor, and holds no place state of
//! its own, so the client's session is the editors'.
//!
//! It starts whether or not an editor is open: a client launches it once,
//! and a tool call made before any editor is reports that in its result.

use std::collections::VecDeque;
use std::io::{BufRead, Lines, StdinLock, StdoutLock, Write};

use serde_json::{json, Value};

use super::broker::{Broker, Client};
use super::protocol;

/// The client's end of stdin/stdout, also asked things by the broker.
struct Stdio {
    lines: Lines<StdinLock<'static>>,
    stdout: StdoutLock<'static>,
    /// Client requests that arrived while a sampling answer was awaited.
    pending: VecDeque<String>,
    next_id: u64,
}

impl Stdio {
    fn next_line(&mut self) -> Option<String> {
        if let Some(line) = self.pending.pop_front() {
            return Some(line);
        }
        self.lines.next()?.ok()
    }

    /// Writes one line; false once the client is gone.
    fn send(&mut self, line: &str) -> bool {
        // A line that sat in a buffer would leave the client waiting.
        writeln!(self.stdout, "{line}")
            .and_then(|()| self.stdout.flush())
            .is_ok()
    }
}

impl Client for Stdio {
    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = format!("rbx-native-sampling-{}", self.next_id);
        let line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if !self.send(&line.to_string()) {
            return Err("the MCP client closed the connection".to_owned());
        }
        loop {
            let Some(Ok(read)) = self.lines.next() else {
                return Err("the MCP client closed the connection".to_owned());
            };
            let Ok(message) = serde_json::from_str::<Value>(&read) else {
                continue;
            };
            match (
                message.get("method").and_then(Value::as_str),
                message.get("id"),
            ) {
                (None, Some(answered)) if answered.as_str() == Some(id.as_str()) => {
                    if let Some(error) = message.get("error") {
                        let text = error["message"]
                            .as_str()
                            .map_or_else(|| error.to_string(), str::to_owned);
                        return Err(text);
                    }
                    return Ok(message.get("result").cloned().unwrap_or(Value::Null));
                }
                (Some("ping"), Some(ping)) => {
                    let pong = protocol::reply(ping.clone(), Ok(json!({})));
                    if !self.send(&pong) {
                        return Err("the MCP client closed the connection".to_owned());
                    }
                }
                (Some(_), Some(_)) => self.pending.push_back(read),
                _ => {}
            }
        }
    }
}

/// Runs the bridge until the client closes stdin; the process's exit code.
pub(crate) fn run() -> i32 {
    let Some(dir) = super::endpoint_dir() else {
        eprintln!("rbxstudio --mcp: no configuration directory to find the editor through");
        return 1;
    };
    let mut broker = Broker::new(dir);
    let mut stdio = Stdio {
        lines: std::io::stdin().lock().lines(),
        stdout: std::io::stdout().lock(),
        pending: VecDeque::new(),
        next_id: 0,
    };
    while let Some(line) = stdio.next_line() {
        if line.trim().is_empty() {
            continue;
        }
        let Some(reply) = broker.handle(&line, &mut stdio) else {
            continue;
        };
        if !stdio.send(&reply) {
            break;
        }
    }
    0
}
