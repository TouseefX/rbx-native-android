//! The Layers dock: the place's layers with their counts and this viewer's
//! hide/lock/solo/isolate toggles, and its 3D notes. Every edit goes
//! through `shell::layers`.
//!
//! An Explorer drag dropped on a layer row assigns the selection: pressing a
//! row outside the selection replaces it, so the selection is what is being
//! dragged.
//!
//! `RBX_STUDIO_LAYERS=1` opens the dock at startup, a screenshot aid like the
//! other `RBX_STUDIO_*` variables.

mod edit;
mod rows;

use std::collections::BTreeSet;

use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, v_flex, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::layout::Panel;
use super::{chrome, menu, Shell};
use crate::folder_colors::Rgb;
use crate::layers::PALETTE;
use crate::tokens;

/// The dock's own widget state.
#[derive(Default)]
pub(super) struct PanelState {
    editing: Option<edit::Editing>,
    scroll: ScrollHandle,
}

const LAYERS_VARIABLE: &str = "RBX_STUDIO_LAYERS";

fn color_of((r, g, b): Rgb) -> Rgba {
    rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))
}

fn element_id(kind: &str, key: impl std::fmt::Display) -> SharedString {
    format!("{kind}-{key}").into()
}

fn flip(set: &mut BTreeSet<String>, id: &str) {
    if !set.remove(id) {
        set.insert(id.to_owned());
    }
}

fn next_color(current: Rgb) -> Rgb {
    let at = PALETTE.iter().position(|&c| c == current);
    PALETTE[at.map_or(0, |i| (i + 1) % PALETTE.len())]
}

/// A small icon toggle; lit while `on`.
fn toggle(id: SharedString, icon: IconName, on: bool, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .size(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(tokens::radius_badge())
        .cursor_pointer()
        .text_color(if on { tokens::text() } else { tokens::text3() })
        .when(on, |this| this.bg(tokens::accent_soft()))
        .hover(|this| this.bg(tokens::hover_subtle()))
        .tooltip(move |window, cx| super::tooltip::text(label, window, cx))
        .child(Icon::new(icon).size(px(14.)))
}

fn heading(text: &'static str) -> Div {
    div()
        .px(px(8.))
        .pt(px(8.))
        .pb(px(2.))
        .text_size(tokens::text_xs())
        .line_height(tokens::line_xs())
        .text_color(tokens::text3())
        .child(text)
}

fn empty(text: &'static str) -> Div {
    div()
        .px(px(8.))
        .py(tokens::row_padding())
        .text_size(tokens::text_sm())
        .text_color(tokens::text3())
        .child(text)
}

impl Shell {
    /// `RBX_STUDIO_LAYERS=1`: see this module's doc comment.
    pub(super) fn apply_debug_layers(&mut self, cx: &mut Context<Self>) {
        if std::env::var_os(LAYERS_VARIABLE).is_some() {
            self.set_panel_open(Panel::Layers, true, cx);
        }
    }

    /// The dock's trailing controls and body (see `shell::workspace`).
    pub(super) fn layers_dock(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Option<AnyElement>, Option<AnyElement>) {
        let overflow = menu::dropdown(
            self,
            menu::MenuId::LayersOverflow,
            chrome::dock_options_trigger(
                "layers-overflow",
                IconName::Ellipsis,
                16.,
                "Layers dock options",
            ),
            self.move_items(Panel::Layers),
            cx,
        )
        .into_any_element();

        let layers = crate::layers::layers(&self.dom);
        let counts = crate::layers::counts(&self.dom);
        let notes = crate::notes::notes(&self.dom);
        let layer_rows: Vec<AnyElement> = layers
            .iter()
            .map(|layer| {
                let count = counts.get(&layer.id).copied().unwrap_or(0);
                self.layer_row(layer, count, cx)
            })
            .collect();
        let note_rows: Vec<AnyElement> = notes.iter().map(|note| self.note_row(note, cx)).collect();

        let body = v_flex()
            .size_full()
            .border_t_1()
            .border_color(tokens::border())
            .child(self.layers_toolbar(cx))
            .child(
                v_flex()
                    .id("layers-scroll")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.layers.panel.scroll)
                    .child(heading("Layers"))
                    .when(layer_rows.is_empty(), |this| {
                        this.child(empty("No layers yet"))
                    })
                    .children(layer_rows)
                    .child(heading("Notes"))
                    .when(note_rows.is_empty(), |this| this.child(empty("No notes")))
                    .children(note_rows),
            )
            .into_any_element();
        (Some(overflow), Some(body))
    }

    fn layers_toolbar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .w_full()
            .flex_none()
            .items_center()
            .gap(tokens::label_gap())
            .px(px(8.))
            .py(tokens::row_padding())
            .border_b_1()
            .border_color(tokens::border())
            .child(
                chrome::button("layers-add", "+ Layer", false)
                    .track_focus(&self.tab_order.claim(cx))
                    .on_click(cx.listener(|shell, _, _, cx| shell.add_layer(cx))),
            )
            .child(
                chrome::button("layers-assign", "Assign selection", false)
                    .track_focus(&self.tab_order.claim(cx))
                    .on_click(
                        cx.listener(|shell, _, _, cx| shell.assign_selection_to_active_layer(cx)),
                    ),
            )
            .child(
                chrome::button("layers-pin", "Pin note", false)
                    .track_focus(&self.tab_order.claim(cx))
                    .on_click(cx.listener(|shell, _, _, cx| shell.pin_note("Note", cx))),
            )
            .into_any_element()
    }
}
