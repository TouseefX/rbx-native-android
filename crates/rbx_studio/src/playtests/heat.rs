//! Heat maps across recordings, on a square grid over the ground (X/Z).

use std::collections::BTreeMap;

use glam::Vec3;

use super::{Recording, Track};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeatKind {
    /// Seconds spent in each cell.
    Presence,
    /// Deaths in each cell.
    Deaths,
    /// Seconds spent stuck in each cell (see [`stuck_spots`]).
    Stuck,
}

/// One cell's total.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Cell {
    pub(crate) weight: f32,
    /// The floor under it: the lowest contributing position's Y minus
    /// `super::ROOT_HEIGHT`.
    pub(crate) floor: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HeatGrid {
    /// Cell size in studs; cell `(i, j)` spans X `i*cell..(i+1)*cell`,
    /// Z `j*cell..(j+1)*cell` (`floor` division, so negatives work).
    pub(crate) cell: f32,
    pub(crate) cells: BTreeMap<(i32, i32), Cell>,
    /// The largest weight; 0 when empty.
    pub(crate) max: f32,
}

/// A stretch where a player barely moved while alive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StuckSpot {
    pub(crate) user: u64,
    pub(crate) start: f32,
    pub(crate) end: f32,
    /// Mean position over the stretch.
    pub(crate) pos: Vec3,
}

/// Default stuck window and radius: 5 s without leaving a 2-stud circle.
pub(crate) const STUCK_WINDOW: f32 = 5.0;
pub(crate) const STUCK_RADIUS: f32 = 2.0;

/// Presence: each sample adds the time until the track's next sample,
/// capped at `2 / rate_hz` (a gap is time away, not time spent); the last
/// sample adds `1 / rate_hz`. Deaths: 1 per `Death` event with a position.
/// Stuck: each spot's `end - start` at its `pos`.
pub(crate) fn grid(recordings: &[&Recording], kind: HeatKind, cell: f32) -> HeatGrid {
    let mut cells: BTreeMap<(i32, i32), Cell> = BTreeMap::new();
    let mut add = |pos: Vec3, weight: f32| {
        let key = ((pos.x / cell).floor() as i32, (pos.z / cell).floor() as i32);
        let floor = pos.y - super::ROOT_HEIGHT;
        cells
            .entry(key)
            .and_modify(|c| {
                c.weight += weight;
                c.floor = c.floor.min(floor);
            })
            .or_insert(Cell { weight, floor });
    };
    for rec in recordings {
        match kind {
            HeatKind::Presence => {
                let rate = rec.rate_hz.max(1e-3);
                for track in &rec.tracks {
                    for (i, s) in track.samples.iter().enumerate() {
                        let dt = match track.samples.get(i + 1) {
                            Some(next) => (next.t - s.t).min(2.0 / rate),
                            None => 1.0 / rate,
                        };
                        add(s.pos, dt.max(0.0));
                    }
                }
            }
            HeatKind::Deaths => {
                for event in &rec.events {
                    if event.kind == super::EventKind::Death {
                        if let Some(pos) = event.pos {
                            add(pos, 1.0);
                        }
                    }
                }
            }
            HeatKind::Stuck => {
                for track in &rec.tracks {
                    let dead = dead_intervals(rec, track.user);
                    for spot in stuck_spots(track, STUCK_WINDOW, STUCK_RADIUS, &dead) {
                        add(spot.pos, spot.end - spot.start);
                    }
                }
            }
        }
    }
    let max = cells.values().map(|c| c.weight).fold(0.0, f32::max);
    HeatGrid { cell, cells, max }
}

/// Maximal stretches of at least `window` seconds where every sample stays
/// within `radius` studs (horizontal X/Z distance) of the stretch's first
/// sample, with no sample gap over `window` inside. A stretch that
/// overlaps `dead` (closed `(from, to)` intervals, death to respawn) is
/// cut there: dead time never counts. Stretches do not overlap; sorted by start.
pub(crate) fn stuck_spots(
    track: &Track,
    window: f32,
    radius: f32,
    dead: &[(f32, f32)],
) -> Vec<StuckSpot> {
    let samples = &track.samples;
    let alive = |t: f32| !dead.iter().any(|&(from, to)| t >= from && t <= to);
    let mut spots = Vec::new();
    let mut i = 0;
    while i < samples.len() {
        let anchor = samples[i];
        if !alive(anchor.t) {
            i += 1;
            continue;
        }
        let mut end = i;
        while end + 1 < samples.len() {
            let next = samples[end + 1];
            let dx = next.pos.x - anchor.pos.x;
            let dz = next.pos.z - anchor.pos.z;
            if !alive(next.t)
                || next.t - samples[end].t > window
                || (dx * dx + dz * dz).sqrt() > radius
            {
                break;
            }
            end += 1;
        }
        if samples[end].t - anchor.t >= window {
            let stretch = &samples[i..=end];
            let sum: Vec3 = stretch.iter().map(|s| s.pos).sum();
            spots.push(StuckSpot {
                user: track.user,
                start: anchor.t,
                end: samples[end].t,
                pos: sum / stretch.len() as f32,
            });
            i = end + 1;
        } else {
            i += 1;
        }
    }
    spots
}

/// Each user's dead intervals in `recording`: from a `Death` to that user's
/// next `Respawn` (or the recording's end).
pub(crate) fn dead_intervals(recording: &Recording, user: u64) -> Vec<(f32, f32)> {
    use super::EventKind;
    let mut out = Vec::new();
    let mut open: Option<f32> = None;
    for event in recording.events.iter().filter(|e| e.user == user) {
        match (event.kind, open) {
            (EventKind::Death, None) => open = Some(event.t),
            (EventKind::Respawn, Some(from)) => {
                out.push((from, event.t));
                open = None;
            }
            _ => {}
        }
    }
    if let Some(from) = open {
        out.push((from, recording.duration));
    }
    out
}
