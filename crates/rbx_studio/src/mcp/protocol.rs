//! JSON-RPC 2.0 as MCP uses it: one message per line, `initialize`, `ping`,
//! `tools/list` and `tools/call` answered, notifications taken silently.
//! The broker reads lines with the same [`parse`], and answers what needs no
//! editor with the same [`answer_alone`].

#[cfg(test)]
mod tests;

use serde_json::{json, Value};

use super::{tools, Session};

/// Newest first; an `initialize` asking for another version is answered
/// with the newest, which is how the specification says to decline it.
const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
pub(crate) const METHOD_NOT_FOUND: i64 = -32601;
pub(crate) const INVALID_PARAMS: i64 = -32602;

/// A JSON-RPC error: its code and message.
pub(crate) type Failure = (i64, String);

/// One line from the client, read.
pub(crate) enum Message {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    /// A notification, or a response to a request this server never sends.
    Silent,
    /// Not a message at all; the error reply to send back.
    Invalid(String),
}

pub(crate) fn parse(line: &str) -> Message {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(err) => return Message::Invalid(error(Value::Null, PARSE_ERROR, err.to_string())),
    };
    let Some(object) = message.as_object() else {
        return Message::Invalid(error(
            Value::Null,
            INVALID_REQUEST,
            "expected one JSON-RPC object per line".to_owned(),
        ));
    };
    let id = object.get("id").cloned();
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        if object.contains_key("result") || object.contains_key("error") {
            return Message::Silent;
        }
        return Message::Invalid(error(
            id.unwrap_or(Value::Null),
            INVALID_REQUEST,
            "no method".to_owned(),
        ));
    };
    // `notifications/initialized`, `notifications/cancelled`: every request
    // is answered before the next line is read, so there is nothing left
    // for either of them to change.
    let Some(id) = id else {
        return Message::Silent;
    };
    Message::Request {
        id,
        method: method.to_owned(),
        params: object.get("params").cloned().unwrap_or(Value::Null),
    }
}

/// The reply to one line from the client, or `None` when it needs none.
pub(crate) fn handle(line: &str, session: &mut Session) -> Option<String> {
    let (id, method, params) = match parse(line) {
        Message::Request { id, method, params } => (id, method, params),
        Message::Silent => return None,
        Message::Invalid(reply) => return Some(reply),
    };
    let outcome = match method.as_str() {
        "tools/call" => tools::call(&params, session),
        _ => session_free(&method, &params),
    };
    Some(reply(id, outcome))
}

/// What is answered the same by every editor, so the broker can answer it
/// without asking one: `None` for a method that needs a session.
pub(crate) fn answer_alone(method: &str, params: &Value) -> Option<Result<Value, Failure>> {
    match method {
        "initialize" => Some(Ok(initialize(params))),
        "ping" => Some(Ok(json!({}))),
        "tools/list" => Some(Ok(json!({ "tools": tools::list() }))),
        _ => None,
    }
}

fn session_free(method: &str, params: &Value) -> Result<Value, Failure> {
    answer_alone(method, params)
        .unwrap_or_else(|| Err((METHOD_NOT_FOUND, format!("method not found: {method}"))))
}

pub(crate) fn reply(id: Value, outcome: Result<Value, Failure>) -> String {
    match outcome {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string(),
        Err((code, message)) => error(id, code, message),
    }
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
