//! The building blocks every wizard step's body is made of.

use super::ui;
use crate::tokens;
use gpui_kit::component::{h_flex, v_flex};

use super::*;

impl Wizard {
    pub(super) fn heading(
        title: &'static str,
        text: &'static str,
        action: Option<AnyElement>,
    ) -> impl IntoElement {
        h_flex()
            .items_start()
            .gap(px(16.))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.))
                    .child(
                        ui::text(20., 28.)
                            .font_weight(FontWeight::BOLD)
                            .text_color(tokens::text())
                            .child(title),
                    )
                    .child(ui::text(12.5, 19.).text_color(tokens::text2()).child(text)),
            )
            .children(action)
    }

    pub(super) fn big_glyph(
        glyph: &'static str,
        fg: Rgba,
        fill: Rgba,
        round: bool,
    ) -> impl IntoElement {
        h_flex()
            .size(px(48.))
            .flex_none()
            .rounded(if round { px(24.) } else { px(12.) })
            .bg(fill)
            .text_color(fg)
            .items_center()
            .justify_center()
            .child(ui::icon(glyph, 22.))
    }

    pub(super) fn feature_card(
        glyph: &'static str,
        title: SharedString,
        text: SharedString,
    ) -> impl IntoElement {
        h_flex()
            .items_start()
            .gap(px(12.))
            .py(px(14.))
            .px(px(16.))
            .rounded(px(8.))
            .border_1()
            .border_color(tokens::border())
            .bg(ui::panel2())
            .child(
                h_flex()
                    .size(px(32.))
                    .flex_none()
                    .rounded(px(7.))
                    .bg(tokens::accent_soft())
                    .text_color(ui::accent())
                    .items_center()
                    .justify_center()
                    .child(ui::icon(glyph, 16.)),
            )
            .child(
                v_flex()
                    .gap(px(2.))
                    .child(
                        ui::text(12.5, 17.)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(tokens::text())
                            .child(title),
                    )
                    .child(ui::text(12., 18.).text_color(tokens::text2()).child(text)),
            )
    }
}
