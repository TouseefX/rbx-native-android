//! The offline intelligence behind the script editor: `lua_intel`'s project
//! index answering the kit editor's `CompletionProvider`, plus the pieces of
//! the signature-help line the panel draws under the code.
//!
//! The Luau language server supplies the same things once it is running; this
//! is what a tab has before that, and all it ever has on Android, where no
//! server is spawned. `attach_luau_lsp` overwrites the editor's provider slot
//! when the server does come up, so the two never answer at once.

use std::sync::{Arc, Mutex, OnceLock};

use anyhow::Result;
use gpui_kit::base::input::{CompletionProvider, Rope, RopeExt as _};
use gpui_kit::{App, Task, Window};
use lsp_types::{CompletionContext, CompletionResponse};
use rbx_dom::{Ref, WeakDom};
use serde_json::json;

use crate::lua_intel::{Completion, ProjectIndex, SignatureHelp};

/// A menu's worth of items; the rest are one more typed letter away.
const MAX_COMPLETIONS: usize = 200;

/// The published index every tab's provider and the signature line read. The
/// Shell swaps a fresh one in (see [`Intel::refresh`]); a request clones the
/// `Arc` out and answers off the lock, so a rebuild never holds a keystroke
/// on the mutex.
fn published() -> &'static Mutex<Arc<ProjectIndex>> {
    static PUBLISHED: OnceLock<Mutex<Arc<ProjectIndex>>> = OnceLock::new();
    PUBLISHED.get_or_init(|| Mutex::new(Arc::new(ProjectIndex::default())))
}

fn publish(index: Arc<ProjectIndex>) {
    *published()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = index;
}

/// The published index, for shell-side queries like the signature-help line.
pub(crate) fn index() -> Arc<ProjectIndex> {
    published()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// The Shell's handle on the index: refresh it as the DOM moves, and hand
/// out per-tab providers.
#[derive(Default)]
pub(crate) struct Intel {
    /// The DOM's mutation counter at the last look; an unchanged counter is
    /// one integer compare, so a panel render that changed nothing never
    /// even reaches the fingerprint.
    mutations: u64,
    /// The fingerprint the published index was built at; an unchanged
    /// fingerprint (a non-script edit, say) skips the rebuild.
    fingerprint: u64,
}

impl Intel {
    /// Rebuild the published index when the DOM's scripts moved. Called from
    /// `resync_scripts`, which already reconciles the open tabs against the
    /// DOM on every panel render, so every path that touches a script,
    /// typing, an undo, a rename, the Command Bar, a fresh place, reaches
    /// this with no hook of its own.
    pub(crate) fn refresh(&mut self, dom: &WeakDom) {
        let mutations = dom.mutations();
        if mutations == self.mutations {
            return;
        }
        let fingerprint = ProjectIndex::fingerprint(dom, std::iter::empty());
        let unchanged = fingerprint == self.fingerprint;
        self.mutations = mutations;
        self.fingerprint = fingerprint;
        if unchanged {
            return;
        }
        publish(Arc::new(ProjectIndex::build(dom)));
    }
}

/// The kit provider for the tab editing `script`.
pub(crate) fn provider(script: Ref) -> IntelProvider {
    IntelProvider { script }
}

/// One tab's offline completions.
pub(crate) struct IntelProvider {
    script: Ref,
}

impl CompletionProvider for IntelProvider {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        _trigger: CompletionContext,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Task<Result<CompletionResponse>> {
        let source = text.to_string();
        let caret = char_count(&source, offset);
        let index = index();
        let items = index
            .complete_at(self.script, &source, caret)
            .into_iter()
            .take(MAX_COMPLETIONS)
            .map(|completion| item(text, offset, completion))
            .collect();
        Task::ready(Ok(CompletionResponse::Array(items)))
    }

    /// The same contract the language-server path keeps: an identifier
    /// character, a member separator, or the quote that opens a string the
    /// engine completes the inside of (`require("`, `GetService("`)
    /// re-opens the menu.
    fn is_completion_trigger(&self, _offset: usize, new_text: &str, _cx: &mut App) -> bool {
        new_text
            .chars()
            .last()
            .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | ':' | '"' | '\''))
    }
}

/// The characters before `offset`, which is how the engine counts carets.
/// Any byte offset is accepted, counting the characters that end at or
/// before it, so an offset inside a multi-byte character answers as the
/// caret on that character's start, the way the editor's own offsets do.
pub(crate) fn char_count(source: &str, offset: usize) -> usize {
    let mut count = 0;
    let mut bytes = 0;
    for character in source.chars() {
        if bytes + character.len_utf8() > offset {
            break;
        }
        bytes += character.len_utf8();
        count += 1;
    }
    count
}

/// The byte offset of character `index`, or the end of `source`.
fn char_to_byte(source: &str, index: usize) -> usize {
    source
        .char_indices()
        .nth(index)
        .map_or(source.len(), |(byte, _)| byte)
}

/// One engine completion as an LSP item: its text replaces the
/// `replace_chars` characters before the caret. Assembled through
/// `serde_json` because `lsp_types`' item has no `Default` to build from,
/// and every field this leaves out deserializes to `None`.
fn item(text: &Rope, offset: usize, completion: Completion) -> lsp_types::CompletionItem {
    let source = text.to_string();
    let start = char_to_byte(
        &source,
        char_count(&source, offset).saturating_sub(completion.replace_chars),
    );
    let detail = (!completion.detail.is_empty()).then_some(completion.detail);
    let value = json!({
        "label": completion.label,
        "detail": detail,
        "textEdit": {
            "range": {
                "start": text.offset_to_position(start),
                "end": text.offset_to_position(offset),
            },
            "newText": completion.insert_text,
        },
    });
    serde_json::from_value(value).expect("a completion item shape lsp_types accepts")
}

/// The signature line's pieces: the callee's name, and each parameter with
/// whether it is the one being filled, so the panel can emphasize that one.
pub(crate) fn signature_pieces(help: &SignatureHelp) -> (String, Vec<(String, bool)>) {
    (
        help.name.clone(),
        help.parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| (parameter.clone(), help.active_parameter == Some(index)))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use lsp_types::CompletionTextEdit;
    use rbx_dom::Variant;

    #[test]
    fn caret_conversions_round_trip_through_multibyte_text() {
        let source = "local icon = \"\u{1F600}\" + name";
        // Every character boundary converts to a count and back exactly.
        let mut boundaries = source
            .char_indices()
            .map(|(byte, _)| byte)
            .collect::<Vec<_>>();
        boundaries.push(source.len());
        for byte in boundaries {
            assert_eq!(char_to_byte(source, char_count(source, byte)), byte);
        }
        // An offset inside a multi-byte character answers as the character
        // before it, never as the start of the text.
        let emoji = source.find('\u{1F600}').expect("the emoji is in the text");
        assert_eq!(char_count(source, emoji + 1), char_count(source, emoji));
        assert!(char_count(source, source.len()) < source.len());
    }

    #[test]
    fn items_replace_the_characters_the_engine_names() {
        let text = Rope::from("local ex = Widg");
        let offset = "local ex = Widg".len();
        let completion = Completion {
            label: "Widget".into(),
            detail: "Roblox datatype".into(),
            insert_text: "Widget".into(),
            replace_chars: 4,
        };
        let item = item(&text, offset, completion);
        assert_eq!(item.label, "Widget");
        assert_eq!(item.detail.as_deref(), Some("Roblox datatype"));
        let edit = match item.text_edit.expect("a plain edit") {
            CompletionTextEdit::Edit(edit) => edit,
            CompletionTextEdit::InsertAndReplace(_) => panic!("an insert-and-replace edit"),
        };
        assert_eq!(edit.new_text, "Widget");
        assert_eq!((edit.range.start.line, edit.range.start.character), (0, 11));
        assert_eq!((edit.range.end.line, edit.range.end.character), (0, 15));
    }

    #[test]
    fn refresh_publishes_an_index_that_completes_a_require() {
        let mut dom = WeakDom::new();
        let root = dom.new_instance("DataModel", "game", None);
        let storage = dom.new_instance("ReplicatedStorage", "ReplicatedStorage", Some(root));
        let module = dom.new_instance("ModuleScript", "Counter", Some(storage));
        dom.set_property(
            module,
            "Source",
            Variant::String("return { Add = function() end }".into()),
        )
        .expect("the module's Source writes");

        let mut intel = Intel::default();
        intel.refresh(&dom);
        let source = "local C = require(game.ReplicatedStorage.Counter)\nC.Ad";
        let suggestions = index().complete_at(module, source, source.chars().count());
        assert!(
            suggestions.iter().any(|item| item.label == "Add"),
            "{suggestions:?}"
        );
    }
}
