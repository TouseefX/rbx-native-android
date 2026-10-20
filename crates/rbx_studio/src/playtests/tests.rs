use std::path::{Path, PathBuf};

use rbx_play::protocol::{LogLevel, RecEvent, RecEventKind, RecSample, Side, Uplink};

use super::*;
use crate::playtests::heat::{self, HeatGrid, HeatKind};
use crate::playtests::recorder::Recorder;
use crate::playtests::{format, replay, store};

fn smp(t: f32, x: f32, y: f32, z: f32) -> Sample {
    Sample {
        t,
        pos: Vec3::new(x, y, z),
        yaw: 0.0,
        camera: None,
    }
}

fn track(user: u64, samples: Vec<Sample>) -> Track {
    Track { user, samples }
}

fn event(t: f32, user: u64, kind: EventKind, pos: Option<Vec3>) -> Event {
    Event {
        t,
        user,
        kind,
        pos,
        amount: 0.0,
        label: String::new(),
        data: String::new(),
    }
}

fn recording(started: u64, tracks: Vec<Track>, events: Vec<Event>) -> Recording {
    Recording {
        name: format!("run {started}"),
        started,
        rate_hz: 10.0,
        duration: 20.0,
        players: vec![Player {
            user: 1,
            name: "Ann".into(),
        }],
        tracks,
        events,
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rbx_playtests_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn standing(user: u64, x: f32, y: f32, z: f32, seconds: u32) -> Track {
    track(
        user,
        (0..seconds * 10)
            .map(|i| smp(i as f32 * 0.1, x, y, z))
            .collect(),
    )
}

// ---- format ----

#[test]
fn format_round_trips_every_field() {
    let mut with_cam = smp(0.5, 1.0, 2.0, 3.0);
    with_cam.yaw = 1.5;
    with_cam.camera = Some((Vec3::new(0.0, 10.0, 5.0), Vec3::new(0.0, -0.5, -0.5)));
    let mut events: Vec<Event> = EventKind::ALL
        .iter()
        .enumerate()
        .map(|(i, kind)| {
            event(
                i as f32,
                1,
                *kind,
                (i % 2 == 0).then_some(Vec3::new(i as f32, 1.0, 2.0)),
            )
        })
        .collect();
    events[6].amount = 12.5;
    events[8].label = "checkpoint".into();
    events[8].data = "{\"n\":1}".into();
    let mut rec = recording(
        1_700_000_000,
        vec![
            track(1, vec![smp(0.0, 0.0, 0.0, 0.0), with_cam]),
            track(2, vec![smp(0.25, -4.0, 5.0, 6.0)]),
        ],
        events,
    );
    rec.name = "Épreuve ✓ 日本".into();
    rec.players.push(Player {
        user: 2,
        name: "Bob".into(),
    });
    assert_eq!(format::decode(&format::encode(&rec)), Ok(rec));
}

#[test]
fn format_rejects_bad_magic() {
    let mut bytes = format::encode(&recording(1, vec![], vec![]));
    bytes[0] = b'X';
    assert!(format::decode(&bytes).is_err());
}

#[test]
fn format_rejects_truncated() {
    let rec = recording(1, vec![standing(1, 0.0, 5.0, 0.0, 1)], vec![]);
    let bytes = format::encode(&rec);
    assert!(format::decode(&bytes[..bytes.len() - 3]).is_err());
    assert!(format::decode(&bytes[..4]).is_err());
}

#[test]
fn format_rejects_newer_version() {
    let mut bytes = format::encode(&recording(1, vec![], vec![]));
    bytes[6..8].copy_from_slice(&2u16.to_le_bytes());
    assert!(format::decode(&bytes).is_err());
}

// ---- store ----

#[test]
fn store_place_key_cloud_vs_file() {
    assert_eq!(
        store::place_key(Path::new("/x/a.rbxl"), Some(123)),
        "cloud:123"
    );
    let missing = Path::new("/definitely/not/here.rbxl");
    assert_eq!(store::place_key(missing, None), missing.to_string_lossy());
}

#[test]
fn store_place_dir_is_hex_under_playtests() {
    let root = Path::new("/cfg");
    let dir = store::place_dir(root, "cloud:1");
    assert_eq!(dir.parent(), Some(root.join("playtests").as_path()));
    let leaf = dir.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(leaf.len(), 16);
    assert!(leaf.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')));
}

#[test]
fn store_save_twice_same_started_is_distinct() {
    let dir = temp_dir("save_twice");
    let rec = recording(77, vec![], vec![]);
    let a = store::save(&dir, &rec).unwrap();
    let b = store::save(&dir, &rec).unwrap();
    assert_ne!(a, b);
    assert!(a.exists() && b.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn store_list_newest_first_skips_junk() {
    let dir = temp_dir("list");
    store::save(&dir, &recording(10, vec![], vec![])).unwrap();
    store::save(&dir, &recording(20, vec![], vec![])).unwrap();
    std::fs::write(dir.join("junk.rbxrec"), b"not a recording").unwrap();
    let listed = store::list(&dir);
    let started: Vec<u64> = listed.iter().map(|(_, s)| s.started).collect();
    assert_eq!(started, vec![20, 10]);
    assert!(store::list(&dir.join("missing")).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn store_rename_persists_and_delete_removes() {
    let dir = temp_dir("rename");
    let path = store::save(&dir, &recording(5, vec![], vec![])).unwrap();
    store::rename(&path, "Boss fight").unwrap();
    assert_eq!(store::load(&path).unwrap().name, "Boss fight");
    store::delete(&path).unwrap();
    assert!(!path.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- heat ----

#[test]
fn heat_presence_sums_time_in_cell() {
    let rec = recording(1, vec![standing(1, 1.0, 5.0, 1.0, 3)], vec![]);
    let grid = heat::grid(&[&rec], HeatKind::Presence, 4.0);
    assert_eq!(grid.cells.len(), 1);
    let cell = grid.cells[&(0, 0)];
    assert!((cell.weight - 3.0).abs() <= 0.11, "{}", cell.weight);
    assert_eq!(grid.max, cell.weight);
}

#[test]
fn heat_presence_gap_adds_little() {
    let t = track(1, vec![smp(0.0, 1.0, 5.0, 1.0), smp(10.0, 41.0, 5.0, 1.0)]);
    let rec = recording(1, vec![t], vec![]);
    let grid = heat::grid(&[&rec], HeatKind::Presence, 4.0);
    assert!(grid.cells[&(0, 0)].weight <= 0.2 + 1e-4);
}

#[test]
fn heat_negative_coordinates_floor_divide() {
    let t = track(1, vec![smp(0.0, -0.5, 5.0, 1.0)]);
    let rec = recording(1, vec![t], vec![]);
    let grid = heat::grid(&[&rec], HeatKind::Presence, 4.0);
    assert!(grid.cells.contains_key(&(-1, 0)));
}

#[test]
fn heat_floor_is_lowest_y_minus_root_height() {
    let t = track(1, vec![smp(0.0, 1.0, 8.0, 1.0), smp(0.1, 1.5, 6.0, 1.0)]);
    let rec = recording(1, vec![t], vec![]);
    let grid = heat::grid(&[&rec], HeatKind::Presence, 4.0);
    assert!((grid.cells[&(0, 0)].floor - (6.0 - ROOT_HEIGHT)).abs() < 1e-4);
}

#[test]
fn heat_deaths_count_only_positioned_deaths() {
    let at = Some(Vec3::new(1.0, 5.0, 1.0));
    let events = vec![
        event(1.0, 1, EventKind::Death, at),
        event(2.0, 1, EventKind::Death, None),
        event(3.0, 1, EventKind::Damage, at),
    ];
    let rec = recording(1, vec![], events);
    let grid = heat::grid(&[&rec], HeatKind::Deaths, 4.0);
    assert_eq!(grid.cells.len(), 1);
    assert_eq!(grid.cells[&(0, 0)].weight, 1.0);
    assert_eq!(grid.max, 1.0);
}

#[test]
fn heat_max_is_largest_weight() {
    let rec = recording(
        1,
        vec![
            standing(1, 1.0, 5.0, 1.0, 3),
            standing(2, 21.0, 5.0, 1.0, 1),
        ],
        vec![],
    );
    let grid = heat::grid(&[&rec], HeatKind::Presence, 4.0);
    let largest = grid.cells.values().map(|c| c.weight).fold(0.0, f32::max);
    assert_eq!(grid.max, largest);
}

fn still_then_walk(still: u32) -> Track {
    let mut samples: Vec<Sample> = (0..=still).map(|i| smp(i as f32, 0.0, 5.0, 0.0)).collect();
    for k in 1..=5 {
        samples.push(smp((still + k) as f32, 10.0 * k as f32, 5.0, 0.0));
    }
    track(1, samples)
}

#[test]
fn stuck_finds_one_spot() {
    let spots = heat::stuck_spots(&still_then_walk(6), 5.0, 2.0, &[]);
    assert_eq!(spots.len(), 1);
    let s = spots[0];
    assert!(s.start.abs() < 0.01 && (s.end - 6.0).abs() < 0.01);
    assert!(s.pos.distance(Vec3::new(0.0, 5.0, 0.0)) < 0.01);
}

#[test]
fn stuck_none_while_moving_or_too_short() {
    let moving = track(
        1,
        (0..20)
            .map(|i| smp(i as f32, 3.0 * i as f32, 5.0, 0.0))
            .collect(),
    );
    assert!(heat::stuck_spots(&moving, 5.0, 2.0, &[]).is_empty());
    assert!(heat::stuck_spots(&still_then_walk(4), 5.0, 2.0, &[]).is_empty());
}

#[test]
fn stuck_excludes_dead_time() {
    let still = track(1, (0..=10).map(|i| smp(i as f32, 0.0, 5.0, 0.0)).collect());
    let spots = heat::stuck_spots(&still, 5.0, 2.0, &[(6.5, 20.0)]);
    assert_eq!(spots.len(), 1);
    assert!(spots[0].end <= 6.5);
}

#[test]
fn stuck_dead_intervals_pair_death_with_respawn() {
    let events = vec![
        event(2.0, 1, EventKind::Death, None),
        event(3.0, 2, EventKind::Death, None),
        event(5.0, 1, EventKind::Respawn, None),
        event(8.0, 1, EventKind::Death, None),
    ];
    let mut rec = recording(1, vec![], events);
    rec.duration = 10.0;
    assert_eq!(heat::dead_intervals(&rec, 1), vec![(2.0, 5.0), (8.0, 10.0)]);
}

// ---- replay ----

#[test]
fn replay_pose_lerps_position() {
    let t = track(1, vec![smp(0.0, 0.0, 0.0, 0.0), smp(1.0, 2.0, 0.0, 0.0)]);
    let pose = replay::pose_at(&t, 0.5).unwrap();
    assert!((pose.pos.x - 1.0).abs() < 1e-4);
}

#[test]
fn replay_yaw_takes_short_way() {
    let mut a = smp(0.0, 0.0, 0.0, 0.0);
    let mut b = smp(1.0, 0.0, 0.0, 0.0);
    a.yaw = 3.0;
    b.yaw = -3.0;
    let yaw = replay::pose_at(&track(1, vec![a, b]), 0.5).unwrap().yaw;
    assert!(yaw.abs() > 3.1, "{yaw}");
}

#[test]
fn replay_pose_none_outside_and_in_gaps() {
    let t = track(1, vec![smp(1.0, 0.0, 0.0, 0.0), smp(2.0, 0.0, 0.0, 0.0)]);
    assert!(replay::pose_at(&t, 0.9).is_none());
    assert!(replay::pose_at(&t, 2.1).is_none());
    let gap = track(1, vec![smp(0.0, 0.0, 0.0, 0.0), smp(2.0, 0.0, 0.0, 0.0)]);
    assert!(replay::pose_at(&gap, 1.0).is_none());
}

#[test]
fn replay_segments_draw_trail_and_past_events() {
    let t = track(
        1,
        (0..5)
            .map(|i| smp(i as f32 * 0.5, i as f32, 5.0, 0.0))
            .collect(),
    );
    let at = Vec3::new(77.0, 5.0, 0.0);
    let rec = recording(1, vec![t], vec![event(1.0, 1, EventKind::Death, Some(at))]);
    let input = [(&rec, [1.0, 0.0, 0.0])];
    let marker = |segs: &[rbx_viewer::Segment]| {
        segs.iter()
            .any(|s| s.from == at && s.to == at && s.on_top && s.width == 10.0)
    };
    let mid = replay::segments(&input, 1.5);
    assert!(!mid.is_empty());
    assert!(marker(&mid));
    assert!(!marker(&replay::segments(&input, 0.5)));
}

#[test]
fn replay_fills_empty_and_one_per_cell() {
    let empty = HeatGrid {
        cell: 4.0,
        cells: Default::default(),
        max: 0.0,
    };
    assert!(replay::fills(&empty, HeatKind::Presence).is_empty());
    let rec = recording(1, vec![standing(1, 1.0, 8.0, 1.0, 1)], vec![]);
    let grid = heat::grid(&[&rec], HeatKind::Presence, 4.0);
    let fills = replay::fills(&grid, HeatKind::Presence);
    assert_eq!(fills.len(), 1);
    assert!(fills[0].corners.iter().all(|c| (c.y - 5.05).abs() < 1e-4));
}

// ---- recorder ----

fn rec_sample(t: f64, user: u64, x: f32) -> RecSample {
    RecSample {
        t,
        user,
        pos: [x, 5.0, 0.0],
        yaw: 0.0,
        cam: None,
    }
}

fn rec_event(t: f64, user: u64, kind: RecEventKind, label: &str) -> Uplink {
    Uplink::RecEvent(RecEvent {
        t,
        user,
        kind,
        pos: None,
        amount: None,
        label: label.into(),
        data: String::new(),
    })
}

fn log(side: Side, level: LogLevel, time: f64) -> Uplink {
    Uplink::Log {
        side,
        level,
        message: "careful".into(),
        time,
        parts: vec![],
        origin: None,
    }
}

fn samples(list: Vec<RecSample>) -> Uplink {
    Uplink::RecSamples { samples: list }
}

#[test]
fn recorder_consumes_only_recording_uplinks() {
    let mut r = Recorder::new("n".into(), 1, 10.0);
    assert!(r.uplink(&samples(vec![rec_sample(1.0, 1, 0.0)])));
    assert!(r.uplink(&rec_event(1.0, 1, RecEventKind::Chat, "hi")));
    assert!(!r.uplink(&log(Side::Server, LogLevel::Output, 1.0)));
}

#[test]
fn recorder_finish_without_samples_is_none() {
    let mut r = Recorder::new("n".into(), 1, 10.0);
    r.uplink(&rec_event(1.0, 1, RecEventKind::Chat, "hi"));
    assert!(r.finish().is_none());
}

#[test]
fn recorder_rebases_times_and_stops() {
    let mut r = Recorder::new("n".into(), 9, 10.0);
    r.uplink(&samples(vec![
        rec_sample(100.0, 1, 0.0),
        rec_sample(102.0, 1, 1.0),
    ]));
    let rec = r.finish().unwrap();
    let times: Vec<f32> = rec.tracks[0].samples.iter().map(|s| s.t).collect();
    assert_eq!(times, vec![0.0, 2.0]);
    assert_eq!(rec.duration, 2.0);
    let last = rec.events.last().unwrap();
    assert_eq!(last.kind, EventKind::SessionStop);
    assert_eq!(last.t, 2.0);
    assert_eq!(rec.started, 9);
}

#[test]
fn recorder_join_adds_player_once() {
    let mut r = Recorder::new("n".into(), 1, 10.0);
    r.uplink(&samples(vec![rec_sample(1.0, 5, 0.0)]));
    r.uplink(&rec_event(1.0, 5, RecEventKind::Join, "Bob"));
    r.uplink(&rec_event(1.5, 5, RecEventKind::Join, "Bob"));
    let rec = r.finish().unwrap();
    assert_eq!(rec.players.len(), 1);
    assert_eq!(rec.players[0].name, "Bob");
}

#[test]
fn recorder_server_warning_becomes_event() {
    let mut r = Recorder::new("n".into(), 1, 10.0);
    r.uplink(&samples(vec![rec_sample(100.0, 1, 0.0)]));
    assert!(!r.uplink(&log(Side::Server, LogLevel::Warning, 100.5)));
    let rec = r.finish().unwrap();
    let warning = rec
        .events
        .iter()
        .find(|e| e.kind == EventKind::Warning)
        .unwrap();
    assert!((warning.t - 0.5).abs() < 1e-4);
    assert!(warning.label.contains("careful"));
}
