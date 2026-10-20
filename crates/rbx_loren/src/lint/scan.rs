//! Per-file issues (`lint.js` lines 719-925): colon-style middleware, the
//! Service-shape warnings, and the "Did you mean" suggestion.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::analyze::analyze_luau;
use super::syntax::{body_uses_self, chain_before, param_list, table_fields, ParamList};
use super::tokenize::{is_kw, is_name, is_op, structure, tokenize, Structure, Token};
use crate::constants::RESERVED_CLIENT_NAMES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warn,
    Fixable,
}

/// Replace `start..end` (byte offsets) with `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub file: String,
    pub line: usize,
    /// `colon-middleware`, `reserved-name`, `orphan-middleware`, `orphan-spec` or `reserved-key`.
    pub kind: &'static str,
    pub message: String,
    pub severity: Severity,
    /// A fixable issue the rewrite cannot make (the body uses `self`).
    pub manual: bool,
    pub edit: Option<Edit>,
}

static CHAIN_BREAK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\n\s*([.:])(\w)").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static INIT_FILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(init|\.src)\.luau?$").unwrap());
static LUA_EXT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\.luau?$").unwrap());
static SCRIPT_EXT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\.(server|client|local|legacy|plugin)\.luau?$").unwrap());

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

/// Edits applied from the last to the first.
pub fn apply_edits(src: &str, edits: &[Edit]) -> String {
    let mut sorted: Vec<&Edit> = edits.iter().collect();
    sorted.sort_by_key(|e| std::cmp::Reverse(e.start));
    let mut out = src.to_string();
    for e in sorted {
        out = format!("{}{}{}", &out[..e.start], e.text, &out[e.end..]);
    }
    out
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

fn colon_issues(src: &str, toks: &[Token], s: &Structure, service: &str) -> Vec<Issue> {
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

/// Case-insensitive edit distance.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut dp: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = dp[0];
        dp[0] = i;
        for j in 1..=b.len() {
            let tmp = dp[j];
            let same = a[i - 1].to_lowercase().eq(b[j - 1].to_lowercase());
            dp[j] = (dp[j] + 1)
                .min(dp[j - 1] + 1)
                .min(prev + usize::from(!same));
            prev = tmp;
        }
    }
    dp[b.len()]
}

/// `" Did you mean 'X'?"` or `""`.
pub fn suggestion(name: &str, candidates: &[String]) -> String {
    let mut best: Option<&String> = None;
    let mut best_d = usize::MAX;
    for c in candidates {
        let d = levenshtein(name, c);
        if d < best_d {
            best = Some(c);
            best_d = d;
        }
    }
    match best {
        Some(b) if best_d <= 2.max(name.chars().count() / 3) => format!(" Did you mean '{b}'?"),
        _ => String::new(),
    }
}

fn reserved_list() -> String {
    let mut names = RESERVED_CLIENT_NAMES.to_vec();
    names.sort_unstable();
    names.join(", ")
}

/// The Instance name Rojo and Argon give a module file (`X.luau`, `X/init.luau`, `X/.src.luau`).
pub fn module_name_of(file: &str) -> String {
    let path = Path::new(file);
    let base = path
        .file_name()
        .map(|b| b.to_string_lossy().into_owned())
        .unwrap_or_default();
    if INIT_FILE.is_match(&base) {
        return path
            .parent()
            .and_then(Path::file_name)
            .map(|b| b.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    LUA_EXT.replace(&base, "").into_owned()
}

/// `.server`/`.client`/`.local`/`.legacy`/`.plugin` scripts.
pub fn is_script_file(file: &str) -> bool {
    SCRIPT_EXT.is_match(file)
}

fn unique(lists: &[&[String]]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in lists.iter().flat_map(|l| l.iter()) {
        if !out.contains(n) {
            out.push(n.clone());
        }
    }
    out
}

/// All issues in one file, sorted by line; `service` enables the Service-shape checks.
pub fn scan_text(text: &str, file: &str, service: bool, legacy: bool) -> Vec<Issue> {
    let toks = tokenize(text);
    let s = structure(&toks);
    let name = module_name_of(file);
    let mut issues = colon_issues(text, &toks, &s, &name);
    for i in &mut issues {
        i.file = file.to_string();
    }
    if !service {
        return issues;
    }

    let info = analyze_luau(text, Some(&toks));
    let reserved = |n: &str| RESERVED_CLIENT_NAMES.contains(&n);
    let mut warn = |kind: &'static str, line: usize, message: String| {
        issues.push(Issue {
            file: file.to_string(),
            line,
            kind,
            message,
            severity: Severity::Warn,
            manual: false,
            edit: None,
        });
    };

    let list = reserved_list();
    for m in &info.client.methods {
        if reserved(&m.name) {
            warn(
                "reserved-name",
                m.line,
                format!("{name}.Client.{} uses a reserved name and is not networked (reserved: {list}). Rename it.", m.name),
            );
        }
    }
    let methods: Vec<String> = info
        .client
        .methods
        .iter()
        .filter(|m| !reserved(&m.name))
        .map(|m| m.name.clone())
        .collect();
    let events: Vec<String> = info.events.names.iter().map(|e| e.name.clone()).collect();
    let signals: Vec<String> = info.signals.names.iter().map(|e| e.name.clone()).collect();
    if info.client.known && info.events.known {
        let candidates = unique(&[&methods, &events]);
        for m in &info.middleware {
            if !candidates.contains(&m.name) {
                warn(
                    "orphan-middleware",
                    m.line,
                    format!(
                        "{name}.Middleware.{} matches no Client method or ClientEvent and is ignored.{}",
                        m.name,
                        suggestion(&m.name, &candidates)
                    ),
                );
            }
        }
    }
    if info.client.known && info.events.known && info.signals.known {
        let candidates = unique(&[&methods, &events, &signals]);
        for sp in &info.spec {
            if !candidates.contains(&sp.name) {
                warn(
                    "orphan-spec",
                    sp.line,
                    format!(
                        "{name}.Spec.{} matches no Client method, ClientEvent or Signal and is ignored.{}",
                        sp.name,
                        suggestion(&sp.name, &candidates)
                    ),
                );
            }
        }
    }
    for k in &info.service_keys {
        if k.empty || (!legacy && (k.modern || k.unknown)) {
            continue;
        }
        let meaning = if k.key == "Spec" {
            "per-member network specs (Loren.Method, Loren.Event, Loren.Signal)"
        } else {
            "the names of client-to-server events"
        };
        warn(
            "reserved-key",
            k.line,
            format!("{name}.{} is a Loren 2.0 key: Loren reads it as {meaning}. If it is your own data, rename it.", k.key),
        );
    }
    issues.sort_by_key(|i| i.line);
    issues
}
