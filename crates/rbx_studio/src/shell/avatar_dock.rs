//! The Avatar Settings dock (Avatar › Settings): Studio's editor for the
//! place's `AvatarSettings` rules, which neither Studio's Explorer nor this
//! one lists. Five tabs down a sidebar, General, Body, Clothing,
//! Accessories and Movement, each a stack of collapsible sections in
//! Studio's order and wording (`studio/avatar-settings.md`).
//!
//! Every control writes straight to the DOM through the same snapshot,
//! reflect and record steps as a Properties edit, so each change is one
//! undo step and Ctrl+S saves it. Studio stages its edits behind Save
//! changes because saving there writes the experience's own copy of the
//! settings; here the place file is what gets saved, so there is nothing
//! to stage. Studio loads that experience copy over the place's rules, so
//! a published place edited here still shows the experience's settings
//! in Studio's dock.
//!
//! The Preview toggle (a lineup of sample avatars under the camera) is
//! drawn but disabled: it needs avatar rigs this editor does not load.
//!
//! `RBX_STUDIO_AVATAR_SETTINGS=<tab>` opens the dock on that tab at startup
//! (`general`, `body`, `clothing`, `accessories`, `movement`), a
//! screenshot aid like the other `RBX_STUDIO_*` variables.

mod body;
mod fields;
mod general;
mod movement;
mod tabs;
mod view;
mod widgets;

use std::collections::HashSet;

use gpui_kit::*;

use crate::avatar_settings::{self, Rules, Writes};

use super::layout::Panel;
use super::Shell;

/// Read once at startup; documented in this module's doc comment.
const OPEN_VARIABLE: &str = "RBX_STUDIO_AVATAR_SETTINGS";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Tab {
    #[default]
    General,
    Body,
    Clothing,
    Accessories,
    Movement,
}

impl Tab {
    const ALL: [Tab; 5] = [
        Tab::General,
        Tab::Body,
        Tab::Clothing,
        Tab::Accessories,
        Tab::Movement,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::General => "General",
            Tab::Body => "Body",
            Tab::Clothing => "Clothing",
            Tab::Accessories => "Accessories",
            Tab::Movement => "Movement",
        }
    }
}

#[derive(Default)]
pub(super) struct AvatarDock {
    tab: Tab,
    sidebar_hidden: bool,
    /// Section titles folded shut, kept across tab switches.
    collapsed: HashSet<&'static str>,
    /// Whether Custom Scale shows a minimum and a maximum. Studio keeps it
    /// as a view choice: a 5.5 to 5.5 range still shows both fields.
    height_range: Option<bool>,
    fields: Option<fields::AvatarFields>,
}

impl Shell {
    /// `RBX_STUDIO_AVATAR_SETTINGS`: see this module's doc comment.
    pub(super) fn apply_debug_avatar_settings(&mut self, cx: &mut Context<Self>) {
        let Ok(wanted) = std::env::var(OPEN_VARIABLE) else {
            return;
        };
        if let Some(tab) = Tab::ALL
            .into_iter()
            .find(|tab| tab.label().eq_ignore_ascii_case(&wanted))
        {
            self.avatar.tab = tab;
        }
        self.set_panel_open(Panel::AvatarSettings, true, cx);
    }

    /// The ribbon tile's click: shows the dock, or hides it when showing.
    pub(super) fn toggle_avatar_settings(&mut self, cx: &mut Context<Self>) {
        let showing = self.is_panel_showing(Panel::AvatarSettings);
        self.set_panel_open(Panel::AvatarSettings, !showing, cx);
    }

    /// The one path every control's edit takes: only the writes that change
    /// something, the rule instances created where the place lacks them,
    /// one undo step for the lot.
    pub(super) fn apply_avatar_writes(&mut self, writes: Writes, cx: &mut Context<Self>) {
        let writes = Rules::find(&self.dom).effective(&self.dom, writes);
        if writes.is_empty() {
            return;
        }
        self.push_history();
        avatar_settings::apply(&mut self.dom, &writes);
        let changes = self.dom.take_changes();
        self.reflect_changes(&changes, cx);
        self.record_history_change(changes);
        cx.notify();
    }

    fn set_avatar_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.avatar.tab = tab;
        cx.notify();
    }

    fn toggle_avatar_section(&mut self, title: &'static str, cx: &mut Context<Self>) {
        if !self.avatar.collapsed.remove(title) {
            self.avatar.collapsed.insert(title);
        }
        cx.notify();
    }
}
