//! The layer and note rows of the Layers dock.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::edit::EditTarget;
use super::{color_of, element_id, flip, next_color, toggle, Shell};
use crate::layers::{Layer, LayerView};
use crate::notes::Note;
use crate::shell::reparent::DraggedInstances;
use crate::tokens;

/// What a view toggle looks like, apart from its layer and state.
#[derive(Clone, Copy)]
struct ToggleSpec {
    kind: &'static str,
    icon: IconName,
    label: &'static str,
}

impl Shell {
    fn view_toggle(
        &self,
        spec: ToggleSpec,
        id: &str,
        on: bool,
        change: fn(&mut LayerView, &str),
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let id = id.to_owned();
        toggle(element_id(spec.kind, &id), spec.icon, on, spec.label).on_click(cx.listener(
            move |shell, _, _, cx| {
                cx.stop_propagation();
                shell.update_layer_view(|view| change(view, &id), cx);
            },
        ))
    }

    /// The label, or the input while this row's text is being edited.
    fn row_text(&self, target: &EditTarget, label: SharedString) -> AnyElement {
        let editing = self
            .layers
            .panel
            .editing
            .as_ref()
            .filter(|e| match (&e.target, target) {
                (EditTarget::Layer(a), EditTarget::Layer(b)) => a == b,
                (EditTarget::Note(a), EditTarget::Note(b)) => a == b,
                _ => false,
            })
            .map(|e| e.input.clone());
        match editing {
            Some(input) => div()
                .flex_1()
                .min_w_0()
                .child(Input::new(&input).xsmall())
                .into_any_element(),
            None => div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(label)
                .into_any_element(),
        }
    }

    pub(super) fn layer_row(
        &self,
        layer: &Layer,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = layer.id.clone();
        let view = &self.layers.view;
        let active = self.layers.active.as_deref() == Some(id.as_str());
        let hidden = view.hidden.contains(&id);
        let name = self.row_text(
            &EditTarget::Layer(id.clone()),
            SharedString::from(layer.name.clone()),
        );

        let swatch = {
            let id = id.clone();
            let color = layer.color;
            div()
                .id(element_id("layer-color", &id))
                .flex_none()
                .size(px(12.))
                .rounded(tokens::radius_badge())
                .cursor_pointer()
                .bg(color_of(color))
                .on_click(cx.listener(move |shell, _, _, cx| {
                    cx.stop_propagation();
                    let next = next_color(color);
                    shell.apply_layer_edit(|dom| crate::layers::recolor_layer(dom, &id, next), cx);
                }))
        };
        let eye = self.view_toggle(
            ToggleSpec {
                kind: "layer-eye",
                icon: if hidden {
                    IconName::EyeOff
                } else {
                    IconName::Eye
                },
                label: "Hide in the viewport",
            },
            &id,
            hidden,
            |view, id| flip(&mut view.hidden, id),
            cx,
        );
        let lock = self.view_toggle(
            ToggleSpec {
                kind: "layer-lock",
                icon: IconName::Lock,
                label: "Lock against viewport clicks",
            },
            &id,
            view.locked.contains(&id),
            |view, id| flip(&mut view.locked, id),
            cx,
        );
        let solo = self.view_toggle(
            ToggleSpec {
                kind: "layer-solo",
                icon: IconName::Check,
                label: "Solo: hide the other layers",
            },
            &id,
            view.solo.contains(&id),
            |view, id| flip(&mut view.solo, id),
            cx,
        );
        let isolate = self.view_toggle(
            ToggleSpec {
                kind: "layer-isolate",
                icon: IconName::Focus,
                label: "Isolate: draw only this layer",
            },
            &id,
            view.isolate.as_deref() == Some(id.as_str()),
            |view, id| {
                view.isolate = match view.isolate.as_deref() == Some(id) {
                    true => None,
                    false => Some(id.to_owned()),
                };
            },
            cx,
        );
        let select = {
            let id = id.clone();
            toggle(
                element_id("layer-select", &id),
                IconName::Layers,
                false,
                "Select the layer's instances",
            )
            .on_click(cx.listener(move |shell, _, _, cx| {
                cx.stop_propagation();
                shell.select_layer_members(&id, cx);
            }))
        };
        let delete = {
            let id = id.clone();
            toggle(
                element_id("layer-delete", &id),
                IconName::Trash,
                false,
                "Delete the layer",
            )
            .on_click(cx.listener(move |shell, _, _, cx| {
                cx.stop_propagation();
                if shell.layers.active.as_deref() == Some(id.as_str()) {
                    shell.layers.active = None;
                }
                shell.apply_layer_edit(|dom| crate::layers::delete_layer(dom, &id), cx);
            }))
        };

        let row_id = id.clone();
        let row_name = layer.name.clone();
        let drop_id = id.clone();
        h_flex()
            .id(element_id("layer", &id))
            .w_full()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .px(px(8.))
            .py(tokens::row_padding())
            .rounded(tokens::radius_row())
            .text_size(tokens::text_sm())
            .line_height(tokens::line_sm())
            .text_color(tokens::text2())
            .cursor_pointer()
            .when(active, |this| this.bg(tokens::accent_soft()))
            .hover(|this| this.bg(tokens::hover_subtle()))
            .on_click(cx.listener(move |shell, event: &ClickEvent, window, cx| {
                shell.layers.active = Some(row_id.clone());
                let double = matches!(event, ClickEvent::Mouse(c) if c.down.click_count == 2);
                if double {
                    shell.begin_edit(EditTarget::Layer(row_id.clone()), &row_name, window, cx);
                }
                cx.notify();
            }))
            .on_drop(cx.listener(move |shell, _: &DraggedInstances, _, cx| {
                let refs = shell.selected_all().to_vec();
                if refs.is_empty() {
                    return;
                }
                shell.apply_layer_edit(
                    |dom| crate::layers::assign(dom, &refs, Some(drop_id.as_str())),
                    cx,
                );
            }))
            .child(swatch)
            .child(name)
            .child(
                div()
                    .flex_none()
                    .text_color(tokens::text3())
                    .child(SharedString::from(count.to_string())),
            )
            .child(eye)
            .child(lock)
            .child(solo)
            .child(isolate)
            .child(select)
            .child(delete)
            .into_any_element()
    }

    pub(super) fn note_row(&self, note: &Note, cx: &mut Context<Self>) -> AnyElement {
        let referent = note.referent;
        let key = referent.value();
        let text = note.text.clone();
        let delete = toggle(
            element_id("note-delete", key),
            IconName::Trash,
            false,
            "Delete the note",
        )
        .on_click(cx.listener(move |shell, _, _, cx| {
            cx.stop_propagation();
            shell.apply_layer_edit(|dom| crate::notes::remove_note(dom, referent), cx);
        }));
        h_flex()
            .id(element_id("note", key))
            .w_full()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .px(px(8.))
            .py(tokens::row_padding())
            .rounded(tokens::radius_row())
            .text_size(tokens::text_sm())
            .line_height(tokens::line_sm())
            .text_color(tokens::text2())
            .cursor_pointer()
            .hover(|this| this.bg(tokens::hover_subtle()))
            .on_click(cx.listener(move |shell, event: &ClickEvent, window, cx| {
                let double = matches!(event, ClickEvent::Mouse(c) if c.down.click_count == 2);
                if double {
                    shell.begin_edit(EditTarget::Note(referent), &text, window, cx);
                    cx.notify();
                    return;
                }
                shell.frame_note(referent, cx);
                shell.select(referent, cx);
            }))
            .child(
                div()
                    .flex_none()
                    .size(px(8.))
                    .rounded_full()
                    .bg(color_of(note.color)),
            )
            .child(self.row_text(
                &EditTarget::Note(referent),
                SharedString::from(note.text.clone()),
            ))
            .child(
                div()
                    .flex_none()
                    .max_w(px(80.))
                    .truncate()
                    .text_size(tokens::text_xs())
                    .line_height(tokens::line_xs())
                    .text_color(tokens::text3())
                    .child(SharedString::from(note.author.clone())),
            )
            .child(delete)
            .into_any_element()
    }
}
