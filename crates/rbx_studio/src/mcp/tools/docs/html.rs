//! An HTML page as readable text: what a model reading `http_get`'s answer
//! needs, not a renderer. Headings keep a Markdown `#`, list items a `- `,
//! and code blocks their own line breaks; scripts, styles and the site's
//! navigation are dropped.

/// Elements whose content is never text to read.
const SKIPPED: &[&str] = &[
    "script", "style", "noscript", "svg", "head", "nav", "header", "footer", "template", "iframe",
    "button", "form",
];

/// Elements that start a line of their own.
const BLOCKS: &[&str] = &[
    "p",
    "div",
    "br",
    "li",
    "tr",
    "section",
    "article",
    "main",
    "table",
    "ul",
    "ol",
    "dl",
    "dt",
    "dd",
    "blockquote",
    "pre",
    "hr",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "aside",
    "figure",
];

pub(super) fn looks_like_html(text: &str) -> bool {
    let head = text.trim_start().get(..512).unwrap_or(text.trim_start());
    let head = head.to_ascii_lowercase();
    head.starts_with("<!doctype html") || head.starts_with("<html") || head.contains("<body")
}

/// `html` as text. Only `<main>`'s content is kept when the page has one:
/// everything outside it on a documentation site is chrome.
pub(super) fn to_text(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let body = match (lower.find("<main"), lower.rfind("</main>")) {
        (Some(start), Some(end)) if start < end => &html[start..end],
        _ => html,
    };
    let mut out = String::new();
    let mut skipping: Option<String> = None;
    let mut preformatted = 0usize;
    let mut rest = body;
    while let Some(open) = rest.find('<') {
        if skipping.is_none() {
            push_text(&mut out, &rest[..open], preformatted > 0);
        }
        rest = &rest[open..];
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map_or("", |end| &rest[end + 3..]);
            continue;
        }
        let Some(close) = rest.find('>') else {
            rest = "";
            break;
        };
        let tag = &rest[1..close];
        rest = &rest[close + 1..];
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        if let Some(skipped) = &skipping {
            if closing && &name == skipped {
                skipping = None;
            }
            continue;
        }
        if !closing && SKIPPED.contains(&name.as_str()) && !tag.ends_with('/') {
            skipping = Some(name);
            continue;
        }
        if name == "pre" {
            preformatted = if closing {
                preformatted.saturating_sub(1)
            } else {
                preformatted + 1
            };
        }
        if BLOCKS.contains(&name.as_str()) {
            new_line(&mut out);
            if !closing {
                if let Some(level) = heading_level(&name) {
                    out.push_str(&"#".repeat(level));
                    out.push(' ');
                } else if name == "li" {
                    out.push_str("- ");
                }
            }
        } else if (name == "td" || name == "th")
            && !closing
            && !out.ends_with('\n')
            && !out.is_empty()
        {
            out.push_str(" | ");
        }
    }
    if skipping.is_none() {
        push_text(&mut out, rest, false);
    }
    tidy(&out)
}

fn heading_level(name: &str) -> Option<usize> {
    let digit = name.strip_prefix('h')?;
    digit.parse().ok().filter(|level| (1..=6).contains(level))
}

fn new_line(out: &mut String) {
    while out.ends_with(' ') {
        out.pop();
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

fn push_text(out: &mut String, text: &str, preformatted: bool) {
    let text = decode_entities(text);
    if preformatted {
        out.push_str(&text);
        return;
    }
    for (index, word) in text.split_whitespace().enumerate() {
        let joined = index == 0 && !text.starts_with(char::is_whitespace);
        if !joined && !out.is_empty() && !out.ends_with([' ', '\n']) {
            out.push(' ');
        }
        out.push_str(word);
    }
    if text.ends_with(char::is_whitespace) && !out.ends_with([' ', '\n']) && !out.is_empty() {
        out.push(' ');
    }
}

/// Trailing spaces off every line, and no more than one blank line in a row.
fn tidy(text: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for line in text.lines().map(str::trim_end) {
        if line.trim().is_empty() {
            blank += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if blank > 0 { "\n\n" } else { "\n" });
        }
        blank = 0;
        out.push_str(line);
    }
    out
}

pub(super) fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let entity = rest
            .get(1..)
            .and_then(|after| after.find(';').filter(|end| *end <= 10))
            .map(|end| &rest[1..=end]);
        match entity.and_then(entity_char) {
            Some(decoded) => {
                out.push(decoded);
                rest = &rest[entity.map_or(0, str::len) + 2..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity_char(entity: &str) -> Option<char> {
    if let Some(number) = entity.strip_prefix('#') {
        let code = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match entity {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "mdash" => '\u{2014}',
        "ndash" => '\u{2013}',
        "hellip" => '\u{2026}',
        "rarr" => '\u{2192}',
        "copy" => '\u{a9}',
        _ => return None,
    })
}
