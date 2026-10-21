//! One module per command (`lib/commands/*.js`) plus the shared refresh
//! steps (`lib/refresh.js`).

pub mod add;
pub mod delegated;
pub mod init;
pub mod migrate;
pub mod modules;
pub mod refresh;
pub mod serve;

use std::path::Path;

use crate::ctx::{Ctx, Tool};

/// `resolveProjectTool`: `--tool`, else the project's; a [`crate::ctx::CliError`] when unknown.
pub fn resolve_project_tool(
    ctx: &Ctx,
    root: &Path,
    tool_override: Option<&str>,
) -> Result<Tool, crate::ctx::CliError> {
    refresh::resolve_project_tool(ctx, root, tool_override)
}
