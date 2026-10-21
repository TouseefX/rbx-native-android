//! The CLI's output: every line starts with Loren's `(LORENঌ)` prefix, warnings
//! and errors go to stderr, colour only on a terminal (`lib/log.js`).

use std::cell::RefCell;
use std::io::{IsTerminal as _, Write as _};
use std::rc::Rc;

use crate::constants::PREFIX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Out,
    Err,
}

const BOLD: u8 = 1;
const DIM: u8 = 2;
const RED: u8 = 31;
const GREEN: u8 = 32;
const YELLOW: u8 = 33;
const CYAN: u8 = 36;

type Writer = Box<dyn FnMut(Channel, &str)>;
type Chunks = Rc<RefCell<Vec<(Channel, String)>>>;

pub struct Log {
    pub color: bool,
    write: RefCell<Writer>,
    chunks: Option<Chunks>,
}

impl Log {
    pub fn new(color: bool, write: impl FnMut(Channel, &str) + 'static) -> Self {
        Self {
            color,
            write: RefCell::new(Box::new(write)),
            chunks: None,
        }
    }

    /// stdout/stderr, coloured when stdout is a terminal (`NO_COLOR` and
    /// `FORCE_COLOR` honoured as upstream).
    pub fn console() -> Self {
        let color = color_enabled(std::io::stdout().is_terminal());
        Self::new(color, |channel, text| {
            let _ = match channel {
                Channel::Out => std::io::stdout().write_all(text.as_bytes()),
                Channel::Err => std::io::stderr().write_all(text.as_bytes()),
            };
        })
    }

    /// Collects everything in memory (tests, and the editor's own calls).
    pub fn memory() -> Self {
        let chunks = Rc::new(RefCell::new(Vec::new()));
        let sink = chunks.clone();
        let mut log = Self::new(false, move |channel, text| {
            sink.borrow_mut().push((channel, text.to_string()))
        });
        log.chunks = Some(chunks);
        log
    }

    fn collected(&self, filter: Option<Channel>) -> String {
        let Some(chunks) = &self.chunks else {
            return String::new();
        };
        chunks
            .borrow()
            .iter()
            .filter(|(c, _)| filter.is_none_or(|f| f == *c))
            .map(|(_, t)| t.as_str())
            .collect()
    }

    /// Everything written so far (memory logger only).
    pub fn output(&self) -> String {
        self.collected(None)
    }

    pub fn stdout(&self) -> String {
        self.collected(Some(Channel::Out))
    }

    pub fn stderr(&self) -> String {
        self.collected(Some(Channel::Err))
    }

    pub fn paint(&self, code: u8, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn bold(&self, text: &str) -> String {
        self.paint(BOLD, text)
    }
    pub fn dim(&self, text: &str) -> String {
        self.paint(DIM, text)
    }
    pub fn cyan(&self, text: &str) -> String {
        self.paint(CYAN, text)
    }
    pub fn green(&self, text: &str) -> String {
        self.paint(GREEN, text)
    }
    pub fn yellow(&self, text: &str) -> String {
        self.paint(YELLOW, text)
    }
    pub fn red(&self, text: &str) -> String {
        self.paint(RED, text)
    }

    pub fn raw(&self, channel: Channel, text: &str) {
        (self.write.borrow_mut())(channel, text);
    }

    pub fn info(&self, msg: &str) {
        self.raw(Channel::Out, &format!("{PREFIX} {msg}\n"));
    }

    pub fn ok(&self, msg: &str) {
        self.raw(
            Channel::Out,
            &format!("{PREFIX} {}\n", self.paint(GREEN, msg)),
        );
    }

    pub fn warn(&self, msg: &str) {
        self.raw(
            Channel::Err,
            &format!("{PREFIX} {} {msg}\n", self.paint(YELLOW, "Warning:")),
        );
    }

    pub fn error(&self, msg: &str) {
        self.raw(
            Channel::Err,
            &format!("{PREFIX} {} {msg}\n", self.paint(RED, "Error:")),
        );
    }

    pub fn plain(&self, msg: &str) {
        self.raw(Channel::Out, &format!("{msg}\n"));
    }
}

pub fn color_enabled(is_tty: bool) -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if std::env::var("FORCE_COLOR").is_ok_and(|v| v != "0") {
        return true;
    }
    is_tty
}
