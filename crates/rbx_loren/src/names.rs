//! Name checks (`lib/names.js`): module, project and package names, and
//! `user/repo[#ref]` specs. Errors are the user-facing sentence.

use std::sync::LazyLock;

use regex::Regex;

use crate::constants::{is_identifier, is_keyword, PROMISE_DIR};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSpec {
    pub owner: String,
    pub repo: String,
    pub git_ref: Option<String>,
    /// `owner/repo` or `owner/repo#ref`.
    pub spec: String,
}

static WINDOWS_RESERVED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(con|prn|aux|nul|com[0-9]|lpt[0-9])(\..*)?$").unwrap());
static PROJECT_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_][A-Za-z0-9_. -]*$").unwrap());
static OWNER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})$").unwrap());
static REPO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._-]{1,100}$").unwrap());
static REF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._/-]{1,200}$").unwrap());
static GITHUB_HOST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:https?://|ssh://)?(?:git@|www\.)?github\.com[/:]").unwrap()
});
static GITHUB_SCHEME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^github:").unwrap());
static OTHER_SCHEME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^[a-z][a-z0-9+.-]*:").unwrap());
static DOT_GIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\.git$").unwrap());
static LUA_PREFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^roblox-lua-").unwrap());
static NON_ALNUM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^A-Za-z0-9]+").unwrap());

pub fn validate_identifier(name: &str, what: &str) -> Result<String, String> {
    if name.is_empty() {
        return Err(format!("{what} is empty."));
    }
    if name.contains(['\\', '/']) || name.contains("..") {
        return Err(format!(
            "{what} \"{name}\" must not contain path separators or \"..\"."
        ));
    }
    if !is_identifier(name) {
        return Err(format!(
            "{what} \"{name}\" is not a valid Luau identifier (letters, digits and _, not starting with a digit)."
        ));
    }
    if is_keyword(name) {
        return Err(format!("{what} \"{name}\" is a Luau keyword."));
    }
    if WINDOWS_RESERVED.is_match(name) {
        return Err(format!(
            "{what} \"{name}\" is a reserved file name on Windows."
        ));
    }
    if name.chars().count() > 100 {
        return Err(format!("{what} is longer than 100 characters."));
    }
    Ok(name.to_string())
}

pub fn validate_project_name(name: &str) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("Project name is empty.".to_string());
    }
    if name == "." || name == ".." || name.contains("..") {
        return Err(format!("Project name \"{name}\" must not contain \"..\"."));
    }
    if name.contains(['\\', '/']) {
        return Err(format!(
            "Project name \"{name}\" must not contain path separators."
        ));
    }
    if !PROJECT_NAME.is_match(name) {
        return Err(format!(
            "Project name \"{name}\" may only use letters, digits, spaces, \"_\", \"-\" and \".\", and must start with a letter, digit or \"_\"."
        ));
    }
    if name.ends_with(['.', ' ']) {
        return Err(format!(
            "Project name \"{name}\" must not end with \".\" or a space."
        ));
    }
    if WINDOWS_RESERVED.is_match(name) {
        return Err(format!("Project name \"{name}\" is reserved on Windows."));
    }
    if name.chars().count() > 100 {
        return Err("Project name is longer than 100 characters.".to_string());
    }
    Ok(name.to_string())
}

pub fn is_pascal_case(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

pub fn parse_repo_spec(input: &str) -> Result<RepoSpec, String> {
    if input.trim().is_empty() {
        return Err("Repository is empty. Use user/repo.".to_string());
    }
    let mut s = input.trim().to_string();
    let mut git_ref: Option<String> = None;

    if let Some(hash) = s.find('#') {
        let r = s[hash + 1..].to_string();
        s.truncate(hash);
        if r.is_empty() {
            return Err(format!("\"{input}\" has an empty #ref."));
        }
        git_ref = Some(r);
    }

    let s = s.strip_prefix("git+").unwrap_or(&s).to_string();
    let s = GITHUB_HOST.replace(&s, "").into_owned();
    let s = GITHUB_SCHEME.replace(&s, "").into_owned();
    if OTHER_SCHEME.is_match(&s) || s.starts_with("//") {
        return Err(format!("\"{input}\" is not a GitHub repository. Use user/repo or https://github.com/user/repo."));
    }

    let s = s.trim_end_matches('/');
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() > 2 {
        // https://github.com/user/repo/tree/<ref>: the ref may itself contain slashes.
        if parts[2] == "tree" && parts.len() > 3 && git_ref.is_none() {
            git_ref = Some(parts[3..].join("/"));
        } else {
            return Err(format!(
                "\"{input}\" is not a repository. Use user/repo[#ref]."
            ));
        }
    }
    let owner = parts[0];
    let repo = DOT_GIT
        .replace(parts.get(1).copied().unwrap_or(""), "")
        .into_owned();

    if owner.is_empty() || repo.is_empty() {
        return Err(format!(
            "\"{input}\" is missing the user or the repository. Use user/repo[#ref]."
        ));
    }
    if !OWNER.is_match(owner) {
        return Err(format!(
            "\"{owner}\" is not a valid GitHub user or organization."
        ));
    }
    if !REPO.is_match(&repo) || repo == "." || repo == ".." {
        return Err(format!("\"{repo}\" is not a valid repository name."));
    }
    if let Some(r) = &git_ref {
        if !REF.is_match(r) || r.contains("..") || r.starts_with('/') || r.ends_with('/') {
            return Err(format!("\"{r}\" is not a valid branch, tag or commit."));
        }
    }

    let git_ref = git_ref.filter(|r| !r.is_empty());
    let spec = match &git_ref {
        Some(r) => format!("{owner}/{repo}#{r}"),
        None => format!("{owner}/{repo}"),
    };
    Ok(RepoSpec {
        owner: owner.to_string(),
        repo,
        git_ref,
        spec,
    })
}

pub fn derive_package_name(repo: &str) -> Result<String, String> {
    let base = DOT_GIT.replace(repo, "").into_owned();
    let base = LUA_PREFIX.replace(&base, "").into_owned();
    if is_identifier(&base) && !is_keyword(&base) {
        return Ok(base);
    }

    let mut joined: String = NON_ALNUM
        .split(&base)
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            let first = chars
                .next()
                .map(|c| c.to_ascii_uppercase())
                .unwrap_or_default();
            format!("{first}{}", chars.as_str())
        })
        .collect();
    if joined.starts_with(|c: char| c.is_ascii_digit()) {
        joined.insert(0, '_');
    }
    validate_identifier(&joined, "Package name").map_err(|_| {
        format!(
            "Could not derive a package name from \"{repo}\". Pass one: loren add <repo> <Name>."
        )
    })
}

/// Loren's own Promise; `loren add` never replaces it without `--force`.
pub fn is_reserved_package(name: &str) -> bool {
    name.to_lowercase() == PROMISE_DIR.to_lowercase()
}
