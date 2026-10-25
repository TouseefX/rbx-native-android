use std::cell::RefCell;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use rbx_dom::WeakDom;
use rbx_reflection::ReflectionDatabase;
use serde_json::{json, Value};

use super::super::call;
use super::super::tests::{place, run, studio};
use crate::mcp::{Insertion, Session};
use crate::properties::Properties;

mod images;
mod insert;
mod search;

thread_local! {
    /// The mock `apis.roblox.com` this thread's calls go to.
    pub(super) static MOCK_ROOT: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// One request the mock saw: `METHOD /path?query` and its body.
pub(super) type Seen = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// Serves `route` on a loopback port for this thread's asset tools; `@` in
/// an answer stands for the server's own root, for delivery's CDN links.
pub(super) fn mock(route: impl Fn(&str, &[u8]) -> (u16, Vec<u8>) + Send + 'static) -> Seen {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    let seen: Seen = Arc::default();
    let log = seen.clone();
    let base = root.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let target = line
                .split_whitespace()
                .take(2)
                .collect::<Vec<_>>()
                .join(" ");
            let mut length = 0;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse().unwrap();
                    }
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let (status, answer) = route(&target, &body);
            log.lock().unwrap().push((target, body));
            let answer = match String::from_utf8(answer) {
                Ok(text) => text.replace('@', &base).into_bytes(),
                Err(bytes) => bytes.into_bytes(),
            };
            let mut stream = reader.into_inner();
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                answer.len()
            );
            let _ = stream.write_all(&answer);
        }
    });
    MOCK_ROOT.with(|mock| *mock.borrow_mut() = Some(root));
    seen
}

pub(super) fn json_body(value: Value) -> (u16, Vec<u8>) {
    (200, value.to_string().into_bytes())
}

pub(super) const INTROSPECT: &str =
    r#"{"name":"k","authorizedUserId":5,"scopes":[],"enabled":true,"expired":false}"#;

/// `version 1.01` text mesh spanning 4 x 2 x 6 studs.
pub(super) const MESH: &[u8] =
    b"version 1.01\n1\n[0,0,0][0,1,0][0,0,0][4,0,0][0,1,0][0,0,0][0,2,6][0,1,0][0,0,0]";

pub(super) const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

pub(super) fn model_bytes() -> Vec<u8> {
    let mut dom = WeakDom::new();
    let model = dom.new_instance("Model", "Crate", None);
    dom.new_instance("Part", "Lid", Some(model));
    rbx_binary::serialize(&dom).unwrap()
}

/// Calls `insert_asset` and runs its download, as the shell would.
pub(super) fn insert(dom: &WeakDom, arguments: Value) -> Result<Insertion, String> {
    let properties = Properties::new(ReflectionDatabase::embedded());
    let studio = studio();
    let mut session = Session::new(dom, &properties, &studio);
    let result = call(
        &json!({ "name": "insert_asset", "arguments": arguments }),
        &mut session,
    )
    .unwrap();
    match session.inserting.take() {
        Some(fetch) => fetch(),
        None => Err(result["content"][0]["text"].as_str().unwrap().to_owned()),
    }
}

/// Delivery's answer for asset 1818 of type `type_id`, holding `bytes`.
pub(super) fn deliver(type_id: u32, bytes: Vec<u8>) -> Seen {
    mock(move |target, _| match target {
        t if t.contains("/asset-delivery/") => (401, Vec::new()),
        t if t.contains("/asset-delivery-api/v1/assetId/1818") => json_body(json!({
            "location": "@/cdn/1818",
            "assetTypeId": type_id,
        })),
        t if t.contains("/cdn/1818") => (200, bytes.clone()),
        t if t.contains("/toolbox-service/v2/assets/1818") => {
            json_body(json!({ "asset": { "id": 1818, "name": "Rusty Crate" } }))
        }
        _ => (404, Vec::new()),
    })
}

#[test]
fn every_asset_tool_is_listed_with_a_studio_id() {
    let tools = super::super::list();
    for name in [
        "search_asset",
        "insert_asset",
        "upload_image",
        "store_image",
        "generate_mesh",
        "generate_material",
        "generate_procedural_model",
        "wait_job_finished",
    ] {
        let tool = tools
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("{name} is not listed"));
        assert!(tool["inputSchema"]["properties"]["studio_id"].is_object());
    }
}

#[test]
fn generation_says_why_it_cannot_run() {
    let dom = place();
    for (tool, arguments) in [
        ("generate_mesh", json!({ "textPrompt": "a crate" })),
        ("generate_procedural_model", json!({ "prompt": "a crate" })),
        (
            "generate_material",
            json!({ "materialPattern": "Regular", "materialId": "m", "baseMaterial": "Brick", "materialDescription": "red" }),
        ),
    ] {
        let (error, text) = run(&dom, tool, arguments);
        assert!(error, "{tool}");
        assert!(text.contains("not available in rbx-native"), "{text}");
    }
    let (error, text) = run(&dom, "wait_job_finished", json!({ "jobId": "j1" }));
    assert!(error);
    assert!(text.starts_with("No job found with id: j1."), "{text}");
}
