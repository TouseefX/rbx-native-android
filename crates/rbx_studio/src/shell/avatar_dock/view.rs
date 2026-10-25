//! The dock's frame: top bar, sidebar and the scrolling content, which
//! each tab's own module fills.

use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, v_flex, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::avatar_settings::{set_avatar_type, Preset, Rules, AVATAR_TYPE};
use crate::tokens;

use super::super::layout::Panel;
use super::super::{chrome, menu, Shell};
use super::fields::AvatarFields;
use super::widgets::note;
use super::Tab;

/// `GameAvatarType` as the "…" menu offers it, in Studio's order and words.
const AVATAR_TYPES: [(u32, &str, &str); 3] = [
    (1, "R15 Only", "Players join as their R15 avatar"),
    (
        0,
        "R6 Only",
        "Players join as their R6 avatar, or an R6 fallback avatar",
    ),
    (
        2,
        "R15 & R6",
        "Players join as their choice in the Customize Avatar page",
    ),
];

impl Shell {
    /// The dock's trailing controls and body (see `shell::workspace`).
    pub(in crate::shell) fn avatar_dock(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Option<AnyElement>, Option<AnyElement>) {
        if self.avatar.fields.is_none() {
            self.avatar.fields = Some(AvatarFields::new(self, window, cx));
        }
        if let Some(fields) = &self.avatar.fields {
            fields.sync(&self.dom, window, cx);
        }
        let overflow = self.avatar_overflow(cx);
        let content = match self.avatar.tab {
            Tab::General => self.avatar_general(cx),
            Tab::Body => self.avatar_body(cx),
            Tab::Clothing => self.avatar_clothing(cx),
            Tab::Accessories => self.avatar_accessories(cx),
            Tab::Movement => self.avatar_movement(cx),
        };
        let body = v_flex()
            .size_full()
            .min_h_0()
            .child(self.avatar_top_bar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(self.avatar_sidebar(cx))
                    .child(
                        div()
                            // One scroll position per tab, so switching starts at the top.
                            .id(SharedString::from(format!(
                                "avatar-settings-content-{}",
                                self.avatar.tab.label()
                            )))
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_y_scroll()
                            .child(v_flex().w_full().pb(px(12.)).children(content)),
                    ),
            )
            .into_any_element();
        (Some(overflow), Some(body))
    }

    /// The "…": the experience's avatar type, then where the dock sits.
    fn avatar_overflow(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = Rules::find(&self.dom).enum_of(&self.dom, AVATAR_TYPE);
        let mut items: Vec<menu::Item> = AVATAR_TYPES
            .into_iter()
            .map(|(value, text, hint)| {
                menu::item(text)
                    .checked(value == current)
                    .hint(hint)
                    .on_click(move |shell, cx| {
                        shell.apply_avatar_writes(set_avatar_type(value), cx)
                    })
            })
            .collect();
        items.push(menu::separator());
        items.extend(self.move_items(Panel::AvatarSettings));
        menu::dropdown_at(
            self,
            menu::MenuId::AvatarOverflow,
            chrome::dock_options_trigger("avatar-overflow", IconName::Ellipsis, 16., "Avatar type"),
            items,
            Anchor::TopRight,
            cx,
        )
        .into_any_element()
    }

    /// Preset (every tab but General, where the cards are the preset) and
    /// the Preview switch.
    fn avatar_top_bar(&self, cx: &mut Context<Self>) -> Div {
        let preset = Preset::detect(&Rules::find(&self.dom), &self.dom);
        let picker = (self.avatar.tab != Tab::General).then(|| {
            let items = Preset::ALL
                .into_iter()
                .map(|choice| {
                    menu::item(choice.label())
                        .checked(preset == Some(choice))
                        .on_click(move |shell, cx| shell.apply_avatar_writes(choice.writes(), cx))
                })
                .collect();
            let trigger = h_flex()
                .id("avatar-preset")
                .h(px(26.))
                .min_w(px(170.))
                .px(px(8.))
                .gap(px(6.))
                .items_center()
                .justify_between()
                .rounded(tokens::radius())
                .bg(tokens::field_select())
                .cursor_pointer()
                .text_size(tokens::text_sm())
                .text_color(tokens::text())
                .child(preset.map_or("-", Preset::label))
                .child(
                    Icon::new(IconName::ChevronDown)
                        .size(px(12.))
                        .text_color(tokens::text3()),
                );
            h_flex()
                .gap(px(8.))
                .items_center()
                .child(note("Preset"))
                .child(menu::dropdown(
                    self,
                    menu::MenuId::AvatarPreset,
                    chrome::Trigger::new(trigger),
                    items,
                    cx,
                ))
        });
        h_flex()
            .w_full()
            .h(px(38.))
            .flex_none()
            .px(px(12.))
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(tokens::border())
            .child(div().children(picker))
            .child(
                h_flex()
                    .id("avatar-preview")
                    .gap(px(8.))
                    .items_center()
                    .cursor_not_allowed()
                    .opacity(0.5)
                    .tooltip(|window, cx| {
                        super::super::tooltip::text(
                            "Preview needs sample avatars, which this editor doesn't load",
                            window,
                            cx,
                        )
                    })
                    .child(note("Preview"))
                    .child(super::super::rows::toggle_pill(Some(false))),
            )
    }

    fn avatar_sidebar(&self, cx: &mut Context<Self>) -> Div {
        let hidden = self.avatar.sidebar_hidden;
        let entry = |tab: Tab, icon: IconName, cx: &mut Context<Self>| {
            let on = self.avatar.tab == tab;
            h_flex()
                .id(SharedString::from(format!("avatar-tab-{}", tab.label())))
                .h(px(30.))
                .px(px(8.))
                .gap(px(8.))
                .items_center()
                .rounded(tokens::radius())
                .cursor_pointer()
                .text_size(tokens::text_sm())
                .text_color(if on {
                    tokens::text_full()
                } else {
                    tokens::text2()
                })
                .when(on, |this| this.bg(tokens::accent_soft()))
                .when(!on, |this| {
                    this.hover(|this| tokens::hover_fx(this).bg(tokens::hover()))
                })
                .when(hidden, |this| {
                    this.tooltip(move |window, cx| {
                        super::super::tooltip::text(tab.label(), window, cx)
                    })
                })
                .on_click(cx.listener(move |shell, _, _, cx| shell.set_avatar_tab(tab, cx)))
                .child(Icon::new(icon).size(px(16.)))
                .when(!hidden, |this| this.child(tab.label()))
        };
        let footer = h_flex()
            .id("avatar-sidebar-toggle")
            .h(px(30.))
            .px(px(8.))
            .gap(px(8.))
            .items_center()
            .rounded(tokens::radius())
            .cursor_pointer()
            .text_size(tokens::text_sm())
            .text_color(tokens::text3())
            .hover(|this| tokens::hover_fx(this).bg(tokens::hover()))
            .on_click(cx.listener(|shell, _, _, cx| {
                shell.avatar.sidebar_hidden = !shell.avatar.sidebar_hidden;
                cx.notify();
            }))
            .child(
                Icon::new(if hidden {
                    IconName::PanelLeftOpen
                } else {
                    IconName::PanelLeftClose
                })
                .size(px(16.)),
            )
            .when(!hidden, |this| this.child("Hide Sidebar"));
        v_flex()
            .h_full()
            .flex_none()
            .w(if hidden { px(46.) } else { px(150.) })
            .p(px(6.))
            .gap(px(2.))
            .border_r_1()
            .border_color(tokens::border())
            .child(entry(Tab::General, IconName::Settings, cx))
            .child(entry(Tab::Body, IconName::PersonStanding, cx))
            .child(entry(Tab::Clothing, IconName::Shirt, cx))
            .child(entry(Tab::Accessories, IconName::Ribbon, cx))
            .child(entry(Tab::Movement, IconName::Diamond, cx))
            .child(div().flex_1())
            .child(footer)
    }
}
