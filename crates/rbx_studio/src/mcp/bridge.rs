//! `rbxstudio --mcp`: the process an MCP client launches. It reads one
//! message per line from stdin and writes each answer to stdout, through a
//! [`Broker`] that reaches every running editor, and holds no place state of
//! its own, so the client's session is the editors'.
//!
//! It starts whether or not an editor is open: a client launches it once,
//! and a tool call made before any editor is reports that in its result.

use std::io::{BufRead, Write};

use super::broker::Broker;

/// Runs the bridge until the client closes stdin; the process's exit code.
pub(crate) fn run() -> i32 {
    let Some(dir) = super::endpoint_dir() else {
        eprintln!("rbxstudio --mcp: no configuration directory to find the editor through");
        return 1;
    };
    let mut broker = Broker::new(dir);
    let mut stdout = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let Some(reply) = broker.handle(&line) else {
            continue;
        };
        // A reply that sat in a buffer would leave the client waiting.
        if writeln!(stdout, "{reply}")
            .and_then(|()| stdout.flush())
            .is_err()
        {
            break;
        }
    }
    0
}
