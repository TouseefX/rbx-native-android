//! The Loren dock: the place's Services and Controllers as Loren would boot
//! them (`rbx_loren::graph`), with each module's dependencies, the cycles
//! and the problems a boot would report. Read-only: a row selects or opens
//! its ModuleScript, nothing here writes to the place.
//!
//! `RBX_STUDIO_LOREN=1` opens the dock at startup, a screenshot aid like the
//! other `RBX_STUDIO_*` variables.

mod view;

use gpui_kit::*;
use rbx_dom::{Ref, WeakDom};
use rbx_loren::graph::{self, Graph, Kind};

use super::layout::Panel;
use super::{chrome, menu, Shell};

/// The graph as of the DOM's mutation counter it was built at, so a frame
/// that changed nothing does not re-analyse every module's source.
#[derive(Default)]
pub(super) struct LorenState {
    cached: Option<(u64, Graph)>,
}

/// Read once at startup; documented in this module's doc comment.
const LOREN_VARIABLE: &str = "RBX_STUDIO_LOREN";

/// The folders `graph::dom_modules` reads, per kind.
const SERVICES_PATH: [&str; 3] = ["ServerScriptService", "Server", "Services"];
const CONTROLLERS_PATH: [&str; 4] = [
    "StarterPlayer",
    "StarterPlayerScripts",
    "Client",
    "Controllers",
];

fn child_named(dom: &WeakDom, parent: Ref, name: &str) -> Option<Ref> {
    dom.get(parent)?
        .children()
        .iter()
        .copied()
        .find(|child| dom.get(*child).is_some_and(|i| i.name() == name))
}

fn folder(dom: &WeakDom, path: &[&str]) -> Option<Ref> {
    let (service, rest) = path.split_first()?;
    let root = dom
        .root_refs()
        .iter()
        .copied()
        .find(|r| dom.get(*r).is_some_and(|i| i.class() == *service))?;
    rest.iter()
        .try_fold(root, |at, name| child_named(dom, at, name))
}

/// The ModuleScript a graph node was read from.
pub(super) fn module_ref(dom: &WeakDom, kind: Kind, name: &str) -> Option<Ref> {
    let path: &[&str] = match kind {
        Kind::Service => &SERVICES_PATH,
        Kind::Controller => &CONTROLLERS_PATH,
    };
    let parent = folder(dom, path)?;
    dom.get(parent)?.children().iter().copied().find(|child| {
        dom.get(*child)
            .is_some_and(|i| i.class() == "ModuleScript" && i.name() == name)
    })
}

impl Shell {
    /// `RBX_STUDIO_LOREN=1`: see this module's doc comment.
    pub(super) fn apply_debug_loren(&mut self, cx: &mut Context<Self>) {
        if std::env::var(LOREN_VARIABLE).is_ok_and(|value| value == "1") {
            self.set_panel_open(Panel::Loren, true, cx);
        }
    }

    fn loren_graph(&mut self) -> Graph {
        let mutations = self.dom.mutations();
        match &self.loren.cached {
            Some((at, graph)) if *at == mutations => graph.clone(),
            _ => {
                let graph = graph::analyze(&graph::dom_modules(&self.dom));
                self.loren.cached = Some((mutations, graph.clone()));
                graph
            }
        }
    }

    /// The dock's trailing controls and body (see `shell::workspace`).
    pub(super) fn loren_dock(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Option<AnyElement>, Option<AnyElement>) {
        let overflow = menu::dropdown(
            self,
            menu::MenuId::LorenOverflow,
            chrome::dock_options_trigger(
                "loren-overflow",
                gpui_kit::assets::IconName::Ellipsis,
                16.,
                "Loren dock options",
            ),
            self.move_items(Panel::Loren),
            cx,
        )
        .into_any_element();
        let graph = self.loren_graph();
        let body = self.loren_body(&graph, cx);
        (Some(overflow), Some(body))
    }

    /// One click selects the module's ModuleScript, a double click opens it.
    fn loren_row_clicked(
        &mut self,
        kind: Kind,
        name: &str,
        count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(reference) = module_ref(&self.dom, kind, name) else {
            return;
        };
        if count >= 2 {
            self.open_script(reference, window, cx);
        } else {
            self.select(reference, cx);
        }
    }
}
