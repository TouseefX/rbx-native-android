//! Step 2, Fit: a rigid item's placement on its attachment, with the size
//! box drawn on the body; a layered item's cages, their nudges and its
//! `WrapLayer`.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;

use super::cage_jobs::Which;
use super::cages::Region;
use super::inputs::Field;
use super::state::{scale_label, AutoSkin, Placement};
use super::widgets::{card, choice, choices, group, note, number, toggle, GAP};
use super::{FittingWindow, Job};
use crate::launcher::ui::{self, Weight};
use crate::shell::rows::rail_slider;
use crate::tokens;

/// A cage edit about to be applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Nudge {
    pub(super) region: Region,
    pub(super) which: Which,
    /// Studs along the cage's normals; negative pulls in.
    pub(super) amount: f32,
    /// Studs from the region's centre where the push fades out.
    pub(super) radius: f32,
}

impl Default for Nudge {
    fn default() -> Self {
        Nudge {
            region: Region::Torso,
            which: Which::Outer,
            amount: 0.1,
            radius: 1.0,
        }
    }
}

fn row(children: impl IntoIterator<Item = Div>) -> Div {
    h_flex().gap(px(6.)).children(children)
}

impl FittingWindow {
    fn placement_card(&self, cx: &mut Context<Self>) -> Div {
        let axes = ["X", "Y", "Z"];
        let moves = [Field::MoveX, Field::MoveY, Field::MoveZ];
        let turns = [Field::TurnX, Field::TurnY, Field::TurnZ];
        let move_row = row((0..3).map(|i| number(moves[i], axes[i], "", self, cx)));
        let turn_row = row((0..3).map(|i| number(turns[i], axes[i], "\u{b0}", self, cx)));
        let scale_row = row([number(Field::Scale, "", "\u{d7}", self, cx)]);
        let reset = ui::icon_button(
            "ugc-fit-reset-placement",
            "rotate-ccw",
            "Reset",
            Weight::Secondary,
            true,
        )
        .on_click(cx.listener(|v, _, window, cx| {
            v.settings.placement = Placement::default();
            v.inputs.show_settings(&v.settings, window, cx);
            v.sync(cx);
        }));
        let legend = match rbx_ugc::spec::item_bounds(
            self.settings.asset,
            self.settings.scale,
            self.settings.attachment,
        ) {
            Some(b) => note(
                "square-dashed",
                if self.box_over { ui::red() } else { ui::green() },
                format!(
                    "The box is Roblox\u{2019}s limit for a {} on a {} body, {:.2} \u{d7} {:.2} \u{d7} {:.2} studs: green fits, red is too big.",
                    self.settings.asset.label(),
                    scale_label(self.settings.scale),
                    b.size[0],
                    b.size[1],
                    b.size[2],
                ),
            ),
            None => note("info", tokens::text3(), "Roblox gives no size limit for this type."),
        };
        let body = v_flex()
            .gap(px(GAP))
            .child(group("Move (studs, along the attachment)", move_row))
            .child(group("Rotate (degrees)", turn_row))
            .child(group(
                "Scale",
                h_flex()
                    .gap(px(8.))
                    .child(div().w(px(160.)).child(scale_row))
                    .child(div().flex_1())
                    .child(reset),
            ))
            .child(legend);
        card(
            "move-3d",
            "Placement",
            format!(
                "Relative to the body\u{2019}s {}",
                super::state::attachment_label(self.settings.attachment)
            ),
            body,
        )
    }

    fn cages_card(&self, cx: &mut Context<Self>) -> Div {
        let status = match &self.cages {
            Job::Idle | Job::Busy => h_flex()
                .gap(px(8.))
                .child(ui::spinner("ugc-fit-cage-spin", 14.))
                .child(
                    ui::text(12., 16.)
                        .text_color(tokens::text2())
                        .child("Making the cages from the Mannequin\u{2019}s body cages\u{2026}"),
                ),
            Job::Failed(e) => note("circle-x", tokens::text_error(), format!("No cages: {e}")),
            Job::Ready(c) => note(
                "badge-check",
                ui::green(),
                format!(
                    "Inner and outer cages, {} vertices each, the Mannequin\u{2019}s own vertex order and UVs.",
                    c.now.inner.positions.len()
                ),
            ),
        };
        let ready = self.cages.ready().is_some();
        let toggles = h_flex()
            .gap(px(16.))
            .child(toggle(
                "ugc-fit-show-inner",
                "Inner cage",
                self.show_inner,
                {
                    let this = cx.entity();
                    move |_, _, cx| {
                        this.update(cx, |v, cx| {
                            v.show_inner = !v.show_inner;
                            v.sync(cx);
                        })
                    }
                },
            ))
            .child(toggle(
                "ugc-fit-show-outer",
                "Outer cage",
                self.show_outer,
                {
                    let this = cx.entity();
                    move |_, _, cx| {
                        this.update(cx, |v, cx| {
                            v.show_outer = !v.show_outer;
                            v.sync(cx);
                        })
                    }
                },
            ))
            .child(
                h_flex()
                    .flex_1()
                    .gap(px(8.))
                    .child(ui::icon("eye", 13.).text_color(tokens::text3()))
                    .child(rail_slider(&self.inputs.opacity, cx)),
            );
        let n = self.nudge;
        let regions = Region::ALL.map(|region| {
            choice(
                SharedString::from(format!("ugc-fit-region-{}", region.label())),
                region.label(),
                None,
                n.region == region,
                ready,
            )
            .on_click(cx.listener(move |v, _, _, cx| {
                v.nudge.region = region;
                cx.notify();
            }))
        });
        let which = [Which::Inner, Which::Outer].map(|w| {
            choice(
                SharedString::from(format!("ugc-fit-which-{}", w.label())),
                w.label(),
                None,
                n.which == w,
                ready,
            )
            .on_click(cx.listener(move |v, _, _, cx| {
                v.nudge.which = w;
                cx.notify();
            }))
        });
        let apply = ui::icon_button(
            "ugc-fit-nudge",
            "wand-sparkles",
            "Nudge",
            Weight::Primary,
            true,
        )
        .on_click(cx.listener(|v, _, _, cx| v.apply_nudge(cx)));
        let resets = [Which::Inner, Which::Outer].map(|w| {
            ui::icon_button(
                SharedString::from(format!("ugc-fit-reset-{}", w.label())),
                "rotate-ccw",
                format!("Reset {}", w.label()),
                Weight::Secondary,
                true,
            )
            .on_click(cx.listener(move |v, _, _, cx| v.reset_cage(w, cx)))
        });
        let body = v_flex()
            .gap(px(GAP))
            .child(status)
            .child(toggles)
            .child(group("Nudge around", choices(regions)))
            .child(group(
                "Cage",
                h_flex()
                    .gap(px(6.))
                    .children(which)
                    .child(div().flex_1())
                    .children(resets),
            ))
            .child(group(
                "Amount (studs, negative pulls in) and falloff radius",
                h_flex()
                    .gap(px(6.))
                    .child(number(Field::NudgeAmount, "", "", self, cx))
                    .child(number(Field::NudgeRadius, "R", "", self, cx))
                    .child(apply),
            ));
        card(
            "shirt",
            "Cages",
            "Inner in blue, outer in orange; the item must sit between them",
            body,
        )
    }

    fn wrap_card(&self, cx: &mut Context<Self>) -> Div {
        let skins = AutoSkin::ALL.map(|a| {
            choice(
                SharedString::from(format!("ugc-fit-skin-{}", a.label())),
                a.label(),
                None,
                self.settings.wrap.auto_skin == a,
                true,
            )
            .on_click(cx.listener(move |v, _, _, cx| {
                v.settings.wrap.auto_skin = a;
                v.sync(cx);
            }))
        });
        let body = v_flex()
            .gap(px(GAP))
            .child(row([
                group("Order", number(Field::Order, "", "", self, cx)).flex_1(),
                group("Puffiness", number(Field::Puffiness, "", "", self, cx)).flex_1(),
                group("Shrink factor", number(Field::Shrink, "", "", self, cx)).flex_1(),
            ]))
            .child(group("Auto skin", choices(skins)));
        card(
            "layers",
            "Wrap layer",
            "HandleWrapLayer, as Studio writes it",
            body,
        )
    }

    pub(super) fn fit_cards(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        if self.settings.layered() {
            vec![self.cages_card(cx), self.wrap_card(cx)]
        } else {
            vec![self.placement_card(cx)]
        }
    }
}
