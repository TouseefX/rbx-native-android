//! 3D notes as billboards over the viewport: each note's text in a badge of
//! its colour, placed where its world position projects, hidden behind the
//! camera or off screen. Clicking one points the camera at it.

use glam::Vec3;
use gpui_kit::*;
use rbx_dom::Ref;

use super::{tokens, WorkspaceView};

/// One note as the viewport draws it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NoteMark {
    pub(crate) note: Ref,
    pub(crate) position: [f32; 3],
    pub(crate) text: SharedString,
    pub(crate) color: (u8, u8, u8),
}

impl WorkspaceView {
    /// Replaces the billboards drawn; an empty list clears them.
    pub(crate) fn set_notes(&mut self, notes: Vec<NoteMark>) {
        self.notes = notes;
    }

    /// Leaves the instances under `hidden` undrawn and unpickable, and with
    /// `only`, draws nothing but what is under those roots, on the place's
    /// own sky and lighting, unlike the Animation Editor's isolation (see
    /// `rbx_viewer::Headless::set_layer_filter`).
    pub(crate) fn set_layer_filter(&mut self, only: Option<Vec<Ref>>, hidden: Vec<Ref>) {
        self.pump.layer_filter(only, hidden);
    }

    /// Points the camera at `target` from 12 studs back along the current
    /// view direction (or from above and behind with no view yet).
    pub(crate) fn frame_point(&self, target: [f32; 3]) {
        let target_v = Vec3::from(target);
        match self.view {
            Some(pose) => {
                let (forward, _, _) = pose.basis();
                self.look_at((target_v - forward * 12.0).to_array(), target);
            }
            None => self.look_at((target_v + Vec3::new(0.0, 8.0, 12.0)).to_array(), target),
        }
    }

    /// The billboards, absolutely placed children of the viewport element.
    pub(super) fn note_elements(&self, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let scale = window.scale_factor();
        self.notes
            .iter()
            .filter_map(|mark| {
                let at = self.project(Vec3::from(mark.position), scale)?;
                let (r, g, b) = mark.color;
                let color = rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b));
                let position = mark.position;
                let badge = div()
                    .id(("note", mark.note.value() as usize))
                    .max_w(px(180.))
                    .px_2()
                    .py_0p5()
                    .border_l_2()
                    .border_color(color)
                    .bg(tokens::viewport_badge())
                    .text_size(crate::tokens::viewport_text(12.))
                    .text_color(tokens::viewport_badge_text())
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.frame_point(position);
                            cx.stop_propagation();
                        }),
                    )
                    .child(mark.text.clone());
                Some(
                    div()
                        .absolute()
                        .left(at.x)
                        .top(at.y)
                        .w(px(180.))
                        .ml(px(-90.))
                        .flex()
                        .justify_center()
                        .child(div().mt(px(-22.)).child(badge))
                        .into_any_element(),
                )
            })
            .collect()
    }
}
