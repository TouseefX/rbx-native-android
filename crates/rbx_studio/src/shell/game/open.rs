//! File › Open Game…: a `.rbxgame` manifest, or several place files that
//! become one game through a manifest written beside them.
//!
//! `RBX_STUDIO_OPEN_GAME=<path>[:<path>...]` skips the file chooser, for
//! driving it where no chooser portal answers.

use std::path::PathBuf;

use gpui_kit::{Context, PathPromptOptions};

use super::super::open_place::is_place_file;
use super::super::Shell;
use super::manifest::{self, Manifest};

const OPEN_GAME_VARIABLE: &str = "RBX_STUDIO_OPEN_GAME";

impl Shell {
    pub(crate) fn open_game_dialog(&mut self, cx: &mut Context<Self>) {
        if let Ok(paths) = std::env::var(OPEN_GAME_VARIABLE) {
            let paths = std::env::split_paths(&paths).collect();
            return self.open_game_paths(paths, cx);
        }
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Open Game".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(picked) = picked.await else {
                return;
            };
            let _ = this.update(cx, |shell, cx| match picked {
                Ok(Some(paths)) => shell.open_game_paths(paths, cx),
                Ok(None) => {}
                Err(err) => shell.warn(format!("Open Game: no file dialog: {err:#}"), cx),
            });
        })
        .detach();
    }

    fn open_game_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        match open_target(paths) {
            Ok(path) => self.request_open(path, cx),
            Err(err) => self.warn(format!("Open Game: {err}"), cx),
        }
    }
}

/// The manifest to open for `paths`: the one picked, or one written for the
/// picked places (merged into the folder's existing manifest, if any).
fn open_target(paths: Vec<PathBuf>) -> Result<PathBuf, String> {
    if let [only] = paths.as_slice() {
        if manifest::is_manifest(only) {
            return Ok(only.clone());
        }
    }
    if let Some(other) = paths.iter().find(|path| !is_place_file(path)) {
        return Err(format!(
            "{} is neither a place file nor a .{} manifest",
            other.display(),
            manifest::EXTENSION
        ));
    }
    if paths.len() < 2 {
        return Err("pick a .rbxgame manifest, or two or more place files".into());
    }
    let (path, _) = Manifest::for_files(&paths, None);
    let existing = path.exists().then(|| manifest::read(&path)).transpose()?;
    let (path, manifest) = Manifest::for_files(&paths, existing);
    manifest::write(&path, &manifest)?;
    Ok(path)
}
