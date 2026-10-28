//! Step 1, Item: what is fitted, as what, on which attachment, for which
//! body.

use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rbx_ugc::{AssetType, BodyScale};

use super::super::{pickers, ItemSource};
use super::state::{attachment_label, attachments, scale_label, Mode, Step};
use super::widgets::{card, choice, choices, group, note, GAP};
use super::{FittingWindow, Job};
use crate::tokens;

impl FittingWindow {
    /// The settings changed in a way that moves the item or its cages.
    fn kind_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.inputs.show_settings(&self.settings, window, cx);
        self.cages = Job::Idle;
        self.sync(cx);
        if self.step == Step::Fit && self.settings.layered() {
            self.make_cages(cx);
        }
        cx.notify();
    }

    fn source_card(&self, cx: &mut Context<Self>) -> Div {
        let this = cx.entity();
        let (sel, file) = (this.clone(), this);
        let shown = self.source.as_ref().map(|s| ItemSource::Instance {
            root: s.part,
            name: s.name.clone(),
            class: "MeshPart".into(),
        });
        let mut body = v_flex().gap(px(10.)).child(pickers::source_card(
            "ugc-fit-source",
            shown.as_ref(),
            move |window, cx| sel.update(cx, |v, cx| v.use_selection(window, cx)),
            move |window, cx| file.update(cx, |v, cx| v.choose_file(window, cx)),
        ));
        if let Some(n) = self.source.as_ref().and_then(|s| s.note.clone()) {
            body = body.child(note("info", tokens::text3(), n));
        }
        if let Some(e) = &self.source_error {
            body = body.child(note("triangle-alert", tokens::warning(), e.clone()));
        }
        card(
            "scan-search",
            "Item",
            "A MeshPart, a Model holding one, or a mesh file",
            body,
        )
    }

    fn kind_card(&self, cx: &mut Context<Self>) -> Div {
        let s = &self.settings;
        let modes = [(Mode::Accessory, "gem"), (Mode::Clothing, "shirt")].map(|(mode, glyph)| {
            choice(
                SharedString::from(format!("ugc-fit-mode-{}", mode.label())),
                mode.label(),
                Some(glyph),
                s.mode == mode,
                true,
            )
            .on_click(cx.listener(move |v, _, window, cx| {
                if v.settings.mode != mode {
                    v.settings.set_mode(mode);
                    v.kind_changed(window, cx);
                }
            }))
        });
        let types = s.mode.types().iter().map(|&asset: &AssetType| {
            choice(
                SharedString::from(format!("ugc-fit-type-{}", asset.label())),
                asset.label(),
                None,
                s.asset == asset,
                true,
            )
            .on_click(cx.listener(move |v, _, window, cx| {
                if v.settings.asset != asset {
                    v.settings.set_asset(asset);
                    v.kind_changed(window, cx);
                }
            }))
        });
        let points = attachments(s.asset);
        let mut sorted: Vec<&'static str> = points.to_vec();
        sorted.sort_by_key(|a| attachment_label(a));
        let picks = sorted.into_iter().map(|name| {
            choice(
                SharedString::from(format!("ugc-fit-att-{name}")),
                attachment_label(name),
                None,
                s.attachment == name,
                true,
            )
            .on_click(cx.listener(move |v, _, window, cx| {
                if v.settings.attachment != name {
                    v.settings.attachment = name;
                    v.settings.placement = Default::default();
                    v.kind_changed(window, cx);
                }
            }))
        });
        let scales = BodyScale::ALL.map(|scale| {
            choice(
                SharedString::from(format!("ugc-fit-scale-{}", scale.label())),
                scale_label(scale),
                None,
                s.scale == scale,
                true,
            )
            .on_click(cx.listener(move |v, _, window, cx| {
                if v.settings.scale != scale {
                    v.settings.scale = scale;
                    v.body = super::state::Body::Preset(scale);
                    v.kind_changed(window, cx);
                }
            }))
        });
        let body = v_flex()
            .gap(px(GAP))
            .child(group("Kind", choices(modes)))
            .child(group("Asset type", choices(types)))
            .when(points.len() > 1, |el| {
                el.child(group("Attachment point", choices(picks)))
            })
            .child(group("Body scale", choices(scales)));
        card(
            "shapes",
            "What it is",
            "Studio\u{2019}s Accessory and Clothing lists",
            body,
        )
    }

    pub(super) fn item_cards(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        vec![self.source_card(cx), self.kind_card(cx)]
    }
}
