//! A game's manifest: the `.rbxgame` file that names the places a game
//! window opens together. It sits beside the place files (or in the
//! editor's `places` folder for a game downloaded from Home) and is the
//! only thing the game workspace writes of its own: nothing about the game
//! is ever stored inside a place file.
//!
//! Plain JSON, so it can be written by hand or committed next to the
//! places: `{"name": ..., "universe_id": ..., "places": [{"path": ...}]}`.
//! A relative `path` is read against the manifest's own folder.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// The manifest's file extension.
pub(crate) const EXTENSION: &str = "rbxgame";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Manifest {
    /// What the window and Recent call the game.
    pub(crate) name: String,
    /// The Roblox experience the places belong to, when downloaded from Home.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) universe_id: Option<u64>,
    pub(crate) places: Vec<PlaceEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PlaceEntry {
    /// Relative to the manifest's folder, or absolute.
    pub(crate) path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) place_id: Option<u64>,
    /// The place's name on Roblox; the file stem stands in when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
}

/// Whether `path` names a manifest (by extension, case-insensitive).
pub(crate) fn is_manifest(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(EXTENSION))
}

/// Reads and parses the manifest at `path`. The error is a line for the user
/// naming the file. A manifest with no places is an error.
pub(crate) fn read(path: &Path) -> Result<Manifest, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("Couldn't read {}: {err}", path.display()))?;
    let manifest: Manifest = serde_json::from_str(&text)
        .map_err(|err| format!("{} isn't a valid game file: {err}", path.display()))?;
    if manifest.places.is_empty() {
        return Err(format!("{} lists no places", path.display()));
    }
    Ok(manifest)
}

/// Writes `manifest` to `path` as pretty-printed JSON, atomically
/// (`crate::save::write_atomic`).
pub(crate) fn write(path: &Path, manifest: &Manifest) -> Result<(), String> {
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|err| format!("Couldn't encode {}: {err}", path.display()))?;
    crate::save::write_atomic(path, json.as_bytes())
}

impl Manifest {
    /// Every place's path, with relative ones joined onto the folder holding
    /// `manifest_path`, in manifest order.
    pub(crate) fn resolve(&self, manifest_path: &Path) -> Vec<PathBuf> {
        let base = manifest_path.parent().unwrap_or_else(|| Path::new(""));
        self.places
            .iter()
            .map(|place| base.join(&place.path))
            .collect()
    }

    /// The label a place's tab shows: its `name`, else its file stem.
    pub(crate) fn label(&self, index: usize) -> String {
        let place = &self.places[index];
        place.name.clone().unwrap_or_else(|| {
            place
                .path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
    }

    /// The manifest File › Open Game… writes for several local place files:
    /// `<folder>/<folder name>.rbxgame` in the first file's folder, named
    /// after that folder. When that manifest already exists (`existing`), the
    /// picked files missing from it are appended and the rest is kept. Paths
    /// in the picked file's folder are stored relative (bare file names);
    /// others are stored absolute. Duplicates (same resolved path) are
    /// listed once. Returns the manifest's path and contents.
    pub(crate) fn for_files(paths: &[PathBuf], existing: Option<Manifest>) -> (PathBuf, Manifest) {
        let folder = paths
            .first()
            .and_then(|first| first.parent())
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        let folder_name = folder
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Game".to_owned());
        let manifest_path = folder.join(format!("{folder_name}.{EXTENSION}"));

        let mut manifest = existing.unwrap_or_else(|| Manifest {
            name: folder_name,
            universe_id: None,
            places: Vec::new(),
        });
        let mut seen = manifest.resolve(&manifest_path);

        for path in paths {
            if seen.contains(path) {
                continue;
            }
            seen.push(path.clone());
            let stored = match (path.parent() == Some(folder.as_path()), path.file_name()) {
                (true, Some(file_name)) => PathBuf::from(file_name),
                _ => path.clone(),
            };
            manifest.places.push(PlaceEntry {
                path: stored,
                place_id: None,
                name: None,
            });
        }
        (manifest_path, manifest)
    }
}
