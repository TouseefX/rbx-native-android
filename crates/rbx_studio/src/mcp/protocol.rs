//! JSON-RPC 2.0 as MCP uses it: one message per line, `initialize`, `ping`,
//! `tools/list` and `tools/call` answered, notifications taken silently.
//! The broker reads lines with the same [`parse`], and answers what needs no
//! editor with the same [`answer_alone`].

#[cfg(test)]
mod tests;

use serde_json::{json, Value};

use super::{tools, Inserting, Job, JobOutcome, Session};

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

/// What one line from the client gets back.
pub(crate) enum Reply {
    Now(String),
    /// A tool's [`super::Deferred`] work, which gives the reply once run.
    Later(Box<dyn FnOnce() -> String + Send>),
    /// Work only the shell can do; [`job_reply`] answers it once done.
    Job {
        id: Value,
        job: Job,
    },
    /// An `insert_asset` call: `fetch` runs off the UI thread, the shell
    /// writes its result into the place, then [`answer`] replies to `id`.
    Insert {
        id: Value,
        fetch: Inserting,
    },
}

impl Reply {
    /// The reply, running deferred work on the calling thread.
    pub(crate) fn wait(self) -> String {
        match self {
            Reply::Now(text) => text,
            Reply::Later(work) => work(),
            // Only the shell can do a job, and it takes this reply itself
            // rather than waiting on it.
            Reply::Job { id, job } => job_reply(
                id,
                Err(format!("this editor cannot run {job:?} without its window")),
            ),
            // Only the shell holds a place to write into.
            Reply::Insert { id, .. } => answer(
                id,
                Err("insert_asset needs an editor window to insert into".to_owned()),
            ),
        }
    }
}

/// The reply to one line from the client, or `None` when it needs none.
pub(crate) fn handle(line: &str, session: &mut Session) -> Option<Reply> {
    let (id, method, params) = match parse(line) {
        Message::Request { id, method, params } => (id, method, params),
        Message::Silent => return None,
        Message::Invalid(reply) => return Some(Reply::Now(reply)),
    };
    let outcome = match method.as_str() {
        "tools/call" => tools::call(&params, session),
        _ => session_free(&method, &params),
    };
    if let Some(job) = session.job.take() {
        return Some(Reply::Job { id, job });
    }
    if let Some(fetch) = session.inserting.take() {
        return Some(Reply::Insert { id, fetch });
    }
    if let Some(work) = session.deferred.take() {
        return Some(Reply::Later(Box::new(move || {
            reply(id, Ok(tools::finished(work())))
        })));
    }
    Some(Reply::Now(reply(id, outcome)))
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

/// The reply to a tool call whose outcome came after the call was read.
pub(crate) fn answer(id: Value, outcome: Result<Value, String>) -> String {
    reply(id, Ok(tools::finished(outcome)))
}

pub(crate) fn reply(id: Value, outcome: Result<Value, Failure>) -> String {
    match outcome {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string(),
        Err((code, message)) => error(id, code, message),
    }
}

/// The whole JSON-RPC line answering a [`Reply::Job`] once it is done.
pub(crate) fn job_reply(id: Value, outcome: JobOutcome) -> String {
    let (content, is_error) = match outcome {
        Ok(content) => (content, false),
        Err(message) => (vec![tools::text_content(&message)], true),
    };
    reply(id, Ok(json!({ "content": content, "isError": is_error })))
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
        "instructions": "Reads the place open in the rbx-native editor and edits its scripts: \
            browse and inspect instances, read, search and grep scripts, and change or create \
            them with multi_edit, each call one undo step. Runs Luau in the edited place or, \
            during a playtest, in its Client or Server, starts and stops play, reads the \
            Output and captures the viewport. Instances are named by dotted paths from the \
            DataModel, as in game.Workspace.Model.Part.",
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
