//! The project template the Loren CLI ships (`cli/project` upstream) and
//! Loren's spec runner, built into the binary from `vendor/loren` and
//! unpacked on first use, so every command can work on plain paths exactly as
//! the Node CLI does on its package folder, and a copied binary never reaches
//! back into the source tree it was built from.

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
    Ok(vendor_dir()?.join("project"))
}

/// The built-in `vendor/loren` copy (`project/` and `test/`), unpacked under
/// the system temp folder.
pub(crate) fn vendor_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("rbx-native-loren-{HASH}"));
    let marker = dir.join(".complete");
    if !marker.is_file() {
        unpack(&dir)
            .map_err(|err| format!("could not unpack Loren into {}: {err}", dir.display()))?;
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

#[cfg(test)]
mod tests {
    #[test]
    fn the_unpacked_copy_holds_the_spec_runner_and_the_template() {
        let dir = super::vendor_dir().unwrap();
        assert!(!dir.starts_with(env!("CARGO_MANIFEST_DIR")));
        assert!(dir.join("test/runner/LorenTest.luau").is_file());
        assert!(dir.join("test/specs/unit").is_dir());
        assert!(dir.join("project/loren/shared").is_dir());
    }
}
