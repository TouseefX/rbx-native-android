//! The project template the Loren CLI ships (`cli/project` upstream), built
//! into the binary from `vendor/loren/project` and unpacked on first use, so
//! every command can work on plain paths exactly as the Node CLI does on its
//! package folder.

use std::fs;
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/template_files.rs"));

/// Overrides the built-in template, as `LOREN_TEMPLATE_DIR` does upstream.
pub const TEMPLATE_DIR_VARIABLE: &str = "LOREN_TEMPLATE_DIR";

/// The template folder: `LOREN_TEMPLATE_DIR` when set, else the built-in copy
/// unpacked under the system temp folder (one folder per template content,
/// so two builds never share a stale copy).
pub fn template_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os(TEMPLATE_DIR_VARIABLE) {
        return std::path::absolute(dir).map_err(|err| err.to_string());
    }
    let dir = std::env::temp_dir()
        .join(format!("rbx-native-loren-{HASH}"))
        .join("project");
    let marker = dir.join(".complete");
    if !marker.is_file() {
        unpack(&dir).map_err(|err| {
            format!(
                "could not unpack the Loren template into {}: {err}",
                dir.display()
            )
        })?;
        fs::write(&marker, HASH).map_err(|err| err.to_string())?;
    }
    Ok(dir)
}

fn unpack(dir: &Path) -> std::io::Result<()> {
    for (rel, bytes) in FILES {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
    }
    Ok(())
}
