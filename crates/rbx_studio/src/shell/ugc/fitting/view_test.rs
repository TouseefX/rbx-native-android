//! Step 3, Test: the item on other bodies, in motion, at other scales and
//! under or over other items.

use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rbx_ugc::BodyScale;

use super::layers::Layer;
use super::motion::Scale;
use super::state::{Body, Motion};
use super::widgets::{card, choice, choices, group, note, toggle, GAP};
use super::{FittingWindow, Job};
use crate::launcher::ui::{self, Weight};
use crate::shell::rows::rail_slider;
use crate::tokens;

fn order_buttons(
    id: String,
    order: Option<i32>,
    up: impl Fn(&mut Window, &mut App) + 'static,
    down: impl Fn(&mut Window, &mut App) + 'static,
) -> Div {
    let Some(order) = order else {
        return h_flex().child(
            ui::text(11., 14.)
                .text_color(tokens::text3())
                .child("Rigid"),
        );
    };
    let arrow = |suffix: &str, glyph: &'static str| {
        div()
            .id(SharedString::from(format!("{id}-{suffix}")))
            .size(px(22.))
            .rounded(px(4.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(tokens::text2())
            .hover(|s| {
                tokens::hover_fx(s)
                    .bg(ui::wash())
                    .text_color(tokens::text())
            })
            .child(ui::icon(glyph, 12.))
    };
    h_flex()
        .gap(px(4.))
        .child(
            ui::text(11.5, 16.)
                .text_color(tokens::text2())
                .child(format!("Order {order}")),
        )
        .child(arrow("down", "chevron-down").on_click(move |_, w, cx| down(w, cx)))
        .child(arrow("up", "chevron-up").on_click(move |_, w, cx| up(w, cx)))
}

impl FittingWindow {
    pub(super) fn load_player(&mut self, cx: &mut Context<Self>) {
        let text = self.inputs.user.read(cx).value().trim().to_string();
        match text.parse::<u64>() {
            Ok(id) if id > 0 => self.set_body(Body::Player(id), cx),
            _ => {
                self.avatar = Job::Failed(format!("\u{201c}{text}\u{201d} is not a user ID"));
                cx.notify();
            }
        }
    }

    fn body_card(&self, cx: &mut Context<Self>) -> Div {
        let presets = [
            (Body::Preset(BodyScale::Normal), "Normal"),
            (Body::Preset(BodyScale::Slender), "Slender"),
            (Body::Preset(BodyScale::Classic), "Classic"),
        ];
        let mut pills: Vec<Stateful<Div>> = presets
            .into_iter()
            .map(|(body, label)| {
                choice(
                    SharedString::from(format!("ugc-fit-body-{label}")),
                    label,
                    Some("person-standing"),
                    self.body == body,
                    true,
                )
                .on_click(cx.listener(move |v, _, window, cx| {
                    v.set_body(body, cx);
                    let s = v.current_scales();
                    v.inputs.show_scales(&s, window, cx);
                }))
            })
            .collect();
        pills.push(
            choice(
                "ugc-fit-body-player",
                "Player",
                Some("user"),
                matches!(self.body, Body::Player(_)),
                true,
            )
            .on_click(cx.listener(|v, _, _, cx| v.load_player(cx))),
        );
        pills.push(
            choice(
                "ugc-fit-body-me",
                "My Avatar",
                Some("user-round"),
                self.body == Body::MyAvatar,
                true,
            )
            .on_click(cx.listener(|v, _, _, cx| v.set_body(Body::MyAvatar, cx))),
        );
        let user = ui::field_frame(None, ui::panel2(), tokens::border(), "user").child(
            div().flex_1().min_w_0().h_full().child(
                Input::new(&self.inputs.user)
                    .appearance(false)
                    .h_full()
                    .px(px(0.))
                    .text_size(px(12.)),
            ),
        );
        let status = match (&self.avatar, self.body) {
            (_, Body::Preset(_)) => None,
            (Job::Busy, _) => Some(
                h_flex()
                    .gap(px(8.))
                    .child(ui::spinner("ugc-fit-avatar-spin", 14.))
                    .child(
                        ui::text(12., 16.)
                            .text_color(tokens::text2())
                            .child("Downloading the avatar\u{2026}"),
                    ),
            ),
            (Job::Failed(e), _) => Some(note("circle-x", tokens::text_error(), e.clone())),
            (Job::Ready(a), _) => Some(note(
                "badge-check",
                ui::green(),
                format!(
                    "Wearing the avatar\u{2019}s {} items with this one.",
                    a.fetched.worn.len()
                ),
            )),
            (Job::Idle, _) => None,
        };
        let body = v_flex()
            .gap(px(GAP))
            .child(choices(pills))
            .child(group(
                "Player",
                h_flex()
                    .gap(px(8.))
                    .child(div().flex_1().child(user))
                    .child(
                        ui::button("ugc-fit-load-player", "Load", Weight::Secondary, true)
                            .on_click(cx.listener(|v, _, _, cx| v.load_player(cx))),
                    ),
            ))
            .children(status);
        card(
            "person-standing",
            "Body",
            "Stock bodies, a player\u{2019}s avatar, or yours (needs an API key)",
            body,
        )
    }

    fn motion_card(&self, cx: &mut Context<Self>) -> Div {
        let pills = Motion::ALL.map(|m| {
            choice(
                SharedString::from(format!("ugc-fit-anim-{}", m.label())),
                m.label(),
                None,
                self.motion == Some(m),
                true,
            )
            .on_click(cx.listener(move |v, _, _, cx| v.play(m, cx)))
        });
        let (glyph, label) = if self.playing {
            ("pause", "Pause")
        } else {
            ("play", "Play")
        };
        let toggle_play = ui::icon_button("ugc-fit-play", glyph, label, Weight::Secondary, true)
            .on_click(cx.listener(|v, _, _, cx| v.toggle_play(cx)));
        let stop = ui::icon_button("ugc-fit-stop", "square", "Rest", Weight::Ghost, true).on_click(
            cx.listener(|v, _, _, cx| {
                v.stop_motion(cx);
                cx.notify();
            }),
        );
        let status = match &self.clip {
            Job::Busy => Some(
                h_flex()
                    .gap(px(8.))
                    .child(ui::spinner("ugc-fit-clip-spin", 14.))
                    .child(
                        ui::text(12., 16.)
                            .text_color(tokens::text2())
                            .child("Downloading the animation\u{2026}"),
                    ),
            ),
            Job::Failed(e) => Some(note("circle-x", tokens::text_error(), e.clone())),
            _ => None,
        };
        let body = v_flex()
            .gap(px(GAP))
            .child(choices(pills))
            .child(h_flex().gap(px(8.)).child(toggle_play).child(stop))
            .children(status);
        card(
            "film",
            "Animation",
            "Roblox\u{2019}s default Animate clips",
            body,
        )
    }

    fn scales_card(&self, cx: &mut Context<Self>) -> Div {
        let now = self.current_scales();
        let rows = self.inputs.scales.iter().map(|(scale, state)| {
            let scale: Scale = *scale;
            h_flex()
                .gap(px(10.))
                .child(
                    ui::text(12., 16.)
                        .w(px(84.))
                        .flex_none()
                        .text_color(tokens::text2())
                        .child(scale.label()),
                )
                .child(div().flex_1().child(rail_slider(state, cx)))
                .child(
                    ui::text(12., 16.)
                        .w(px(40.))
                        .flex_none()
                        .text_color(tokens::text())
                        .child(format!("{:.0}%", scale.read(&now) * 100.)),
                )
        });
        let reset = ui::icon_button(
            "ugc-fit-reset-scales",
            "rotate-ccw",
            "Reset",
            Weight::Secondary,
            true,
        )
        .on_click(cx.listener(|v, _, window, cx| {
            v.scales = None;
            let s = v.current_scales();
            v.inputs.show_scales(&s, window, cx);
            v.sync(cx);
        }));
        let body = v_flex()
            .gap(px(8.))
            .children(rows)
            .child(h_flex().child(div().flex_1()).child(reset));
        card(
            "ruler",
            "Body scale",
            "The Humanoid\u{2019}s six scales",
            body,
        )
    }

    fn layer_row(&self, index: usize, layer: &Layer, cx: &mut Context<Self>) -> Div {
        let this = cx.entity();
        let (up, down) = (this.clone(), this.clone());
        let state = match &layer.model {
            Job::Busy => Some(ui::spinner("ugc-fit-layer-spin", 12.)),
            Job::Failed(_) => Some(
                ui::icon("circle-x", 12.)
                    .text_color(tokens::text_error())
                    .into_any_element(),
            ),
            _ => None,
        };
        h_flex()
            .gap(px(8.))
            .child(
                toggle(
                    SharedString::from(format!("ugc-fit-layer-{index}")),
                    layer.label.clone(),
                    layer.on,
                    move |_, _, cx| this.update(cx, |v, cx| v.toggle_layer(index, cx)),
                )
                .flex_1(),
            )
            .children(state)
            .child(order_buttons(
                format!("ugc-fit-layer-order-{index}"),
                layer.order,
                move |_, cx| up.update(cx, |v, cx| v.reorder(Some(index), true, cx)),
                move |_, cx| down.update(cx, |v, cx| v.reorder(Some(index), false, cx)),
            ))
    }

    fn layers_card(&self, cx: &mut Context<Self>) -> Div {
        let name = self
            .source
            .as_ref()
            .map_or("This item".into(), |s| s.name.clone());
        let this = cx.entity();
        let (up, down) = (this.clone(), this);
        let mine = h_flex()
            .gap(px(8.))
            .p(px(8.))
            .rounded(px(6.))
            .bg(tokens::accent_soft())
            .child(ui::icon("sparkles", 13.).text_color(ui::accent()))
            .child(
                ui::text(12., 16.)
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(tokens::text_strong())
                    .child(name),
            )
            .child(order_buttons(
                "ugc-fit-order-mine".into(),
                self.settings.layered().then_some(self.settings.wrap.order),
                move |_, cx| up.update(cx, |v, cx| v.reorder(None, true, cx)),
                move |_, cx| down.update(cx, |v, cx| v.reorder(None, false, cx)),
            ));
        let rows: Vec<Div> = self
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| self.layer_row(i, l, cx))
            .collect();
        let body = v_flex().gap(px(6.)).child(mine).children(rows).when(
            self.layers
                .iter()
                .any(|l| matches!(l.model, Job::Failed(_))),
            |el| {
                let why = self
                    .layers
                    .iter()
                    .find_map(|l| match &l.model {
                        Job::Failed(e) => Some(e.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                el.child(note("circle-x", tokens::text_error(), why))
            },
        );
        card(
            "layers",
            "Layer with",
            "Catalog garments and the place\u{2019}s accessories; higher Order is worn over lower",
            body,
        )
    }

    pub(super) fn test_cards(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Vec<Div> {
        vec![
            self.body_card(cx),
            self.motion_card(cx),
            self.scales_card(cx),
            self.layers_card(cx),
        ]
    }
}
