//! The `.rbxrec` file: a recording as compact little-endian binary.
//!
//! Layout: magic `b"RBXREC"`, `u16` version (1), then the name and the
//! header fields, the players, the tracks and the events. Strings are a
//! `u32` byte length then UTF-8; counts are `u32`; times and coordinates
//! are `f32`; an optional camera or position is a `u8` flag then its
//! floats; an event's kind is `EventKind as u8`.

use glam::Vec3;

use super::{Event, EventKind, Player, Recording, Sample, Track};

pub(crate) const MAGIC: &[u8; 6] = b"RBXREC";
pub(crate) const VERSION: u16 = 1;
/// The extension recordings are saved under.
pub(crate) const EXTENSION: &str = "rbxrec";

fn put_u32(out: &mut Vec<u8>, value: usize) {
    out.extend_from_slice(&(value as u32).to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, value: &str) {
    put_u32(out, value.len());
    out.extend_from_slice(value.as_bytes());
}

fn put_vec3(out: &mut Vec<u8>, value: Vec3) {
    put_f32(out, value.x);
    put_f32(out, value.y);
    put_f32(out, value.z);
}

pub(crate) fn encode(recording: &Recording) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    put_str(&mut out, &recording.name);
    out.extend_from_slice(&recording.started.to_le_bytes());
    put_f32(&mut out, recording.rate_hz);
    put_f32(&mut out, recording.duration);

    put_u32(&mut out, recording.players.len());
    for player in &recording.players {
        out.extend_from_slice(&player.user.to_le_bytes());
        put_str(&mut out, &player.name);
    }

    put_u32(&mut out, recording.tracks.len());
    for track in &recording.tracks {
        out.extend_from_slice(&track.user.to_le_bytes());
        put_u32(&mut out, track.samples.len());
        for sample in &track.samples {
            put_f32(&mut out, sample.t);
            put_vec3(&mut out, sample.pos);
            put_f32(&mut out, sample.yaw);
            match sample.camera {
                Some((pos, look)) => {
                    out.push(1);
                    put_vec3(&mut out, pos);
                    put_vec3(&mut out, look);
                }
                None => out.push(0),
            }
        }
    }

    put_u32(&mut out, recording.events.len());
    for event in &recording.events {
        put_f32(&mut out, event.t);
        out.extend_from_slice(&event.user.to_le_bytes());
        out.push(event.kind as u8);
        match event.pos {
            Some(pos) => {
                out.push(1);
                put_vec3(&mut out, pos);
            }
            None => out.push(0),
        }
        put_f32(&mut out, event.amount);
        put_str(&mut out, &event.label);
        put_str(&mut out, &event.data);
    }
    out
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| "truncated file".to_string())?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    fn vec3(&mut self) -> Result<Vec3, String> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }

    fn string(&mut self) -> Result<String, String> {
        let len = self.u32()? as usize;
        String::from_utf8(self.take(len)?.to_vec()).map_err(|_| "invalid UTF-8 text".to_string())
    }
}

/// `Err` names what was wrong: bad magic, a newer version, a truncated
/// file, an unknown event tag or invalid UTF-8.
pub(crate) fn decode(bytes: &[u8]) -> Result<Recording, String> {
    let mut cur = Cursor { bytes, at: 0 };
    if cur.take(MAGIC.len())? != MAGIC {
        return Err("not a recording (bad magic)".into());
    }
    let version = cur.u16()?;
    if version > VERSION {
        return Err(format!(
            "recording version {version} is newer than supported"
        ));
    }
    let name = cur.string()?;
    let started = cur.u64()?;
    let rate_hz = cur.f32()?;
    let duration = cur.f32()?;

    let mut players = Vec::new();
    for _ in 0..cur.u32()? {
        players.push(Player {
            user: cur.u64()?,
            name: cur.string()?,
        });
    }

    let mut tracks = Vec::new();
    for _ in 0..cur.u32()? {
        let user = cur.u64()?;
        let mut samples = Vec::new();
        for _ in 0..cur.u32()? {
            let t = cur.f32()?;
            let pos = cur.vec3()?;
            let yaw = cur.f32()?;
            let camera = match cur.u8()? {
                0 => None,
                _ => Some((cur.vec3()?, cur.vec3()?)),
            };
            samples.push(Sample {
                t,
                pos,
                yaw,
                camera,
            });
        }
        tracks.push(Track { user, samples });
    }

    let mut events = Vec::new();
    for _ in 0..cur.u32()? {
        let t = cur.f32()?;
        let user = cur.u64()?;
        let tag = cur.u8()?;
        let kind = EventKind::from_tag(tag).ok_or_else(|| format!("unknown event tag {tag}"))?;
        let pos = match cur.u8()? {
            0 => None,
            _ => Some(cur.vec3()?),
        };
        events.push(Event {
            t,
            user,
            kind,
            pos,
            amount: cur.f32()?,
            label: cur.string()?,
            data: cur.string()?,
        });
    }

    Ok(Recording {
        name,
        started,
        rate_hz,
        duration,
        players,
        tracks,
        events,
    })
}
