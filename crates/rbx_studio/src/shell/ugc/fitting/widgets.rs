//! The pieces every step is built from: the card, the step rail, choice
//! pills, number fields with steppers, toggles and the entrance motion. One
//! radius family throughout: 8 for cards, 6 for fields, the kit's 4 for
//! pills.

use std::time::Duration;

use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::inputs::Field;
use super::state::Step;
use super::FittingWindow;
use crate::launcher::ui;
use crate::shell::rows::toggle_pill;
use crate::tokens;

/// The gap between and the padding inside every card.
pub(super) const GAP: f32 = 12.;
pub(super) const PAD: f32 = 16.;

/// A card: a glyph tile, a title and a hint, then `body`.
pub(super) fn card(
    glyph: &'static str,
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
    body: impl IntoElement,
) -> Div {
    v_flex()
        .gap(px(GAP))
        .p(px(PAD))
        .rounded(px(8.))
        .bg(ui::panel())
        .border_1()
        .border_color(tokens::border())
        .child(
            h_flex()
                .gap(px(10.))
                .child(
                    div()
                        .flex_none()
                        .size(px(28.))
                        .rounded(px(6.))
                        .bg(tokens::accent_soft())
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(ui::icon(glyph, 15.).text_color(ui::accent())),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            ui::text(13.5, 18.)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(tokens::text_strong())
                                .child(title.into()),
                        )
                        .child(
                            ui::text(12., 16.)
                                .text_color(tokens::text2())
                                .child(hint.into()),
                        ),
                ),
        )
        .child(body)
}

/// A small caption over a group of controls.
pub(super) fn caption(text: impl Into<SharedString>) -> Div {
    ui::text(11., 14.)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(tokens::text3())
        .child(text.into())
}

/// A caption and its controls.
pub(super) fn group(label: impl Into<SharedString>, body: impl IntoElement) -> Div {
    v_flex().gap(px(6.)).child(caption(label)).child(body)
}

/// A clickable choice pill; `on` wears the accent.
pub(super) fn choice(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    glyph: Option<&'static str>,
    on: bool,
    enabled: bool,
) -> Stateful<Div> {
    let pill = ui::pill(label, glyph, on).h(px(26.)).px(px(10.)).id(id);
    if enabled {
        pill.cursor_pointer()
            .hover(|s| tokens::hover_fx(s).opacity(0.85))
    } else {
        pill.opacity(0.45)
    }
}

/// A row of wrapping choice pills.
pub(super) fn choices(pills: impl IntoIterator<Item = Stateful<Div>>) -> Div {
    h_flex().flex_wrap().gap(px(6.)).children(pills)
}

/// One number field with a stepper either side: `axis` names it inside the
/// box (X, Y, Z) and `unit` follows the value.
pub(super) fn number(
    field: Field,
    axis: &'static str,
    unit: &'static str,
    view: &FittingWindow,
    cx: &mut Context<FittingWindow>,
) -> Div {
    let step = field.step();
    let stepper = |glyph: &'static str, delta: f32, cx: &mut Context<FittingWindow>| {
        div()
            .id(SharedString::from(format!("ugc-fit-{field:?}-{glyph}")))
            .flex_none()
            .size(px(22.))
            .rounded(px(4.))
            .flex()
            .items_center()
            .justify_center()
            .text_color(tokens::text2())
            .cursor_pointer()
            .hover(|s| {
                tokens::hover_fx(s)
                    .bg(ui::wash())
                    .text_color(tokens::text())
            })
            .child(ui::icon(glyph, 12.))
            .on_click(cx.listener(move |v, _, window, cx| {
                let now = v.field_value(field);
                v.set_field(field, now + delta, window, cx);
            }))
    };
    h_flex()
        .flex_1()
        .min_w(px(96.))
        .h(px(30.))
        .gap(px(2.))
        .px(px(4.))
        .rounded(px(6.))
        .border_1()
        .border_color(tokens::border())
        .bg(ui::panel2())
        .when(!axis.is_empty(), |el| {
            el.child(
                ui::text(10.5, 14.)
                    .flex_none()
                    .w(px(14.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(tokens::text3())
                    .child(axis),
            )
        })
        .child(stepper("minus", -step, cx))
        .child(
            div().flex_1().min_w_0().h_full().child(
                Input::new(view.inputs.field(field))
                    .appearance(false)
                    .h_full()
                    .px(px(2.))
                    .text_size(px(12.))
                    .text_color(tokens::text()),
            ),
        )
        .when(!unit.is_empty(), |el| {
            el.child(
                ui::text(11., 14.)
                    .flex_none()
                    .text_color(tokens::text3())
                    .child(unit),
            )
        })
        .child(stepper("plus", step, cx))
}

/// A label and a toggle, the whole row clickable.
pub(super) fn toggle(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    h_flex()
        .id(id)
        .gap(px(8.))
        .py(px(2.))
        .cursor_pointer()
        .child(toggle_pill(Some(on)))
        .child(
            ui::text(12., 16.)
                .text_color(tokens::text())
                .child(label.into()),
        )
        .on_click(on_click)
}

/// A status line: a glyph and a sentence, tinted by `fg`.
pub(super) fn note(glyph: &'static str, fg: Rgba, text: impl Into<SharedString>) -> Div {
    h_flex()
        .items_start()
        .gap(px(8.))
        .child(ui::icon(glyph, 14.).flex_none().mt(px(1.)).text_color(fg))
        .child(
            ui::text(12., 17.)
                .flex_1()
                .min_w_0()
                .text_color(tokens::text2())
                .child(text.into()),
        )
}

/// The four steps across the top: done ones ticked, the current one in the
/// accent, the ones out of reach dimmed.
pub(super) fn rail(view: &FittingWindow, cx: &mut Context<FittingWindow>) -> Div {
    let current = view.step.index();
    let reach = view.source.is_some();
    let mut row = h_flex().items_center().gap(px(6.));
    for (i, step) in Step::ALL.into_iter().enumerate() {
        if i > 0 {
            let lit = i <= current;
            row = row.child(div().flex_1().h(px(2.)).rounded_full().bg(if lit {
                ui::accent()
            } else {
                tokens::border()
            }));
        }
        let (fill, fg, glyph) = if i == current {
            (ui::accent(), tokens::text_full(), None)
        } else if i < current {
            (tokens::accent_soft(), ui::accent(), Some("check"))
        } else {
            (ui::panel2(), tokens::text3(), None)
        };
        let open = step.reachable(reach);
        let dot = div()
            .flex_none()
            .size(px(22.))
            .rounded_full()
            .bg(fill)
            .text_color(fg)
            .flex()
            .items_center()
            .justify_center()
            .map(|el| match glyph {
                Some(g) => el.child(ui::icon(g, 12.)),
                None => el.child(
                    ui::text(11., 14.)
                        .font_weight(FontWeight::BOLD)
                        .child((i + 1).to_string()),
                ),
            });
        row = row.child(
            h_flex()
                .id(SharedString::from(format!("ugc-fit-rail-{i}")))
                .flex_none()
                .gap(px(6.))
                .px(px(4.))
                .py(px(2.))
                .rounded(px(6.))
                .when(!open, |el| el.opacity(0.5))
                .when(open, |el| {
                    el.cursor_pointer()
                        .hover(|s| tokens::hover_fx(s).bg(ui::wash()))
                        .on_click(cx.listener(move |v, _, _, cx| v.go(step, cx)))
                })
                .child(dot)
                .child(
                    ui::text(12., 16.)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if i == current {
                            tokens::text_strong()
                        } else {
                            tokens::text2()
                        })
                        .child(step.label()),
                ),
        );
    }
    row
}

/// Slides `el` up into place as it fades in; `key` restarts it.
pub(super) fn entrance(key: impl Into<SharedString>, el: Div, delay: usize) -> AnyElement {
    if tokens::reduced_motion() {
        return el.into_any_element();
    }
    let start = 0.12 * delay as f32;
    el.with_animation(
        key.into(),
        Animation::new(Duration::from_millis(260 + 60 * delay as u64))
            .with_easing(tokens::easing_soft),
        move |el, t| {
            let t = ((t - start) / (1.0 - start)).clamp(0.0, 1.0);
            el.opacity(t).mt(px(12. * (1. - t)))
        },
    )
    .into_any_element()
}
