//! The landmarks over the face in the viewport: each eye's eyelid loop and
//! the mouth loop, where they project onto the head as it is posed now,
//! and in red the cage vertices of any landmark that cannot project.

use glam::{Mat4, Vec3};
use gpui_kit::*;
use rbx_dom::{Ref, Variant, WeakDom};
use rbx_ugc::head::{Landmark, LandmarkPoints};
use rbx_viewer::Segment;

use super::{values, Face, FaceWindow};
use crate::tokens::linear;

const LINE: f32 = 2.0;
const DOT: f32 = 7.0;

fn color(landmark: Landmark) -> [f32; 4] {
    linear(match landmark {
        Landmark::LeftEye | Landmark::RightEye => rgb(0x38bdf8),
        Landmark::Mouth => rgb(0xf472b6),
    })
}

fn cframe(dom: &WeakDom, part: Ref) -> Option<[f32; 12]> {
    match dom.get(part)?.properties().get("CFrame")? {
        Variant::CFrame(c) => {
            let mut out = [0.0; 12];
            out[..3].copy_from_slice(&[c.position.x, c.position.y, c.position.z]);
            out[3..].copy_from_slice(&c.rotation);
            Some(out)
        }
        _ => None,
    }
}

fn matrix(c: &[f32; 12]) -> Mat4 {
    Mat4::from_cols_array(&[
        c[3], c[6], c[9], 0.0, c[4], c[7], c[10], 0.0, c[5], c[8], c[11], 0.0, c[0], c[1], c[2],
        1.0,
    ])
}

fn segment(from: Vec3, to: Vec3, color: [f32; 4], width: f32) -> Segment {
    Segment {
        from,
        to,
        color,
        on_top: true,
        width,
    }
}

/// A closed loop through `points` with a dot on each.
fn ring(points: &[Vec3], color: [f32; 4], out: &mut Vec<Segment>) {
    for (i, p) in points.iter().enumerate() {
        out.push(segment(*p, *p, color, DOT));
        out.push(segment(*p, points[(i + 1) % points.len()], color, LINE));
    }
}

fn landmark(
    face: &Face,
    points: &LandmarkPoints,
    posed: &[[f32; 3]],
    to_world: Mat4,
) -> Vec<Segment> {
    let world = |p: &[f32; 3]| to_world.transform_point3(Vec3::from(*p));
    let mut out = Vec::new();
    if points.missing == 0 && points.hits.iter().all(Option::is_some) {
        let on: Vec<Vec3> = points.on(&face.geometry, posed).iter().map(world).collect();
        ring(&on, color(points.landmark), &mut out);
    } else {
        let red = linear(rgb(0xef4444));
        for p in &points.cage {
            let p = world(p);
            out.push(segment(p, p, red, DOT));
        }
    }
    out
}

impl FaceWindow {
    pub(super) fn clear_overlay(&self, cx: &mut Context<Self>) {
        self.shell.update(cx, |shell, cx| {
            shell
                .viewport
                .update(cx, |v, _| v.set_face_landmarks(Vec::new()))
        });
    }

    /// Redraws the landmarks when the face or the head moved since the
    /// last drawing; clears them while hidden or with no head.
    pub(super) fn redraw_overlay(&mut self, cx: &mut Context<Self>) {
        let Some(face) = self.face().filter(|_| self.show_landmarks) else {
            if self.drawn.take().is_some() || !self.show_landmarks {
                self.clear_overlay(cx);
            }
            return;
        };
        let (now, at) = {
            let dom = &self.shell.read(cx).dom;
            (values(dom, face.part), cframe(dom, face.part))
        };
        let Some(at) = at else { return };
        let key = (now, at);
        if self.drawn.as_ref() == Some(&key) {
            return;
        }
        let weight = |name: &str| key.0.iter().find(|(n, _)| n == name).map_or(0.0, |v| v.1);
        let posed = face.mesh.posed(&weight);
        let to_world = matrix(&at);
        let segments: Vec<Segment> = face
            .landmarks
            .iter()
            .flat_map(|points| landmark(face, points, &posed, to_world))
            .collect();
        self.drawn = Some(key);
        self.shell.update(cx, |shell, cx| {
            shell
                .viewport
                .update(cx, |v, _| v.set_face_landmarks(segments))
        });
    }
}
