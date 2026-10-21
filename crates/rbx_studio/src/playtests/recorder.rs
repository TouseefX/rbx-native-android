//! Builds a [`Recording`] from a running Play session's uplinks.

use glam::Vec3;
use rbx_play::protocol::{LogLevel, RecEvent, RecEventKind, RecSample, Side, Uplink};

use super::{Event, EventKind, Player, Recording, Sample, Track};

/// One session's recording in progress. Sample and event times arrive on
/// the server probe's clock; the first one seen becomes 0 unless
/// `SessionStart` pinned it earlier.
#[derive(Debug, Default)]
pub(crate) struct Recorder {
    recording: Recording,
    /// Server clock value at recording time 0, once known.
    origin: Option<f64>,
    /// Latest recording time seen, for events with no clock of their own.
    latest: f32,
}

impl Recorder {
    /// `name` is the default display name (place name and local time),
    /// `started` Unix seconds, `rate_hz` the requested rate. Adds a
    /// `SessionStart` event at 0.
    pub(crate) fn new(name: String, started: u64, rate_hz: f32) -> Recorder {
        let mut recorder = Recorder {
            recording: Recording {
                name,
                started,
                rate_hz,
                ..Recording::default()
            },
            ..Recorder::default()
        };
        recorder.push_event(0.0, 0, EventKind::SessionStart, None, String::new());
        recorder
    }

    /// Takes what a recording needs from `uplink`; true when it was a
    /// recording uplink (`RecSamples`/`RecEvent`), which nothing else
    /// wants. Also, without consuming them: a server `Log` at warning or
    /// error level becomes a Warning/Error event at its own `time`; a client
    /// one at the latest time; either positioned at the first track's last
    /// sample. A `Join` event adds a `Player` (once per user) named by its label.
    pub(crate) fn uplink(&mut self, uplink: &Uplink) -> bool {
        match uplink {
            Uplink::RecSamples { samples } => {
                for sample in samples {
                    self.add_sample(sample);
                }
                true
            }
            Uplink::RecEvent(event) => {
                self.add_event(event);
                true
            }
            Uplink::Log {
                side,
                level,
                message,
                time,
                ..
            } => {
                let kind = match level {
                    LogLevel::Warning => EventKind::Warning,
                    LogLevel::Error => EventKind::Error,
                    _ => return false,
                };
                let t = match side {
                    Side::Server => self.rebase(*time),
                    Side::Client => self.latest,
                };
                let pos = self
                    .recording
                    .tracks
                    .first()
                    .and_then(|track| track.samples.last())
                    .map(|sample| sample.pos);
                self.push_event(t, 0, kind, pos, message.clone());
                false
            }
            _ => false,
        }
    }

    /// Ends the recording: a `SessionStop` at the latest time, `duration`
    /// set to it, tracks and events sorted by time. `None` when no sample
    /// ever arrived (nothing to replay).
    pub(crate) fn finish(mut self) -> Option<Recording> {
        if self.recording.tracks.iter().all(|t| t.samples.is_empty()) {
            return None;
        }
        let end = self.latest;
        self.push_event(end, 0, EventKind::SessionStop, None, String::new());
        let mut recording = self.recording;
        recording.duration = end;
        for track in &mut recording.tracks {
            track.samples.sort_by(|a, b| a.t.total_cmp(&b.t));
        }
        recording.events.sort_by(|a, b| a.t.total_cmp(&b.t));
        Some(recording)
    }

    fn rebase(&mut self, server_t: f64) -> f32 {
        let origin = *self.origin.get_or_insert(server_t);
        let t = (server_t - origin).max(0.0) as f32;
        self.latest = self.latest.max(t);
        t
    }

    fn push_event(&mut self, t: f32, user: u64, kind: EventKind, pos: Option<Vec3>, label: String) {
        self.recording.events.push(Event {
            t,
            user,
            kind,
            pos,
            amount: 0.0,
            label,
            data: String::new(),
        });
    }

    fn add_sample(&mut self, raw: &RecSample) {
        let t = self.rebase(raw.t);
        let sample = Sample {
            t,
            pos: vec3(raw.pos),
            yaw: raw.yaw,
            camera: raw
                .cam
                .map(|c| (vec3([c[0], c[1], c[2]]), vec3([c[3], c[4], c[5]]))),
        };
        let tracks = &mut self.recording.tracks;
        match tracks.iter_mut().find(|track| track.user == raw.user) {
            Some(track) => track.samples.push(sample),
            None => tracks.push(Track {
                user: raw.user,
                samples: vec![sample],
            }),
        }
    }

    fn add_event(&mut self, raw: &RecEvent) {
        let t = self.rebase(raw.t);
        let kind = match raw.kind {
            RecEventKind::Join => EventKind::Join,
            RecEventKind::Leave => EventKind::Leave,
            RecEventKind::Death => EventKind::Death,
            RecEventKind::Respawn => EventKind::Respawn,
            RecEventKind::Damage => EventKind::Damage,
            RecEventKind::Chat => EventKind::Chat,
            RecEventKind::Mark => EventKind::Mark,
        };
        if kind == EventKind::Join && !self.recording.players.iter().any(|p| p.user == raw.user) {
            self.recording.players.push(Player {
                user: raw.user,
                name: raw.label.clone(),
            });
        }
        self.recording.events.push(Event {
            t,
            user: raw.user,
            kind,
            pos: raw.pos.map(vec3),
            amount: raw.amount.unwrap_or(0.0) as f32,
            label: raw.label.clone(),
            data: raw.data.clone(),
        });
    }
}

fn vec3(v: [f32; 3]) -> Vec3 {
    Vec3::from_array(v)
}
