//! `loren update` (`lib/update.js`): moves a project to the runtime this
//! build ships, backing up what it replaces.

mod apply;
mod patch;
mod plan;
mod project;
mod runtime;

use std::path::Path;

use crate::ctx::{UpdateOptions, UpdateResult};

pub use patch::{patch_project, Patched};
pub use project::{classify_loren_file, compare_versions, detect_project, strip_native, Detected};
pub use runtime::promise_problems;

pub fn run_update(root: &Path, opts: &UpdateOptions) -> UpdateResult {
    apply::run_update(root, opts)
}
