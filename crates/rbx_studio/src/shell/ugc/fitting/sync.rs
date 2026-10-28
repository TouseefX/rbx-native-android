//! Keeping the preview in step with the window: the test body rebuilt when
//! it changes, everything worn on it rebuilt on every change, the camera
//! framed on a new body, the size box recoloured by the check, and the
//! check run again on the working item.

use std::collections::HashSet;

use glam::Vec3;
use gpui_kit::*;
use rbx_dom::{Ref, WeakDom};
use rbx_ugc::{Rule, Severity};

use super::super::check::Run;
use super::super::{ItemKind, ItemSource};
use super::cages::{bounds, geometry};
use super::preview::Overlay;
use super::state::{rig_scale, Body, Step};
use super::{preview, slot, workspace, FittingWindow, Job, Live};
use crate::shell::rig::{
    self, apply_packages, dress, options_for, BodyShape, JointStyle, RigOptions, RigType, Scales,
};

/// What a test body is built from; a change rebuilds it.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct BodyKey {
    body: Body,
    scales: Option<Scales>,
    feet: [f32; 3],
}

/// Studs kept clear between the item and the test body.
const AISLE: f32 = 3.0;

const INNER_COLOUR: [u8; 3] = [86, 156, 255];
const OUTER_COLOUR: [u8; 3] = [255, 170, 64];

/// Everything one preview rebuild needs, read off the window first.
struct Plan {
    key: BodyKey,
    options: RigOptions,
    worn: Option<std::sync::Arc<rig::Fetched>>,
    source: Option<Ref>,
    settings: super::state::Settings,
    uris: Option<super::generate::CageUris>,
    size_box: bool,
    over: bool,
    overlays: Vec<Overlay>,
    transparency: f32,
    layers: Vec<rig::Worn>,
}

/// The children of `rig` that `dress` added, as `before` lists the ones it
/// had.
fn added(dom: &WeakDom, rig: Ref, before: &HashSet<Ref>) -> Vec<Ref> {
    dom.get(rig)
        .map(|r| r.children().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|c| !before.contains(c))
        .collect()
}

fn children(dom: &WeakDom, rig: Ref) -> HashSet<Ref> {
    dom.get(rig)
        .map(|r| r.children().iter().copied().collect())
        .unwrap_or_default()
}

/// Builds or refreshes the preview by `plan`; the camera target when the
/// body is new, and what went wrong wearing the item.
fn apply(
    dom: &mut WeakDom,
    ws: Ref,
    live: Option<Live>,
    plan: Plan,
) -> (Live, Option<(Vec3, f32)>, Option<String>) {
    let stale =
        |l: &Live| l.body != plan.key || dom.get(l.rig).is_none() || dom.get(l.folder).is_none();
    let (mut live, fresh) = match live.filter(|l| !stale(l)) {
        Some(mut live) => {
            for r in live.dressing.drain(..) {
                dom.remove(r);
            }
            (live, false)
        }
        None => {
            let (folder, rig) = preview::build(dom, ws, &plan.options);
            if let Some(fetched) = &plan.worn {
                dress(dom, rig, &fetched.worn);
            }
            let live = Live {
                folder,
                rig,
                body: plan.key.clone(),
                dressing: Vec::new(),
                item: None,
                outline: None,
            };
            (live, true)
        }
    };
    live.item = None;
    live.outline = None;
    let mut error = None;
    if let Some(source) = plan.source.filter(|s| dom.get(*s).is_some()) {
        let worn = preview::wear(
            dom,
            live.folder,
            live.rig,
            source,
            &plan.settings,
            plan.uris.as_ref(),
        );
        match worn {
            Ok(acc) => {
                live.dressing.push(acc);
                live.item = Some(acc);
            }
            Err(e) => error = Some(e),
        }
    }
    if plan.size_box {
        if let Some((part, outline)) =
            preview::size_box(dom, live.folder, live.rig, &plan.settings, plan.over)
        {
            live.dressing.extend([part, outline]);
            live.outline = Some(outline);
        }
    }
    if let Ok((handle, _, _)) = preview::frames(dom, live.rig, &plan.settings) {
        for cage in &plan.overlays {
            let part = preview::overlay(dom, live.folder, handle, cage, plan.transparency);
            live.dressing.push(part);
        }
    }
    if !plan.layers.is_empty() {
        let before = children(dom, live.rig);
        dress(dom, live.rig, &plan.layers);
        live.dressing.extend(added(dom, live.rig, &before));
    }
    let framing = fresh
        .then(|| frame(dom, &live, &plan.settings, plan.size_box))
        .flatten();
    (live, framing, error)
}

/// What the camera looks at and from how far: the attachment while a rigid
/// item is fitted, the whole body otherwise.
fn frame(
    dom: &WeakDom,
    live: &Live,
    settings: &super::state::Settings,
    close: bool,
) -> Option<(Vec3, f32)> {
    if close {
        let (_, at) = preview::attachment(dom, live.rig, settings.attachment)?;
        return Some((Vec3::from(at.p), 3.5));
    }
    let (_, head) = preview::part(dom, live.rig, "Head")?;
    let (_, root) = preview::part(dom, live.rig, "HumanoidRootPart")?;
    let top = Vec3::from(head.p);
    let feet = Vec3::from(root.p) * 2.0 - top;
    Some(((top + feet) / 2.0, (top - feet).length().max(2.5) * 0.9))
}

impl FittingWindow {
    /// The body the preview shows: the test body while testing, else the
    /// one the item is fitted for.
    fn active_body(&self) -> Body {
        if self.step == Step::Test {
            self.body
        } else if self.settings.layered() {
            // Cages are made from the Mannequin's, so the item is fitted on
            // the Mannequin, as Studio's tool does: on a blocky body they
            // would be stretched off the shape the item was made around.
            Body::Preset(rbx_ugc::BodyScale::Normal)
        } else {
            Body::Preset(self.settings.scale)
        }
    }

    /// The test body's build options, or `None` while its avatar loads.
    fn rig_options(
        &self,
        joints: JointStyle,
    ) -> Option<(RigOptions, Option<std::sync::Arc<rig::Fetched>>)> {
        let testing = self.step == Step::Test;
        let (mut options, worn) = match self.active_body() {
            Body::Preset(scale) => (
                RigOptions::new(RigType::R15, BodyShape::Masculine, rig_scale(scale), joints),
                None,
            ),
            Body::Player(_) | Body::MyAvatar => {
                let avatar = self.avatar.ready()?;
                let fetched = avatar.fetched.clone();
                let options = options_for(&fetched.avatar, joints, [0.0; 3], Some(RigType::R15));
                (options, Some(fetched))
            }
        };
        if let Some(scales) = self.scales.filter(|_| testing) {
            options.set_scales(scales);
        }
        if let Some(fetched) = &worn {
            apply_packages(&mut options, &fetched.worn);
        }
        options.name = "FittingMannequin".into();
        Some((options, worn))
    }

    /// Where the test body stands: on the spawn, stepped aside when the item
    /// itself stands there, so the two never overlap.
    fn feet(&self, ws: Ref, cx: &App) -> [f32; 3] {
        let dom = &self.shell.read(cx).dom;
        let mut feet = rig::spawn_top(dom, ws);
        if let Some(part) = self.source.as_ref().map(|s| s.part) {
            let at = preview::cframe(dom, part, "CFrame").p;
            let reach = preview::size(dom, part).into_iter().fold(0.0, f32::max) / 2.0 + AISLE;
            if (at[0] - feet[0]).hypot(at[2] - feet[2]) < reach {
                feet[0] = at[0] - reach - AISLE;
            }
        }
        feet
    }

    /// Rebuilds the preview from the window's state and checks the item
    /// again.
    pub(super) fn sync(&mut self, cx: &mut Context<Self>) {
        if self.generated.is_some() {
            return;
        }
        let joints = JointStyle::of_place(&self.shell.read(cx).dom);
        let Some((mut options, worn)) = self.rig_options(joints) else {
            return;
        };
        let testing = self.step == Step::Test;
        let fitting = self.step == Step::Fit;
        let overlays: Vec<Overlay> = match (&self.cages, fitting && self.settings.layered()) {
            (Job::Ready(c), true) => {
                let name = self.source.as_ref().map_or("Item", |s| s.name.as_str());
                [
                    (
                        self.show_inner,
                        "InnerCage",
                        &c.uris.inner,
                        &c.now.inner,
                        INNER_COLOUR,
                    ),
                    (
                        self.show_outer,
                        "OuterCage",
                        &c.uris.outer,
                        &c.now.outer,
                        OUTER_COLOUR,
                    ),
                ]
                .into_iter()
                .filter(|(on, ..)| *on)
                .map(|(_, suffix, uri, mesh, color)| Overlay {
                    name: format!("{name}_{suffix}"),
                    uri: uri.clone(),
                    size: bounds(mesh).1,
                    color,
                })
                .collect()
            }
            _ => Vec::new(),
        };
        let layers = if testing {
            self.worn_layers()
        } else {
            Vec::new()
        };
        let ws = workspace(&self.shell.read(cx).dom);
        let Some(ws) = ws else { return };
        options.feet = self.feet(ws, cx);
        let plan = Plan {
            key: BodyKey {
                body: self.active_body(),
                scales: self.scales.filter(|_| testing),
                feet: options.feet,
            },
            options,
            worn,
            source: self.source.as_ref().map(|s| s.part),
            settings: self.settings.clone(),
            uris: self.cages.ready().map(|c| c.uris.clone()),
            size_box: fitting && !self.settings.layered(),
            over: self.box_over,
            overlays,
            transparency: 1.0 - self.cage_opacity,
            layers,
        };
        let live = self.live.take();
        let (live, framing, error) = self.shell.update(cx, |shell, cx| {
            let result = apply(&mut shell.dom, ws, live, plan);
            shell.commit_preview(true, cx);
            if let Some((target, reach)) = result.1 {
                let eye = target + Vec3::new(-0.55, 0.25, -1.0).normalize() * reach * 2.4;
                shell
                    .viewport
                    .read(cx)
                    .look_at(eye.to_array(), target.to_array());
            }
            result
        });
        if framing.is_some() {
            self.anim_rig = None;
        }
        self.live = Some(live);
        self.wear_error = error;
        self.restart_check(cx);
    }

    /// Checks the working item (or the generated one) again.
    pub(super) fn restart_check(&mut self, cx: &mut Context<Self>) {
        let (root, name) = match &self.generated {
            Some(g) => (Some(g.accessory), g.name.clone()),
            None => (
                self.live.as_ref().and_then(|l| l.item),
                self.source
                    .as_ref()
                    .map(|s| s.name.clone())
                    .unwrap_or_default(),
            ),
        };
        self.check.source = root.map(|root| ItemSource::Instance {
            root,
            name,
            class: "Accessory".into(),
        });
        self.check.kind = ItemKind {
            asset: self.settings.asset,
            scale: self.settings.scale,
        };
        self.check.template = self.cages.ready().map(|c| geometry(&c.template));
        let shell = self.shell.clone();
        self.check.start(&shell, cx, slot);
    }

    /// Whether the check says the item is bigger than its box.
    fn over(&self) -> bool {
        matches!(&self.check.run, Run::Done(done) if done.findings.iter().any(|f| {
            f.rule == Rule::ItemSize && f.severity != Severity::Note
        }))
    }

    /// Repaints the size box when a finished check changed its verdict.
    pub(super) fn recolour_box(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.check.run, Run::Done(_)) {
            return;
        }
        let over = self.over();
        if over == self.box_over {
            return;
        }
        self.box_over = over;
        let Some(outline) = self.live.as_ref().and_then(|l| l.outline) else {
            return;
        };
        self.shell.update(cx, |shell, cx| {
            if shell.dom.get(outline).is_some() {
                preview::paint_box(&mut shell.dom, outline, over);
                shell.commit_preview(false, cx);
            }
        });
    }
}
