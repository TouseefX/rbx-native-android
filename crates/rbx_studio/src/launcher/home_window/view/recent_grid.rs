//! Recent as icons, the list/icons switch, and the `…` button both views
//! open the place's menu from.

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::*;
use crate::home::RecentPlace;
use crate::launcher::home_window::{file_name, HomeWindow};
use crate::launcher::ui::{self};
use crate::tokens;

impl HomeWindow {
    pub(super) fn recent_view_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let segment = |id: &'static str, glyph: &'static str, tip: &'static str, icons: bool| {
            let on = self.recent_icons == icons;
            h_flex()
                .id(id)
                .size(px(24.))
                .rounded(px(5.))
                .items_center()
                .justify_center()
                .cursor_pointer()
                .map(|this| {
                    if on {
                        this.bg(ui::wash()).text_color(tokens::text())
                    } else {
                        this.text_color(tokens::text3())
                            .hover(|this| tokens::hover_fx(this).text_color(tokens::text()))
                    }
                })
                .tooltip(move |window, cx| crate::shell::tooltip::text(tip, window, cx))
                .child(ui::icon(glyph, 13.))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.recent_icons != icons {
                        this.set_recent_icons(icons, cx);
                    }
                }))
        };
        h_flex()
            .h(px(28.))
            .p(px(1.))
            .gap(px(1.))
            .items_center()
            .rounded(px(6.))
            .border_1()
            .border_color(tokens::border())
            .bg(ui::panel2())
            .child(segment("recent-view-list", "list", "List", false))
            .child(segment("recent-view-icons", "layout-grid", "Icons", true))
    }

    pub(super) fn more_button(
        &self,
        id: impl Into<ElementId>,
        place: &RecentPlace,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let place = place.clone();
        h_flex()
            .id(id.into())
            .size(px(MORE))
            .flex_none()
            .rounded(px(5.))
            .items_center()
            .justify_center()
            .text_color(tokens::text3())
            .hover(|this| {
                tokens::hover_fx(this)
                    .bg(ui::wash())
                    .text_color(tokens::text())
            })
            .tooltip(|window, cx| crate::shell::tooltip::text("More", window, cx))
            .child(ui::icon("ellipsis-vertical", 14.))
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.open_recent_menu(place.clone(), event.position(), cx);
            }))
    }

    pub(super) fn recent_tiles(
        &self,
        rows: &[RecentPlace],
        grid: &Grid,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        h_flex().flex_wrap().gap(px(GAP)).children(
            rows.iter()
                .enumerate()
                .map(|(index, place)| self.recent_tile(index, place, grid, cx)),
        )
    }

    fn recent_tile(
        &self,
        index: usize,
        place: &RecentPlace,
        grid: &Grid,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let open = place.clone();
        let menu = place.clone();
        let roblox = match (&place.name, place.place_id) {
            (Some(name), Some(_)) => name.clone(),
            (None, Some(id)) => format!("Place {id}"),
            _ => "Local only".to_string(),
        };
        v_flex()
            .id(SharedString::from(format!("recent-tile-{index}")))
            .w(px(grid.card))
            .gap(px(8.))
            .p(px(10.))
            .rounded(px(8.))
            .border_1()
            .border_color(tokens::border())
            .bg(ui::panel2())
            .cursor_pointer()
            .hover(|this| {
                tokens::hover_fx(this)
                    .border_color(tokens::border2())
                    .bg(tokens::secondary_hover())
            })
            .child(self.recent_thumb(place, grid.card - 22.))
            .child(
                h_flex()
                    .gap(px(6.))
                    .items_center()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(1.))
                            .child(
                                ui::text(12.5, 17.)
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(tokens::text())
                                    .child(file_name(&place.path)),
                            )
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .gap(px(6.))
                                    .items_center()
                                    .child(
                                        ui::text(11., 15.)
                                            .truncate()
                                            .text_color(tokens::text3())
                                            .child(roblox),
                                    )
                                    .when(self.place_archived(place), |this| {
                                        this.child(super::recent::archived_tag())
                                    }),
                            )
                            .child(
                                ui::text(11., 15.)
                                    .truncate()
                                    .text_color(tokens::text3())
                                    .child(place.opened.map(opened_label).unwrap_or_default()),
                            ),
                    )
                    .child(self.more_button(("recent-tile-more", index), place, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.open_recent_menu(menu.clone(), event.position, cx)
                }),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.open_recent(&open, cx)))
    }

    /// The linked experience's icon; a placeholder until it lands, and a
    /// file glyph for a place with no Roblox side.
    fn recent_thumb(&self, place: &RecentPlace, size: f32) -> impl IntoElement {
        let image: Option<Arc<RenderImage>> = place
            .universe_id
            .and_then(|id| self.icons.get(&id).cloned());
        let (glyph, label) = if place.place_id.is_some() {
            ("image", "No icon yet")
        } else {
            ("file", "Local file")
        };
        div()
            .size(px(size))
            .flex_none()
            .rounded(px(6.))
            .overflow_hidden()
            .bg(ui::bg())
            .map(|this| match image {
                Some(image) => this.child(img(image).size_full().object_fit(ObjectFit::Cover)),
                None => this.child(
                    v_flex()
                        .size_full()
                        .items_center()
                        .justify_center()
                        .gap(px(6.))
                        .text_color(tokens::text3())
                        .child(ui::icon(glyph, 30.))
                        .child(ui::text(10.5, 14.).child(label)),
                ),
            })
    }
}
