use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{Endpoint, Server};
use crate::mcp::bridge;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rbx-native-mcp-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("mcp.json")
}

/// Answers whatever arrives, the way the shell's timer does, until `done`.
fn answer_until(server: &Server, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() && Instant::now() < deadline {
        for request in server.drain() {
            let reply = format!("echo {}", request.line());
            request.answer(Some(reply));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_endpoint_file_round_trips_and_rejects_what_it_cannot_read() {
    let endpoint = Endpoint {
        port: 51234,
        token: "abc".to_owned(),
    };
    assert_eq!(Endpoint::from_json(&endpoint.to_json()), Some(endpoint));
    assert_eq!(Endpoint::from_json(r#"{"port":70000,"token":"x"}"#), None);
    assert_eq!(Endpoint::from_json("nonsense"), None);
}

#[test]
fn a_client_with_the_token_is_answered_line_by_line() {
    let file = scratch("answered");
    let server = Server::start(file.clone()).unwrap();
    assert_eq!(Endpoint::load(&file).as_ref(), Some(server.endpoint()));
    assert_eq!(server.endpoint().token.len(), 32);

    let stream = bridge::connect(&file).unwrap();
    let client = std::thread::spawn(move || {
        let mut writer = stream.try_clone().unwrap();
        writeln!(writer, "first\n\nsecond").unwrap();
        let mut lines = BufReader::new(stream).lines();
        [
            lines.next().unwrap().unwrap(),
            lines.next().unwrap().unwrap(),
        ]
    });
    answer_until(&server, || client.is_finished());
    assert_eq!(client.join().unwrap(), ["echo first", "echo second"]);
}

#[test]
fn a_notification_sends_nothing_back() {
    let file = scratch("silent");
    let server = Server::start(file.clone()).unwrap();
    let stream = bridge::connect(&file).unwrap();
    let client = std::thread::spawn(move || {
        let mut writer = stream.try_clone().unwrap();
        writeln!(writer, "quiet\nloud").unwrap();
        BufReader::new(stream).lines().next().unwrap().unwrap()
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while !client.is_finished() && Instant::now() < deadline {
        for request in server.drain() {
            let reply = (request.line() == "loud").then(|| "heard".to_owned());
            request.answer(reply);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(client.join().unwrap(), "heard");
}

#[test]
fn a_client_without_the_token_is_hung_up_on_unheard() {
    let file = scratch("refused");
    let server = Server::start(file).unwrap();
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", server.endpoint().port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    writeln!(stream, "not-the-token\nhello").unwrap();
    let mut rest = Vec::new();
    let read = stream.read_to_end(&mut rest);
    assert!(read.is_err() || rest.is_empty(), "{read:?} {rest:?}");
    assert!(server.drain().is_empty());
}

#[test]
fn dropping_the_server_takes_its_file_back_but_not_a_newer_editors() {
    let file = scratch("dropped");
    let server = Server::start(file.clone()).unwrap();
    drop(server);
    assert!(!file.exists());

    let older = Server::start(file.clone()).unwrap();
    let newer = Server::start(file.clone()).unwrap();
    drop(older);
    assert_eq!(Endpoint::load(&file).as_ref(), Some(newer.endpoint()));
    drop(newer);
    assert!(!file.exists());
}
