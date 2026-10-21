//! Which of Row A's editors have a tab. The place tab always does; the
//! Script, UI and Animation editors close from their tab's ×, a middle
//! click or Delete, and come back from the strip's "+" or from anything
//! that opens one (a script, a ScreenGui, a rig), because every one of
//! those goes through `Shell::set_document`.

use super::chrome::Document;
use super::{menu, Shell};
use gpui_kit::Context;

impl Document {
    /// The name a closed editor is saved under in `settings.json`, stable
    /// across renames of the tab's label.
    fn key(self) -> &'static str {
        match self {
            Document::Viewport => "viewport",
            Document::Scripts => "scripts",
            Document::UiEditor => "ui_editor",
            Document::Animation => "animation",
        }
    }

    /// Only the editors close. The place tab is the place itself, and
    /// closing it means closing the place, which File already does.
    pub(super) fn closable(self) -> bool {
        self != Document::Viewport
    }
}

/// The closed editors, in no particular order; everything else has a tab.
/// Keeping the closed ones rather than the open ones means an editor added
/// later shows up for someone whose settings predate it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct OpenDocuments {
    closed: Vec<Document>,
}

impl OpenDocuments {
    /// From the saved keys. An unknown key, a repeat or the place's own is
    /// dropped, so a hand-edited file can't hide the place tab.
    pub(super) fn restore(saved: &[String]) -> Self {
        OpenDocuments {
            closed: Document::ALL
                .into_iter()
                .filter(|document| {
                    document.closable() && saved.iter().any(|key| key == document.key())
                })
                .collect(),
        }
    }

    pub(super) fn saved(&self) -> Vec<String> {
        self.closed
            .iter()
            .map(|document| document.key().to_owned())
            .collect()
    }

    pub(super) fn is_open(&self, document: Document) -> bool {
        !self.closed.contains(&document)
    }

    /// Whether this gave `document` its tab back.
    pub(super) fn open(&mut self, document: Document) -> bool {
        let before = self.closed.len();
        self.closed.retain(|closed| *closed != document);
        self.closed.len() != before
    }

    /// Whether this took `document`'s tab away.
    pub(super) fn close(&mut self, document: Document) -> bool {
        if !document.closable() || self.closed.contains(&document) {
            return false;
        }
        self.closed.push(document);
        true
    }

    /// The tabs, in strip order.
    pub(super) fn tabs(&self) -> Vec<Document> {
        Document::ALL
            .into_iter()
            .filter(|document| self.is_open(*document))
            .collect()
    }

    /// The closed editors, in strip order, for the "+" menu.
    pub(super) fn closed(&self) -> Vec<Document> {
        Document::ALL
            .into_iter()
            .filter(|document| !self.is_open(*document))
            .collect()
    }
}

impl Shell {
    /// Closes an editor's tab. Closing the one showing lands back on the
    /// place, the tab that can't close.
    pub(super) fn close_document(&mut self, document: Document, cx: &mut Context<Self>) {
        if !self.documents.close(document) {
            return;
        }
        if self.document == document {
            self.set_document(Document::Viewport, cx);
        }
        self.save_settings();
        cx.notify();
    }

    /// Gives `document` its tab back if it was closed, for `set_document`.
    pub(super) fn reopen_document(&mut self, document: Document) {
        if self.documents.open(document) {
            self.save_settings();
        }
    }

    /// The "+" menu: another place, then a row per closed editor.
    pub(super) fn document_add_items(&self) -> Vec<menu::Item> {
        let mut items = vec![
            menu::item("New Place").on_click(|shell, cx| shell.request_new(cx)),
            menu::item("Open…").on_click(|shell, cx| shell.open_place_dialog(cx)),
        ];
        let closed = self.documents.closed();
        if !closed.is_empty() {
            items.push(menu::separator());
        }
        items.extend(closed.into_iter().map(|document| {
            menu::item(document.name())
                .icon(document.icon())
                .on_click(move |shell, cx| shell.set_document(document, cx))
        }));
        items
    }
}

#[cfg(test)]
#[path = "documents/tests.rs"]
mod tests;
