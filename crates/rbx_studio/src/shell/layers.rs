//! The shell's half of editor layers and 3D notes (`crate::layers`,
//! `crate::notes`): every edit to them as one undoable step, this viewer's
//! [`LayerView`] kept per place in the editor's config, and what the
//! viewport is told, the roots it leaves undrawn or isolates to, and the
//! note billboards it draws. The Layers dock (`shell::layers_panel`) and the
//! Explorer row menu call in here.

use gpui_kit::*;
use rbx_dom::{Change, Ref, WeakDom};

use crate::command_bar::Feedback;
use crate::layers::{LayerView, LayerViews, PALETTE};
use crate::properties::attributes::ATTRIBUTES_PROPERTY;
use crate::workspace_view::NoteMark;

use super::Shell;

/// The Output dock's `source` for every row this module pushes.
const SOURCE: &str = "Layers";

pub(super) struct LayerState {
    views: LayerViews,
    /// This place's view, as in `views`.
    pub(super) view: LayerView,
    /// The layer the dock has picked: what "Move to Active Layer" assigns.
    pub(super) active: Option<String>,
    /// What the viewport was last sent, so an unchanged sync sends nothing.
    sent_filter: (Option<Vec<Ref>>, Vec<Ref>),
    sent_notes: Vec<NoteMark>,
    pub(super) panel: super::layers_panel::PanelState,
}

impl LayerState {
    /// Reads the stored views and picks `place`'s.
    pub(super) fn load(place: &std::path::Path) -> Self {
        let views = LayerViews::load();
        let view = views.get(place);
        Self {
            views,
            view,
            active: None,
            sent_filter: (None, Vec::new()),
            sent_notes: Vec::new(),
            panel: Default::default(),
        }
    }
}

/// Longest note text a billboard shows; the dock lists it in full.
const BILLBOARD_CHARS: usize = 60;

impl Shell {
    /// The one path every layer or note edit takes: snapshotted for undo,
    /// reflected like any edit (Explorer rebuilt when instances came or
    /// went), and a refusal pushed to the Output dock. `None` on refusal.
    pub(super) fn apply_layer_edit<T>(
        &mut self,
        edit: impl FnOnce(&mut WeakDom) -> Result<T, String>,
        cx: &mut Context<Self>,
    ) -> Option<T> {
        self.push_history();
        let mut dom = std::mem::replace(&mut self.dom, WeakDom::new());
        let result = edit(&mut dom);
        self.dom = dom;
        let changes = self.dom.take_changes();
        let structural = changes
            .iter()
            .any(|change| matches!(change, Change::Added(_) | Change::Removed(_)));
        self.reflect_changes(&changes, cx);
        self.record_history_change(changes);
        if structural {
            self.rebuild_explorer(cx);
        }
        cx.notify();
        match result {
            Ok(value) => Some(value),
            Err(err) => {
                self.output.push(SOURCE, Feedback::Error(err));
                None
            }
        }
    }

    /// Sends the viewport the layer filter and the note billboards as they
    /// stand, each only when it changed. Runs after every reflected edit,
    /// every view toggle, and once at startup.
    pub(super) fn sync_layers(&mut self, cx: &mut Context<Self>) {
        let defined = crate::layers::layers(&self.dom);
        self.layers.view.prune(&defined);
        let filter = (
            self.layers.view.isolate_roots(&self.dom),
            self.layers.view.hidden_roots(&self.dom),
        );
        if filter != self.layers.sent_filter {
            let (only, hidden) = filter.clone();
            self.viewport
                .update(cx, |viewport, _| viewport.set_layer_filter(only, hidden));
            self.layers.sent_filter = filter;
        }
        self.sync_notes(cx);
        cx.notify();
    }

    /// [`Shell::sync_layers`] for one reflected edit. Finding the hidden
    /// roots walks the whole tree, so it is left out for an edit that only
    /// wrote ordinary properties, every move of a drag is one, and only
    /// the notes are placed again, while there are any: one may be pinned
    /// to what moved.
    pub(super) fn sync_layers_after(&mut self, changes: &[Change], cx: &mut Context<Self>) {
        let reshaped = changes.iter().any(|change| match change {
            Change::Property { name, .. } => name == ATTRIBUTES_PROPERTY,
            _ => true,
        });
        if reshaped {
            self.sync_layers(cx);
        } else if !self.layers.sent_notes.is_empty() {
            self.sync_notes(cx);
        }
    }

    fn sync_notes(&mut self, cx: &mut Context<Self>) {
        let marks: Vec<NoteMark> = crate::notes::notes(&self.dom)
            .into_iter()
            .map(|note| NoteMark {
                note: note.referent,
                position: note.position,
                text: note
                    .text
                    .chars()
                    .take(BILLBOARD_CHARS)
                    .collect::<String>()
                    .into(),
                color: note.color,
            })
            .collect();
        if marks != self.layers.sent_notes {
            let sent = marks.clone();
            self.viewport
                .update(cx, |viewport, _| viewport.set_notes(sent));
            self.layers.sent_notes = marks;
        }
    }

    /// Changes this place's view (hide, lock, solo, isolate), stores it and
    /// syncs the viewport and the Explorer's dimming.
    pub(super) fn update_layer_view(
        &mut self,
        change: impl FnOnce(&mut LayerView),
        cx: &mut Context<Self>,
    ) {
        change(&mut self.layers.view);
        self.layers.views.set(&self.path, self.layers.view.clone());
        if let Err(err) = self.layers.views.save() {
            self.output.push(
                SOURCE,
                Feedback::Error(format!("Could not store the layer view: {err}")),
            );
        }
        self.sync_layers(cx);
    }

    /// A new layer, `Layer <n>` in the next palette colour, made active.
    pub(super) fn add_layer(&mut self, cx: &mut Context<Self>) {
        let count = crate::layers::layers(&self.dom).len();
        let name = format!("Layer {}", count + 1);
        let color = PALETTE[count % PALETTE.len()];
        if let Some(id) =
            self.apply_layer_edit(|dom| crate::layers::add_layer(dom, &name, color), cx)
        {
            self.layers.active = Some(id);
        }
    }

    /// Puts the selection on layer `id`, or off any layer with `None`.
    pub(super) fn assign_selection_to_layer(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        let refs = self.selected_all().to_vec();
        if refs.is_empty() {
            return;
        }
        self.apply_layer_edit(|dom| crate::layers::assign(dom, &refs, id.as_deref()), cx);
    }

    /// The Explorer row menu's "Move to Active Layer" and its shortcut:
    /// assigns to the active layer, adding one first when there is none.
    pub(super) fn assign_selection_to_active_layer(&mut self, cx: &mut Context<Self>) {
        let defined = crate::layers::layers(&self.dom);
        let live = self
            .layers
            .active
            .as_ref()
            .is_some_and(|active| defined.iter().any(|layer| &layer.id == active));
        if !live {
            self.add_layer(cx);
        }
        if let Some(active) = self.layers.active.clone() {
            self.assign_selection_to_layer(Some(active), cx);
        }
    }

    /// Selects every instance assigned to layer `id`.
    pub(super) fn select_layer_members(&mut self, id: &str, cx: &mut Context<Self>) {
        let members = crate::layers::members(&self.dom, id);
        if !members.is_empty() {
            self.reselect(members, cx);
        }
    }

    /// Pins a note with `text`: to the selected instance at its anchor when
    /// one is selected and has one, else at the world origin. Selects it.
    pub(super) fn pin_note(&mut self, text: &str, cx: &mut Context<Self>) {
        let anchored = self
            .selected()
            .and_then(|target| crate::notes::anchor(&self.dom, target).map(|at| (target, at)));
        let (target, at) = match anchored {
            Some((target, at)) => (Some(target), at),
            None => (None, [0.0; 3]),
        };
        let author = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "Me".to_owned());
        let text = text.to_owned();
        let created = self.apply_layer_edit(
            |dom| crate::notes::add_note(dom, &text, &author, PALETTE[0], at, target),
            cx,
        );
        if let Some(note) = created {
            self.select(note, cx);
        }
    }

    /// Points the camera at a note.
    pub(super) fn frame_note(&mut self, note: Ref, cx: &mut Context<Self>) {
        let position = crate::notes::notes(&self.dom)
            .into_iter()
            .find(|n| n.referent == note)
            .map(|n| n.position);
        if let Some(position) = position {
            self.viewport.read(cx).frame_point(position);
        }
    }

    /// Whether a viewport click must pass through `referent` (its layer is
    /// locked).
    pub(super) fn layer_locked(&self, referent: Ref) -> bool {
        self.layers.view.is_locked(&self.dom, referent)
    }
}
