//! `rbxaudit`: runs the whole-place script security audit from the command
//! line, for CI.
//!
//! ```text
//! rbxaudit <place.rbxl|.rbxlx|.rbxm|.rbxmx>
//! ```
//!
//! One line per finding on stdout (`severity check path:line message`, the
//! `:line` omitted for a place-level finding), then a summary line.
//!
//! Exit codes: `0` no High finding, `1` at least one High finding, `2` bad
//! usage or an unreadable file (message on stderr).

use std::path::Path;
use std::process::ExitCode;

use rbx_audit::{Finding, Severity};
use rbx_dom::WeakDom;

const USAGE: &str = "usage: rbxaudit <place.rbxl|.rbxlx|.rbxm|.rbxmx>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.as_slice(), [flag] if flag == "--help" || flag == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let [path] = args.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match read_dom(Path::new(path)) {
        Ok(dom) => {
            let findings = rbx_audit::audit(&dom);
            for finding in &findings {
                println!("{}", line(finding));
            }
            println!("{}", summary(&findings));
            if findings.iter().any(|f| f.severity == Severity::High) {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(err) => {
            eprintln!("rbxaudit: {err}");
            ExitCode::from(2)
        }
    }
}

fn line(finding: &Finding) -> String {
    let location = match finding.line {
        Some(line) => format!("{}:{line}", finding.path),
        None => finding.path.clone(),
    };
    format!(
        "{:<6} {} {location} {}",
        finding.severity.label(),
        finding.check.id(),
        finding.message
    )
}

fn summary(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "no findings".to_string();
    }
    let count = |severity| findings.iter().filter(|f| f.severity == severity).count();
    format!(
        "{} findings: {} high, {} medium, {} low",
        findings.len(),
        count(Severity::High),
        count(Severity::Medium),
        count(Severity::Low)
    )
}

/// Sniffs the format from the bytes, not the extension, like `rbxdump`.
fn read_dom(path: &Path) -> Result<WeakDom, String> {
    let display = path.display();
    let bytes = std::fs::read(path).map_err(|err| format!("failed to read '{display}': {err}"))?;

    if rbx_xml::is_xml(&bytes) {
        let text = std::str::from_utf8(&bytes)
            .map_err(|err| format!("'{display}' is not valid UTF-8 XML: {err}"))?;
        rbx_xml::deserialize(text).map_err(|err| format!("failed to parse '{display}': {err}"))
    } else {
        rbx_binary::deserialize(&bytes).map_err(|err| format!("failed to parse '{display}': {err}"))
    }
}
