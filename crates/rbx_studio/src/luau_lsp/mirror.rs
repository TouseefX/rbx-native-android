//! The place as `luau-lsp` needs to see it: a folder holding one file per
//! script and a Rojo-format `sourcemap.json` naming the whole instance tree,
//! so `require(script.Parent.Module)` resolves, and `workspace.Baseplate`
//! has a type, exactly as they would against a Rojo project on disk.
//!
//! Files are named by referent rather than by instance path: two siblings may
//! share a name, and a name may hold anything, but a referent is unique and
//! survives a rename or a reparent, so moving a script only rewrites the
//! sourcemap and never renames its file under an open document.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use lsp_types::FileChangeType;
use rbx_dom::{Ref, WeakDom};
use rbx_reflection::ReflectionDatabase;

use crate::script_editor::source;

pub(crate) const SOURCEMAP: &str = "sourcemap.json";

pub(crate) struct Mirror {
    root: PathBuf,
    /// What each script's file held after the last sync; a sync writes only
    /// what moved since.
    written: HashMap<Ref, String>,
    sourcemap: String,
}

impl Mirror {
    pub(crate) fn new(root: PathBuf) -> Self {
        Mirror {
            root,
            written: HashMap::new(),
            sourcemap: String::new(),
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn path_of(&self, reference: Ref) -> PathBuf {
        self.root.join(file_name(reference))
    }

    /// By file name alone: the server may hand a path back spelled
    /// differently from how it was given (a drive letter's case on Windows),
    /// and nothing but this folder holds files named like a live referent.
    pub(crate) fn script_at(&self, path: &Path) -> Option<Ref> {
        let id = path.file_name()?.to_str()?.strip_suffix(".luau")?;
        let reference = Ref::new(id.parse().ok()?);
        self.written.contains_key(&reference).then_some(reference)
    }

    /// Brings the folder in line with `dom`, returning every file it created,
    /// changed or deleted, what the server has to be told about, since it
    /// only rereads a file it has not opened when told the file moved.
    pub(crate) fn sync(
        &mut self,
        dom: &WeakDom,
        db: &ReflectionDatabase,
    ) -> io::Result<Vec<(PathBuf, FileChangeType)>> {
        fs::create_dir_all(&self.root)?;
        let mut scripts = HashMap::new();
        let sourcemap = sourcemap(dom, db, &mut scripts);

        let mut changes = Vec::new();
        for (&reference, text) in &scripts {
            let kind = match self.written.get(&reference) {
                Some(old) if old == text => continue,
                Some(_) => FileChangeType::CHANGED,
                None => FileChangeType::CREATED,
            };
            let path = self.path_of(reference);
            fs::write(&path, text)?;
            changes.push((path, kind));
        }
        for reference in self.written.keys() {
            if !scripts.contains_key(reference) {
                let path = self.path_of(*reference);
                // Already gone is as good as deleted.
                let _ = fs::remove_file(&path);
                changes.push((path, FileChangeType::DELETED));
            }
        }
        self.written = scripts;

        if sourcemap != self.sourcemap {
            let path = self.root.join(SOURCEMAP);
            fs::write(&path, &sourcemap)?;
            changes.push((path, FileChangeType::CHANGED));
            self.sourcemap = sourcemap;
        }
        Ok(changes)
    }
}

impl Drop for Mirror {
    fn drop(&mut self) {
        // A scratch copy of the place; the DOM is the only one that counts.
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn file_name(reference: Ref) -> String {
    format!("{}.luau", reference.value())
}

/// The whole tree under a `DataModel` root, collecting every script's source
/// into `scripts` on the way. Non-scripts are kept too, they are what gives
/// `workspace.Part` a type (Rojo's `--include-non-scripts`).
///
/// Emitted as a stream of text over an explicit work stack rather than a
/// recursive walk building a `serde_json::Value` tree: the recursion, the
/// tree's serialization and the tree's drop all nest as deep as the place
/// does, and `sync` runs on the app thread the moment a script opens. A
/// joined place with a deep chain in it — rigs and their bones, nested
/// models — overflowed the phone's smaller stack right here, the process
/// aborting with no panic and no dialog: the script editor's silent crash.
/// The tree's depth now costs the call stack nothing.
fn sourcemap(dom: &WeakDom, db: &ReflectionDatabase, scripts: &mut HashMap<Ref, String>) -> String {
    let mut out = String::from("{\"name\":\"game\",\"className\":\"DataModel\"");
    let roots: Vec<Ref> = dom
        .root_refs()
        .iter()
        .copied()
        .filter(|&reference| dom.get(reference).is_some())
        .collect();
    if roots.is_empty() {
        out.push('}');
        return out;
    }
    out.push_str(",\"children\":[");

    /// One unit of work: open an instance's object (`first` holds off the
    /// comma an earlier sibling already wrote), or close the children array
    /// or the object an open left pending.
    enum Step {
        Open(Ref, bool),
        CloseArray,
        CloseObject,
    }
    let mut stack = vec![Step::CloseObject, Step::CloseArray];
    for (index, &root) in roots.iter().enumerate().rev() {
        stack.push(Step::Open(root, index == 0));
    }
    while let Some(step) = stack.pop() {
        match step {
            Step::CloseArray => out.push(']'),
            Step::CloseObject => out.push('}'),
            Step::Open(reference, first) => {
                let Some(instance) = dom.get(reference) else {
                    continue;
                };
                if !first {
                    out.push(',');
                }
                out.push_str("{\"name\":");
                quoted(&mut out, instance.name());
                out.push_str(",\"className\":");
                quoted(&mut out, instance.class());
                if source::is_script_class(db, instance.class()) {
                    scripts.insert(reference, source::read(dom, reference).unwrap_or_default());
                    out.push_str(",\"filePaths\":[");
                    quoted(&mut out, &file_name(reference));
                    out.push(']');
                }
                let children: Vec<Ref> = instance
                    .children()
                    .iter()
                    .copied()
                    .filter(|&child| dom.get(child).is_some())
                    .collect();
                if children.is_empty() {
                    out.push('}');
                    continue;
                }
                out.push_str(",\"children\":[");
                stack.push(Step::CloseObject);
                stack.push(Step::CloseArray);
                for (index, &child) in children.iter().enumerate().rev() {
                    stack.push(Step::Open(child, index == 0));
                }
            }
        }
    }
    out
}

/// `text` as a quoted JSON string, escapes and all. Serializing a bare
/// string cannot fail; the fallback is only so nothing on this path can
/// panic.
fn quoted(out: &mut String, text: &str) {
    let quoted = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string());
    out.push_str(&quoted);
}

#[cfg(test)]
mod tests;
