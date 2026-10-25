//! The pieces every Avatar Settings tab is built from, drawn after
//! Studio's dock: a collapsible section strip, a full-width segmented
//! control with the chosen option's sentence under it, square checkboxes,
//! and the custom asset rows (tick, name, id field).

use gpui_kit::assets::IconName;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{h_flex, v_flex, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::avatar_settings::{Rules, SlotValue, Writes};
use crate::tokens;

use super::super::rows::text_field;
use super::super::Shell;
use super::fields::{Field, Group};

/// A section's strip and, unless folded shut, its contents.
pub(super) fn section(
    shell: &Shell,
    title: &'static str,
    rows: Vec<AnyElement>,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let open = !shell.avatar.collapsed.contains(title);
    let header = h_flex()
        .id(SharedString::from(format!("avatar-section-{title}")))
        .w_full()
        .h(px(26.))
        .flex_none()
        .px(px(10.))
        .gap(px(8.))
        .items_center()
        .bg(tokens::hover_subtle())
        .border_y_1()
        .border_color(tokens::border())
        .cursor_pointer()
        .text_size(tokens::text_sm())
        .line_height(tokens::line_sm())
        .font_weight(tokens::WEIGHT_SEMIBOLD)
        .text_color(tokens::text_full())
        .hover(|this| tokens::hover_fx(this).bg(tokens::hover()))
        .focus_visible(|this| this.shadow(tokens::focus_ring_inset()))
        .on_click(cx.listener(move |shell, _, _, cx| shell.toggle_avatar_section(title, cx)))
        .child(
            Icon::new(if open {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            })
            .size(px(12.))
            .text_color(tokens::text3()),
        )
        .child(title);
    v_flex()
        .w_full()
        .child(header)
        .when(open, |this| {
            this.child(
                v_flex()
                    .w_full()
                    .gap(px(10.))
                    .pt(px(14.))
                    .pb(px(14.))
                    .px(px(24.))
                    .children(rows),
            )
        })
        .into_any_element()
}

/// One option of a segmented control: its value, label and the sentence
/// shown under the control while it's chosen.
pub(super) type Choice = (u32, &'static str, &'static str);

/// Studio's full-width segmented control, then the chosen option's
/// sentence. `pick` turns a click into the writes it makes.
pub(super) fn segmented(
    id: &'static str,
    choices: &[Choice],
    current: u32,
    cx: &mut Context<Shell>,
    pick: fn(u32) -> Writes,
) -> AnyElement {
    let sentence = choices
        .iter()
        .find(|choice| choice.0 == current)
        .map_or("", |choice| choice.2);
    let bar = h_flex()
        .w_full()
        .p(px(2.))
        .gap(px(2.))
        .rounded(tokens::radius_segment())
        .bg(tokens::field_select())
        .children(choices.iter().map(|&(value, text, _)| {
            let on = value == current;
            div()
                .id(SharedString::from(format!("{id}-{value}")))
                .flex_1()
                .min_w_0()
                .h(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(tokens::radius_tiny())
                .cursor_pointer()
                .truncate()
                .text_size(tokens::text_sm())
                .line_height(tokens::line_sm())
                .text_color(if on {
                    tokens::text_full()
                } else {
                    tokens::text2()
                })
                .when(on, |this| this.bg(tokens::accent_soft()))
                .when(!on, |this| {
                    this.hover(|this| tokens::hover_fx(this).bg(tokens::hover()))
                })
                .focus_visible(|this| this.shadow(tokens::focus_ring_inset()))
                .on_click(
                    cx.listener(move |shell, _, _, cx| shell.apply_avatar_writes(pick(value), cx)),
                )
                .child(text)
        }));
    v_flex()
        .w_full()
        .gap(px(6.))
        .child(bar)
        .when(!sentence.is_empty(), |this| this.child(note(sentence)))
        .into_any_element()
}

/// A muted line of explanation.
pub(super) fn note(text: &'static str) -> Div {
    div()
        .text_size(tokens::text_sm())
        .line_height(tokens::line_sm())
        .text_color(tokens::text3())
        .child(text)
}

/// A small heading inside a section ("Height", "Limit Bounds").
pub(super) fn label(text: &'static str) -> Div {
    div()
        .text_size(tokens::text_sm())
        .line_height(tokens::line_sm())
        .text_color(tokens::text2())
        .child(text)
}

/// Studio's square checkbox, alone.
pub(super) fn tick(checked: bool) -> Div {
    div()
        .flex_none()
        .size(px(14.))
        .rounded(px(3.))
        .flex()
        .items_center()
        .justify_center()
        .map(|this| {
            if checked {
                this.bg(tokens::check_on()).child(
                    Icon::new(IconName::Check)
                        .size(px(11.))
                        .text_color(tokens::knob()),
                )
            } else {
                this.border_1().border_color(tokens::text3())
            }
        })
}

/// A checkbox and its label, the whole row clickable.
pub(super) fn check_row(
    id: impl Into<SharedString>,
    text: &'static str,
    checked: bool,
    cx: &mut Context<Shell>,
    on_click: impl Fn(&mut Shell, &mut Context<Shell>) + 'static,
) -> AnyElement {
    h_flex()
        .id(ElementId::Name(id.into()))
        .h(px(24.))
        .gap(px(8.))
        .items_center()
        .cursor_pointer()
        .rounded(tokens::radius_badge())
        .focus_visible(|this| this.shadow(tokens::focus_ring_inset()))
        .on_click(cx.listener(move |shell, _, _, cx| on_click(shell, cx)))
        .child(tick(checked))
        .child(
            div()
                .text_size(tokens::text_sm())
                .line_height(tokens::line_sm())
                .text_color(tokens::text())
                .child(text),
        )
        .into_any_element()
}

/// A number field with its unit after it.
pub(super) fn field(input: &Entity<InputState>, width: f32, unit: &'static str, cx: &App) -> Div {
    h_flex()
        .gap(px(6.))
        .items_center()
        .child(div().w(px(width)).child(text_field(input, 0, cx)))
        .when(!unit.is_empty(), |this| this.child(note(unit)))
}

/// A row of fields with a separator between them ("min – max").
pub(super) fn fields_row(parts: Vec<Div>, between: &'static str, unit: &'static str) -> Div {
    let last = parts.len().saturating_sub(1);
    h_flex()
        .gap(px(6.))
        .items_center()
        .children(parts.into_iter().enumerate().flat_map(|(i, part)| {
            let gap = (i < last && !between.is_empty()).then(|| note(between).into_any_element());
            std::iter::once(part.into_any_element()).chain(gap)
        }))
        .when(!unit.is_empty(), |this| this.child(note(unit)))
}

/// The custom asset rows of one table: name, tick, id, in Studio's
/// order. A ticked row with no id is a removal, which only clothing and
/// accessories allow.
pub(super) fn slot_rows(shell: &Shell, group: Group, cx: &mut Context<Shell>) -> Vec<AnyElement> {
    let rules = Rules::find(&shell.dom);
    let Some(fields) = &shell.avatar.fields else {
        return Vec::new();
    };
    group
        .slots()
        .iter()
        .enumerate()
        .map(|(row, slot)| {
            let value = slot.read(&rules, &shell.dom);
            let input = fields.input(Field::Slot(group, row));
            h_flex()
                .w_full()
                .gap(px(8.))
                .items_center()
                .child(
                    h_flex()
                        .id(SharedString::from(format!("avatar-slot-{group:?}-{row}")))
                        .w(px(136.))
                        .flex_none()
                        .gap(px(8.))
                        .items_center()
                        .cursor_pointer()
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.toggle_avatar_slot(group, row, window, cx)
                        }))
                        .child(
                            div()
                                .flex_1()
                                .truncate()
                                .text_size(tokens::text_sm())
                                .line_height(tokens::line_sm())
                                .text_color(tokens::text())
                                .child(slot.label),
                        )
                        .child(tick(value != SlotValue::Off)),
                )
                .child(div().w(px(180.)).child(text_field(input, 0, cx)))
                .when(value == SlotValue::Remove, |this| {
                    this.child(note("Removed"))
                })
                .into_any_element()
        })
        .collect()
}
