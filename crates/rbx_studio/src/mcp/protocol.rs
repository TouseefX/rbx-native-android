//! JSON-RPC 2.0 as MCP uses it: one message per line, `initialize`, `ping`,
//! `tools/list` and `tools/call` answered, notifications taken silently.

#[cfg(test)]
mod tests;

use serde_json::{json, Value};

use super::{tools, Session};

/// Newest first; an `initialize` asking for another version is answered
/// with the newest, which is how the specification says to decline it.
const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
pub(super) const INVALID_PARAMS: i64 = -32602;

/// A JSON-RPC error: its code and message.
pub(super) type Failure = (i64, String);

/// The reply to one line from the client, or `None` when it needs none (a
/// notification, or a response to a request this server never sends).
pub(crate) fn handle(line: &str, session: &Session) -> Option<String> {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(err) => return Some(error(Value::Null, PARSE_ERROR, err.to_string())),
    };
    let Some(object) = message.as_object() else {
        return Some(error(
            Value::Null,
            INVALID_REQUEST,
            "expected one JSON-RPC object per line".to_owned(),
        ));
    };
    let id = object.get("id").cloned();
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        if object.contains_key("result") || object.contains_key("error") {
            return None;
        }
        return Some(error(
            id.unwrap_or(Value::Null),
            INVALID_REQUEST,
            "no method".to_owned(),
        ));
    };
    // `notifications/initialized`, `notifications/cancelled`: every request
    // is answered before the next line is read, so there is nothing left
    // for either of them to change.
    let id = id?;
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    let outcome = match method {
        "initialize" => Ok(initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::list() })),
        "tools/call" => tools::call(&params, session),
        _ => Err((METHOD_NOT_FOUND, format!("method not found: {method}"))),
    };
    Some(match outcome {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string(),
        Err((code, message)) => error(id, code, message),
    })
}

fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = PROTOCOL_VERSIONS
        .iter()
        .copied()
        .find(|known| Some(*known) == asked)
        .unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "rbx-native", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "Reads the place open in the rbx-native editor. Instances are \
            named by dotted paths from the DataModel, as in game.Workspace.Model.Part.",
    })
}

fn error(id: Value, code: i64, message: String) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
    .to_string()
}
