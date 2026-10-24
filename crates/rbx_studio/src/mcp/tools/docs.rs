//! `http_get`: Roblox's documentation as text, from an allowlist of sites,
//! with an optional keyword search inside the page. Studio's own tool reads
//! the same pages; `create.roblox.com/docs` serves every page as Markdown
//! at its URL with `.md` added, and indexes at `llms.txt`.

mod fetch;
mod html;
mod search;

#[cfg(test)]
mod tests;

use serde_json::{json, Value};

use super::{count_argument, flag_argument, required_text, text_argument, Outcome};

pub(super) use fetch::Fetched;

pub(super) const NAME: &str = "http_get";

pub(super) fn schema() -> Value {
    super::tool(
        NAME,
        "Fetches a page of Roblox's documentation as text: the Engine API reference \
         (https://create.roblox.com/docs/reference/engine/classes/<Class>.md, also datatypes/, \
         enums/, globals/, libraries/), creator guides (https://create.roblox.com/docs/<path>.md), \
         the Cloud API (https://create.roblox.com/docs/cloud/...), performance guides, and the \
         llms.txt indexes (https://create.roblox.com/docs/llms.txt, \
         .../reference/engine/llms.txt, .../cloud/llms.txt). Roblox's creator-docs repository on \
         GitHub is allowed too; nothing else is. With query, only the lines holding it (ignoring \
         case) are returned, with context_lines around each, or one line saying nothing matched.",
        json!({
            "url": super::string("The page to fetch, such as https://create.roblox.com/docs/reference/engine/classes/TweenService.md."),
            "query": super::string("A keyword to search the page for. Only matching lines and their context are returned."),
            "context_lines": super::integer("Lines kept on each side of a match. Defaults to 3."),
            "return_full": super::boolean("With query: return the whole page when the query matches, instead of excerpts."),
        }),
        &["url"],
    )
}

/// `http_get` over the network.
pub(super) fn call(arguments: &Value) -> Outcome {
    call_with(arguments, fetch::get)
}

/// `http_get` with the fetch left to `get`, so tests need no network.
pub(super) fn call_with(
    arguments: &Value,
    get: impl FnOnce(&url::Url) -> Result<Fetched, String>,
) -> Outcome {
    let url = fetch::allowed(required_text(arguments, "url")?)?;
    let fetched = get(&url)?;
    let text = readable(&fetched);
    let query = text_argument(arguments, "query").filter(|query| !query.trim().is_empty());
    let Some(query) = query else {
        return Ok(Value::String(text));
    };
    let context = count_argument(arguments, "context_lines").unwrap_or(search::DEFAULT_CONTEXT);
    Ok(Value::String(
        match search::excerpts(&text, query, context) {
            Some(_) if flag_argument(arguments, "return_full") == Some(true) => text,
            Some(excerpts) => format!("{url}\n{excerpts}"),
            None => format!("No match for \"{}\" in {url}", query.trim()),
        },
    ))
}

/// The page as text: Markdown and plain text as they came, HTML stripped.
fn readable(fetched: &Fetched) -> String {
    let declared_html = fetched.content_type.to_ascii_lowercase().contains("html");
    if declared_html || html::looks_like_html(&fetched.body) {
        html::to_text(&fetched.body)
    } else {
        fetched.body.trim().to_owned()
    }
}
