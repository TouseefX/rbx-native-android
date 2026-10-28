//! Face Preview's layout: the head card, the 5 expressions, the FACS
//! sliders by region and the check summary.

use std::time::Duration;

use gpui_kit::component::slider::SliderValue;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rbx_animation::facs::REGIONS;
use rbx_ugc::head::{HeadRule, ACTIONS, REQUIRED_POSES};
use rbx_ugc::Rule;

use super::super::super::rows::{rail_slider, toggle_pill};
use super::super::report;
use super::{values, Face, FaceWindow, Load};
use crate::launcher::ui::{self, Weight};
use crate::tokens;

/// "LeftEyeClosed" as "Left Eye Closed".
fn spaced(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

fn card() -> Div {
    v_flex()
        .flex_none()
        .gap(px(12.))
        .p(px(16.))
        .rounded(px(8.))
        .bg(ui::panel())
        .border_1()
        .border_color(tokens::border())
}

/// A card sliding up into place, `order` cards after the first.
fn entering(id: &'static str, order: u64, el: Div) -> AnyElement {
    if tokens::reduced_motion() {
        return el.into_any_element();
    }
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(220 + 60 * order)).with_easing(tokens::easing_soft),
        |el, delta| el.opacity(delta).mt(px(10. * (1. - delta))),
    )
    .into_any_element()
}

fn caption(text: &'static str) -> Div {
    ui::text(11., 14.)
        .font_weight(FontWeight::BOLD)
        .text_color(tokens::text2())
        .child(text)
}

/// Whether the check found the `index`th expression moving its landmark.
fn passes(face: &Face, index: usize) -> bool {
    !face.findings.iter().any(|f| {
        f.rule == Rule::Head(HeadRule::FacialAction) && f.message.starts_with(ACTIONS[index].name)
    })
}

impl FaceWindow {
    fn head_card(&self, cx: &mut Context<Self>) -> Div {
        let (title, detail) = match &self.load {
            Load::Ready(face) => {
                let found = face
                    .landmarks
                    .iter()
                    .filter(|l| l.missing == 0 && l.hits.iter().all(Option::is_some))
                    .count();
                (
                    face.name.clone(),
                    format!(
                        "{} FACS poses \u{b7} {found} of 3 landmarks on the face",
                        face.controls.len()
                    ),
                )
            }
            Load::Loading => ("Reading the head\u{2026}".into(), String::new()),
            Load::Idle(hint) => ("No head".into(), hint.to_string()),
            Load::Failed(message) => ("Could not read the head".into(), message.clone()),
        };
        let ready = self.face().is_some();
        let on = self.show_landmarks;
        h_flex()
            .gap(px(12.))
            .child(
                div()
                    .flex_none()
                    .size(px(40.))
                    .rounded(px(8.))
                    .bg(tokens::accent_soft())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(match self.load {
                        Load::Loading => ui::spinner("face-load-spin", 18.),
                        _ => ui::icon("scan-face", 20.)
                            .text_color(ui::accent())
                            .into_any_element(),
                    }),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        ui::text(14., 20.)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(tokens::text_strong())
                            .truncate()
                            .child(title),
                    )
                    .child(
                        ui::text(12., 16.)
                            .text_color(tokens::text2())
                            .truncate()
                            .child(detail),
                    ),
            )
            .when(ready, |el| {
                el.child(
                    h_flex()
                        .id("face-landmarks-toggle")
                        .gap(px(8.))
                        .cursor_pointer()
                        .child(
                            ui::text(12., 16.)
                                .text_color(tokens::text())
                                .child("Landmarks"),
                        )
                        .child(toggle_pill(Some(on)))
                        .on_click(cx.listener(|view, _, _, cx| view.toggle_landmarks(cx))),
                )
            })
            .child(
                ui::icon_button(
                    "face-use-selection",
                    "mouse-pointer-2",
                    "Use Selection",
                    Weight::Secondary,
                    true,
                )
                .on_click(cx.listener(|view, _, _, cx| view.use_selection(cx))),
            )
    }

    fn expressions(&self, cx: &mut Context<Self>) -> Div {
        let face = self.face();
        let chip = |id: &'static str, label: &'static str, ok: Option<bool>| {
            h_flex()
                .id(id)
                .h(px(30.))
                .px(px(12.))
                .gap(px(6.))
                .rounded(px(6.))
                .bg(ui::wash())
                .border_1()
                .border_color(tokens::border())
                .cursor_pointer()
                .hover(|el| el.bg(tokens::accent_soft()).border_color(ui::accent()))
                .when_some(ok, |el, ok| {
                    el.child(ui::status_dot(
                        if ok { "check" } else { "x" },
                        if ok { ui::green() } else { ui::red() },
                        Some(if ok { ui::green_soft() } else { ui::red_soft() }),
                        16.,
                    ))
                })
                .child(
                    ui::text(12., 16.)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tokens::text())
                        .child(label),
                )
        };
        const IDS: [&str; 5] = ["face-x-0", "face-x-1", "face-x-2", "face-x-3", "face-x-4"];
        const LABELS: [&str; 5] = ["Left Blink", "Right Blink", "Mouth Open", "Happy", "Sad"];
        let mut row = h_flex().flex_wrap().gap(px(8.)).child(
            chip("face-neutral", "Neutral", None)
                .on_click(cx.listener(|view, _, _, cx| view.expression(None, cx))),
        );
        for i in 0..ACTIONS.len() {
            row = row.child(
                chip(IDS[i], LABELS[i], face.map(|f| passes(f, i)))
                    .on_click(cx.listener(move |view, _, _, cx| view.expression(Some(i), cx))),
            );
        }
        card()
            .child(caption("EXPRESSIONS \u{b7} THE 5 VALIDATION TESTS"))
            .child(row)
    }

    fn controls(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let Some(face) = self.face() else {
            return card().child(
                ui::text(12.5, 18.)
                    .text_color(tokens::text2())
                    .child("The FACS sliders show up once a dynamic head is loaded."),
            );
        };
        let present: Vec<String> = face.controls.clone();
        let part = face.part;
        let now = values(&self.shell.read(cx).dom, part);
        let mut sections = Vec::new();
        for (region, channels) in REGIONS {
            let mut rows = Vec::new();
            for &channel in channels {
                let value = now.iter().find(|(n, _)| n == channel).map_or(0.0, |v| v.1);
                let has = present.iter().any(|c| c == channel);
                let required = REQUIRED_POSES.contains(&channel);
                let label = h_flex()
                    .w(px(190.))
                    .flex_none()
                    .gap(px(6.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(if has {
                                tokens::text()
                            } else {
                                tokens::text_muted()
                            })
                            .child(spaced(channel)),
                    )
                    .when(required, |el| {
                        el.child(
                            div()
                                .size(px(5.))
                                .flex_none()
                                .rounded_full()
                                .bg(ui::accent()),
                        )
                    });
                let control = if has {
                    let state = self.slider(channel, cx);
                    let shown = match state.read(cx).value() {
                        SliderValue::Single(v) | SliderValue::Range(_, v) => v,
                    };
                    if self.dragging != Some(channel) && (shown - value).abs() > 1e-4 {
                        state.update(cx, |s, cx| s.set_value(value, window, cx));
                    }
                    h_flex()
                        .flex_1()
                        .gap(px(10.))
                        .child(rail_slider(&state, cx))
                        .child(
                            ui::mono(11.5, 16.)
                                .w(px(34.))
                                .flex_none()
                                .text_color(tokens::text2())
                                .child(format!("{value:.2}")),
                        )
                        .into_any_element()
                } else {
                    ui::text(11.5, 16.)
                        .flex_1()
                        .text_color(tokens::text_muted())
                        .child("Not in this head")
                        .into_any_element()
                };
                rows.push(h_flex().h(px(30.)).gap(px(12.)).child(label).child(control));
            }
            sections.push(
                v_flex()
                    .gap(px(2.))
                    .child(caption(region).pb(px(4.)))
                    .children(rows),
            );
        }
        // The rail is drawn in the dock's own colour, so the sliders sit on
        // the raised field fill to read at all.
        card().gap(px(18.)).bg(ui::panel2()).children(sections)
    }

    fn footer(&self, cx: &mut Context<Self>) -> Div {
        let (glyph, fg, text) = match self.face() {
            Some(face) if face.findings.is_empty() => (
                "circle-check",
                ui::green(),
                "Passes the dynamic head checks".to_string(),
            ),
            Some(face) => (
                "circle-x",
                ui::red(),
                format!("Dynamic head checks: {}", report::summary(&face.findings)),
            ),
            None => ("info", tokens::text2(), "Nothing to check yet".to_string()),
        };
        let ready = self.face().is_some();
        h_flex()
            .flex_none()
            .gap(px(10.))
            .pt(px(12.))
            .border_t_1()
            .border_color(tokens::border())
            .child(ui::icon(glyph, 16.).text_color(fg))
            .child(
                ui::text(12.5, 18.)
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(tokens::text())
                    .child(text),
            )
            .when(ready, |el| {
                el.child(
                    ui::button(
                        "face-check-item",
                        "Open in Check Item",
                        Weight::Secondary,
                        true,
                    )
                    .on_click(cx.listener(|view, _, _, cx| view.check_item(cx))),
                )
            })
    }
}

impl Render for FaceWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let head = card().child(self.head_card(cx));
        let expressions = self.expressions(cx);
        let controls = self.controls(window, cx);
        let footer = self.footer(cx);
        v_flex()
            .size_full()
            .bg(tokens::dock())
            .font_family(tokens::font_ui())
            .child(crate::shell::chrome::window_topbar(
                "Face Preview".into(),
                true,
                |window, cx| window.defer(cx, |window, _| window.remove_window()),
            ))
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .p(px(16.))
                    .gap(px(12.))
                    .child(entering("face-in-head", 0, head))
                    .child(entering("face-in-expressions", 1, expressions))
                    .child(
                        div()
                            .id("face-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .child(entering("face-in-controls", 2, controls)),
                    )
                    .child(footer),
            )
    }
}
