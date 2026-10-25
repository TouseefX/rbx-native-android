//! The two presets Studio's General tab offers, as the values they set,
//! and which one (if either) a place matches. Measured in Studio: Player
//! Choice is every rule at its default; Consistent Gameplay is that plus a
//! custom height of 5.5 studs (minimum and maximum both set) and a 2×3×1
//! single collider hit-tested by the avatar's parts.

use rbx_dom::{Variant, WeakDom};

use super::schema::{SlotValue, ACCESSORY_SLOTS, BODY_SLOTS, CLIP_SLOTS, CLOTHING_SLOTS};
use super::{vector, Key, Rule, Rules, Writes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Preset {
    PlayerChoice,
    ConsistentGameplay,
}

/// Every mode a section's segmented control sets.
pub(crate) const SCALE_MODE: Key = (Rule::Body, "ScaleMode");
pub(crate) const APPEARANCE_MODE: Key = (Rule::Body, "AppearanceMode");
pub(crate) const BUILD_MODE: Key = (Rule::Body, "BuildMode");
pub(crate) const CLOTHING_MODE: Key = (Rule::Clothing, "ClothingMode");
pub(crate) const CUSTOM_CLOTHING_MODE: Key = (Rule::Clothing, "CustomClothingMode");
pub(crate) const ACCESSORY_MODE: Key = (Rule::Accessory, "AccessoryMode");
pub(crate) const CUSTOM_ACCESSORY_MODE: Key = (Rule::Accessory, "CustomAccessoryMode");
pub(crate) const COLLISION_MODE: Key = (Rule::Collision, "CollisionMode");
pub(crate) const PACKS_MODE: Key = (Rule::Animation, "AnimationPacksMode");
pub(crate) const CLIPS_MODE: Key = (Rule::Animation, "AnimationClipsMode");
pub(crate) const CONTROLLER_MODE: Key = (Rule::Ability, "CharacterControllerMode");
pub(crate) const COLLIDER_SIZE: Key = (Rule::Collision, "SingleColliderSize");
pub(crate) const HIT_DETECTION: Key = (Rule::Collision, "HitAndTouchDetectionMode");

const MODES: [Key; 11] = [
    SCALE_MODE,
    APPEARANCE_MODE,
    BUILD_MODE,
    CLOTHING_MODE,
    CUSTOM_CLOTHING_MODE,
    ACCESSORY_MODE,
    CUSTOM_ACCESSORY_MODE,
    COLLISION_MODE,
    PACKS_MODE,
    CLIPS_MODE,
    CONTROLLER_MODE,
];

impl Preset {
    pub(crate) const ALL: [Preset; 2] = [Preset::PlayerChoice, Preset::ConsistentGameplay];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Preset::PlayerChoice => "Player Choice",
            Preset::ConsistentGameplay => "Consistent Gameplay",
        }
    }

    /// The values the preset owns. Anything else (a Build range, a limit
    /// bound) is left as it stands, as Studio leaves it.
    fn values(self) -> Writes {
        let consistent = self == Preset::ConsistentGameplay;
        let mut values: Writes = MODES
            .into_iter()
            .map(|key| {
                let on = consistent && (key == SCALE_MODE || key == COLLISION_MODE);
                (key, Variant::Enum(u32::from(on)))
            })
            .collect();
        if consistent {
            values.extend(super::ranges::write_height(5.5, 5.5));
            values.push((COLLIDER_SIZE, vector([2.0, 3.0, 1.0])));
            values.push((HIT_DETECTION, Variant::Enum(0)));
        }
        values
    }

    /// The preset's values, plus every custom asset row cleared: a slot
    /// left set would still apply under a Player Choice mode.
    pub(crate) fn writes(self) -> Writes {
        let mut writes = self.values();
        for slot in BODY_SLOTS
            .iter()
            .chain(&CLOTHING_SLOTS)
            .chain(&ACCESSORY_SLOTS)
            .chain(&CLIP_SLOTS)
        {
            writes.extend(slot.write(SlotValue::Off));
        }
        writes
    }

    /// The preset the place matches, `None` for Studio's "--".
    pub(crate) fn detect(rules: &Rules, dom: &WeakDom) -> Option<Preset> {
        let slots_clear = BODY_SLOTS
            .iter()
            .chain(&CLOTHING_SLOTS)
            .chain(&ACCESSORY_SLOTS)
            .chain(&CLIP_SLOTS)
            .all(|slot| slot.read(rules, dom) == SlotValue::Off);
        if !slots_clear {
            return None;
        }
        Preset::ALL.into_iter().find(|preset| {
            preset.values().iter().all(|(key, value)| {
                // The deprecated height twin is written, never compared: a
                // file from before it existed would match nothing.
                key.1 == "CustomHeight" || rules.value(dom, *key) == *value
            })
        })
    }
}
