//! The test step's body and motion: stock or downloaded avatars, body
//! scale overrides, and the stock `Animate` clips played on the preview
//! body by writing its joints' `Transform`s, as the Animation Editor poses
//! a rig, without touching the main document's editor.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::*;
use rbx_animation::{Frame, Rig};

use super::state::{rig_scale, Body, Motion};
use super::{Avatar, FittingWindow, Job};
use crate::shell::animation::published::fetch_clip;
use crate::shell::animation::write_transforms;
use crate::shell::rig::{fetch_avatar, r15_animation, BodyShape, Scales};

/// The six `Humanoid` scales the sliders set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scale {
    Height,
    Width,
    Depth,
    Head,
    BodyType,
    Proportion,
}

impl Scale {
    pub(super) const ALL: [Scale; 6] = [
        Scale::Height,
        Scale::Width,
        Scale::Depth,
        Scale::Head,
        Scale::BodyType,
        Scale::Proportion,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Scale::Height => "Height",
            Scale::Width => "Width",
            Scale::Depth => "Depth",
            Scale::Head => "Head",
            Scale::BodyType => "Body Type",
            Scale::Proportion => "Proportion",
        }
    }

    /// The range Roblox's avatar editor allows.
    pub(super) fn range(self) -> (f32, f32) {
        match self {
            Scale::Height => (0.9, 1.05),
            Scale::Width | Scale::Depth => (0.7, 1.0),
            Scale::Head => (0.95, 1.0),
            Scale::BodyType | Scale::Proportion => (0.0, 1.0),
        }
    }

    pub(super) fn read(self, s: &Scales) -> f32 {
        match self {
            Scale::Height => s.height,
            Scale::Width => s.width,
            Scale::Depth => s.depth,
            Scale::Head => s.head,
            Scale::BodyType => s.body_type,
            Scale::Proportion => s.proportion,
        }
    }

    fn write(self, s: &mut Scales, v: f32) {
        let slot = match self {
            Scale::Height => &mut s.height,
            Scale::Width => &mut s.width,
            Scale::Depth => &mut s.depth,
            Scale::Head => &mut s.head,
            Scale::BodyType => &mut s.body_type,
            Scale::Proportion => &mut s.proportion,
        };
        *slot = v;
    }
}

/// How long slider moves settle before the body is rebuilt.
const SETTLE: Duration = Duration::from_millis(120);
const FRAME: Duration = Duration::from_millis(16);

impl FittingWindow {
    /// The scales the test body stands at now.
    pub(super) fn current_scales(&self) -> Scales {
        if let Some(s) = self.scales {
            return s;
        }
        match self.body {
            Body::Preset(scale) => Scales::preset(rig_scale(scale), BodyShape::Masculine),
            Body::Player(_) | Body::MyAvatar => match self.avatar.ready() {
                Some(a) => {
                    let s = &a.fetched.avatar.scales;
                    Scales {
                        height: s.height as f32,
                        width: s.width as f32,
                        depth: s.depth as f32,
                        head: s.head as f32,
                        body_type: s.body_type as f32,
                        proportion: s.proportion as f32,
                    }
                }
                None => {
                    Scales::preset(rig_scale(rbx_ugc::BodyScale::Classic), BodyShape::Masculine)
                }
            },
        }
    }

    /// One slider moved: the body is rebuilt once the moves settle.
    pub(super) fn set_scale(&mut self, scale: Scale, value: f32, cx: &mut Context<Self>) {
        let mut s = self.current_scales();
        scale.write(&mut s, value);
        self.scales = Some(s);
        if self.scale_pending {
            return;
        }
        self.scale_pending = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            let _ = this.update(cx, |view, cx| {
                view.scale_pending = false;
                view.sync(cx);
            });
        })
        .detach();
    }

    pub(super) fn set_body(&mut self, body: Body, cx: &mut Context<Self>) {
        self.body = body;
        self.scales = None;
        match body {
            Body::Preset(_) => self.avatar = Job::Idle,
            Body::Player(id) => self.fetch_avatar(Some(id), cx),
            Body::MyAvatar => self.fetch_avatar(None, cx),
        }
        self.sync(cx);
        cx.notify();
    }

    fn fetch_avatar(&mut self, user: Option<u64>, cx: &mut Context<Self>) {
        self.avatar = Job::Busy;
        let task = cx.background_spawn(async move { fetch_avatar(user) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |view, cx| {
                // A body picked while this loaded wins.
                if !matches!(view.body, Body::Player(_) | Body::MyAvatar) {
                    return;
                }
                view.avatar = match result {
                    Ok(fetched) => Job::Ready(Avatar {
                        fetched: Arc::new(fetched),
                    }),
                    Err(e) => Job::Failed(e),
                };
                view.sync(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Plays `motion` from its start, downloading its clip first.
    pub(super) fn play(&mut self, motion: Motion, cx: &mut Context<Self>) {
        let Some(id) = r15_animation(motion.state()) else {
            return;
        };
        self.motion = Some(motion);
        self.time = 0.0;
        self.playing = true;
        self.clip = Job::Busy;
        cx.notify();
        let task = cx.background_spawn(async move { fetch_clip(id) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |view, cx| {
                if view.motion != Some(motion) {
                    return;
                }
                view.clip = match result {
                    Ok(clip) => Job::Ready(Arc::new(clip)),
                    Err(e) => Job::Failed(e),
                };
                view.start_ticker(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn toggle_play(&mut self, cx: &mut Context<Self>) {
        if self.motion.is_none() {
            return self.play(super::state::Motion::Idle, cx);
        }
        self.playing = !self.playing;
        if self.playing {
            self.start_ticker(cx);
        }
        cx.notify();
    }

    /// Stops playback and stands the body at rest.
    pub(super) fn stop_motion(&mut self, cx: &mut Context<Self>) {
        self.playing = false;
        self.motion = None;
        self.clip = Job::Idle;
        self.ticker = None;
        self.pose(&Default::default(), cx);
    }

    fn start_ticker(&mut self, cx: &mut Context<Self>) {
        if self.ticker.is_some() {
            return;
        }
        self.ticker = Some(cx.spawn(async move |this, cx| {
            let mut last = Instant::now();
            loop {
                cx.background_executor().timer(FRAME).await;
                let now = Instant::now();
                let dt = now.duration_since(last).as_secs_f32().min(0.25);
                last = now;
                let going = this
                    .update(cx, |view, cx| view.tick(dt, cx))
                    .unwrap_or(false);
                if !going {
                    break;
                }
            }
        }));
    }

    /// One playback frame; whether to keep going.
    fn tick(&mut self, dt: f32, cx: &mut Context<Self>) -> bool {
        let clip = match (&self.clip, self.playing) {
            (Job::Ready(clip), true) => clip.clone(),
            _ => {
                self.ticker = None;
                return false;
            }
        };
        self.time += dt;
        let length = clip.length();
        let t = if length > 0.0 {
            self.time % length
        } else {
            0.0
        };
        let transforms = rbx_animation::sample::sample(&clip, t)
            .into_iter()
            .map(|(j, s)| (j, Frame::IDENTITY.lerp(&s.transform, s.weight.min(1.0))))
            .collect();
        self.pose(&transforms, cx);
        true
    }

    /// Writes `transforms` (identity where absent) to the test body's
    /// joints.
    fn pose(
        &mut self,
        transforms: &std::collections::BTreeMap<String, Frame>,
        cx: &mut Context<Self>,
    ) {
        let Some(rig_ref) = self.live.as_ref().map(|l| l.rig) else {
            return;
        };
        if self.anim_rig.as_ref().is_none_or(|(r, _)| *r != rig_ref) {
            let rig = Rig::read(&self.shell.read(cx).dom, rig_ref);
            self.anim_rig = Some((rig_ref, rig));
        }
        let Some((_, rig)) = &self.anim_rig else {
            return;
        };
        self.shell.update(cx, |shell, cx| {
            if write_transforms(&mut shell.dom, rig, transforms, &mut HashMap::new()) {
                shell.commit_preview(false, cx);
            }
        });
    }
}
