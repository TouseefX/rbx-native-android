//! Snapping the camera square-on: a click on the orientation indicator's
//! cube, or one of the View menu's fixed views.

use glam::Vec3;
use gpui_kit::Context;
use rbx_viewer::pick::{self, PartSurface, Ray};

use crate::workspace_view::snapped_forward;

use super::Shell;

/// How far ahead the camera turns around when nothing is under the centre
/// of the view.
const EMPTY_FOCUS: f32 = 50.0;

/// The View menu's fixed views: where the camera looks from, named the way
/// Roblox names a part's faces (`Enum.NormalId`), so Front looks at a
/// part's Front face (`-Z`) from in front of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresetView {
    Top,
    Front,
    Right,
}

impl PresetView {
    /// The way the camera looks in this view.
    fn forward(self) -> Vec3 {
        match self {
            PresetView::Top => Vec3::NEG_Y,
            PresetView::Front => Vec3::Z,
            PresetView::Right => Vec3::NEG_X,
        }
    }
}

/// The eye and target that turn the camera to look along `forward`, about
/// the point `distance` ahead of it.
fn orbit(eye: Vec3, look: Vec3, distance: f32, forward: Vec3) -> (Vec3, Vec3) {
    let focus = eye + look * distance;
    (focus - forward * distance, focus)
}

impl Shell {
    /// Turns the camera to look along `forward` (straight up or down tilts
    /// a degree short, see `snapped_forward`), around the surface under the
    /// centre of the view, so what was in the middle stays there.
    pub(super) fn snap_view(&mut self, forward: Vec3, cx: &mut Context<Self>) {
        let view = self.viewport.read(cx);
        let Some(pose) = view.pose() else {
            return;
        };
        let forward = snapped_forward(pose, forward);
        let look = pose.basis().0;
        let ray = Ray::new(pose.position, look);
        let meshes = view.meshes().clone();
        let distance = pick::parts_along(&self.dom, &self.database, &meshes, ray)
            .into_iter()
            .find_map(|part| {
                PartSurface::read(&self.dom, &self.database, &meshes, part)?.raycast(ray)
            })
            .map_or(EMPTY_FOCUS, |(distance, _)| distance);
        let (eye, target) = orbit(pose.position, look, distance, forward);
        self.viewport
            .read(cx)
            .look_at(eye.to_array(), target.to_array());
    }

    /// The View menu's Top, Front and Right.
    pub(crate) fn preset_view(&mut self, view: PresetView, cx: &mut Context<Self>) {
        self.snap_view(view.forward(), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snap_keeps_the_focus_point_and_its_distance() {
        let eye = Vec3::new(0.0, 10.0, 20.0);
        let look = Vec3::new(0.0, -0.6, -0.8);
        let (new_eye, target) = orbit(eye, look, 25.0, Vec3::NEG_Y);
        assert!((target - Vec3::new(0.0, -5.0, 0.0)).length() < 1e-4);
        assert!((new_eye - Vec3::new(0.0, 20.0, 0.0)).length() < 1e-4);
    }

    #[test]
    fn front_looks_at_a_parts_front_face() {
        // A part's Front face points -Z, so the camera looking at it looks +Z.
        assert_eq!(PresetView::Front.forward(), Vec3::Z);
        assert_eq!(PresetView::Right.forward(), Vec3::NEG_X);
        assert_eq!(PresetView::Top.forward(), Vec3::NEG_Y);
    }
}
