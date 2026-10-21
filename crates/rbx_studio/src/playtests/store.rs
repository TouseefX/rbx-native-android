//! Where recordings live: `<config dir>/playtests/<place hash>/`, one
//! `.rbxrec` file each, so two places never mix and nothing is written
//! into the place itself.

use std::io;
use std::path::{Path, PathBuf};

use super::format;
use super::{Recording, Summary};

/// A place's identity: `cloud:<placeId>` for a place opened from Roblox,
/// the canonical file path otherwise (the path as given when it cannot be
/// canonicalized).
pub(crate) fn place_key(path: &Path, cloud_place_id: Option<u64>) -> String {
    match cloud_place_id {
        Some(id) => format!("cloud:{id}"),
        None => path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .into_owned(),
    }
}

/// `root/playtests/<16 lowercase hex digits of crate::recovery::fnv1a(key)>`.
pub(crate) fn place_dir(root: &Path, key: &str) -> PathBuf {
    root.join("playtests")
        .join(format!("{:016x}", crate::recovery::fnv1a(key.as_bytes())))
}

/// Writes `recording` to `dir` (made if missing) as
/// `<started>.rbxrec`, or `<started>-2.rbxrec`, `-3`… when taken, with
/// `crate::settings::write_atomic`. Returns the path.
pub(crate) fn save(dir: &Path, recording: &Recording) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let mut path = dir.join(format!("{}.{}", recording.started, format::EXTENSION));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{}-{n}.{}", recording.started, format::EXTENSION));
        n += 1;
    }
    crate::settings::write_atomic(&path, &format::encode(recording))
        .map_err(|err| io::Error::other(err.to_string()))?;
    Ok(path)
}

/// Every `.rbxrec` in `dir` with its summary, or why it can't be read:
/// readable ones newest `started` first, then unreadable ones by path. A
/// missing `dir` is an empty list.
pub(crate) fn list(dir: &Path) -> Vec<(PathBuf, Result<Summary, String>)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(PathBuf, Result<Summary, String>)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == format::EXTENSION))
        .map(|path| {
            let summary = load(&path).map(|recording| recording.summary());
            (path, summary)
        })
        .collect();
    found.sort_by(|a, b| match (&a.1, &b.1) {
        (Ok(x), Ok(y)) => y.started.cmp(&x.started).then_with(|| a.0.cmp(&b.0)),
        (Ok(_), Err(_)) => std::cmp::Ordering::Less,
        (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
        (Err(_), Err(_)) => a.0.cmp(&b.0),
    });
    found
}

pub(crate) fn load(path: &Path) -> Result<Recording, String> {
    let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
    format::decode(&bytes)
}

/// Rewrites the file with `name` as its recording's name.
pub(crate) fn rename(path: &Path, name: &str) -> Result<(), String> {
    let mut recording = load(path)?;
    recording.name = name.to_string();
    crate::settings::write_atomic(path, &format::encode(&recording)).map_err(|err| err.to_string())
}

pub(crate) fn delete(path: &Path) -> io::Result<()> {
    std::fs::remove_file(path)
}
