//! The Places bar: one tab per place of the game, above the document tabs,
//! with the game-wide commands at its right end. Drawn by each place's own
//! `Shell` (only while it belongs to a game), reading the game's slots.

use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::launcher::ui;
use crate::tokens;

use super::super::Shell;
use super::workspace::GameWorkspace;

impl Shell {
    pub(in crate::shell) fn places_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let game = self.game.as_ref()?.upgrade()?;
        let me = cx.entity_id();
        let tabs: Vec<(usize, String, bool, bool, bool)> = {
            let workspace = game.read(cx);
            workspace
                .slots
                .iter()
                .enumerate()
                .map(|(index, slot)| {
                    let (loaded, dirty) = match &slot.shell {
                        // This shell is the one being drawn: reading its
                        // entity here would be a re-entrant borrow.
                        Some(shell) if shell.entity_id() == me => {
                            (true, self.has_unsaved_changes())
                        }
                        Some(shell) => (true, shell.read(cx).has_unsaved_changes()),
                        None => (false, false),
                    };
                    (
                        index,
                        slot.label.clone(),
                        index == workspace.active,
                        loaded,
                        dirty,
                    )
                })
                .collect()
        };
        Some(
            h_flex()
                .id("places-bar")
                .w_full()
                .h(px(32.))
                .flex_none()
                .items_center()
                .gap(px(2.))
                .px(px(8.))
                .bg(tokens::chrome())
                .border_b_1()
                .border_color(tokens::border())
                .child(
                    div()
                        .flex_none()
                        .pr(px(8.))
                        .text_size(tokens::text_xs())
                        .text_color(tokens::text3())
                        .child("PLACES"),
                )
                .child(
                    h_flex()
                        .id("places-tabs")
                        .flex_1()
                        .min_w_0()
                        .gap(px(2.))
                        .overflow_x_scroll()
                        .children(
                            tabs.into_iter()
                                .map(|(index, label, active, loaded, dirty)| {
                                    place_tab(&game, index, label, active, loaded, dirty)
                                }),
                        ),
                )
                .child(action(
                    &game,
                    "game-find",
                    "search",
                    "Find in Game",
                    |game, window, cx| game.open_find(window, cx),
                ))
                .child(action(
                    &game,
                    "game-sync",
                    "refresh-cw",
                    "Sync Shared",
                    |game, window, cx| game.open_sync(window, cx),
                ))
                .child(action(
                    &game,
                    "game-save",
                    "save",
                    "Save All",
                    |game, _, cx| game.save_all(cx),
                ))
                .child(action(
                    &game,
                    "game-publish",
                    "upload",
                    "Publish All",
                    |game, _, cx| game.ask_publish_all(cx),
                ))
                .into_any_element(),
        )
    }
}

fn place_tab(
    game: &Entity<GameWorkspace>,
    index: usize,
    label: String,
    active: bool,
    loaded: bool,
    dirty: bool,
) -> impl IntoElement {
    let game = game.downgrade();
    h_flex()
        .id(("place-tab", index))
        .flex_none()
        .h(px(24.))
        .items_center()
        .gap(px(6.))
        .px(px(10.))
        .rounded(tokens::radius_tile())
        .cursor_pointer()
        .text_size(tokens::text_sm())
        .line_height(tokens::line_sm())
        .text_color(if active {
            tokens::text_full()
        } else if loaded {
            tokens::text()
        } else {
            tokens::text_placeholder()
        })
        .when(active, |this| {
            this.bg(tokens::dock()).font_weight(tokens::WEIGHT_SEMIBOLD)
        })
        .when(!active, |this| {
            this.hover(|this| tokens::hover_fx(this).bg(tokens::hover_subtle()))
        })
        .tooltip(move |window, cx| {
            let text = if loaded {
                "Switch to this place"
            } else {
                "Not opened yet: switching reads the place file"
            };
            crate::shell::tooltip::text(text, window, cx)
        })
        .on_click(move |_, window, cx| {
            let _ = game.update(cx, |game, cx| game.activate(index, window, cx));
        })
        .child(ui::icon("map", 12.))
        .child(div().truncate().max_w(px(180.)).child(label))
        .when(dirty, |this| {
            this.child(div().size(px(6.)).rounded_full().bg(tokens::text()))
        })
}

fn action(
    game: &Entity<GameWorkspace>,
    id: &'static str,
    glyph: &'static str,
    label: &'static str,
    run: impl Fn(&mut GameWorkspace, &mut Window, &mut Context<GameWorkspace>) + 'static,
) -> impl IntoElement {
    let game = game.downgrade();
    h_flex()
        .id(id)
        .flex_none()
        .h(px(24.))
        .items_center()
        .gap(px(6.))
        .px(px(8.))
        .rounded(tokens::radius_tile())
        .cursor_pointer()
        .text_size(tokens::text_sm())
        .text_color(tokens::text3())
        .hover(|this| {
            tokens::hover_fx(this)
                .bg(tokens::hover_subtle())
                .text_color(tokens::text())
        })
        .on_click(move |_, window, cx| {
            let _ = game.update(cx, |game, cx| run(game, window, cx));
        })
        .child(ui::icon(glyph, 12.))
        .child(label)
}
