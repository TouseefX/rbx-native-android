//! Reading tables, parameter lists and `self` uses off the tokens (`lint.js` lines 272-470).

use super::tokenize::{is_kw, is_name, is_op, is_opener, tokenize, Structure, Token, TokenKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Field {
    /// `None` for a positional entry.
    pub key: Option<String>,
    pub key_idx: Option<usize>,
    pub val_start: usize,
    /// Exclusive.
    pub val_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Param {
    pub first: usize,
    pub last: usize,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParamList {
    pub open: usize,
    pub close: usize,
    pub params: Vec<Param>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Chain {
    pub parts: Vec<String>,
    pub first: usize,
}

fn mat(s: &Structure, i: usize) -> i32 {
    s.matching.get(i).copied().unwrap_or(-1)
}

/// Direct fields of the table constructor whose `{` is at `open_idx`.
pub(crate) fn table_fields(toks: &[Token], s: &Structure, open_idx: usize) -> Vec<Field> {
    let close = mat(s, open_idx);
    if close < 0 {
        return Vec::new();
    }
    let close = close as usize;
    let mut fields = Vec::new();
    let mut i = open_idx + 1;
    while i < close {
        let mut key = None;
        let mut key_idx = None;
        let mut val_start = i;
        let t = &toks[i];
        if (t.kind == TokenKind::Name || t.kind == TokenKind::Kw) && is_op(toks.get(i + 1), "=") {
            key = Some(t.value.clone());
            key_idx = Some(i);
            val_start = i + 2;
        } else if is_op(Some(t), "[")
            && toks
                .get(i + 1)
                .is_some_and(|n| n.kind == TokenKind::Str && n.text.is_some())
            && is_op(toks.get(i + 2), "]")
            && is_op(toks.get(i + 3), "=")
        {
            key = toks[i + 1].text.clone();
            key_idx = Some(i + 1);
            val_start = i + 4;
        }
        let mut j = val_start;
        while j < close && !is_op(toks.get(j), ",") && !is_op(toks.get(j), ";") {
            let m = mat(s, j);
            if m > j as i32 && (is_opener(toks.get(j)) || is_kw(toks.get(j), "function")) {
                j = m as usize + 1;
                continue;
            }
            j += 1;
        }
        if j > val_start || key.is_some() {
            fields.push(Field {
                key,
                key_idx,
                val_start,
                val_end: j,
            });
        }
        i = j + 1;
    }
    fields
}

/// `valueKind`.
pub(crate) fn value_kind(toks: &[Token], start: usize, end: usize) -> &'static str {
    let mut k = start;
    while k < end && is_op(toks.get(k), "(") {
        k += 1;
    }
    let t = match toks.get(k) {
        Some(t) if k < end => t,
        _ => return "expr",
    };
    if is_kw(Some(t), "function") {
        return "function";
    }
    if is_op(Some(t), "{") {
        return "table";
    }
    if t.kind == TokenKind::Str
        || t.kind == TokenKind::Num
        || is_kw(Some(t), "nil")
        || is_kw(Some(t), "true")
        || is_kw(Some(t), "false")
    {
        return if end - k == 1 || is_op(toks.get(k + 1), "::") || is_op(toks.get(k + 1), ")") {
            "literal"
        } else {
            "expr"
        };
    }
    "expr"
}

/// `tableOpenOf`.
pub(crate) fn table_open_of(toks: &[Token], start: usize, end: usize) -> Option<usize> {
    let mut k = start;
    while k < end && is_op(toks.get(k), "(") {
        k += 1;
    }
    is_op(toks.get(k), "{").then_some(k)
}

/// The parameters of the `function` keyword at `fi`.
pub(crate) fn param_list(toks: &[Token], s: &Structure, fi: usize) -> Option<ParamList> {
    let mut k = fi + 1;
    while k < toks.len() && !is_op(toks.get(k), "(") {
        let t = &toks[k];
        let ok = t.kind == TokenKind::Name
            || [".", ":", "<", ">", ","].iter().any(|v| is_op(Some(t), v));
        if !ok {
            return None;
        }
        k += 1;
    }
    let open = k;
    if open >= toks.len() {
        return None;
    }
    let close = mat(s, open);
    if close < 0 {
        return None;
    }
    let close = close as usize;
    let mut params = Vec::new();
    let mut start = open + 1;
    let mut angle = 0i32;
    let mut j = open + 1;
    while j <= close {
        let t = toks.get(j);
        if j < close {
            if is_opener(t) && mat(s, j) > j as i32 {
                j = mat(s, j) as usize + 1;
                continue;
            }
            if is_op(t, "<") {
                angle += 1;
            } else if is_op(t, ">") {
                angle = (angle - 1).max(0);
            }
        }
        if j == close || (is_op(t, ",") && angle == 0) {
            if j > start {
                params.push(Param {
                    first: start,
                    last: j - 1,
                    name: toks[start].value.clone(),
                });
            }
            start = j + 1;
        }
        j += 1;
    }
    Some(ParamList {
        open,
        close,
        params,
    })
}

pub(crate) fn binds_self(toks: &[Token], s: &Structure, fi: usize) -> bool {
    let mut k = fi + 1;
    while k < toks.len() && !is_op(toks.get(k), "(") {
        if is_op(toks.get(k), ":") {
            return true;
        }
        if k - fi > 64 {
            break;
        }
        k += 1;
    }
    param_list(toks, s, fi).is_some_and(|pl| pl.params.iter().any(|p| p.name == "self"))
}

/// Index just past the `}` closing an interpolation whose body starts at `i`.
fn skip_interp_expr(raw: &str, mut i: usize) -> usize {
    let b = raw.as_bytes();
    let mut depth = 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 1,
            q @ (b'"' | b'\'') => {
                i += 1;
                while i < b.len() && b[i] != q {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    b.len()
}

/// The `{...}` expressions of an interpolated string token (its raw text, backticks included).
fn interp_expressions(raw: &str) -> Vec<String> {
    let b = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 1;
    while i < b.len() {
        match b[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'`' => break,
            b'{' => {
                let end = skip_interp_expr(raw, i + 1);
                let stop = (i + 1).max(end.saturating_sub(1)).min(b.len());
                out.push(String::from_utf8_lossy(&b[(i + 1).min(b.len())..stop]).into_owned());
                i = end;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

fn reads_self(toks: &[Token], k: usize) -> bool {
    let t = &toks[k];
    if t.kind == TokenKind::Str && t.text.is_none() && interp_uses_self(&t.value) {
        return true;
    }
    let prev = k.checked_sub(1).and_then(|p| toks.get(p));
    is_name(Some(t), Some("self")) && !is_op(prev, ".") && !is_op(prev, ":")
}

fn interp_uses_self(raw: &str) -> bool {
    interp_expressions(raw).iter().any(|expr| {
        let toks = tokenize(expr);
        (0..toks.len()).any(|k| reads_self(&toks, k))
    })
}

/// Where the statement whose expression starts at `k` ends (exclusive).
fn statement_end(toks: &[Token], s: &Structure, mut k: usize) -> usize {
    const STOPS: [&str; 12] = [
        "end", "local", "return", "for", "while", "repeat", "until", "do", "break", "else",
        "elseif", "then",
    ];
    let first = k;
    while k < toks.len() {
        let t = &toks[k];
        if is_op(Some(t), ";") || (t.kind == TokenKind::Kw && STOPS.contains(&t.value.as_str())) {
            break;
        }
        // A name on a new line right after a finished operand begins the next statement.
        if k > first && t.kind == TokenKind::Name {
            let p = &toks[k - 1];
            let operand_end = matches!(p.kind, TokenKind::Name | TokenKind::Num | TokenKind::Str)
                || is_op(Some(p), ")")
                || is_op(Some(p), "}")
                || is_op(Some(p), "]")
                || ["true", "false", "nil", "end"]
                    .iter()
                    .any(|v| is_kw(Some(p), v));
            if operand_end && p.line < t.line {
                break;
            }
        }
        let m = mat(s, k);
        if m > k as i32 && (is_opener(Some(t)) || is_kw(Some(t), "function")) {
            k = m as usize;
        }
        k += 1;
    }
    k
}

/// After `local <names>` (at `k`: an optional `: type`, then `=`): does the value read `self`?
fn rhs_uses_self(toks: &[Token], s: &Structure, mut k: usize) -> bool {
    while k < toks.len() && !is_op(toks.get(k), "=") && toks[k].kind != TokenKind::Kw {
        if is_opener(toks.get(k)) && mat(s, k) > k as i32 {
            k = mat(s, k) as usize;
        }
        k += 1;
    }
    if !is_op(toks.get(k), "=") {
        return false;
    }
    let stop = statement_end(toks, s, k + 1).min(toks.len());
    (k + 1..stop).any(|x| reads_self(toks, x))
}

/// `bodyUsesSelf`; `skip` is the parameter declaring `self` itself, if any.
pub(crate) fn body_uses_self(
    toks: &[Token],
    s: &Structure,
    fi: usize,
    params_open: usize,
    skip: Option<&Param>,
) -> bool {
    let end = mat(s, fi);
    if end < 0 {
        return true;
    }
    let end = end as usize;
    let body_depth = s.depth.get(fi).copied().unwrap_or(0) + 1;
    let mut j = params_open + 1;
    while j < end {
        if skip.is_some_and(|p| j >= p.first && j <= p.last) {
            j += 1;
            continue;
        }
        let t = &toks[j];
        if is_kw(Some(t), "function") {
            if binds_self(toks, s, j) && mat(s, j) > j as i32 {
                j = mat(s, j) as usize;
            }
            j += 1;
            continue;
        }
        if t.kind == TokenKind::Str && t.text.is_none() {
            if interp_uses_self(&t.value) {
                return true;
            }
            j += 1;
            continue;
        }
        if is_kw(Some(t), "local") {
            let mut k = j + 1;
            let mut shadows = false;
            while is_name(toks.get(k), None) || is_op(toks.get(k), ",") {
                if is_name(toks.get(k), Some("self")) && s.depth.get(j).copied() == Some(body_depth)
                {
                    shadows = true;
                }
                k += 1;
            }
            if shadows {
                return rhs_uses_self(toks, s, k);
            }
            j = k;
            continue;
        }
        if is_name(Some(t), Some("self")) {
            let p = toks.get(j - 1);
            let skipped = is_op(p, ".")
                || is_op(p, ":")
                || (is_op(toks.get(j + 1), "=")
                    && (is_op(p, "{") || is_op(p, ",") || is_op(p, ";")));
            if !skipped {
                return true;
            }
        }
        j += 1;
    }
    false
}

/// The dotted name chain ending right before `end_idx`.
pub(crate) fn chain_before(toks: &[Token], end_idx: usize) -> Option<Chain> {
    let at = |k: isize| usize::try_from(k).ok().and_then(|k| toks.get(k));
    let mut k = end_idx as isize - 1;
    if !is_name(at(k), None) {
        return None;
    }
    let mut parts = vec![at(k)?.value.clone()];
    k -= 1;
    while is_op(at(k), ".") && is_name(at(k - 1), None) {
        parts.insert(0, at(k - 1)?.value.clone());
        k -= 2;
    }
    if is_op(at(k), ".") || is_op(at(k), ":") {
        return None;
    }
    Some(Chain {
        parts,
        first: (k + 1) as usize,
    })
}
