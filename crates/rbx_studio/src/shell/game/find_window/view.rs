//! Find in Game's controls and its place › script › line list.

use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::launcher::ui::{self, Weight};
use crate::tokens;

use super::{FindWindow, PlaceRows, ScriptRows};

impl FindWindow {
    pub(super) fn controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_query = !self.query.read(cx).value().is_empty();
        let replacing = has_query && !self.replacement.read(cx).value().is_empty();
        let (lines, scripts) = self.counts();
        let status = match (&self.notice, has_query) {
            (Some(notice), _) => notice.clone(),
            (None, false) => "Searches every script\u{2019}s Source in every place of the game. \
                              Not case-sensitive."
                .to_owned(),
            (None, true) if lines == 0 => "No matches.".to_owned(),
            (None, true) => format!(
                "{lines} matching line{} in {scripts} script{} across {} place{}{}",
                if lines == 1 { "" } else { "s" },
                if scripts == 1 { "" } else { "s" },
                self.results.len(),
                if self.results.len() == 1 { "" } else { "s" },
                if self.truncated {
                    " (list cut short)"
                } else {
                    ""
                },
            ),
        };
        v_flex()
            .flex_none()
            .gap(px(8.))
            .p(px(12.))
            .border_b_1()
            .border_color(tokens::border())
            .child(Input::new(&self.query).small())
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(div().flex_1().child(Input::new(&self.replacement).small()))
                    .child(if replacing && lines > 0 {
                        ui::button("find-in-game-replace", "Replace All", Weight::Primary, true)
                            .on_click(cx.listener(|this, _, _, cx| this.replace_all(cx)))
                            .into_any_element()
                    } else {
                        ui::disabled_button("find-in-game-replace", "Replace All")
                            .into_any_element()
                    }),
            )
            .child(
                ui::text(11.5, 16.)
                    .text_color(tokens::text3())
                    .child(status),
            )
    }

    fn counts(&self) -> (usize, usize) {
        self.results.iter().fold((0, 0), |(lines, scripts), place| {
            (
                lines
                    + place
                        .scripts
                        .iter()
                        .map(|s| s.hits.lines.len())
                        .sum::<usize>(),
                scripts + place.scripts.len(),
            )
        })
    }

    pub(super) fn results_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let replacing = !self.replacement.read(cx).value().is_empty();
        v_flex()
            .id("find-in-game-results")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .py(px(6.))
            .children(
                self.results
                    .iter()
                    .map(|place| place_view(place, replacing, cx)),
            )
    }
}

fn place_view(
    place: &PlaceRows,
    replacing: bool,
    cx: &mut Context<FindWindow>,
) -> impl IntoElement {
    let lines: usize = place.scripts.iter().map(|s| s.hits.lines.len()).sum();
    v_flex()
        .child(
            h_flex()
                .h(px(28.))
                .px(px(12.))
                .gap(px(8.))
                .items_center()
                .bg(tokens::chrome())
                .child(ui::icon("map", 13.))
                .child(
                    ui::text(12.5, 17.)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(place.label.clone()),
                )
                .child(
                    ui::text(11.5, 16.)
                        .text_color(tokens::text3())
                        .child(format!("{lines} line{}", if lines == 1 { "" } else { "s" })),
                )
                .when(place.from_file, |this| {
                    this.child(
                        ui::text(11., 15.)
                            .text_color(tokens::text3())
                            .child("\u{b7} not opened, read from file"),
                    )
                }),
        )
        .children(
            place
                .scripts
                .iter()
                .enumerate()
                .map(|(index, script)| script_view(place.slot, index, script, replacing, cx)),
        )
}

fn script_view(
    slot: usize,
    index: usize,
    script: &ScriptRows,
    replacing: bool,
    cx: &mut Context<FindWindow>,
) -> impl IntoElement {
    let rows = script.hits.lines.iter().enumerate().map(|(row, hit)| {
        let range = hit.range.clone();
        let preview = script.previews.iter().find(|p| p.line == hit.line);
        h_flex()
            .id(SharedString::from(format!("find-hit-{slot}-{index}-{row}")))
            .pl(px(44.))
            .pr(px(12.))
            .py(px(2.))
            .gap(px(10.))
            .items_start()
            .cursor_pointer()
            .hover(|this| tokens::hover_fx(this).bg(tokens::hover_subtle()))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.reveal(slot, index, range.clone(), cx);
            }))
            .child(
                ui::mono(11.5, 17.)
                    .w(px(40.))
                    .flex_none()
                    .text_color(tokens::text3())
                    .child(hit.line.to_string()),
            )
            .child(match preview.filter(|_| replacing) {
                Some(preview) => v_flex()
                    .min_w_0()
                    .child(
                        ui::mono(11.5, 17.)
                            .truncate()
                            .text_color(ui::red())
                            .child(format!("- {}", preview.before.trim_start())),
                    )
                    .child(
                        ui::mono(11.5, 17.)
                            .truncate()
                            .text_color(ui::green())
                            .child(format!("+ {}", preview.after.trim_start())),
                    )
                    .into_any_element(),
                None => ui::mono(11.5, 17.)
                    .min_w_0()
                    .truncate()
                    .child(hit.text.clone())
                    .into_any_element(),
            })
    });
    v_flex()
        .child(
            h_flex()
                .h(px(24.))
                .pl(px(24.))
                .pr(px(12.))
                .gap(px(8.))
                .items_center()
                .child(ui::icon("file-code", 12.))
                .child(
                    ui::text(12., 16.)
                        .truncate()
                        .child(script.hits.path.clone()),
                )
                .child(
                    ui::text(11., 15.)
                        .text_color(tokens::text3())
                        .child(script.hits.class.clone()),
                ),
        )
        .children(rows)
}
