//! Making, nudging and resetting a layered item's cages from the window.

use glam::Vec3;
use gpui_kit::*;
use rbx_autorig::Part;

use super::super::collect::{fetch_cached, mesh_key};
use super::cages::{self, Placed};
use super::{preview, Cages, FittingWindow, Job};
use crate::shell::rig::mannequin_cages;

/// Which cage an edit is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Which {
    Inner,
    Outer,
}

impl Which {
    pub(super) fn label(self) -> &'static str {
        match self {
            Which::Inner => "Inner",
            Which::Outer => "Outer",
        }
    }
}

fn mesh_uri(dom: &rbx_dom::WeakDom, part: rbx_dom::Ref) -> Option<String> {
    let props = dom.get(part)?.properties();
    ["MeshId", "MeshContent"]
        .iter()
        .find_map(|k| match props.get(*k) {
            Some(rbx_dom::Variant::Content(rbx_dom::Content::Uri(u))) if !u.is_empty() => {
                Some(u.clone())
            }
            Some(rbx_dom::Variant::String(u)) if !u.is_empty() => Some(u.clone()),
            _ => None,
        })
}

impl FittingWindow {
    /// Makes the cages off the UI thread, from the body the preview stands.
    pub(super) fn make_cages(&mut self, cx: &mut Context<Self>) {
        let (Some(live), Some(source)) = (self.live.as_ref(), self.source.as_ref()) else {
            return;
        };
        let name = source.name.clone();
        let read = {
            let dom = &self.shell.read(cx).dom;
            let placed: Vec<Placed> = Part::ALL
                .iter()
                .filter_map(|&part| {
                    let (r, frame) = preview::part(dom, live.rig, part.name())?;
                    Some(Placed {
                        part,
                        frame,
                        size: Vec3::from(preview::size(dom, r)),
                    })
                })
                .collect();
            let handle = preview::frames(dom, live.rig, &self.settings).map(|f| f.0);
            let key = mesh_uri(dom, source.part)
                .and_then(|u| mesh_key(&u))
                .ok_or_else(|| {
                    format!("{name} has no uploaded or local mesh to make cages around")
                });
            handle.and_then(|h| key.map(|k| (placed, h, k, preview::size(dom, source.part))))
        };
        let (placed, handle, key, size) = match read {
            Ok(read) => read,
            Err(e) => {
                self.cages = Job::Failed(e);
                cx.notify();
                return;
            }
        };
        self.cages = Job::Busy;
        cx.notify();
        let task = cx.background_spawn(async move {
            let templates = mannequin_cages()?;
            let item = cages::item_mesh(&fetch_cached(&key)?, size)?;
            let template = cages::template(&templates, &placed, handle)?;
            let made = rbx_autorig::item_cages(&item, &template);
            let uris = cages::write(&name, &made)?;
            Ok::<_, String>(Cages {
                now: made.clone(),
                made,
                uris,
                template,
                placed,
            })
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |view, cx| {
                view.cages = match result {
                    Ok(c) => Job::Ready(c),
                    Err(e) => Job::Failed(e),
                };
                view.sync(cx);
            });
        })
        .detach();
    }

    /// Pushes (or pulls, negative) one cage around a body region.
    pub(super) fn apply_nudge(&mut self, cx: &mut Context<Self>) {
        let nudge = self.nudge;
        let name = self
            .source
            .as_ref()
            .map_or("Item".into(), |s| s.name.clone());
        let handle = {
            let (Some(live), dom) = (self.live.as_ref(), &self.shell.read(cx).dom) else {
                return;
            };
            preview::frames(dom, live.rig, &self.settings).map(|f| f.0)
        };
        let (Job::Ready(c), Ok(handle)) = (&mut self.cages, handle) else {
            return;
        };
        let Some(centre) = nudge.region.centre(&c.placed, handle) else {
            return;
        };
        let cage = match nudge.which {
            Which::Inner => &mut c.now.inner,
            Which::Outer => &mut c.now.outer,
        };
        rbx_autorig::nudge(cage, centre, nudge.radius, nudge.amount);
        self.rewrite_cages(&name, cx);
    }

    /// Puts one cage back as it was made.
    pub(super) fn reset_cage(&mut self, which: Which, cx: &mut Context<Self>) {
        let name = self
            .source
            .as_ref()
            .map_or("Item".into(), |s| s.name.clone());
        let Job::Ready(c) = &mut self.cages else {
            return;
        };
        match which {
            Which::Inner => c.now.inner = c.made.inner.clone(),
            Which::Outer => c.now.outer = c.made.outer.clone(),
        }
        self.rewrite_cages(&name, cx);
    }

    fn rewrite_cages(&mut self, name: &str, cx: &mut Context<Self>) {
        let Job::Ready(c) = &mut self.cages else {
            return;
        };
        match cages::write(name, &c.now) {
            Ok(uris) => c.uris = uris,
            Err(e) => self.cages = Job::Failed(e),
        }
        self.sync(cx);
    }
}
