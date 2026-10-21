//! Yes/no and pick-one questions (`lib/prompt.js`). Without a terminal nothing
//! ever reads: the default answer wins, so the CLI never hangs in CI.

use std::cell::RefCell;
use std::io::{BufRead, IsTerminal as _, Write};

/// True on an interactive terminal, false under `CI` (unless `CI` is `false`/`0`).
pub fn detect_tty() -> bool {
    if std::env::var("CI").is_ok_and(|ci| ci != "false" && ci != "0") {
        return false;
    }
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

pub struct Prompter {
    pub yes: bool,
    pub is_tty: bool,
    input: RefCell<Box<dyn BufRead>>,
    output: RefCell<Box<dyn Write>>,
}

pub struct Choice<T> {
    pub value: T,
    pub label: String,
}

impl Prompter {
    pub fn new(yes: bool, is_tty: bool, input: Box<dyn BufRead>, output: Box<dyn Write>) -> Self {
        Self {
            yes,
            is_tty,
            input: RefCell::new(input),
            output: RefCell::new(output),
        }
    }

    pub fn stdio(yes: bool, is_tty: bool) -> Self {
        Self::new(
            yes,
            is_tty,
            Box::new(std::io::BufReader::new(std::io::stdin())),
            Box::new(std::io::stdout()),
        )
    }

    /// One line, or `None` at end of input.
    fn ask_line(&self, question: &str) -> Option<String> {
        let mut output = self.output.borrow_mut();
        let _ = output.write_all(question.as_bytes());
        let _ = output.flush();
        let mut line = String::new();
        match self.input.borrow_mut().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line.trim_end_matches(['\r', '\n']).to_string()),
        }
    }

    pub fn confirm(&self, message: &str, default: bool) -> bool {
        if self.yes {
            return true;
        }
        if !self.is_tty {
            return default;
        }
        let hint = if default { "(Y/n)" } else { "(y/N)" };
        // Some callers (the lint) already end the question with the hint.
        let trimmed = message.trim_end();
        let lower = trimmed.to_ascii_lowercase();
        let text = if lower.ends_with("(y/n)") {
            format!("{trimmed} ")
        } else {
            format!("{message} {hint} ")
        };
        let Some(answer) = self.ask_line(&text) else {
            return false;
        };
        let answer = answer.trim().to_lowercase();
        if answer.is_empty() {
            return default;
        }
        answer == "y" || answer == "yes"
    }

    /// The default (`default_index`) without a terminal or with `--yes`.
    pub fn choose<T: Clone + ToString>(
        &self,
        message: &str,
        choices: &[Choice<T>],
        default_index: usize,
    ) -> T {
        let fallback = choices[default_index].value.clone();
        if self.yes || !self.is_tty {
            return fallback;
        }
        let lines: Vec<String> = choices
            .iter()
            .enumerate()
            .map(|(i, c)| {
                format!(
                    "  [{}] {}{}",
                    i + 1,
                    c.label,
                    if i == default_index { " (default)" } else { "" }
                )
            })
            .collect();
        let Some(answer) = self.ask_line(&format!("{message}\n{}\n> ", lines.join("\n"))) else {
            return fallback;
        };
        let trimmed = answer.trim().to_lowercase();
        if let Ok(n) = leading_int(&trimmed) {
            if n >= 1 && (n as usize) <= choices.len() {
                return choices[n as usize - 1].value.clone();
            }
        }
        choices
            .iter()
            .find(|c| c.value.to_string().to_lowercase() == trimmed)
            .map_or(fallback, |c| c.value.clone())
    }
}

/// `Number.parseInt`: the leading digits (after an optional sign).
fn leading_int(text: &str) -> Result<i64, ()> {
    let end = text
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+'))))
        .map_or(text.len(), |(i, _)| i);
    text[..end].parse().map_err(|_| ())
}
