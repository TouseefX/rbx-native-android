//! The Body tab: Scale, Appearance and Build.

use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, Icon};
use gpui_kit::*;
use rbx_dom::Variant;

use crate::avatar_settings::{
    write_height, Key, Rules, SlotValue, Writes, APPEARANCE_MODE, AVATAR_TYPE, BUILD_MODE,
    BUILD_RANGES, SCALE_MODE, WORLD_HEIGHT,
};
use crate::tokens;

use super::super::rows::rail_slider;
use super::super::Shell;
use super::fields::{Field, Group};
use super::widgets::{check_row, field, fields_row, label, note, section, segmented, slot_rows};

impl Shell {
    /// Scale, Appearance and Build.
    pub(super) fn avatar_body(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let rules = Rules::find(&self.dom);
        let dom = &self.dom;
        let Some(fields) = &self.avatar.fields else {
            return Vec::new();
        };
        let mut scale = vec![segmented(
            "avatar-scale",
            &[
                (0, "Player Choice", "Use players' choice of scale"),
                (
                    1,
                    "Custom Scale",
                    "Scale avatars uniformly to a custom, absolute height or range",
                ),
            ],
            rules.enum_of(dom, SCALE_MODE),
            cx,
            |value| vec![(SCALE_MODE, Variant::Enum(value))],
        )];
        if rules.enum_of(dom, SCALE_MODE) == 1 {
            let (min, max) = rules.range_of(dom, WORLD_HEIGHT);
            let ranged = self.avatar.height_range.unwrap_or(min != max);
            scale.push(label("Height").into_any_element());
            let control = if ranged {
                fields_row(
                    vec![
                        field(fields.input(Field::HeightMin), 70., "", cx),
                        field(fields.input(Field::HeightMax), 70., "", cx),
                    ],
                    "-",
                    "studs",
                )
            } else {
                h_flex()
                    .gap(px(12.))
                    .items_center()
                    .child(div().w(px(220.)).child(rail_slider(fields.height(), cx)))
                    .child(field(fields.input(Field::Height), 70., "studs", cx))
            };
            scale.push(control.into_any_element());
            scale.push(check_row(
                "avatar-height-range",
                "Set minimum and maximum",
                ranged,
                cx,
                move |shell, cx| {
                    shell.avatar.height_range = Some(!ranged);
                    if ranged {
                        // Back to one height: the range collapses onto its top.
                        shell.apply_avatar_writes(write_height(max, max), cx);
                    }
                    cx.notify();
                },
            ));
        }

        let mut appearance = vec![segmented(
            "avatar-appearance",
            &[
                (0, "Player Choice", "Use players' choice of body appearance"),
                (
                    1,
                    "Custom Parts",
                    "Replace specific avatar parts with custom parts",
                ),
            ],
            self.avatar_slot_mode(APPEARANCE_MODE, Group::Body),
            cx,
            |value| slot_mode(APPEARANCE_MODE, Group::Body, value),
        )];
        if self.avatar_slot_mode(APPEARANCE_MODE, Group::Body) == 1 {
            appearance.extend(slot_rows(self, Group::Body, cx));
        }

        let mut build = vec![segmented(
            "avatar-build",
            &[
                (
                    0,
                    "Player Choice",
                    "Use players' choice of Build settings in the Customize Avatar page",
                ),
                (
                    1,
                    "Custom Build",
                    "Configure players' Build settings of Customize Avatar page",
                ),
            ],
            rules.enum_of(dom, BUILD_MODE),
            cx,
            |value| vec![(BUILD_MODE, Variant::Enum(value))],
        )];
        if rules.enum_of(dom, AVATAR_TYPE) != 1 {
            build.push(
                h_flex()
                    .gap(px(6.))
                    .items_center()
                    .child(
                        Icon::new(IconName::TriangleAlert)
                            .size(px(13.))
                            .text_color(tokens::warning()),
                    )
                    .child(note("This feature will apply to R15 avatars only"))
                    .into_any_element(),
            );
        }
        if rules.enum_of(dom, BUILD_MODE) == 1 {
            build.extend(BUILD_RANGES.iter().enumerate().map(|(row, range)| {
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(div().w(px(110.)).flex_none().child(label(range.label)))
                    .child(fields_row(
                        vec![
                            field(fields.input(Field::Build(row, false)), 60., "", cx),
                            field(fields.input(Field::Build(row, true)), 60., "", cx),
                        ],
                        "-",
                        "%",
                    ))
                    .into_any_element()
            }));
        }
        vec![
            section(self, "Scale", scale, cx),
            section(self, "Appearance", appearance, cx),
            section(self, "Build", build, cx),
        ]
    }

    /// A custom-rows section's mode: Custom when its mode says so or any
    /// row is set, which is how a file saved with rows but no mode reads.
    pub(super) fn avatar_slot_mode(&self, key: Key, group: Group) -> u32 {
        let rules = Rules::find(&self.dom);
        let custom = rules.enum_of(&self.dom, key) == 1
            || group
                .slots()
                .iter()
                .any(|slot| slot.read(&rules, &self.dom) != SlotValue::Off);
        u32::from(custom)
    }
}

/// A custom-rows section's segmented pick: Custom sets the mode, Player
/// Choice also clears every row, which would otherwise still apply.
pub(super) fn slot_mode(key: Key, group: Group, value: u32) -> Writes {
    let mut writes = vec![(key, Variant::Enum(value))];
    if value == 0 {
        for slot in group.slots() {
            writes.extend(slot.write(SlotValue::Off, None));
        }
    }
    writes
}
