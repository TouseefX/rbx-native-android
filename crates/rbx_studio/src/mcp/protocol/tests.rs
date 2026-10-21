use rbx_dom::WeakDom;
use rbx_reflection::ReflectionDatabase;
use serde_json::{json, Value};

use super::handle;
use crate::mcp::{PlayState, Session, Studio};
use crate::properties::Properties;

fn reply(line: &str) -> Option<Value> {
    let dom = WeakDom::new();
    let properties = Properties::new(ReflectionDatabase::embedded());
    let studio = Studio {
        id: "a1".to_owned(),
        name: "Arena".to_owned(),
        place_id: Some(42),
        play: PlayState::Edit,
    };
    let mut session = Session::new(&dom, &properties, &studio);
    handle(line, &mut session).map(|reply| serde_json::from_str(&reply.wait()).unwrap())
}

#[test]
fn initialize_answers_the_version_asked_for_when_it_is_known() {
    let answer = reply(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#,
    )
    .unwrap();
    assert_eq!(answer["id"], 1);
    assert_eq!(answer["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(answer["result"]["serverInfo"]["name"], "rbx-native");
    assert!(answer["result"]["capabilities"]["tools"].is_object());
}

#[test]
fn initialize_offers_the_newest_version_for_one_it_does_not_know() {
    let answer = reply(
        r#"{"jsonrpc":"2.0","id":"a","method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
    )
    .unwrap();
    assert_eq!(answer["id"], "a");
    assert_eq!(answer["result"]["protocolVersion"], "2025-06-18");
}

#[test]
fn notifications_and_client_responses_get_no_reply() {
    assert!(reply(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    assert!(reply(r#"{"jsonrpc":"2.0","id":4,"result":{}}"#).is_none());
}

#[test]
fn ping_is_an_empty_result() {
    let answer = reply(r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#).unwrap();
    assert_eq!(answer["result"], json!({}));
}

#[test]
fn tools_list_names_every_tool_with_a_schema() {
    let answer = reply(r#"{"jsonrpc":"2.0","id":3,"method":"tools/list"}"#).unwrap();
    let tools = answer["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "search_game_tree",
            "inspect_instance",
            "script_read",
            "script_search",
            "script_grep",
            "multi_edit",
            "get_studio_state",
            "list_roblox_studios"
        ]
    );
    for tool in tools {
        assert_eq!(tool["inputSchema"]["type"], "object", "{}", tool["name"]);
        // Every call names its editor, as Studio's do; listing them is how
        // a client learns the names.
        assert_eq!(
            tool["inputSchema"]["properties"].get("studio_id").is_some(),
            tool["name"] != "list_roblox_studios",
            "{}",
            tool["name"]
        );
        for required in tool["inputSchema"]["required"].as_array().unwrap() {
            let name = required.as_str().unwrap();
            assert!(
                tool["inputSchema"]["properties"].get(name).is_some(),
                "{} requires {name}, which it does not describe",
                tool["name"]
            );
        }
    }
}

#[test]
fn malformed_and_unknown_messages_are_json_rpc_errors() {
    let parse = reply("{not json").unwrap();
    assert_eq!(parse["error"]["code"], -32700);
    assert_eq!(parse["id"], Value::Null);

    let batch = reply("[]").unwrap();
    assert_eq!(batch["error"]["code"], -32600);

    let unknown = reply(r#"{"jsonrpc":"2.0","id":5,"method":"resources/list"}"#).unwrap();
    assert_eq!(unknown["error"]["code"], -32601);
    assert_eq!(unknown["id"], 5);

    let tool = reply(
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"generate_mesh"}}"#,
    )
    .unwrap();
    assert_eq!(tool["error"]["code"], -32602);
}

#[test]
fn a_tool_that_fails_says_so_in_its_result() {
    let answer = reply(
        r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"inspect_instance","arguments":{"path":"game.Workspace.Nothing"}}}"#,
    )
    .unwrap();
    assert_eq!(answer["result"]["isError"], true);
    let text = answer["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("game.Workspace.Nothing"), "{text}");
}
