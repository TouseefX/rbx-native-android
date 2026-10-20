//! The editor's side of the bridge: a loopback listener, and the file that
//! tells [`super::broker`] its port, token and `studio_id`.
//!
//! Each connection gets a thread that reads one line at a time and waits for
//! the UI thread's answer before reading the next, so requests from one
//! client are answered in order and never two at once.

#[cfg(test)]
mod tests;

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use serde_json::json;

/// What an editor's `<studio_id>.json` holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Endpoint {
    pub(crate) studio_id: String,
    pub(crate) port: u16,
    pub(crate) token: String,
}

impl Endpoint {
    pub(crate) fn to_json(&self) -> String {
        json!({ "studio_id": self.studio_id, "port": self.port, "token": self.token }).to_string()
    }

    pub(crate) fn from_json(text: &str) -> Option<Endpoint> {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        Some(Endpoint {
            studio_id: value.get("studio_id")?.as_str()?.to_owned(),
            port: u16::try_from(value.get("port")?.as_u64()?).ok()?,
            token: value.get("token")?.as_str()?.to_owned(),
        })
    }

    pub(crate) fn load(path: &Path) -> Option<Endpoint> {
        Endpoint::from_json(&std::fs::read_to_string(path).ok()?)
    }
}

/// Where the editor `studio_id` names says how to reach it.
pub(crate) fn file_of(dir: &Path, studio_id: &str) -> PathBuf {
    dir.join(format!("{studio_id}.json"))
}

/// One line from a client, waiting for the UI thread's answer.
pub(crate) struct Request {
    line: String,
    reply: Sender<Option<String>>,
}

impl Request {
    pub(crate) fn line(&self) -> &str {
        &self.line
    }

    /// `None` sends nothing back, for a notification.
    pub(crate) fn answer(self, reply: Option<String>) {
        // A client that hung up meanwhile has nobody left to tell.
        let _ = self.reply.send(reply);
    }
}

/// The listener, for as long as it is kept. Dropping it stops accepting,
/// ends every connection at its next line and removes its file.
pub(crate) struct Server {
    endpoint: Endpoint,
    file: PathBuf,
    requests: Receiver<Request>,
    stopped: Arc<AtomicBool>,
}

impl Server {
    /// Listens on a port of the system's choosing, under a new `studio_id`,
    /// and writes `<dir>/<studio_id>.json`.
    pub(crate) fn start(dir: &Path) -> std::io::Result<Server> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let endpoint = Endpoint {
            studio_id: random_hex(1),
            port: listener.local_addr()?.port(),
            token: random_hex(2),
        };
        std::fs::create_dir_all(dir)?;
        let file = file_of(dir, &endpoint.studio_id);
        // Written whole and renamed, so a broker scanning the directory never
        // reads half a file.
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, endpoint.to_json())?;
        std::fs::rename(&tmp, &file)?;

        let (sender, requests) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let token = endpoint.token.clone();
        let flag = Arc::clone(&stopped);
        std::thread::Builder::new()
            .name("mcp-listener".to_owned())
            .spawn(move || accept(&listener, &token, &sender, &flag))?;
        Ok(Server {
            endpoint,
            file,
            requests,
            stopped,
        })
    }

    pub(crate) fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Everything clients sent since the last call.
    pub(crate) fn drain(&self) -> Vec<Request> {
        self.requests.try_iter().collect()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        // `accept` blocks; a connection of our own is what wakes it to see
        // the flag.
        let _ = TcpStream::connect((Ipv4Addr::LOCALHOST, self.endpoint.port));
        let _ = std::fs::remove_file(&self.file);
    }
}

fn accept(listener: &TcpListener, token: &str, sender: &Sender<Request>, stopped: &AtomicBool) {
    for stream in listener.incoming() {
        if stopped.load(Ordering::SeqCst) {
            return;
        }
        let Ok(stream) = stream else {
            continue;
        };
        let token = token.to_owned();
        let sender = sender.clone();
        let _ = std::thread::Builder::new()
            .name("mcp-connection".to_owned())
            .spawn(move || serve(stream, &token, &sender));
    }
}

fn serve(stream: TcpStream, token: &str, sender: &Sender<Request>) {
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut lines = BufReader::new(stream).lines();
    if !matches!(lines.next(), Some(Ok(first)) if first.trim_end() == token) {
        let _ = writer.shutdown(Shutdown::Both);
        return;
    }
    for line in lines {
        let Ok(line) = line else {
            return;
        };
        if line.trim().is_empty() {
            continue;
        }
        let (reply, answer) = mpsc::channel();
        if sender.send(Request { line, reply }).is_err() {
            return;
        }
        // The server is gone if this fails: nothing will ever answer.
        let Ok(answer) = answer.recv() else {
            return;
        };
        if let Some(text) = answer {
            if writeln!(writer, "{text}")
                .and_then(|()| writer.flush())
                .is_err()
            {
                return;
            }
        }
    }
}

/// `words` times 64 bits as hex, from the standard library's random hash
/// keys, so no RNG crate is needed for the token and the id this makes.
fn random_hex(words: usize) -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::BuildHasher;
    (0..words)
        .map(|_| format!("{:016x}", RandomState::new().hash_one(std::process::id())))
        .collect()
}
