//! Avatar › Check Item and Avatar › Upload Item: the avatar item validator
//! (`rbx_ugc`) run on the selected `Accessory`/`MeshPart`/`Model` or on a
//! source file, and the upload of the checked item as a `Model` through Open
//! Cloud, handing over to the Creator Hub for the paid avatar item step that
//! no public API offers.

use std::path::PathBuf;

use rbx_dom::Ref;
use rbx_ugc::{AssetType, BodyScale, Finding, Rule};

mod check;
mod collect;
mod fitting;
mod pickers;
mod report;
mod source;
mod upload;
mod upload_window;
mod validator_window;

pub(crate) use source::check_file;

/// The asset type an item is checked and uploaded as, and the body scale
/// its rigid size table is read for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ItemKind {
    pub(crate) asset: AssetType,
    pub(crate) scale: BodyScale,
}

impl Default for ItemKind {
    fn default() -> Self {
        ItemKind {
            asset: AssetType::Hat,
            scale: BodyScale::Normal,
        }
    }
}

/// Where the item being checked comes from.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ItemSource {
    Instance {
        root: Ref,
        name: String,
        class: String,
    },
    File(PathBuf),
}

impl ItemSource {
    pub(crate) fn label(&self) -> String {
        match self {
            ItemSource::Instance { name, .. } => name.clone(),
            ItemSource::File(path) => path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
        }
    }
}

/// An item read for checking, with what reading it could not do.
#[derive(Debug, Clone)]
pub(crate) struct Collected {
    pub(crate) item: rbx_ugc::Item,
    /// A `SurfaceAppearance` with an emissive mask: the upload fee is the
    /// higher one.
    pub(crate) emissive: bool,
    /// `Rule::Unreadable` notes from reading (a texture that would not
    /// download, a mesh Studio ships rather than an uploaded one).
    pub(crate) notes: Vec<Finding>,
    /// Read from a source file, which has no Roblox attachments yet (the
    /// Fitting Tool or the importer adds them): attachment findings are
    /// dropped.
    pub(crate) from_file: bool,
}

impl Collected {
    /// The validator's findings plus the reading notes, errors first.
    pub(crate) fn findings(&self) -> Vec<Finding> {
        // A mesh that failed to read has a note here saying why; the
        // validator's generic one for the same part is dropped.
        let explained = |f: &Finding| {
            self.notes
                .iter()
                .any(|n| n.rule == f.rule && n.subject == f.subject)
        };
        let mut all: Vec<Finding> = rbx_ugc::validate(&self.item)
            .into_iter()
            .filter(|f| !explained(f) && !(self.from_file && f.rule == Rule::Attachment))
            .collect();
        all.extend(self.notes.iter().cloned());
        all.sort_by_key(|f| f.severity);
        all
    }
}
