//! The General tab: Studio's Preset section, a segmented pick between the
//! two presets and a card saying what the chosen one does. Studio's card
//! also has a picture of three sample avatars, which this one leaves out.

use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, v_flex, Icon};
use gpui_kit::*;

use crate::avatar_settings::{Preset, Rules};
use crate::tokens;

use super::super::Shell;
use super::widgets::{label, note, section, segmented};

const BULLETS: [&str; 3] = [
    "Retain players' intended look",
    "Physically accurate collisions",
    "Consistent size & physics",
];

/// A preset's two lines and which of [`BULLETS`] it keeps.
fn summary(preset: Preset) -> ([&'static str; 2], [bool; 3]) {
    match preset {
        Preset::PlayerChoice => (
            [
                "Avatars are exactly as intended by players, without modification",
                "Ideal for Casual and Social genres",
            ],
            [true, true, false],
        ),
        Preset::ConsistentGameplay => (
            [
                "Avatars have equal height and physics",
                "Ideal for Obby, Action and Adventure genres",
            ],
            [true, false, true],
        ),
    }
}

impl Shell {
    pub(super) fn avatar_general(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let current = Preset::detect(&Rules::find(&self.dom), &self.dom);
        let index = current.map_or(u32::MAX, |preset| preset as u32);
        let mut rows = vec![segmented(
            "avatar-preset-pick",
            &[
                (0, Preset::PlayerChoice.label(), ""),
                (1, Preset::ConsistentGameplay.label(), ""),
            ],
            index,
            cx,
            |value| Preset::ALL[value as usize].writes(),
        )];
        rows.push(match current {
            Some(preset) => card(preset).into_any_element(),
            None => {
                note("These settings are custom: they match neither preset.").into_any_element()
            }
        });
        vec![section(self, "Preset", rows, cx)]
    }
}

fn card(preset: Preset) -> Div {
    let (lines, marks) = summary(preset);
    v_flex()
        .w(px(520.))
        .max_w_full()
        .p(px(16.))
        .gap(px(8.))
        .rounded(tokens::radius())
        .bg(tokens::hover_subtle())
        .child(
            div()
                .text_size(tokens::text_md())
                .line_height(tokens::line_md())
                .text_color(tokens::text_full())
                .child(lines[0]),
        )
        .child(note(lines[1]))
        .children(BULLETS.into_iter().zip(marks).map(|(text, yes)| {
            h_flex()
                .gap(px(8.))
                .items_center()
                .child(
                    Icon::new(if yes { IconName::Check } else { IconName::X })
                        .size(px(14.))
                        .text_color(if yes {
                            tokens::check_on()
                        } else {
                            tokens::text3()
                        }),
                )
                .child(label(text))
        }))
}
