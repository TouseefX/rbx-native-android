//! One row of the Output log: what was run or said, and how the panel
//! labels it.

use std::time::SystemTime;

use gpui_kit::SharedString;
use rbx_lua::PrintPart;

use super::game;
use super::row_kind::{format_timestamp, RowKind};
use crate::command_bar::Feedback;

/// Past this many characters a logged command's source is cut with an
/// ellipsis. Shorter than `command_bar::feedback`'s own 200: here the source
/// is a supporting label next to the result, not the thing being read.
pub(super) const SOURCE_MAX_LEN: usize = 80;

/// Past this many characters a row's result is cut. Rows wrap, so this is
/// far above the Command Bar label's 200, a path in a warning has to fit,
/// but still a cap: a `print` loop's whole output would otherwise wrap into
/// one row hundreds of lines tall.
pub(super) const LABEL_MAX_LEN: usize = 2000;

/// One run's worth of history: what was typed and what it did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutputEntry {
    /// Unique for the log's lifetime, unlike a position that shifts as the
    /// oldest entries drop: what a table's open/closed state is keyed by.
    pub(super) id: u64,
    pub(super) source: String,
    pub(super) feedback: Feedback,
    /// Captured unconditionally at [`OutputEntry::new`] time, cheap to keep
    /// on every entry, but only ever painted when the Output panel's own
    /// "Show Timestamp" toggle (`Shell::output_show_timestamps`) is on; see
    /// [`OutputEntry::timestamp_label`].
    pub(super) timestamp: SystemTime,
    /// Set for a message from the running game during Play (see `game`):
    /// such a row shows its Server/Client tag and, for an error, a link to
    /// where it happened instead of a Command Bar source.
    pub(super) game: Option<game::GameMessage>,
    /// A Command Bar run's `print` calls with their tables kept as trees
    /// (see `tables`); empty when nothing it printed was a table.
    pub(super) printed: Vec<Vec<PrintPart>>,
    /// Said by a plugin (see `shell::plugins`): Studio's **User Plugin**
    /// context.
    pub(super) plugin: bool,
}

impl OutputEntry {
    pub(super) fn new(id: u64, source: &str, feedback: Feedback) -> Self {
        OutputEntry {
            id,
            source: source.to_string(),
            feedback,
            timestamp: SystemTime::now(),
            game: None,
            printed: Vec::new(),
            plugin: false,
        }
    }

    pub(crate) fn is_plugin(&self) -> bool {
        self.plugin
    }

    /// Where the message came from: the game's side during Play, `None` for
    /// everything said in edit mode.
    pub(crate) fn context(&self) -> Option<game::Context> {
        self.game.as_ref().map(|message| message.context)
    }

    pub(crate) fn is_error(&self) -> bool {
        self.feedback.is_error()
    }

    /// Which color/icon family this row paints with, see `row_kind::RowKind`.
    pub(crate) fn kind(&self) -> RowKind {
        RowKind::of(&self.feedback)
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn feedback(&self) -> &Feedback {
        &self.feedback
    }

    /// Whether this entry answers a free-text search, matched against both
    /// halves of what the row actually shows, the command that was run and
    /// the result beside it. Case-insensitive, because nobody searching a log
    /// for `attempt to index nil` types it the way the error did.
    ///
    /// An empty query matches everything rather than nothing: the box is a
    /// narrowing on top of the level filter, not a second thing to satisfy.
    pub(super) fn matches_query(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let query = query.to_lowercase();
        self.source.to_lowercase().contains(&query) || self.label().to_lowercase().contains(&query)
    }

    /// The row's text. An error or a warning reads bare, as in Studio's
    /// Output: the row's icon and colour already say which it is.
    pub(super) fn label(&self) -> SharedString {
        match &self.feedback {
            Feedback::Error(text) | Feedback::Warning(text) => truncate(text, LABEL_MAX_LEN),
            feedback => feedback.label_within(LABEL_MAX_LEN),
        }
    }

    pub(super) fn truncated_source(&self) -> SharedString {
        truncate(&self.source, SOURCE_MAX_LEN)
    }

    pub(super) fn timestamp_label(&self) -> SharedString {
        format_timestamp(self.timestamp)
    }
}

/// Collapses embedded newlines into one row, then cuts to `max_len`
/// characters, the same shape as `command_bar::feedback::truncate`, kept
/// separate since the two truncate to different lengths for different
/// reasons (see that function's own doc comment).
pub(super) fn truncate(text: &str, max_len: usize) -> SharedString {
    let flat = text.replace('\n', "  ");
    match flat.char_indices().nth(max_len) {
        Some((cut, _)) => SharedString::from(format!("{}…", &flat[..cut])),
        None => SharedString::from(flat),
    }
}
