//! Reading the project before an update: which Loren it holds, and version
//! and `@native` helpers.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::constants::SHIM_SOURCE;
use crate::fsutil::{is_file, normalize_eol, read_text_if_exists, rel, strip_bom};
use crate::lint::Layout;
use crate::scriptsync::runtime_version;

const RUNTIME_DIR: &str = "loren";
const SHIM_NAMES: [&str; 2] = ["Loren.luau", "Loren.lua"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    /// `legacy`, `v2` or `none`.
    pub kind: &'static str,
    pub reason: String,
    pub loren_file: PathBuf,
    /// Every `Loren.luau`/`Loren.lua` in the shared folder.
    pub existing: Vec<PathBuf>,
    /// `missing`, `shim`, `runtime`, `shim-modified` or `other`.
    pub loren_state: &'static str,
    pub installed_version: Option<String>,
    pub has_runtime: bool,
}

/// Markers of the 1.5.1 single-file runtime (its remotes and its own definitions).
static LEGACY_MARKERS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"\bLorenBridge\b",
        r"\bLorenHandshake\b",
        r"function\s+Loren\s*[:.]\s*SetOnFire\s*\(",
        r"function\s+Loren\s*[:.]\s*AddServices\s*\(",
    ]
    .iter()
    .map(|p| Regex::new(p).unwrap())
    .collect()
});
static LOREN_RUNTIME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bLorenRuntime\b").unwrap());

fn parse_int(s: &str) -> i64 {
    let digits: String = s
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().unwrap_or(0)
}

fn parse_version(v: &str) -> (Vec<i64>, Option<Vec<String>>) {
    let s = v.trim();
    let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
    let s = s.split('+').next().unwrap_or("");
    let (core, pre) = match s.find('-') {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    let nums = core.split('.').map(parse_int).collect();
    let pre = pre
        .filter(|p| !p.is_empty())
        .map(|p| p.split('.').map(String::from).collect());
    (nums, pre)
}

fn ordering_int(o: Ordering) -> i32 {
    match o {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

/// -1, 0 or 1, semver-style with prereleases.
pub fn compare_versions(a: &str, b: &str) -> i32 {
    let (xn, xp) = parse_version(a);
    let (yn, yp) = parse_version(b);
    for i in 0..3 {
        let d = xn
            .get(i)
            .copied()
            .unwrap_or(0)
            .cmp(&yn.get(i).copied().unwrap_or(0));
        if d != Ordering::Equal {
            return ordering_int(d);
        }
    }
    let (x, y) = match (xp, yp) {
        (None, None) => return 0,
        (None, Some(_)) => return 1,
        (Some(_), None) => return -1,
        (Some(x), Some(y)) => (x, y),
    };
    let numeric = |p: &str| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit());
    for i in 0..x.len().max(y.len()) {
        let (Some(p), Some(q)) = (x.get(i), y.get(i)) else {
            return if x.get(i).is_none() { -1 } else { 1 };
        };
        if p == q {
            continue;
        }
        if numeric(p) && numeric(q) {
            let (pn, qn) = (
                p.parse::<f64>().unwrap_or(0.0),
                q.parse::<f64>().unwrap_or(0.0),
            );
            return if pn < qn { -1 } else { 1 };
        }
        if numeric(p) != numeric(q) {
            return if numeric(p) { -1 } else { 1 };
        }
        return if p < q { -1 } else { 1 };
    }
    0
}

fn strip_native_line(line: &str) -> String {
    let (content, cr) = match line.strip_suffix('\r') {
        Some(c) => (c, "\r"),
        None => (line, ""),
    };
    let blank = |c: char| c == ' ' || c == '\t';
    let body = content.trim_start_matches(blank);
    for marker in ["--!native", "@native"] {
        if body
            .strip_prefix(marker)
            .is_some_and(|r| r.trim_start_matches(blank).is_empty())
        {
            return cr.to_string();
        }
    }
    if let Some(rest) = body.strip_prefix("@native") {
        let trimmed = rest.trim_start_matches(blank);
        if trimmed.len() < rest.len() && trimmed.chars().next().is_some_and(|c| !c.is_whitespace())
        {
            let indent = &content[..content.len() - body.len()];
            return format!("{indent}{trimmed}{cr}");
        }
    }
    line.to_string()
}

/// `--no-native`: drops `--!native` and `@native`, keeping line numbers.
pub fn strip_native(text: &str) -> String {
    text.split('\n')
        .map(strip_native_line)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn classify_loren_file(text: Option<&str>) -> &'static str {
    let Some(text) = text else { return "missing" };
    let t = normalize_eol(strip_bom(text));
    if t == SHIM_SOURCE {
        return "shim";
    }
    if LEGACY_MARKERS.iter().any(|re| re.is_match(&t)) {
        return "runtime";
    }
    if LOREN_RUNTIME.is_match(&t) {
        return "shim-modified";
    }
    "other"
}

pub fn detect_project(root: &Path, layout: &Layout) -> Detected {
    let shared_dir = root.join(&layout.shared);
    let existing: Vec<PathBuf> = SHIM_NAMES
        .iter()
        .map(|n| shared_dir.join(n))
        .filter(|p| is_file(p))
        .collect();
    let loren_file = existing
        .first()
        .cloned()
        .unwrap_or_else(|| shared_dir.join(SHIM_NAMES[0]));
    let loren_state = classify_loren_file(read_text_if_exists(&loren_file).as_deref());
    let runtime_label = layout
        .runtime_shared
        .clone()
        .unwrap_or_else(|| format!("{RUNTIME_DIR}/shared"));
    let runtime_shared = root.join(&runtime_label);
    let has_runtime =
        is_file(&runtime_shared.join("init.luau")) || is_file(&runtime_shared.join("init.lua"));
    let installed_version = if has_runtime {
        runtime_version(&runtime_shared)
    } else {
        None
    };
    let where_ = rel(root, &loren_file);
    let (kind, reason) = if loren_state == "runtime" {
        ("legacy", format!("{where_} is the whole 1.x runtime"))
    } else if loren_state == "shim" || loren_state == "shim-modified" {
        ("v2", format!("{where_} is the 2.x shim"))
    } else if has_runtime {
        ("v2", format!("{runtime_label} holds the 2.x runtime"))
    } else if loren_state == "missing" {
        let dir = layout.runtime_shared.as_deref().unwrap_or(RUNTIME_DIR);
        (
            "none",
            format!("there is no {where_} and no {dir}/ runtime"),
        )
    } else {
        (
            "none",
            format!("{where_} is neither the Loren 1.x runtime nor the 2.x shim"),
        )
    };
    Detected {
        kind,
        reason,
        loren_file,
        existing,
        loren_state,
        installed_version,
        has_runtime,
    }
}
