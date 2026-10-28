use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use super::*;

fn read_request(stream: &mut TcpStream) -> String {
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = stream.read(&mut buf).unwrap_or(0);
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&data).to_string();
        if let Some(end) = text.find("\r\n\r\n") {
            let length = text[..end]
                .lines()
                .find_map(|l| {
                    let l = l.to_lowercase();
                    l.strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                })
                .unwrap_or(0);
            if data.len() >= end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&data).to_string()
}

fn answer(stream: &mut TcpStream, status: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn serve(
    answers: Vec<(&'static str, &'static str)>,
) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for (status, body) in answers {
            let (mut stream, _) = listener.accept().unwrap();
            seen.push(read_request(&mut stream));
            answer(&mut stream, status, body);
        }
        seen
    });
    (root, handle)
}

fn client(root: String) -> rbx_cloud::Client {
    rbx_cloud::Client::new(Some(rbx_cloud::ApiKey::new("k"))).with_apis_root(root)
}

#[test]
fn the_creator_hub_url_names_the_asset() {
    assert_eq!(
        creator_hub_url(42),
        "https://create.roblox.com/dashboard/creations/store/42/configure"
    );
}

#[test]
fn a_group_upload_posts_the_model_and_returns_the_new_id() {
    let (root, handle) = serve(vec![
        ("200 OK", r#"{"path":"operations/u1","done":false}"#),
        ("200 OK", r#"{"done":true,"response":{"assetId":"901"}}"#),
    ]);
    let payload = Payload::Rbxm(b"<roblox!".to_vec());
    let id = send_with(
        &client(root),
        5,
        &payload,
        "Hat",
        "A hat",
        Creator::Group(7),
    )
    .unwrap();
    assert_eq!(id, 901);
    let seen = handle.join().unwrap();
    assert!(
        seen[0].starts_with("POST /assets/v1/assets "),
        "{}",
        seen[0]
    );
    assert!(seen[0].contains("\"assetType\":\"Model\""), "{}", seen[0]);
    assert!(seen[0].contains("\"groupId\":\"7\""), "{}", seen[0]);
    assert!(seen[0].contains("model/x-rbxm"), "{}", seen[0]);
    assert!(
        seen[1].starts_with("GET /assets/v1/operations/u1 "),
        "{}",
        seen[1]
    );
}

#[test]
fn a_refused_key_says_which_permission_is_missing() {
    let (root, handle) = serve(vec![("403 Forbidden", r#"{"message":"Forbidden"}"#)]);
    let payload = Payload::Rbxm(vec![1, 2, 3]);
    let err = send_with(&client(root), 5, &payload, "Hat", "", Creator::Me).unwrap_err();
    assert!(err.contains("asset:write"), "{err}");
    handle.join().unwrap();
}
