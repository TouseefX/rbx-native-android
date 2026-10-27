//! `http_get`'s `query`: the lines of a fetched page that hold a keyword,
//! each with a few lines around it, so a model can check a page for a
//! member without reading all of it.

/// Lines kept on each side of a match when `context_lines` is not given.
pub(super) const DEFAULT_CONTEXT: usize = 3;

/// More matches than this are cut, with a note, so a common word cannot
/// return the whole page in pieces.
const MAX_MATCHES: usize = 30;

/// The excerpts of `text` around every line holding `query`, ignoring case,
/// or `None` when no line does. Each excerpt runs `context` lines either
/// side of its matches; excerpts that touch are joined, and every line keeps
/// its number in the page.
pub(super) fn excerpts(text: &str, query: &str, context: usize) -> Option<String> {
    let needle = query.trim().to_lowercase();
    let lines: Vec<&str> = text.lines().collect();
    let hits: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.to_lowercase().contains(&needle))
        .map(|(index, _)| index)
        .collect();
    if hits.is_empty() {
        return None;
    }
    let shown = &hits[..hits.len().min(MAX_MATCHES)];
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for &hit in shown {
        let start = hit.saturating_sub(context);
        let end = (hit + context).min(lines.len() - 1);
        match ranges.last_mut() {
            Some(last) if start <= last.1 + 1 => last.1 = end,
            _ => ranges.push((start, end)),
        }
    }
    let width = (ranges.last().map_or(0, |range| range.1) + 1)
        .to_string()
        .len();
    let blocks: Vec<String> = ranges
        .iter()
        .map(|&(start, end)| {
            (start..=end)
                .map(|index| format!("{:>width$}: {}", index + 1, lines[index]))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect();
    let mut out = format!(
        "{} {} for \"{}\":\n\n{}",
        hits.len(),
        if hits.len() == 1 { "match" } else { "matches" },
        query.trim(),
        blocks.join("\n...\n")
    );
    if hits.len() > MAX_MATCHES {
        out.push_str(&format!(
            "\n\n({} more matches not shown; use a more specific query)",
            hits.len() - MAX_MATCHES
        ));
    }
    Some(out)
}
