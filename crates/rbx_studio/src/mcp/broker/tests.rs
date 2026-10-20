use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use rbx_dom::{Variant, WeakDom};
use rbx_reflection::ReflectionDatabase;
use serde_json::{json, Value};

use super::{Broker, NOT_RUNNING};
use crate::mcp::endpoint::{file_of, Endpoint, Server};
use crate::mcp::{apply_edit, protocol, PlayState, Session, Studio};
use crate::properties::Properties;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rbx-native-mcp-broker-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// An editor serving a one-script place on a thread of its own, answering
/// exactly as `shell::mcp` does: a planned edit is applied before the reply
/// goes back. Stops, and takes its file back, when dropped.
struct Editor {
    id: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Editor {
    fn start(dir: &Path, name: &str) -> Editor {
        let server = Server::start(dir).unwrap();
        let id = server.endpoint().studio_id.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let studio = Studio {
            id: id.clone(),
            name: name.to_owned(),
            place_id: None,
            play: PlayState::Edit,
        };
        let flag = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let mut dom = WeakDom::new();
            let service = dom.new_instance("ServerScriptService", "ServerScriptService", None);
            let main = dom.new_instance("Script", "Main", Some(service));
            let text = format!("print(\"{}\")\n", studio.name);
            dom.set_property(main, "Source", Variant::String(text))
                .unwrap();
            let properties = Properties::new(ReflectionDatabase::embedded());
            while !flag.load(Ordering::SeqCst) {
                for request in server.drain() {
                    let mut session = Session::new(&dom, &properties, &studio);
                    let reply = protocol::handle(request.line(), &mut session);
                    if let Some(edit) = session.planned.take() {
                        apply_edit(&mut dom, &edit);
                    }
                    request.answer(reply);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        Editor {
            id,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

fn request(broker: &mut Broker, method: &str, params: Value) -> Value {
    let line = json!({ "jsonrpc": "2.0", "id": 9, "method": method, "params": params });
    let reply = broker
        .handle(&line.to_string())
        .expect("a request is answered");
    serde_json::from_str(&reply).unwrap()
}

/// Whether the tool failed, and its text.
fn tool(broker: &mut Broker, name: &str, arguments: Value) -> (bool, String) {
    let reply = request(
        broker,
        "tools/call",
        json!({ "name": name, "arguments": arguments }),
    );
    assert_eq!(reply["id"], 9, "{reply}");
    let result = &reply["result"];
    (
        result["isError"].as_bool().unwrap(),
        result["content"][0]["text"].as_str().unwrap().to_owned(),
    )
}

fn studios(broker: &mut Broker) -> Value {
    let (failed, text) = tool(broker, "list_roblox_studios", json!({}));
    assert!(!failed, "{text}");
    serde_json::from_str(&text).unwrap()
}

const MAIN: &str = "game.ServerScriptService.Main";

#[test]
fn with_no_editor_the_protocol_still_answers_and_tools_say_why_not() {
    let mut broker = Broker::new(scratch("empty"));
    let init = request(
        &mut broker,
        "initialize",
        json!({ "protocolVersion": "2025-06-18" }),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "rbx-native");
    let list = request(&mut broker, "tools/list", json!({}));
    assert!(list["result"]["tools"].as_array().unwrap().len() > 5);
    assert_eq!(studios(&mut broker), json!([]));
    assert_eq!(
        tool(&mut broker, "script_read", json!({ "path": MAIN })),
        (true, NOT_RUNNING.to_owned())
    );
    assert!(broker
        .handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
        .is_none());
    let unknown = request(&mut broker, "resources/list", json!({}));
    assert_eq!(unknown["error"]["code"], -32601);
}

#[test]
fn one_editor_is_used_without_naming_it() {
    let dir = scratch("one");
    let editor = Editor::start(&dir, "Arena");
    let mut broker = Broker::new(dir);
    assert_eq!(
        studios(&mut broker),
        json!([{ "studio_id": editor.id, "name": "Arena" }])
    );
    let (failed, text) = tool(&mut broker, "script_read", json!({ "path": MAIN }));
    assert!(!failed, "{text}");
    assert_eq!(text, "1\tprint(\"Arena\")");
}

#[test]
fn with_two_editors_each_call_goes_to_the_one_it_names() {
    let dir = scratch("two");
    let arena = Editor::start(&dir, "Arena");
    let lobby = Editor::start(&dir, "Lobby");
    let mut broker = Broker::new(dir);

    let listed = studios(&mut broker);
    let mut names: Vec<&str> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["Arena", "Lobby"]);

    let (failed, text) = tool(&mut broker, "script_read", json!({ "path": MAIN }));
    assert!(failed);
    assert!(text.contains("2 editors"), "{text}");

    let (failed, text) = tool(
        &mut broker,
        "multi_edit",
        json!({
            "studio_id": lobby.id,
            "path": MAIN,
            "edits": [{ "old_string": "Lobby", "new_string": "Lobby, edited" }],
        }),
    );
    assert!(!failed, "{text}");
    let read = |broker: &mut Broker, id: &str| {
        tool(
            broker,
            "script_read",
            json!({ "path": MAIN, "studio_id": id }),
        )
        .1
    };
    assert_eq!(read(&mut broker, &lobby.id), "1\tprint(\"Lobby, edited\")");
    assert_eq!(
        read(&mut broker, &arena.id),
        "1\tprint(\"Arena\")",
        "the other editor's place is left alone"
    );
}

#[test]
fn an_editor_that_closed_or_never_was_is_reported_and_its_file_cleared() {
    let dir = scratch("gone");
    let editor = Editor::start(&dir, "Arena");
    let id = editor.id.clone();
    let mut broker = Broker::new(dir.clone());
    assert!(
        !tool(
            &mut broker,
            "script_read",
            json!({ "path": MAIN, "studio_id": id })
        )
        .0
    );
    drop(editor);
    let (failed, text) = tool(
        &mut broker,
        "script_read",
        json!({ "path": MAIN, "studio_id": id }),
    );
    assert!(failed);
    assert!(text.contains(&id), "{text}");

    // An editor killed outright leaves its file behind, pointing at a port
    // nothing listens on any more.
    let port = {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.local_addr().unwrap().port()
    };
    let stale = Endpoint {
        studio_id: "deadbeef".to_owned(),
        port,
        token: "t".to_owned(),
    };
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(file_of(&dir, &stale.studio_id), stale.to_json()).unwrap();
    assert_eq!(studios(&mut broker), json!([]));
    assert!(!file_of(&dir, &stale.studio_id).exists());
}
