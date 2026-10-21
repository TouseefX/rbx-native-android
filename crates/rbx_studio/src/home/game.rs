//! Home's "Open all places": every place of an experience as one game
//! window. Each place gets the same local copy a single open makes
//! (`<universe>-<place>.rbxl` in the places folder, linked so Publish to
//! Roblox targets it), and a `<universe>.rbxgame` manifest lists them.
//!
//! A local copy that already exists is kept as it is rather than replaced:
//! it may hold work that was never published, and the whole-game open has
//! no per-place question to ask about it.

use std::path::PathBuf;

use rbx_cloud::{Client, Experience};

use super::{link_key, links_path, places_dir, read_links, write_atomic, Format, Link, OpenError};
use crate::shell::game::manifest::{self, Manifest, PlaceEntry};

/// Downloads what is missing, links every place, writes the manifest and
/// returns its path. The experience's start place comes first.
pub(crate) fn open_whole_game(
    client: &Client,
    experience: &Experience,
) -> Result<PathBuf, OpenError> {
    let dir = places_dir().ok_or("no config directory to download into".to_string())?;
    let mut places = client
        .universe_places(experience.universe_id)
        .map_err(OpenError::from_cloud)?;
    if places.is_empty() {
        return Err("Roblox listed no places for this experience"
            .to_string()
            .into());
    }
    places.sort_by_key(|place| place.id != experience.root_place_id);
    let mut entries = Vec::new();
    for place in &places {
        let stem = format!("{}-{}", experience.universe_id, place.id);
        let existing = ["rbxl", "rbxlx"]
            .map(|ext| dir.join(format!("{stem}.{ext}")))
            .into_iter()
            .find(|path| path.exists());
        let path = match existing {
            Some(path) => path,
            None => {
                let bytes = client
                    .download_place(place.id)
                    .map_err(OpenError::from_cloud)?;
                let ext = match Format::sniff(&bytes) {
                    Format::Binary => "rbxl",
                    Format::Xml => "rbxlx",
                };
                let path = dir.join(format!("{stem}.{ext}"));
                write_atomic(&path, &bytes).map_err(|err| err.to_string())?;
                path
            }
        };
        record_link(
            &path,
            Link {
                universe_id: experience.universe_id,
                place_id: place.id,
                name: Some(experience.name.clone()),
            },
        )?;
        entries.push(PlaceEntry {
            path: PathBuf::from(path.file_name().unwrap_or_default()),
            place_id: Some(place.id),
            name: Some(place.name.clone()),
        });
    }
    let manifest_path = dir.join(format!(
        "{}.{}",
        experience.universe_id,
        manifest::EXTENSION
    ));
    let manifest = Manifest {
        name: experience.name.clone(),
        universe_id: Some(experience.universe_id),
        places: entries,
    };
    manifest::write(&manifest_path, &manifest)?;
    Ok(manifest_path)
}

/// Links `path` to its Roblox place without putting it on Recent: the game
/// is what Recent lists, not each of its places.
fn record_link(path: &std::path::Path, link: Link) -> Result<(), String> {
    let file = links_path().ok_or("no config directory for links.json")?;
    let mut links = read_links(&file);
    links.insert(link_key(path), link);
    let bytes = serde_json::to_vec_pretty(&links).expect("Links serialize");
    write_atomic(&file, &bytes).map_err(|err| err.to_string())
}
