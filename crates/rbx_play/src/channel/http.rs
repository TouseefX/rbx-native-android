//! The HTTP/1.1 wire format of the channel: reading a request's head and
//! writing a reply.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use crate::protocol::TOKEN_HEADER;

const MAX_HEAD: usize = 16 * 1024;

pub(super) struct Request {
    pub(super) method: String,
    pub(super) target: String,
    pub(super) token: Option<String>,
    pub(super) content_length: usize,
}

pub(super) enum Head {
    Ok(Request),
    Bad,
}

pub(super) fn read_head(reader: &mut BufReader<TcpStream>) -> std::io::Result<Head> {
    let mut total = 0;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    total += line.len();
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Ok(Head::Bad);
    };
    let mut req = Request {
        method: method.to_owned(),
        target: target.to_owned(),
        token: None,
        content_length: 0,
    };
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        total += n;
        if n == 0 || total > MAX_HEAD {
            return Ok(Head::Bad);
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            return Ok(Head::Ok(req));
        }
        let Some((name, value)) = trimmed.split_once(':') else {
            return Ok(Head::Bad);
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            match value.parse() {
                Ok(n) => req.content_length = n,
                Err(_) => return Ok(Head::Bad),
            }
        } else if name.eq_ignore_ascii_case(TOKEN_HEADER) {
            req.token = Some(value.to_owned());
        }
    }
}

pub(super) fn reply(
    stream: &mut TcpStream,
    status: u16,
    body: &str,
    json: bool,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        413 => "Payload Too Large",
        _ => "Error",
    };
    let ctype = if json {
        "application/json"
    } else {
        "text/plain"
    };
    let msg = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(msg.as_bytes())?;
    stream.flush()
}
