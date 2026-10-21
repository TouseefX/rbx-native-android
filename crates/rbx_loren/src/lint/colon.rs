//! Colon-style middleware detection: finds `Middleware:Name` and `self`-taking
//! middleware and builds the dot-style rewrite for each.

use std::sync::LazyLock;

use regex::Regex;

use super::scan::{apply_edits, Edit, Issue, Severity};
use super::syntax::{body_uses_self, chain_before, param_list, table_fields, ParamList};
use super::tokenize::{is_kw, is_name, is_op, Structure, Token};

static CHAIN_BREAK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\n\s*([.:])(\w)").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

fn collapse(text: &str) -> String {
    let joined = CHAIN_BREAK.replace_all(text, "${1}${2}");
    let one_line = SPACES.replace_all(&joined, " ");
    one_line
        .replace("( ", "(")
        .replace(" )", ")")
        .trim()
        .to_string()
}

fn at(toks: &[Token], i: usize, back: usize) -> Option<&Token> {
    i.checked_sub(back).and_then(|j| toks.get(j))
}

fn drop_self_edit(toks: &[Token], pl: &ParamList) -> Edit {
    if pl.params.len() == 1 {
        return Edit {
            start: toks[pl.open].end,
            end: toks[pl.close].start,
            text: String::new(),
        };
    }
    Edit {
        start: toks[pl.params[0].first].start,
        end: toks[pl.params[1].first].start,
        text: String::new(),
    }
}

/// The signature text (from `from` to the `)`), with the edit applied, on one line.
fn preview(src: &str, toks: &[Token], from: usize, pl: &ParamList, edit: &Edit) -> String {
    let start = toks[from].start;
    let end = toks[pl.close].end;
    let shifted = Edit {
        start: edit.start.saturating_sub(start),
        end: edit.end.saturating_sub(start),
        text: edit.text.clone(),
    };
    collapse(&apply_edits(&src[start..end], &[shifted]))
}

fn hands_off_middleware(toks: &[Token]) -> bool {
    for i in 2..toks.len() {
        if !is_name(toks.get(i), Some("Middleware"))
            || !is_op(toks.get(i - 1), "=")
            || !is_name(toks.get(i - 2), Some("Middleware"))
        {
            continue;
        }
        let next = toks.get(i + 1);
        if is_op(next, ".") || is_op(next, ":") || is_op(next, "[") || is_op(next, "(") {
            continue;
        }
        let before = at(toks, i, 3);
        if is_op(before, ".") || is_op(before, "{") || is_op(before, ",") || is_op(before, ";") {
            return true;
        }
    }
    false
}

struct Scan<'a> {
    src: &'a str,
    toks: &'a [Token],
    s: &'a Structure,
    service: &'a str,
}

impl Scan<'_> {
    /// `at` is (function keyword index, first token shown).
    fn add(
        &self,
        found: &mut Vec<Issue>,
        at: (usize, usize),
        name: &str,
        colon: bool,
        pl: &ParamList,
        edit: Edit,
    ) {
        let (fi, from) = at;
        let skip = if colon { None } else { pl.params.first() };
        let manual = body_uses_self(self.toks, self.s, fi, pl.open, skip);
        let shown = preview(self.src, self.toks, from, pl, &edit);
        let service = self.service;
        let what = if colon {
            format!("{service}.Middleware:{name} is colon-style")
        } else {
            format!("{service}.Middleware.{name} takes self")
        };
        let mut message = format!(
            "{what}. Loren calls middleware as (player, ...args) without self, so 2.0 denies every {name} call. Dot style: {shown}"
        );
        if manual {
            message.push_str(&format!(
                ". Its body uses self, so rewrite it by hand (use {service} for the Service)."
            ));
        }
        found.push(Issue {
            file: String::new(),
            line: self.toks[from].line,
            kind: "colon-middleware",
            message,
            severity: Severity::Fixable,
            manual,
            edit: if manual { None } else { Some(edit) },
        });
    }
}

fn bare_ok(bare: &mut Option<bool>, toks: &[Token]) -> bool {
    *bare.get_or_insert_with(|| hands_off_middleware(toks))
}

fn first_is_self(pl: &ParamList) -> bool {
    pl.params.first().is_some_and(|p| p.name == "self")
}

pub(super) fn colon_issues(src: &str, toks: &[Token], s: &Structure, service: &str) -> Vec<Issue> {
    let scan = Scan {
        src,
        toks,
        s,
        service,
    };
    let mut found = Vec::new();
    let mut bare: Option<bool> = None;

    for i in 0..toks.len() {
        let t = &toks[i];
        if is_kw(Some(t), "function") && is_name(toks.get(i + 1), None) {
            let mut parts = vec![toks[i + 1].value.clone()];
            let mut k = i + 2;
            let mut colon_idx = None;
            while is_op(toks.get(k), ".") && is_name(toks.get(k + 1), None) {
                parts.push(toks[k + 1].value.clone());
                k += 2;
            }
            if is_op(toks.get(k), ":") && is_name(toks.get(k + 1), None) {
                colon_idx = Some(k);
                parts.push(toks[k + 1].value.clone());
            }
            if parts.len() < 2 || parts[parts.len() - 2] != "Middleware" {
                continue;
            }
            if parts.len() == 2 && !bare_ok(&mut bare, toks) {
                continue;
            }
            let Some(pl) = param_list(toks, s, i) else {
                continue;
            };
            let name = parts[parts.len() - 1].clone();
            if let Some(ci) = colon_idx {
                let edit = Edit {
                    start: toks[ci].start,
                    end: toks[ci].end,
                    text: ".".to_string(),
                };
                scan.add(&mut found, (i, i), &name, true, &pl, edit);
            } else if first_is_self(&pl) {
                let edit = drop_self_edit(toks, &pl);
                scan.add(&mut found, (i, i), &name, false, &pl, edit);
            }
            continue;
        }
        if is_op(Some(t), "=") && is_kw(toks.get(i + 1), "function") {
            let Some(chain) = chain_before(toks, i) else {
                continue;
            };
            if chain.parts.len() < 2 || chain.parts[chain.parts.len() - 2] != "Middleware" {
                continue;
            }
            if chain.parts.len() == 2 && !bare_ok(&mut bare, toks) {
                continue;
            }
            if let Some(pl) = param_list(toks, s, i + 1) {
                if first_is_self(&pl) {
                    let edit = drop_self_edit(toks, &pl);
                    let name = chain.parts[chain.parts.len() - 1].clone();
                    scan.add(&mut found, (i + 1, chain.first), &name, false, &pl, edit);
                }
            }
            continue;
        }
        if is_name(Some(t), Some("Middleware"))
            && is_op(toks.get(i + 1), "=")
            && is_op(toks.get(i + 2), "{")
            && !is_op(at(toks, i, 1), ":")
        {
            let p = at(toks, i, 1);
            let field = is_op(p, ".") || is_op(p, "{") || is_op(p, ",") || is_op(p, ";");
            if !field && !bare_ok(&mut bare, toks) {
                continue;
            }
            for f in table_fields(toks, s, i + 2) {
                let (Some(key), Some(key_idx)) = (&f.key, f.key_idx) else {
                    continue;
                };
                if !is_kw(toks.get(f.val_start), "function") {
                    continue;
                }
                let Some(pl) = param_list(toks, s, f.val_start) else {
                    continue;
                };
                if first_is_self(&pl) {
                    let from = if is_op(at(toks, key_idx, 1), "[") {
                        key_idx - 1
                    } else {
                        key_idx
                    };
                    let edit = drop_self_edit(toks, &pl);
                    scan.add(&mut found, (f.val_start, from), key, false, &pl, edit);
                }
            }
        }
    }
    found
}
