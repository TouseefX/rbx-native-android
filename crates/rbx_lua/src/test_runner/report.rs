//! A [`Report`] as text for a terminal, as JSON, and as JUnit XML (what CI
//! services render test results from).

use std::fmt::Write as _;

use serde_json::json;

use super::{Report, Status, TestResult};

/// The test's name for a report: its describes and its own, or a
/// stand-in for an error loading the file.
fn title(result: &TestResult) -> String {
    if result.path.is_empty() {
        "(loading the spec)".into()
    } else {
        result.path.join(" › ")
    }
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::Passed => "passed",
        Status::Failed => "failed",
        Status::Skipped => "skipped",
    }
}

/// One line per test, then each failure with its location and stack, then
/// the totals.
pub fn human(report: &Report) -> String {
    let mut out = String::new();
    for file in &report.files {
        let _ = writeln!(out, "{}", file.file.name);
        for result in &file.results {
            let mark = match result.status {
                Status::Passed => "+",
                Status::Failed => "x",
                Status::Skipped => "~",
            };
            let _ = writeln!(
                out,
                "  {mark} {} ({} ms)",
                title(result),
                result.duration.as_millis()
            );
        }
    }
    let failures: Vec<_> = report
        .files
        .iter()
        .flat_map(|f| f.results.iter().map(move |r| (f, r)))
        .filter(|(_, r)| r.status == Status::Failed)
        .collect();
    for (file, result) in &failures {
        let _ = writeln!(out, "\nFAIL {} › {}", file.file.name, title(result));
        if let Some(message) = &result.message {
            let _ = writeln!(out, "  {message}");
        }
        if let Some(at) = &result.location {
            let _ = writeln!(out, "  at {}:{}", at.name, at.line);
        }
        if let Some(stack) = result.stack.as_deref().filter(|s| !s.is_empty()) {
            for line in stack.lines() {
                let _ = writeln!(out, "    {line}");
            }
        }
    }
    let _ = writeln!(
        out,
        "\n{} passed, {} failed, {} skipped ({} files)",
        report.count(Status::Passed),
        report.count(Status::Failed),
        report.count(Status::Skipped),
        report.files.len()
    );
    out
}

pub fn json(report: &Report) -> String {
    let files: Vec<_> = report
        .files
        .iter()
        .map(|file| {
            let tests: Vec<_> = file
                .results
                .iter()
                .map(|r| {
                    json!({
                        "path": r.path,
                        "status": status_name(r.status),
                        "message": r.message,
                        "stack": r.stack,
                        "location": r.location.as_ref().map(|at| json!({
                            "script": at.name,
                            "line": at.line,
                        })),
                        "durationMs": r.duration.as_secs_f64() * 1000.0,
                    })
                })
                .collect();
            json!({
                "file": file.file.name,
                "durationMs": file.duration.as_secs_f64() * 1000.0,
                "output": file.output.lines(),
                "tests": tests,
            })
        })
        .collect();
    let value = json!({
        "passed": report.count(Status::Passed),
        "failed": report.count(Status::Failed),
        "skipped": report.count(Status::Skipped),
        "files": files,
    });
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 has no escape for most control characters.
            c if (c as u32) < 0x20 && !matches!(c, '\n' | '\r' | '\t') => {}
            c => out.push(c),
        }
    }
    out
}

/// One `<testsuite>` per spec file, one `<testcase>` per test.
pub fn junit(report: &Report) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        "<testsuites name=\"rbxlua test\" tests=\"{}\" failures=\"{}\" skipped=\"{}\">",
        report.results().count(),
        report.count(Status::Failed),
        report.count(Status::Skipped)
    );
    for file in &report.files {
        let count = |status| file.results.iter().filter(|r| r.status == status).count();
        let _ = writeln!(
            out,
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" skipped=\"{}\" time=\"{:.3}\">",
            escape(&file.file.name),
            file.results.len(),
            count(Status::Failed),
            count(Status::Skipped),
            file.duration.as_secs_f64()
        );
        for result in &file.results {
            let _ = write!(
                out,
                "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\"",
                escape(&file.file.name),
                escape(&title(result)),
                result.duration.as_secs_f64()
            );
            match result.status {
                Status::Passed => out.push_str("/>\n"),
                Status::Skipped => out.push_str(">\n      <skipped/>\n    </testcase>\n"),
                Status::Failed => {
                    let message = result.message.as_deref().unwrap_or("failed");
                    let mut body = message.to_string();
                    if let Some(at) = &result.location {
                        let _ = write!(body, "\nat {}:{}", at.name, at.line);
                    }
                    if let Some(stack) = result.stack.as_deref().filter(|s| !s.is_empty()) {
                        let _ = write!(body, "\n{stack}");
                    }
                    let _ = write!(
                        out,
                        ">\n      <failure message=\"{}\">{}</failure>\n    </testcase>\n",
                        escape(message),
                        escape(&body)
                    );
                }
            }
        }
        out.push_str("  </testsuite>\n");
    }
    out.push_str("</testsuites>\n");
    out
}
