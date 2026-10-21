//! A recording at one moment, as what the viewport draws: a ghost per
//! character, its trail, its camera, and the events as markers; and a
//! heat grid as flat cells on the ground.

use glam::Vec3;
use rbx_viewer::{Fill, Segment};

use super::heat::{HeatGrid, HeatKind};
use super::{EventKind, Recording, Sample, Track};

/// The longest gap between two samples still drawn as one stretch of
/// trail (and interpolated across), in seconds.
pub(crate) const MAX_GAP: f32 = 1.5;

/// `track` at time `t`: position lerped and yaw lerped the short way round
/// between the samples either side; `camera` from the earlier one. `None`
/// before the first sample, after the last, or inside a gap over `MAX_GAP`.
pub(crate) fn pose_at(track: &Track, t: f32) -> Option<Sample> {
    let samples = &track.samples;
    let idx = samples.partition_point(|s| s.t <= t);
    let a = *samples.get(idx.checked_sub(1)?)?;
    let Some(b) = samples.get(idx) else {
        return (t == a.t).then_some(a);
    };
    let gap = b.t - a.t;
    if gap > MAX_GAP {
        return None;
    }
    let f = (t - a.t) / gap;
    let turn = (b.yaw - a.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    Some(Sample {
        t,
        pos: a.pos.lerp(b.pos, f),
        yaw: a.yaw + turn * f,
        camera: a.camera,
    })
}

/// A distinct colour (linear RGB) per overlaid recording, cycling.
pub(crate) fn recording_color(index: usize) -> [f32; 3] {
    const COLORS: [[f32; 3]; 6] = [
        [0.10, 0.45, 1.0],
        [1.0, 0.30, 0.02],
        [0.10, 0.80, 0.15],
        [1.0, 0.25, 0.55],
        [1.0, 0.85, 0.05],
        [0.50, 0.20, 1.0],
    ];
    COLORS[index % COLORS.len()]
}

/// The colour (linear RGB) of an event marker.
pub(crate) fn event_color(kind: EventKind) -> [f32; 3] {
    match kind {
        EventKind::Death => [1.0, 0.03, 0.03],
        EventKind::Respawn => [0.1, 0.9, 0.2],
        EventKind::Damage => [1.0, 0.4, 0.02],
        EventKind::Chat => [0.5, 0.75, 1.0],
        EventKind::Mark => [1.0, 0.05, 1.0],
        EventKind::Warning => [1.0, 0.65, 0.0],
        EventKind::Error => [0.7, 0.01, 0.05],
        EventKind::SessionStart | EventKind::SessionStop | EventKind::Join | EventKind::Leave => {
            [0.5, 0.5, 0.5]
        }
    }
}

fn rgba(color: [f32; 3], alpha: f32) -> [f32; 4] {
    [color[0], color[1], color[2], alpha]
}

fn line(from: Vec3, to: Vec3, color: [f32; 4], on_top: bool, width: f32) -> Segment {
    Segment {
        from,
        to,
        color,
        on_top,
        width,
    }
}

fn trail(track: &Track, color: [f32; 3], t: f32, out: &mut Vec<Segment>) {
    let past = rgba(color, 0.9);
    let future = rgba(color, 0.35);
    for pair in track.samples.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if b.t - a.t > MAX_GAP {
            continue;
        }
        if b.t <= t {
            out.push(line(a.pos, b.pos, past, false, 2.5));
        } else if a.t >= t {
            out.push(line(a.pos, b.pos, future, false, 1.5));
        } else {
            let mid = a.pos.lerp(b.pos, (t - a.t) / (b.t - a.t));
            out.push(line(a.pos, mid, past, false, 2.5));
            out.push(line(mid, b.pos, future, false, 1.5));
        }
    }
}

fn capsule(pose: &Sample, color: [f32; 3], out: &mut Vec<Segment>) {
    const SIDES: usize = 12;
    let c = rgba(color, 1.0);
    let center = pose.pos - Vec3::Y * 0.5;
    let ring = |y: f32, i: usize| {
        let a = i as f32 / SIDES as f32 * std::f32::consts::TAU;
        center + Vec3::new(a.cos(), y, a.sin())
    };
    for y in [-2.5, 2.5] {
        for i in 0..SIDES {
            out.push(line(ring(y, i), ring(y, i + 1), c, true, 2.0));
        }
    }
    for i in (0..SIDES).step_by(SIDES / 4) {
        out.push(line(ring(-2.5, i), ring(2.5, i), c, true, 2.0));
    }
    let facing = Vec3::new(-pose.yaw.sin(), 0.0, -pose.yaw.cos());
    out.push(line(pose.pos, pose.pos + facing * 1.5, c, true, 2.0));
}

fn camera(pose: &Sample, color: [f32; 3], out: &mut Vec<Segment>) {
    if let Some((pos, look)) = pose.camera {
        let c = rgba(color, 0.8);
        out.push(line(pos, pos, c, true, 6.0));
        out.push(line(pos, pos + look * 3.0, c, true, 2.0));
    }
}

fn markers(rec: &Recording, t: f32, out: &mut Vec<Segment>) {
    for event in rec.events.iter().filter(|e| e.t <= t) {
        let Some(pos) = event.pos else { continue };
        let c = rgba(event_color(event.kind), 1.0);
        out.push(line(pos, pos, c, true, 10.0));
        if event.kind == EventKind::Death {
            let d = Vec3::new(0.8, 0.8, 0.0);
            let e = Vec3::new(0.8, -0.8, 0.0);
            out.push(line(pos - d, pos + d, c, true, 2.0));
            out.push(line(pos - e, pos + e, c, true, 2.0));
        }
    }
}

/// Everything drawn for `recordings` (each with its colour) at time `t`:
/// - trail: the track's whole path as 1.5 px lines in the colour at alpha
///   0.35, the part before `t` at alpha 0.9 and 2.5 px; broken at gaps over
///   `MAX_GAP`; occluded (`on_top: false`).
/// - ghost at `pose_at`: a wire capsule 2 studs wide, 5 tall, centred 0.5
///   below the root (two 12-sided rings at the ends, 4 verticals) plus a
///   1.5-stud facing line from the root, on top, 2 px, alpha 1.
/// - camera: a 6 px dot at its position and a 3-stud look line, alpha 0.8.
/// - every event with a position and `t <= t_now`: a 10 px dot in
///   `event_color`, on top; Death also gets an X of two 1.6-stud lines.
pub(crate) fn segments(recordings: &[(&Recording, [f32; 3])], t: f32) -> Vec<Segment> {
    let mut out = Vec::new();
    for (rec, color) in recordings {
        for track in &rec.tracks {
            trail(track, *color, t, &mut out);
            if let Some(pose) = pose_at(track, t) {
                capsule(&pose, *color, &mut out);
                camera(&pose, *color, &mut out);
            }
        }
        markers(rec, t, &mut out);
    }
    out
}

fn ramp(x: f32) -> [f32; 3] {
    const BLUE: [f32; 3] = [0.0, 0.1, 1.0];
    const YELLOW: [f32; 3] = [1.0, 0.9, 0.0];
    const RED: [f32; 3] = [1.0, 0.05, 0.0];
    let (from, to, f) = if x < 0.5 {
        (BLUE, YELLOW, x * 2.0)
    } else {
        (YELLOW, RED, (x - 0.5) * 2.0)
    };
    std::array::from_fn(|i| from[i] + (to[i] - from[i]) * f)
}

/// Each cell as a square on its floor, raised 0.05 studs, inset 3% per
/// side; colour from a blue → yellow → red ramp of `weight / max`, alpha
/// 0.35 + 0.5 × that share. Empty for an empty grid. `kind` only picks
/// the ramp: Deaths and Stuck start at yellow rather than blue.
pub(crate) fn fills(grid: &HeatGrid, kind: HeatKind) -> Vec<Fill> {
    let inset = grid.cell * 0.03;
    grid.cells
        .iter()
        .map(|(&(i, j), cell)| {
            let share = if grid.max > 0.0 {
                (cell.weight / grid.max).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let x = match kind {
                HeatKind::Presence => share,
                HeatKind::Deaths | HeatKind::Stuck => 0.5 + 0.5 * share,
            };
            let (x0, z0) = (i as f32 * grid.cell + inset, j as f32 * grid.cell + inset);
            let (x1, z1) = (x0 + grid.cell - 2.0 * inset, z0 + grid.cell - 2.0 * inset);
            let y = cell.floor + 0.05;
            Fill {
                corners: [
                    Vec3::new(x0, y, z0),
                    Vec3::new(x1, y, z0),
                    Vec3::new(x1, y, z1),
                    Vec3::new(x0, y, z1),
                ],
                color: rgba(ramp(x), 0.35 + 0.5 * share),
            }
        })
        .collect()
}
