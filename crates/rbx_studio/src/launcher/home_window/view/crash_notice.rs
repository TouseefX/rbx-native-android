//! The last crash, offered where it can be copied until dismissed.
//!
//! A panic on the device takes the process with it and, without `adb`,
//! took its reason too. `crate::panic_log` keeps the last one; this is
//! Home holding it up: what the panic said, when, a Copy that pastes
//! the whole report, and a Dismiss that clears the way.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::*;
use crate::launcher::home_window::HomeWindow;
use crate::launcher::ui::{self, Weight};
use crate::tokens;

impl HomeWindow {
    /// The notice, or nothing when the last run closed cleanly.
    pub(super) fn crash_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let crash = self.last_crash.as_ref()?;
        let report = crash.text.clone();
        let when = crash.when as i64;
        Some(
            v_flex()
                .gap(px(12.))
                .child(Self::section_head("Last crash", None))
                .child(
                    h_flex()
                        .h(px(48.))
                        .items_center()
                        .gap(px(GAP))
                        .px(px(14.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(tokens::border())
                        .bg(ui::panel2())
                        .child(
                            ui::icon("triangle-alert", 14.)
                                .flex_none()
                                .text_color(tokens::warning()),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    ui::text(12.5, 17.)
                                        .truncate()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(tokens::text())
                                        .child("The app closed unexpectedly"),
                                )
                                .child(
                                    ui::text(11.5, 16.)
                                        .truncate()
                                        .text_color(tokens::text3())
                                        .child(format!(
                                            "Panicked {}. Copy pastes the report.",
                                            opened_label(when)
                                        )),
                                ),
                        )
                        .child(
                            ui::button("crash-copy", "Copy", Weight::Secondary, true).on_click(
                                cx.listener(move |_, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        report.clone(),
                                    ));
                                }),
                            ),
                        )
                        .child(
                            ui::button("crash-dismiss", "Dismiss", Weight::Ghost, true).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.last_crash = None;
                                    cx.notify();
                                }),
                            ),
                        ),
                )
                .into_any_element(),
        )
    }
}
