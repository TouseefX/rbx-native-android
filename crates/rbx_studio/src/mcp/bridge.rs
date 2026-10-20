//! `rbxstudio --mcp`: the process an MCP client launches. It forwards stdin
//! to the running editor and the editor's answers to stdout, and holds no
//! state of its own, so the client's session is the editor's.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpStream};
use std::path::Path;

use super::endpoint::Endpoint;

const NOT_RUNNING: &str = "rbxstudio --mcp: no editor is serving MCP. Open a place in rbxstudio \
with Settings › Beta features › MCP server switched on, then start this again.";

/// Runs the bridge until either side hangs up; the process's exit code.
pub(crate) fn run() -> i32 {
    let Some(file) = super::endpoint_file() else {
        eprintln!("rbxstudio --mcp: no configuration directory to find the editor through");
        return 1;
    };
    let stream = match connect(&file) {
        Ok(stream) => stream,
        Err(err) => {
            eprintln!("{NOT_RUNNING} ({err})");
            return 1;
        }
    };
    let Ok(upstream) = stream.try_clone() else {
        eprintln!("rbxstudio --mcp: the connection to the editor could not be shared");
        return 1;
    };
    std::thread::spawn(move || {
        pump(std::io::stdin().lock(), &upstream);
        // The client closed stdin: let the editor see the end too, which
        // ends the copy below once it has answered.
        let _ = upstream.shutdown(Shutdown::Write);
    });
    pump(&stream, std::io::stdout().lock());
    0
}

/// Connects to the editor `file` names and introduces itself with the token.
pub(crate) fn connect(file: &Path) -> std::io::Result<TcpStream> {
    let endpoint = Endpoint::load(file).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} is missing or unreadable", file.display()),
        )
    })?;
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, endpoint.port))?;
    writeln!(stream, "{}", endpoint.token)?;
    stream.flush()?;
    Ok(stream)
}

/// Copies until the end of `from`, flushing each read, since a reply that
/// sat in a buffer would leave the client waiting.
fn pump(mut from: impl Read, mut to: impl Write) {
    let mut buffer = [0u8; 16 * 1024];
    loop {
        match from.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => {
                if to
                    .write_all(&buffer[..read])
                    .and_then(|()| to.flush())
                    .is_err()
                {
                    return;
                }
            }
        }
    }
}
