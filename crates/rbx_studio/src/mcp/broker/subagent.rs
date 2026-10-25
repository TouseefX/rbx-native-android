//! The `subagent` tool: a short agent loop the broker runs itself, asking
//! the client's model through `sampling/createMessage` and forwarding each
//! tool call it makes to the routed editor. The editor has no model of its
//! own, so a client without sampling cannot use it.

#[cfg(test)]
mod tests;

use serde_json::{json, Map, Value};

use super::{Broker, Client};
use crate::mcp::tools::{self, text_content};

pub(super) const SUBAGENT: &str = "subagent";
pub(super) const MAX_TURNS: usize = 30;

const NO_SAMPLING: &str = "Subagent execution is not available: the MCP client did not \
advertise the sampling capability (sampling/createMessage), and the editor has no model of its own.";

const EXPLORE_TOOLS: &[&str] = &[
    "search_game_tree",
    "inspect_instance",
    "script_read",
    "script_search",
    "script_grep",
    "get_studio_state",
    "execute_luau",
    "get_console_output",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Explore,
    ScreenCapture,
    UnitTest,
}

/// The types Studio's own `subagent` takes, named in its order.
const KINDS: &str = "screen_capture, unit_test, explore";

impl Kind {
    pub(super) fn parse(text: &str) -> Option<Kind> {
        match text {
            "explore" => Some(Kind::Explore),
            "screen_capture" => Some(Kind::ScreenCapture),
            "unit_test" => Some(Kind::UnitTest),
            _ => None,
        }
    }

    pub(super) fn allows(self, tool: &str) -> bool {
        EXPLORE_TOOLS.contains(&tool)
            || match self {
                Kind::Explore => false,
                Kind::ScreenCapture => tool == "screen_capture",
                Kind::UnitTest => tool == "multi_edit" || tool == "start_stop_play",
            }
    }

    fn brief(self) -> &'static str {
        match self {
            Kind::Explore => {
                "You are an explore subagent inside a Roblox place editor. Investigate the place \
                 read-only: find instances with search_game_tree, look at them with \
                 inspect_instance, read and search scripts with script_read, script_search and \
                 script_grep, and use get_studio_state, execute_luau and get_console_output to \
                 check facts. Do not change the place."
            }
            Kind::ScreenCapture => {
                "You are a screen capture subagent inside a Roblox place editor. Find what the \
                 task is about with search_game_tree and inspect_instance, capture the viewport \
                 with screen_capture (give camera_position and look_at_position to aim at it), \
                 and describe what the images show. Do not change the place."
            }
            Kind::UnitTest => {
                "You are a unit test subagent inside a Roblox place editor. Read the code under \
                 test (script_read, script_grep, inspect_instance), then write each test as a \
                 ModuleScript under ServerStorage.UnitTest.Cases with multi_edit, returning a \
                 function that errors when the test fails. Start play with start_stop_play, run \
                 every case on the Server with execute_luau, read get_console_output, stop play, \
                 and report PASS or FAIL for each test with the reason."
            }
        }
    }

    /// The listed tools this kind may call, as `tools/list` describes them.
    fn tools(self) -> Vec<Value> {
        tools::list()
            .as_array()
            .into_iter()
            .flatten()
            .filter(|tool| tool["name"].as_str().is_some_and(|name| self.allows(name)))
            .map(|tool| {
                json!({
                    "name": tool["name"],
                    "description": tool["description"],
                    "inputSchema": tool["inputSchema"],
                })
            })
            .collect()
    }
}

/// The tool as `tools/list` shows it, with Studio's arguments.
pub(crate) fn schema() -> Value {
    json!({
        "name": SUBAGENT,
        "description": "Launch a specialized subagent to handle complex, multi-step tasks \
            autonomously. It runs on the MCP client's own model through sampling \
            (sampling/createMessage), so the client must support sampling.\n\nAvailable \
            subagents:\n- explore: investigates the place read-only (the tree, scripts, \
            execute_luau, the console) and answers questions about it.\n- screen_capture: \
            aims the camera, captures the viewport and describes what it shows.\n- \
            unit_test: writes tests under ServerStorage.UnitTest, runs them during play and \
            reports PASS or FAIL.\n\nThe subagent returns one final text result; it cannot \
            hold a conversation or start other subagents.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "description": {
                    "type": "string",
                    "description": "A short 3-5 word description of the task in present continuous tense (e.g., 'Exploring player spawn logic'). Used for display.",
                },
                "subagent_type": {
                    "type": "string",
                    "enum": ["explore", "screen_capture", "unit_test"],
                    "description": "The type of subagent to invoke.",
                },
                "task": {
                    "type": "string",
                    "description": "Detailed task description and instructions for the subagent. Be specific about what you need it to do and what information to return.",
                },
            },
            "required": ["task", "description", "subagent_type"],
        },
    })
}

/// What the model asked for in one reply of the text protocol.
#[derive(Debug, PartialEq)]
pub(super) enum Ask {
    Tool { name: String, arguments: Value },
    Final(String),
}

/// Reads the one JSON object a text-protocol reply should be, also inside a
/// Markdown fence or after a sentence of preamble.
pub(super) fn parse_ask(text: &str) -> Option<Ask> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let object: Value = serde_json::from_str(text.get(start..=end)?).ok()?;
    if let Some(answer) = object.get("final") {
        return Some(Ask::Final(
            answer
                .as_str()
                .map_or_else(|| answer.to_string(), str::to_owned),
        ));
    }
    let name = object.get("tool")?.as_str()?.to_owned();
    let arguments = object.get("arguments").cloned().unwrap_or(json!({}));
    Some(Ask::Tool { name, arguments })
}

/// The content blocks of a sampling result, which may be one block or many.
fn blocks(result: &Value) -> Vec<Value> {
    match &result["content"] {
        Value::Array(blocks) => blocks.clone(),
        Value::Null => Vec::new(),
        block => vec![block.clone()],
    }
}

fn joined_text(blocks: &[Value]) -> String {
    blocks
        .iter()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn failed(text: &str) -> Value {
    tools::result(&Value::String(text.to_owned()), true)
}

impl Broker {
    /// Runs one subagent to its final answer: the tools/call result.
    pub(super) fn subagent(&mut self, arguments: &Value, client: &mut dyn Client) -> Value {
        let text = |key: &str| arguments.get(key).and_then(Value::as_str).unwrap_or("");
        let kind_name = text("subagent_type");
        let Some(kind) = Kind::parse(kind_name) else {
            return failed(&format!(
                "Unknown subagent type: \"{kind_name}\". Available types: {KINDS}"
            ));
        };
        if !self.sampling {
            return failed(NO_SAMPLING);
        }
        let studio_id = text("studio_id").to_owned();
        let native = self.sampling_tools;
        let allowed = kind.tools();

        let task = format!("Task: {}\n\n{}", text("description"), text("task"));
        let mut system = kind.brief().to_owned();
        if !native {
            system.push_str(&format!(
                "\n\nTools you can call, with their JSON input schemas:\n{}\n\nReply with \
                 exactly one JSON object and nothing else: {{\"tool\": <name>, \"arguments\": \
                 {{..}}}} to call a tool, whose result comes back in the next message, or \
                 {{\"final\": <text>}} once you have the answer.",
                serde_json::to_string_pretty(&allowed).unwrap_or_default()
            ));
        }
        let mut messages = vec![json!({ "role": "user", "content": text_content(&task) })];
        let mut last_text = String::new();

        for _ in 0..MAX_TURNS {
            let mut params = json!({
                "messages": messages,
                "systemPrompt": system,
                "maxTokens": 4096,
                "includeContext": "none",
            });
            if native {
                params["tools"] = Value::Array(allowed.clone());
            }
            let result = match client.request("sampling/createMessage", params) {
                Ok(result) => result,
                Err(err) => return failed(&format!("Subagent sampling failed: {err}")),
            };
            let content = blocks(&result);
            last_text = joined_text(&content);
            messages.push(json!({ "role": "assistant", "content": content.clone() }));

            if native {
                let uses: Vec<&Value> = content
                    .iter()
                    .filter(|block| block["type"] == "tool_use")
                    .collect();
                if uses.is_empty() {
                    return tools::result(&Value::String(last_text), false);
                }
                let results: Vec<Value> = uses
                    .into_iter()
                    .map(|block| {
                        let name = block["name"].as_str().unwrap_or("");
                        let (content, is_error) =
                            self.run_tool(kind, &studio_id, name, &block["input"]);
                        json!({
                            "type": "tool_result",
                            "toolUseId": block["id"],
                            "content": content,
                            "isError": is_error,
                        })
                    })
                    .collect();
                messages.push(json!({ "role": "user", "content": results }));
                continue;
            }

            let reply = match parse_ask(&last_text) {
                Some(Ask::Final(answer)) => return tools::result(&Value::String(answer), false),
                Some(Ask::Tool { name, arguments }) => {
                    let (content, is_error) = self.run_tool(kind, &studio_id, &name, &arguments);
                    let status = if is_error { "failed" } else { "returned" };
                    let mut reply = vec![text_content(&format!("Tool \"{name}\" {status}:"))];
                    reply.extend(content);
                    reply
                }
                None => vec![text_content(
                    "Reply with exactly one JSON object: {\"tool\": <name>, \"arguments\": \
                     {..}} or {\"final\": <text>}.",
                )],
            };
            messages.push(json!({ "role": "user", "content": reply }));
        }
        failed(&format!(
            "Subagent stopped after {MAX_TURNS} turns without a final answer\n\n{last_text}"
        ))
    }

    /// Forwards one tool call exactly as a client's would be: its content
    /// blocks and whether it failed.
    fn run_tool(
        &mut self,
        kind: Kind,
        studio_id: &str,
        name: &str,
        input: &Value,
    ) -> (Vec<Value>, bool) {
        if !kind.allows(name) {
            let refusal = format!("Error: Tool \"{name}\" is not available to this subagent.");
            return (vec![text_content(&refusal)], true);
        }
        let mut arguments = input.as_object().cloned().unwrap_or_else(Map::new);
        if !studio_id.is_empty() {
            arguments.insert("studio_id".to_owned(), Value::String(studio_id.to_owned()));
        }
        let line = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments },
        })
        .to_string();
        let wanted = (!studio_id.is_empty()).then_some(studio_id);
        let reply = self.call(json!(0), wanted, name, &line);
        let answer: Value = serde_json::from_str(&reply).unwrap_or(Value::Null);
        let result = &answer["result"];
        match result["content"].as_array() {
            Some(content) => (
                content.clone(),
                result["isError"].as_bool().unwrap_or(false),
            ),
            None => {
                let message = answer["error"]["message"]
                    .as_str()
                    .map_or_else(|| reply.clone(), str::to_owned);
                (vec![text_content(&message)], true)
            }
        }
    }
}
