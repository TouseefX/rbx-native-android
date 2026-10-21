//! Editing a layer's name or a note's text in place.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use rbx_dom::Ref;

use super::{Panel, Shell};

/// What the open input will write back when it commits.
pub(super) enum EditTarget {
    Layer(String),
    Note(Ref),
}

/// The text being typed in place of a row's label.
pub(super) struct Editing {
    pub(super) target: EditTarget,
    pub(super) input: Entity<InputState>,
    _subscription: Subscription,
}

impl Shell {
    /// Seeds the input with the full current text, since a row only shows a
    /// truncated one, selected so typing replaces it.
    pub(super) fn begin_edit(
        &mut self,
        target: EditTarget,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| InputState::new(window, cx).default_value(text.to_owned()));
        let subscription = cx.subscribe(&input, |shell, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                let text = input.read(cx).value().to_string();
                shell.commit_edit(&text, cx);
            }
        });
        input.update(cx, |state, cx| {
            state.focus(window, cx);
            state.select_all(window, cx);
        });
        self.layers.panel.editing = Some(Editing {
            target,
            input,
            _subscription: subscription,
        });
    }

    /// A note just pinned: shows the Layers dock and opens its text there.
    pub(in crate::shell) fn edit_new_note(
        &mut self,
        note: Ref,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_panel_open(Panel::Layers, true, cx);
        self.begin_edit(
            EditTarget::Note(note),
            crate::notes::DEFAULT_TEXT,
            window,
            cx,
        );
        cx.notify();
    }

    /// Enter and blur both commit, so the second finds nothing left to take.
    fn commit_edit(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(editing) = self.layers.panel.editing.take() else {
            return;
        };
        match editing.target {
            EditTarget::Layer(id) => {
                self.apply_layer_edit(|dom| crate::layers::rename_layer(dom, &id, text), cx);
            }
            EditTarget::Note(note) => {
                self.apply_layer_edit(|dom| crate::notes::set_text(dom, note, text), cx);
            }
        }
    }
}
