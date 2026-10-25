//! The Movement tab: Collision, Animation Packs, Animation Clips and
//! Abilities.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use rbx_dom::Variant;

use crate::avatar_settings::{
    Rule, Rules, CLIPS_MODE, COLLIDER_SIZE, COLLISION_MODE, CONTROLLER_MODE, HIT_DETECTION,
    PACKS_MODE,
};
use crate::tokens;

use super::super::Shell;
use super::body::slot_mode;
use super::fields::{Field, Group};
use super::widgets::{
    check_row, field, fields_row, label, note, section, segmented, slot_rows, tick,
};

/// The abilities Studio lists, as `AvatarAbilityRules` names them. The
/// class also has `EnableHolding` and `EnableReaching`, which the dock
/// doesn't show.
const ABILITIES: [(&str, &str); 10] = [
    ("EnableClimbing", "Climbing"),
    ("EnableCrouching", "Crouching"),
    ("EnableFallingDown", "Falling Down"),
    ("EnableGettingUp", "Getting Up"),
    ("EnableJumping", "Jumping"),
    ("EnableRunning", "Running"),
    ("EnableSitting", "Sitting"),
    ("EnableSprinting", "Sprinting"),
    ("EnableSwimming", "Swimming"),
    ("EnableTurning", "Turning"),
];

/// `HitAndTouchDetectionMode`, as two radio rows.
const HIT_MODES: [(u32, &str, &str); 2] = [
    (
        0,
        "Use parts",
        "Use avatar part geometry for hit and touch events",
    ),
    (1, "Use collider", "Use colliders for hit and touch events"),
];

impl Shell {
    pub(super) fn avatar_movement(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let rules = Rules::find(&self.dom);
        let dom = &self.dom;
        let Some(fields) = &self.avatar.fields else {
            return Vec::new();
        };

        let mut collision = vec![segmented(
            "avatar-collision",
            &[
                (0, "Default", "Use default collidable parts, uniquely sized per avatar"),
                (1, "Single Collider", "Collide with a single collider box of a consistent size. Works best when avatars are the same custom height"),
                (2, "Legacy", "Use Inner Box colliders to collide with volumes based on attachment points. Compatibility mode for older experiences"),
            ],
            rules.enum_of(dom, COLLISION_MODE),
            cx,
            |value| vec![(COLLISION_MODE, Variant::Enum(value))],
        )];
        if rules.enum_of(dom, COLLISION_MODE) == 1 {
            let parts = (0..3)
                .map(|axis| field(fields.input(Field::Axis(COLLIDER_SIZE, axis)), 56., "", cx))
                .collect();
            collision.push(label("Size").into_any_element());
            collision.push(fields_row(parts, "", "studs").into_any_element());
            collision.push(label("Hit & Touch Detection").into_any_element());
            let current = rules.enum_of(dom, HIT_DETECTION);
            collision.extend(HIT_MODES.into_iter().map(|(value, text, sentence)| {
                h_flex()
                    .id(SharedString::from(format!("avatar-hit-{value}")))
                    .gap(px(8.))
                    .items_start()
                    .cursor_pointer()
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.apply_avatar_writes(vec![(HIT_DETECTION, Variant::Enum(value))], cx)
                    }))
                    .child(radio(value == current))
                    .child(v_flex().child(label(text)).child(note(sentence)))
                    .into_any_element()
            }));
        }

        let packs = vec![segmented(
            "avatar-packs",
            &[
                (
                    0,
                    "Player Choice",
                    "Use standard R15 animations and allow players' purchased animation packs",
                ),
                (
                    1,
                    "Standard R15",
                    "Use standard R15 animations and disable players' purchased animation packs",
                ),
                (
                    2,
                    "Standard R6",
                    "Use R6 animations and disable players' purchased animation packs",
                ),
            ],
            rules.enum_of(dom, PACKS_MODE),
            cx,
            |value| vec![(PACKS_MODE, Variant::Enum(value))],
        )];

        let mut clips = vec![segmented(
            "avatar-clips",
            &[
                (0, "Player Choice", "Leave animation clips as-is"),
                (1, "Custom Clips", "Replace specific animation clips with custom clips. Players' purchased animation packs will override the custom clips below"),
            ],
            self.avatar_slot_mode(CLIPS_MODE, Group::Clip),
            cx,
            |value| slot_mode(CLIPS_MODE, Group::Clip, value),
        )];
        if self.avatar_slot_mode(CLIPS_MODE, Group::Clip) == 1 {
            clips.extend(slot_rows(self, Group::Clip, cx));
        }

        let mut abilities = vec![segmented(
            "avatar-abilities",
            &[
                (
                    0,
                    "Legacy Humanoid",
                    "Use the legacy Humanoid for movement.",
                ),
                (
                    1,
                    "Character Controller Library",
                    "Choose abilities you turn on at runtime",
                ),
            ],
            rules.enum_of(dom, CONTROLLER_MODE),
            cx,
            |value| vec![(CONTROLLER_MODE, Variant::Enum(value))],
        )];
        if rules.enum_of(dom, CONTROLLER_MODE) == 1 {
            abilities.extend(ABILITIES.into_iter().map(|(name, text)| {
                let key = (Rule::Ability, name);
                let on = rules.bool_of(dom, key);
                check_row(format!("avatar-{name}"), text, on, cx, move |shell, cx| {
                    shell.apply_avatar_writes(vec![(key, Variant::Bool(!on))], cx)
                })
            }));
        }

        vec![
            section(self, "Collision", collision, cx),
            section(self, "Animation Packs", packs, cx),
            section(self, "Animation Clips", clips, cx),
            section(self, "Abilities", abilities, cx),
        ]
    }
}

/// A round radio mark, filled when chosen.
fn radio(on: bool) -> Div {
    let ring = tick(false).rounded_full().mt(px(2.));
    if on {
        ring.border_color(tokens::check_on())
            .child(div().size(px(6.)).rounded_full().bg(tokens::check_on()))
    } else {
        ring
    }
}
