//! Declaration readers: how one `Key = value` declaration of a Service
//! (`Client`, `Middleware`, `Spec`, ...) is recorded into a `LuauInfo`.

use super::analyze::{ClientMethod, LuauInfo, NameList, Named, ServiceKey};
use super::syntax::{table_fields, table_open_of, value_kind};
use super::tokenize::{is_kw, is_name, is_op, is_opener, Structure, Token, TokenKind};

pub(super) const DECL_KEYS: [&str; 6] = [
    "Client",
    "Middleware",
    "Spec",
    "Signals",
    "ClientEvents",
    "Dependencies",
];
const SPEC_CALLS: [&str; 3] = ["Method", "Event", "Signal"];

/// Pushes the table's plain string entries; false when anything else is in it.
fn string_entries(toks: &[Token], s: &Structure, open_idx: usize, into: &mut Vec<Named>) -> bool {
    let mut known = true;
    for f in table_fields(toks, s, open_idx) {
        let t = &toks[f.val_start];
        match (&f.key, &t.text) {
            (None, Some(text)) if f.val_end - f.val_start == 1 && t.kind == TokenKind::Str => {
                into.push(Named {
                    name: text.clone(),
                    line: t.line,
                });
            }
            _ => known = false,
        }
    }
    known
}

/// Spec values in 2.0 are calls such as Loren.Method(...), T.Event(...) or Signal(...).
fn looks_modern_spec(toks: &[Token], s: &Structure, open_idx: usize) -> bool {
    table_fields(toks, s, open_idx).iter().all(|f| {
        if f.key.is_none() {
            return false;
        }
        let mut chain: Vec<&str> = Vec::new();
        let mut k = f.val_start;
        while k < f.val_end && (is_name(toks.get(k), None) || is_op(toks.get(k), ".")) {
            if is_name(toks.get(k), None) {
                chain.push(&toks[k].value);
            }
            k += 1;
        }
        chain.last().is_some_and(|l| SPEC_CALLS.contains(l)) && is_op(toks.get(k), "(")
    })
}

fn read_names(
    toks: &[Token],
    s: &Structure,
    open: Option<usize>,
    kind: &str,
    bucket: &mut NameList,
) {
    match open {
        None => {
            if kind != "literal" {
                bucket.known = false;
            }
        }
        Some(o) => {
            if !string_entries(toks, s, o, &mut bucket.names) {
                bucket.known = false;
            }
        }
    }
}

/// Records one declaration `Key = <value>` (a module-table field or a `X.Key = ...` statement).
pub(super) fn record_decl(
    info: &mut LuauInfo,
    toks: &[Token],
    s: &Structure,
    key: &str,
    key_tok: &Token,
    val_start: usize,
    val_end: usize,
) {
    let kind = value_kind(toks, val_start, val_end);
    let open = if kind == "table" {
        table_open_of(toks, val_start, val_end)
    } else {
        None
    };
    let empty = open.is_some_and(|o| s.matching[o] == o as i32 + 1);
    match key {
        "Client" => {
            let Some(open) = open else {
                if !(kind == "literal" && is_kw(toks.get(val_start), "nil")) {
                    info.client.known = false;
                }
                info.client.declared = info.client.declared || kind != "literal";
                return;
            };
            for f in table_fields(toks, s, open) {
                let Some(name) = f.key else { continue };
                info.client.declared = true;
                let vk = value_kind(toks, f.val_start, f.val_end);
                if vk == "function" || vk == "expr" {
                    let line = f.key_idx.map_or(key_tok.line, |i| toks[i].line);
                    info.client.methods.push(ClientMethod {
                        name,
                        line,
                        kind: vk.to_string(),
                    });
                }
            }
        }
        "Middleware" => {
            let Some(open) = open else { return };
            info.middleware_tables.push(open);
            for f in table_fields(toks, s, open) {
                if let (Some(name), Some(i)) = (f.key, f.key_idx) {
                    info.middleware.push(Named {
                        name,
                        line: toks[i].line,
                    });
                }
            }
        }
        "Spec" => {
            let unknown = open.is_none() && kind != "literal";
            if unknown {
                info.spec_unknown = true;
            }
            if let Some(o) = open {
                for f in table_fields(toks, s, o) {
                    if let (Some(name), Some(i)) = (f.key, f.key_idx) {
                        info.spec.push(Named {
                            name,
                            line: toks[i].line,
                        });
                    }
                }
            }
            let modern = open.is_some_and(|o| looks_modern_spec(toks, s, o));
            info.service_keys.push(ServiceKey {
                key: key.to_string(),
                line: key_tok.line,
                empty: empty || kind == "literal",
                unknown,
                modern,
            });
        }
        "Signals" => read_names(toks, s, open, kind, &mut info.signals),
        "Dependencies" => read_names(toks, s, open, kind, &mut info.dependencies),
        "ClientEvents" => {
            read_names(toks, s, open, kind, &mut info.events);
            let modern = open.is_some_and(|o| {
                table_fields(toks, s, o)
                    .iter()
                    .all(|f| f.key.is_none() && toks[f.val_start].kind == TokenKind::Str)
            });
            info.service_keys.push(ServiceKey {
                key: key.to_string(),
                line: key_tok.line,
                empty: empty || kind == "literal",
                unknown: open.is_none() && kind != "literal",
                modern,
            });
        }
        _ => {}
    }
}

pub(super) fn statement_end(toks: &[Token], s: &Structure, start: usize) -> usize {
    let mut j = start;
    let mut need_value = true;
    while j < toks.len() {
        let t = &toks[j];
        if is_op(Some(t), ";") {
            break;
        }
        if is_kw(Some(t), "and") || is_kw(Some(t), "or") || is_kw(Some(t), "not") {
            need_value = true;
            j += 1;
            continue;
        }
        if !need_value && matches!(t.kind, TokenKind::Name | TokenKind::Num | TokenKind::Kw) {
            break;
        }
        let m = s.matching[j];
        if m > j as i32 && (is_opener(Some(t)) || is_kw(Some(t), "function")) {
            j = m as usize + 1;
            need_value = false;
            continue;
        }
        need_value =
            t.kind == TokenKind::Op && !matches!(t.value.as_str(), ")" | "}" | "]" | "...");
        j += 1;
    }
    j
}
