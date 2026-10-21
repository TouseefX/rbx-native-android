//! Embeds the vendored Loren project template (`vendor/loren/project`) as a
//! sorted `(relative path, bytes)` table, plus a content hash that names the
//! folder [`crate::template`] unpacks it into.

use std::env;
use std::fs;
use std::hash::{DefaultHasher, Hash as _, Hasher as _};
use std::path::{Path, PathBuf};

fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("{}: {err}", dir.display()))
        .map(|entry| entry.expect("readable entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(&path, base, out);
        } else {
            let rel = path.strip_prefix(base).expect("under base");
            let rel = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            out.push((rel, path));
        }
    }
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let root = manifest
        .join("../../vendor/loren/project")
        .canonicalize()
        .expect("vendor/loren/project exists");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    walk(&root, &root, &mut files);

    let mut hasher = DefaultHasher::new();
    let mut table = String::from("pub(crate) static FILES: &[(&str, &[u8])] = &[\n");
    for (rel, path) in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        rel.hash(&mut hasher);
        fs::read(path).expect("readable file").hash(&mut hasher);
        table.push_str(&format!(
            "    ({rel:?}, include_bytes!({:?})),\n",
            path.display().to_string()
        ));
    }
    table.push_str("];\n");
    table.push_str(&format!(
        "pub(crate) const HASH: &str = \"{:016x}\";\n",
        hasher.finish()
    ));
    let out = PathBuf::from(env::var("OUT_DIR").expect("set by cargo")).join("template_files.rs");
    fs::write(out, table).expect("writable OUT_DIR");
}
