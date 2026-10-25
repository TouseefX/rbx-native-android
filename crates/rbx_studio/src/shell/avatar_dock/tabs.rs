//! The Clothing and Accessories tabs: a scale-limit section, a custom
//! rows section, and for accessories their behaviour switches.

use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, Icon};
use gpui_kit::*;
use rbx_dom::Variant;

use crate::avatar_settings::{
    Key, Rule, Rules, ACCESSORY_MODE, CLOTHING_MODE, CUSTOM_ACCESSORY_MODE, CUSTOM_CLOTHING_MODE,
};
use crate::tokens;

use super::super::{chrome, menu, Shell};
use super::body::slot_mode;
use super::fields::{Field, Group, ACCESSORY_BOUNDS, CLOTHING_BOUNDS};
use super::widgets::{check_row, field, fields_row, label, note, section, segmented, slot_rows};

const LIMIT_METHOD: Key = (Rule::Accessory, "LimitMethod");
/// `AvatarSettingsAccessoryLimitMethod`, the two values Studio offers.
const LIMIT_METHODS: [(u32, &str); 2] = [(0, "Scale"), (1, "Remove")];

const BEHAVIOURS: [(&str, &str); 3] = [
    ("EnableSound", "Enable Sound"),
    ("EnableVFX", "Enable VFX"),
    ("EnableEmissives", "Enable Emissives"),
];

impl Shell {
    pub(super) fn avatar_clothing(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let rules = Rules::find(&self.dom);
        let mut scale = vec![segmented(
            "avatar-clothing-scale",
            &[
                (0, "Player Choice", "Leave clothing scale as-is"),
                (
                    1,
                    "Custom Limit",
                    "Remove or scale clothes that exceed limit bounds",
                ),
            ],
            rules.enum_of(&self.dom, CLOTHING_MODE),
            cx,
            |value| vec![(CLOTHING_MODE, Variant::Enum(value))],
        )];
        if rules.enum_of(&self.dom, CLOTHING_MODE) == 1 {
            scale.extend(self.avatar_bounds(CLOTHING_BOUNDS, cx));
        }
        let mut custom = vec![segmented(
            "avatar-custom-clothing",
            &[
                (0, "Player Choice", "Leave clothing as-is"),
                (
                    1,
                    "Custom Clothing",
                    "Add or replace categories of clothes with custom clothes",
                ),
            ],
            self.avatar_slot_mode(CUSTOM_CLOTHING_MODE, Group::Clothing),
            cx,
            |value| slot_mode(CUSTOM_CLOTHING_MODE, Group::Clothing, value),
        )];
        if self.avatar_slot_mode(CUSTOM_CLOTHING_MODE, Group::Clothing) == 1 {
            custom.extend(slot_rows(self, Group::Clothing, cx));
        }
        vec![
            section(self, "Clothing Scale", scale, cx),
            section(self, "Custom Clothing", custom, cx),
        ]
    }

    pub(super) fn avatar_accessories(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let rules = Rules::find(&self.dom);
        let mut scale = vec![segmented(
            "avatar-accessory-scale",
            &[
                (0, "Player Choice", "Leave accessory scale as-is"),
                (
                    1,
                    "Custom Limit",
                    "Remove or scale accessories that exceed limit bounds",
                ),
            ],
            rules.enum_of(&self.dom, ACCESSORY_MODE),
            cx,
            |value| vec![(ACCESSORY_MODE, Variant::Enum(value))],
        )];
        if rules.enum_of(&self.dom, ACCESSORY_MODE) == 1 {
            scale.push(label("Limit Method").into_any_element());
            scale.push(self.avatar_limit_method(&rules, cx));
            scale.extend(self.avatar_bounds(ACCESSORY_BOUNDS, cx));
        }
        let mut custom = vec![segmented(
            "avatar-custom-accessories",
            &[
                (0, "Player Choice", "Leave accessories as-is"),
                (
                    1,
                    "Custom Accessories",
                    "Add or replace categories of accessories with custom accessories",
                ),
            ],
            self.avatar_slot_mode(CUSTOM_ACCESSORY_MODE, Group::Accessory),
            cx,
            |value| slot_mode(CUSTOM_ACCESSORY_MODE, Group::Accessory, value),
        )];
        if self.avatar_slot_mode(CUSTOM_ACCESSORY_MODE, Group::Accessory) == 1 {
            custom.extend(slot_rows(self, Group::Accessory, cx));
        }
        let mut behaviours =
            vec![note("Configure special behaviors of accessories").into_any_element()];
        behaviours.extend(BEHAVIOURS.into_iter().map(|(name, text)| {
            let key = (Rule::Accessory, name);
            let on = rules.bool_of(&self.dom, key);
            check_row(format!("avatar-{name}"), text, on, cx, move |shell, cx| {
                shell.apply_avatar_writes(vec![(key, Variant::Bool(!on))], cx)
            })
        }));
        vec![
            section(self, "Accessory Scale", scale, cx),
            section(self, "Custom Accessories", custom, cx),
            section(self, "Avatar and Accessory Behaviors", behaviours, cx),
        ]
    }

    /// "Limit Bounds": the box, as padding around the avatar per axis.
    fn avatar_bounds(&self, key: Key, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(fields) = &self.avatar.fields else {
            return Vec::new();
        };
        let parts = (0..3)
            .map(|axis| field(fields.input(Field::Axis(key, axis)), 56., "", cx))
            .collect();
        vec![
            label("Limit Bounds").into_any_element(),
            fields_row(parts, "", "% padding around avatar").into_any_element(),
        ]
    }

    fn avatar_limit_method(&self, rules: &Rules, cx: &mut Context<Self>) -> AnyElement {
        let current = rules.enum_of(&self.dom, LIMIT_METHOD);
        let shown = LIMIT_METHODS
            .iter()
            .find(|(value, _)| *value == current)
            .map_or("Remove", |(_, text)| *text);
        let items = LIMIT_METHODS
            .into_iter()
            .map(|(value, text)| {
                menu::item(text)
                    .checked(value == current)
                    .on_click(move |shell, cx| {
                        shell.apply_avatar_writes(vec![(LIMIT_METHOD, Variant::Enum(value))], cx)
                    })
            })
            .collect();
        let trigger = h_flex()
            .id("avatar-limit-method")
            .h(px(26.))
            .w(px(180.))
            .px(px(8.))
            .items_center()
            .justify_between()
            .rounded(tokens::radius())
            .bg(tokens::field_select())
            .cursor_pointer()
            .text_size(tokens::text_sm())
            .text_color(tokens::text())
            .child(shown)
            .child(
                Icon::new(IconName::ChevronDown)
                    .size(px(12.))
                    .text_color(tokens::text3()),
            );
        menu::dropdown(
            self,
            menu::MenuId::AvatarLimit,
            chrome::Trigger::new(trigger),
            items,
            cx,
        )
        .into_any_element()
    }
}
