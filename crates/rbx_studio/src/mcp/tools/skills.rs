//! `skill`: guidance a client pulls before a kind of work, such as building
//! UI or writing a DataStore. Each skill is a Markdown file under
//! `mcp/skills/`, its `name` and `description` in a front matter block as
//! Studio's own skills carry them, so the text is edited without touching
//! code.

#[cfg(test)]
mod tests;

use serde_json::{json, Value};

use super::{text_argument, Outcome};

pub(super) const NAME: &str = "skill";

const FILES: &[&str] = &[
    include_str!("../skills/accessibility.md"),
    include_str!("../skills/animation.md"),
    include_str!("../skills/data-persistence.md"),
    include_str!("../skills/debugging.md"),
    include_str!("../skills/docs-search.md"),
    include_str!("../skills/input-and-devices.md"),
    include_str!("../skills/live-ops.md"),
    include_str!("../skills/luau-scripting.md"),
    include_str!("../skills/monetization.md"),
    include_str!("../skills/performance.md"),
    include_str!("../skills/security.md"),
    include_str!("../skills/server-authority.md"),
    include_str!("../skills/testing.md"),
    include_str!("../skills/ui-design.md"),
    include_str!("../skills/ux-patterns.md"),
];

struct Skill {
    name: &'static str,
    description: &'static str,
    body: &'static str,
}

/// `file` split into its front matter's `name` and `description` and the
/// text after it; `None` for a file without both.
fn parse(file: &'static str) -> Option<Skill> {
    let rest = file.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    let field = |key: &str| {
        rest[..end].lines().find_map(|line| {
            let value = line.strip_prefix(key)?.strip_prefix(':')?.trim();
            Some(value.trim_matches('"'))
        })
    };
    Some(Skill {
        name: field("name")?,
        description: field("description")?,
        body: rest[end + 5..].trim_start(),
    })
}

fn skills() -> impl Iterator<Item = Skill> {
    FILES.iter().filter_map(|file| parse(file))
}

pub(super) fn schema() -> Value {
    let names: Vec<&str> = skills().map(|skill| skill.name).collect();
    super::tool(
        NAME,
        "Returns detailed guidance and best practices for one kind of Roblox work, to read \
         before doing it. Called without a name, lists every skill with a one-line \
         description. Skills cover Luau scripting, UI design that fits the game, UX, \
         accessibility, animation and tweening, client/server security, data persistence, \
         monetization, analytics and live configs, Server Authority, input across devices, \
         performance, debugging, testing and searching the documentation with http_get.",
        json!({
            "name": {
                "type": "string",
                "enum": names,
                "description": "The skill to read. Leave it out to list them all.",
            },
        }),
        &[],
    )
}

pub(super) fn call(arguments: &Value) -> Outcome {
    let Some(wanted) = text_argument(arguments, "name").map(str::trim) else {
        return Ok(Value::String(catalogue()));
    };
    skills()
        .find(|skill| skill.name.eq_ignore_ascii_case(wanted))
        .map(|skill| Value::String(format!("# Skill: {}\n\n{}", skill.name, skill.body)))
        .ok_or_else(|| format!("Unknown skill: {wanted}\n\n{}", catalogue()))
}

fn catalogue() -> String {
    let rows: Vec<String> = skills()
        .map(|skill| format!("- {}: {}", skill.name, skill.description))
        .collect();
    format!("Available skills:\n{}", rows.join("\n"))
}
