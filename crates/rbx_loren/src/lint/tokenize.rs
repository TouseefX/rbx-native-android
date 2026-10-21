//! Tokens and block structure (`lint.js` lines 1-270).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Name,
    Kw,
    Op,
    Str,
    Num,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    /// The source slice (an operator or keyword's text, a string with its quotes).
    pub value: String,
    /// Byte offsets into the source.
    pub start: usize,
    pub end: usize,
    /// 1-based.
    pub line: usize,
    /// A string's contents without its delimiters; `None` for other tokens and interpolated strings.
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Structure {
    /// The matching bracket or block end of each token, or -1.
    pub matching: Vec<i32>,
    /// The block depth at each token.
    pub depth: Vec<i32>,
}

const OPS: [&str; 17] = [
    "...", "..=", "//=", "::", "..", "==", "~=", "<=", ">=", "->", "+=", "-=", "*=", "/=", "%=",
    "^=", "//",
];

/// Source slice by byte offsets, clamped; lossy so an offset inside a character cannot panic.
fn slice(src: &[u8], a: usize, b: usize) -> String {
    let b = b.min(src.len());
    let a = a.min(b);
    String::from_utf8_lossy(&src[a..b]).into_owned()
}

fn at(src: &[u8], i: usize) -> Option<u8> {
    src.get(i).copied()
}

/// A long-bracket opener at `i` (`[`, `=`*, `[`): its level and where the body starts.
fn long_open(src: &[u8], i: usize) -> Option<(usize, usize)> {
    let mut j = i + 1;
    let mut level = 0;
    while at(src, j) == Some(b'=') {
        level += 1;
        j += 1;
    }
    (at(src, j) == Some(b'[')).then_some((level, j + 1))
}

fn skip_long(src: &[u8], from: usize, level: usize) -> usize {
    let mut close = vec![b']'];
    close.extend(std::iter::repeat_n(b'=', level));
    close.push(b']');
    let from = from.min(src.len());
    match src[from..]
        .windows(close.len())
        .position(|w| w == close.as_slice())
    {
        Some(k) => from + k + close.len(),
        None => src.len(),
    }
}

fn skip_quoted(src: &[u8], i: usize) -> usize {
    let q = src[i];
    let mut j = i + 1;
    while j < src.len() {
        let c = src[j];
        if c == b'\\' {
            j += 2;
            continue;
        }
        if c == q {
            return j + 1;
        }
        if c == b'\n' {
            return j; // unterminated: stop at the line end
        }
        j += 1;
    }
    j
}

fn skip_interp(src: &[u8], i: usize) -> usize {
    let mut j = i + 1;
    while j < src.len() {
        match src[j] {
            b'\\' => j += 2,
            b'`' => return j + 1,
            b'{' => j = skip_interp_expr(src, j + 1),
            _ => j += 1,
        }
    }
    j
}

fn skip_interp_expr(src: &[u8], mut j: usize) -> usize {
    let mut depth = 1;
    while j < src.len() {
        let c = src[j];
        if c == b'"' || c == b'\'' {
            j = skip_quoted(src, j);
            continue;
        }
        if c == b'`' {
            j = skip_interp(src, j);
            continue;
        }
        if c == b'[' {
            if let Some((level, body)) = long_open(src, j) {
                j = skip_long(src, body, level);
                continue;
            }
        }
        if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth == 0 {
                return j + 1;
            }
        }
        j += 1;
    }
    j
}

/// Maps a byte offset to its 1-based line.
fn line_locator(src: &[u8]) -> impl Fn(usize) -> usize {
    let mut starts = vec![0usize];
    starts.extend(
        src.iter()
            .enumerate()
            .filter(|(_, &b)| b == b'\n')
            .map(|(i, _)| i + 1),
    );
    move |offset| starts.partition_point(|&s| s <= offset)
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

pub fn tokenize(src: &str) -> Vec<Token> {
    let bytes = src.as_bytes();
    let line_of = line_locator(bytes);
    let mut toks = Vec::new();
    let n = bytes.len();
    let mut i = 0;
    let mut push =
        |kind: TokenKind, value: String, start: usize, end: usize, text: Option<String>| {
            toks.push(Token {
                kind,
                value,
                start,
                end,
                line: line_of(start),
                text,
            });
        };
    while i < n {
        let c = bytes[i];
        if matches!(c, b' ' | b'\t' | b'\r' | b'\n' | 0x0c | 0x0b) {
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(&[0xEF, 0xBB, 0xBF]) {
            i += 3;
            continue;
        }
        if c == b'-' && at(bytes, i + 1) == Some(b'-') {
            i += 2;
            if at(bytes, i) == Some(b'[') {
                if let Some((level, body)) = long_open(bytes, i) {
                    i = skip_long(bytes, body, level);
                    continue;
                }
            }
            while i < n && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'"' || c == b'\'' {
            let start = i;
            i = skip_quoted(bytes, i).min(n);
            let closed = bytes[i - 1] == c && i - 1 > start;
            push(
                TokenKind::Str,
                slice(bytes, start, i),
                start,
                i,
                Some(slice(bytes, start + 1, if closed { i - 1 } else { i })),
            );
            continue;
        }
        if c == b'`' {
            let start = i;
            i = skip_interp(bytes, i).min(n);
            push(TokenKind::Str, slice(bytes, start, i), start, i, None);
            continue;
        }
        if c == b'[' {
            if let Some((level, body)) = long_open(bytes, i) {
                let start = i;
                i = skip_long(bytes, body, level);
                let body_end = body.max(i.saturating_sub(level + 2));
                push(
                    TokenKind::Str,
                    slice(bytes, start, i),
                    start,
                    i,
                    Some(slice(bytes, body, body_end)),
                );
                continue;
            }
        }
        if is_ident_start(c) {
            let start = i;
            while i < n && is_ident_char(bytes[i]) {
                i += 1;
            }
            let word = slice(bytes, start, i);
            let kind = if crate::constants::is_keyword(&word) {
                TokenKind::Kw
            } else {
                TokenKind::Name
            };
            push(kind, word, start, i, None);
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && at(bytes, i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            let start = i;
            while i < n {
                let d = bytes[i];
                if (d == b'+' || d == b'-')
                    && matches!(bytes[i - 1], b'e' | b'E' | b'p' | b'P')
                    && !(bytes[start] == b'0'
                        && matches!(at(bytes, start + 1), Some(b'x' | b'X'))
                        && i > start + 1)
                {
                    i += 1;
                } else if is_ident_char(d) || d == b'.' {
                    if d == b'.' && at(bytes, i + 1) == Some(b'.') {
                        break;
                    }
                    i += 1;
                } else {
                    break;
                }
            }
            push(TokenKind::Num, slice(bytes, start, i), start, i, None);
            continue;
        }
        if let Some(op) = OPS.iter().find(|o| bytes[i..].starts_with(o.as_bytes())) {
            push(TokenKind::Op, (*op).to_string(), i, i + op.len(), None);
            i += op.len();
            continue;
        }
        let len = match c {
            0xF0.. => 4,
            0xE0.. => 3,
            0xC0.. => 2,
            _ => 1,
        }
        .min(n - i);
        push(TokenKind::Op, slice(bytes, i, i + len), i, i + len, None);
        i += len;
    }
    toks
}

// An `if` after one of these starts an if-expression (no `end`), not an if statement.
const EXPR_PREV_OPS: [&str; 26] = [
    "=", "(", "[", "{", ",", "..", "+", "-", "*", "/", "//", "%", "^", "#", "==", "~=", "<", ">",
    "<=", ">=", "+=", "-=", "*=", "/=", "//=", "%=",
];
const EXPR_PREV_OPS_MORE: [&str; 2] = ["^=", "..="];
const EXPR_PREV_KW: [&str; 9] = [
    "return", "and", "or", "not", "in", "while", "until", "elseif", "if",
];

#[derive(PartialEq, Clone, Copy)]
enum Block {
    Function,
    Do,
    Repeat,
    If,
    IfExpr,
}

pub fn structure(toks: &[Token]) -> Structure {
    let n = toks.len();
    let mut matching = vec![-1i32; n];
    let mut depths = vec![0i32; n];
    let mut brackets: Vec<usize> = Vec::new();
    let mut blocks: Vec<(Block, usize)> = Vec::new();
    let mut depth = 0;
    let mut next_if_expr = false;
    for (i, t) in toks.iter().enumerate() {
        depths[i] = depth;
        let expr_context = next_if_expr;
        next_if_expr = false;
        if t.kind == TokenKind::Op {
            if is_opener(Some(t)) {
                brackets.push(i);
            } else if matches!(t.value.as_str(), ")" | "}" | "]") {
                if let Some(open) = brackets.pop() {
                    matching[open] = i as i32;
                    matching[i] = open as i32;
                }
            }
            continue;
        }
        if t.kind != TokenKind::Kw {
            continue;
        }
        let top = blocks.last().map(|b| b.0);
        match t.value.as_str() {
            "function" | "do" | "repeat" => {
                let kind = match t.value.as_str() {
                    "function" => Block::Function,
                    "do" => Block::Do,
                    _ => Block::Repeat,
                };
                blocks.push((kind, i));
                depth += 1;
            }
            "if" => {
                let after_expr_op = i > 0 && {
                    let p = &toks[i - 1];
                    (p.kind == TokenKind::Op
                        && (EXPR_PREV_OPS.contains(&p.value.as_str())
                            || EXPR_PREV_OPS_MORE.contains(&p.value.as_str())))
                        || (p.kind == TokenKind::Kw && EXPR_PREV_KW.contains(&p.value.as_str()))
                };
                if expr_context || after_expr_op {
                    blocks.push((Block::IfExpr, i));
                } else {
                    blocks.push((Block::If, i));
                    depth += 1;
                }
            }
            "then" => {
                if top == Some(Block::IfExpr) {
                    next_if_expr = true;
                }
            }
            "else" => {
                if top == Some(Block::IfExpr) {
                    blocks.pop();
                    next_if_expr = true;
                }
            }
            "end" | "until" => {
                while blocks.last().map(|b| b.0) == Some(Block::IfExpr) {
                    blocks.pop();
                }
                if let Some((_, idx)) = blocks.pop() {
                    matching[idx] = i as i32;
                    matching[i] = idx as i32;
                    depth -= 1;
                }
            }
            _ => {}
        }
    }
    Structure {
        matching,
        depth: depths,
    }
}

pub(crate) fn is_op(t: Option<&Token>, v: &str) -> bool {
    t.is_some_and(|t| t.kind == TokenKind::Op && t.value == v)
}

pub(crate) fn is_kw(t: Option<&Token>, v: &str) -> bool {
    t.is_some_and(|t| t.kind == TokenKind::Kw && t.value == v)
}

/// A name token, of value `v` when given.
pub(crate) fn is_name(t: Option<&Token>, v: Option<&str>) -> bool {
    t.is_some_and(|t| t.kind == TokenKind::Name && v.is_none_or(|v| t.value == v))
}

pub(crate) fn is_opener(t: Option<&Token>) -> bool {
    is_op(t, "(") || is_op(t, "{") || is_op(t, "[")
}

#[cfg(test)]
mod tests;
