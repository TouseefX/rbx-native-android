//! Splits a query into tokens, each carrying the byte span it came from so
//! that every later error can point at the exact text.

use std::ops::Range;

use super::QueryError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Like,
    Contains,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Kind {
    Ident(String),
    Number(f64),
    Str(String),
    Op(Op),
    Star,
    Dot,
    Open,
    Close,
    Minus,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Token {
    pub(super) kind: Kind,
    pub(super) span: Range<usize>,
}

fn error(message: &str, span: Range<usize>) -> QueryError {
    QueryError {
        message: message.to_string(),
        span,
    }
}

pub(super) fn lex(source: &str) -> Result<Vec<Token>, QueryError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let b = bytes[i];
        let kind = match b {
            b' ' | b'\t' | b'\n' | b'\r' => {
                i += 1;
                continue;
            }
            b'*' => {
                i += 1;
                Kind::Star
            }
            b'.' => {
                i += 1;
                Kind::Dot
            }
            b'(' => {
                i += 1;
                Kind::Open
            }
            b')' => {
                i += 1;
                Kind::Close
            }
            b'-' => {
                i += 1;
                Kind::Minus
            }
            b'~' => {
                i += 1;
                Kind::Op(Op::Like)
            }
            b'=' => {
                i += 1;
                if bytes.get(i) == Some(&b'=') {
                    i += 1;
                }
                Kind::Op(Op::Eq)
            }
            b'!' => {
                if bytes.get(i + 1) != Some(&b'=') {
                    return Err(error("unexpected \"!\"", i..i + 1));
                }
                i += 2;
                Kind::Op(Op::Ne)
            }
            b'<' | b'>' => {
                i += 1;
                let orequal = bytes.get(i) == Some(&b'=');
                if orequal {
                    i += 1;
                }
                Kind::Op(match (b, orequal) {
                    (b'<', false) => Op::Lt,
                    (b'<', true) => Op::Le,
                    (_, false) => Op::Gt,
                    (_, true) => Op::Ge,
                })
            }
            b'"' | b'\'' => {
                let (text, end) = string(source, i)
                    .ok_or_else(|| error("unterminated string", start..source.len()))?;
                i = end;
                Kind::Str(text)
            }
            b'0'..=b'9' => {
                while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
                if bytes.get(i) == Some(&b'.') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                        i += 1;
                    }
                }
                Kind::Number(source[start..i].parse().expect("digits parse as a number"))
            }
            _ if b.is_ascii_alphabetic() || b == b'_' => {
                while bytes
                    .get(i)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
                {
                    i += 1;
                }
                Kind::Ident(source[start..i].to_string())
            }
            _ => {
                let c = source[i..].chars().next().unwrap_or(' ');
                return Err(error(&format!("unexpected \"{c}\""), i..i + c.len_utf8()));
            }
        };
        tokens.push(Token {
            kind,
            span: start..i,
        });
    }
    Ok(tokens)
}

/// The text of the string opening at `open` and the offset just past its
/// closing quote, or `None` when it never closes.
fn string(source: &str, open: usize) -> Option<(String, usize)> {
    let quote = source[open..].chars().next()?;
    let mut chars = source[open + 1..].char_indices();
    let mut text = String::new();
    while let Some((offset, c)) = chars.next() {
        if c == '\\' {
            match chars.next()?.1 {
                escaped @ ('"' | '\'' | '\\') => text.push(escaped),
                other => {
                    text.push('\\');
                    text.push(other);
                }
            }
        } else if c == quote {
            return Some((text, open + 1 + offset + 1));
        } else {
            text.push(c);
        }
    }
    None
}
