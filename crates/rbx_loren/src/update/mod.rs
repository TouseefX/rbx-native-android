//! `loren update` (`lib/update.js`): moves a project to the runtime this
//! build ships, backing up what it replaces.

mod apply;
mod project;

use std::path::Path;

use crate::ctx::{UpdateOptions, UpdateResult};

pub use project::{
    classify_loren_file, compare_versions, detect_project, patch_project, promise_problems,
    strip_native, Detected, Patched,
};

pub fn run_update(root: &Path, opts: &UpdateOptions) -> UpdateResult {
    apply::run_update(root, opts)
}
