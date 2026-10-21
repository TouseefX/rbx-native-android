//! Sync Shared's header, per-place list and footer.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::launcher::ui::{self, Weight};
use crate::tokens;

use super::{counts, SyncWindow, Target};

impl SyncWindow {
    pub(super) fn header(&self, _: &mut Context<Self>) -> impl IntoElement {
        let text = if self.assets.is_empty() {
            format!(
                "{} holds no shared assets. Select a Model, Folder or ModuleScript and use \
                 Model \u{203a} Mark as Shared; copy it into the other places, and Sync Shared \
                 keeps every copy like this one.",
                self.source_label
            )
        } else {
            format!(
                "From {}: {}. Each place below holding the same asset gets this version.",
                self.source_label,
                self.assets
                    .iter()
                    .map(|(_, name)| name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        v_flex()
            .flex_none()
            .gap(px(4.))
            .p(px(12.))
            .border_b_1()
            .border_color(tokens::border())
            .child(ui::text(12.5, 18.).child(text))
            .children(self.notice.clone().map(|notice| {
                ui::text(11.5, 16.)
                    .text_color(tokens::text3())
                    .child(notice)
            }))
    }

    pub(super) fn list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("sync-shared-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(self.targets.iter().map(|target| target_view(target, cx)))
    }

    pub(super) fn footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.targets.iter().filter(|t| t.apply).count();
        h_flex()
            .h(px(57.))
            .flex_none()
            .items_center()
            .justify_end()
            .gap(px(8.))
            .px(px(16.))
            .border_t_1()
            .border_color(tokens::border())
            .child(
                ui::button("sync-refresh", "Refresh", Weight::Secondary, false)
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            )
            .child(if count == 0 {
                ui::disabled_button("sync-apply", "Apply").into_any_element()
            } else {
                ui::button(
                    "sync-apply",
                    format!(
                        "Apply to {count} place{}",
                        if count == 1 { "" } else { "s" }
                    ),
                    Weight::Primary,
                    false,
                )
                .on_click(cx.listener(|this, _, _, cx| this.apply(cx)))
                .into_any_element()
            })
    }
}

fn target_view(target: &Target, cx: &mut Context<SyncWindow>) -> impl IntoElement {
    let slot = target.slot;
    let changed = target.changed();
    let status = if target.pairs.is_empty() {
        "Holds none of these assets".to_owned()
    } else if changed {
        format!(
            "{} of {} asset{} differ",
            target.pairs.iter().filter(|p| !p.nodes.is_empty()).count(),
            target.pairs.len(),
            if target.pairs.len() == 1 { "" } else { "s" }
        )
    } else {
        "Up to date".to_owned()
    };
    v_flex()
        .border_b_1()
        .border_color(tokens::border())
        .child(
            h_flex()
                .id(("sync-place", slot))
                .h(px(36.))
                .px(px(12.))
                .gap(px(10.))
                .items_center()
                .bg(tokens::chrome())
                .when(changed, |this| {
                    this.cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle(slot, cx)))
                })
                .child(checkbox(target.apply, changed))
                .child(ui::icon("map", 13.))
                .child(
                    ui::text(12.5, 17.)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(target.label.clone()),
                )
                .child(
                    ui::text(11.5, 16.)
                        .text_color(tokens::text3())
                        .child(status),
                )
                .when(target.from_file, |this| {
                    this.child(
                        ui::text(11., 15.)
                            .text_color(tokens::text3())
                            .child("\u{b7} not opened, read from file"),
                    )
                }),
        )
        .children(target.pairs.iter().enumerate().map(|(index, pair)| {
            let (updated, added, removed) = counts(&pair.nodes);
            h_flex()
                .h(px(30.))
                .pl(px(44.))
                .pr(px(12.))
                .gap(px(10.))
                .items_center()
                .child(ui::icon("package", 12.))
                .child(ui::text(12., 16.).child(pair.name.clone()))
                .child(
                    ui::text(11., 15.)
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(tokens::text3())
                        .child(pair.target_path.clone()),
                )
                .child(if pair.nodes.is_empty() {
                    ui::text(11.5, 16.)
                        .text_color(tokens::text3())
                        .child("identical")
                        .into_any_element()
                } else {
                    h_flex()
                        .gap(px(8.))
                        .items_center()
                        .child(
                            ui::mono(11., 15.)
                                .text_color(tokens::text3())
                                .child(format!("~{updated} +{added} \u{2212}{removed}")),
                        )
                        .child(
                            ui::button(
                                SharedString::from(format!("sync-diff-{slot}-{index}")),
                                "View diff",
                                Weight::Ghost,
                                true,
                            )
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.view_diff(slot, index, cx)),
                            ),
                        )
                        .into_any_element()
                })
        }))
}

fn checkbox(on: bool, enabled: bool) -> impl IntoElement {
    div()
        .size(px(14.))
        .flex_none()
        .rounded(px(3.))
        .border_1()
        .border_color(if on { ui::accent() } else { tokens::border() })
        .when(on, |this| this.bg(ui::accent()))
        .when(!enabled, |this| this.opacity(0.4))
        .flex()
        .items_center()
        .justify_center()
        .when(on, |this| {
            this.text_color(tokens::text_full())
                .child(ui::icon("check", 10.))
        })
}
