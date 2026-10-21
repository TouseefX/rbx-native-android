//! Loren (`fabideveloper/Loren-Framework`, MIT, by fabi), rbx-native's
//! optional built-in game framework: a Rust port of its `loren` CLI, the
//! vendored Luau runtime it installs (`vendor/loren`), and what the editor
//! reads from a Loren project (its Services/Controllers graph, its place,
//! its specs).
//!
//! The port follows the Node CLI (`cli/` at the pinned commit, see
//! `vendor/loren/README.md`) module for module: same files written for the
//! same inputs, same messages, same exit codes.

pub mod cli;
pub mod commands;
pub mod constants;
pub mod ctx;
pub mod fsutil;
pub mod graph;
pub mod json;
pub mod lint;
pub mod log;
pub mod names;
pub mod place;
pub mod project;
pub mod prompt;
pub mod run;
pub mod scriptsync;
pub mod specs;
pub mod template;
pub mod tool;
pub mod toolchain;
pub mod types;
pub mod update;

/// Shown wherever the editor offers Loren.
pub const CREDIT: &str = "Loren by fabi \u{2014} github.com/fabideveloper/Loren-Framework";
pub const REPOSITORY_URL: &str = "https://github.com/fabideveloper/Loren-Framework";
