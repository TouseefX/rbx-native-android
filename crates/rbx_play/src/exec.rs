//! `execute_luau` support: the probe-side Luau that runs a snippet on a
//! side and serializes its first return value, and the command that
//! carries the snippet there.

use std::fmt::Write as _;

use crate::protocol::{CommandBody, Side};

pub const PROBE_SERIALIZE: &str = include_str!("probe/serialize.luau");
pub const PROBE_EXEC: &str = include_str!("probe/exec.luau");
pub const PROBE_FIU: &str = include_str!("probe/fiu.luau");

pub const SERIALIZE_NAME: &str = "__rbxnative_serialize";
pub const EXEC_NAME: &str = "__rbxnative_exec";
pub const FIU_NAME: &str = "__rbxnative_fiu";

/// The command `execute_luau` sends for `source` on `side`. The client has
/// no `loadstring`, so its code travels as bytecode for the Fiu interpreter.
pub fn command(side: Side, source: &str) -> Result<CommandBody, String> {
    let bytecode = match side {
        Side::Server => String::new(),
        Side::Client => hex(&compile(source)?),
    };
    let source = match side {
        Side::Server => source.to_owned(),
        Side::Client => String::new(),
    };
    Ok(CommandBody::Exec {
        side,
        source,
        bytecode,
    })
}

/// Source of the injected ModuleScript: the probe chunk with its siblings
/// and `loadstring` passed in.
pub fn exec_module_source() -> String {
    format!(
        "return (function(...)\n{PROBE_EXEC}\nend)(require(script.Parent.{SERIALIZE_NAME}), require(script.Parent.{FIU_NAME}), loadstring)"
    )
}

/// A bare expression yields its value, so `return <source>` goes first.
/// A syntax error reads `AssistantCommand:<line>: <message>`, as the Server
/// side's `loadstring` and the Edit side name the chunk.
fn compile(source: &str) -> Result<Vec<u8>, String> {
    let compiler = mlua::chunk::Compiler::new();
    compiler
        .compile(format!("return {source}"))
        .or_else(|_| compiler.compile(source))
        .map_err(|error| match error {
            // The compiler has no chunk name: its message starts at the line.
            mlua::Error::SyntaxError { message, .. } => format!("AssistantCommand:{message}"),
            other => other.to_string(),
        })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

#[cfg(test)]
mod tests;
