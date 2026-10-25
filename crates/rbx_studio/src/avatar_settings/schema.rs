//! What each control of the dock reads and writes: Studio's defaults for
//! the properties `reflection-defaults.json` predates, the asset slots
//! (a checkbox and an ID), and the slider bounds.

use rbx_dom::{NumberRange, Variant, Vector3Data, WeakDom};

use super::{Rule, Rules, Writes};

/// Studio's value for a property a file leaves out, read off a place it
/// saved with nothing changed.
pub(super) fn default(name: &str) -> Variant {
    let range = |min, max| Variant::NumberRange(NumberRange { min, max });
    match name {
        "CustomWorldHeight" | "CustomHeight" => range(5.5, 5.5),
        "CustomBuildHeight" | "CustomHeightScale" => range(0.9, 1.05),
        "CustomBuildWidth" | "CustomWidthScale" => range(0.7, 1.0),
        "CustomBuildHead" | "CustomHeadScale" => range(0.95, 1.0),
        "CustomBuildBodyType"
        | "CustomBodyTypeScale"
        | "CustomBuildProportions"
        | "CustomProportionsScale" => range(0.0, 1.0),
        "SingleColliderSize" => Variant::Vector3(Vector3Data {
            x: 2.0,
            y: 3.0,
            z: 1.0,
        }),
        "LimitBounds" => super::vector([0.0; 3]),
        "AvatarType" | "LegacyCollisionMode" | "LimitMethod" => Variant::Enum(1),
        _ if name.starts_with("Enable") || name == "KeepPlayerHead" => Variant::Bool(true),
        _ if name.ends_with("Enabled") => Variant::Bool(false),
        _ if name.ends_with("Override") || name.ends_with("Id") => Variant::Int64(0),
        _ => Variant::Enum(0),
    }
}

/// One asset row: its label, the property Studio saves it in now, the
/// older `…Enabled`/`…Id` pair it still saves beside it, and the other
/// properties a row covering several asset types clears with it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Slot {
    pub(crate) label: &'static str,
    rule: Rule,
    current: Option<&'static str>,
    legacy: Option<(&'static str, &'static str)>,
    also: &'static [&'static str],
    /// Whether a ticked row may stay empty, which removes that category
    /// (clothing and accessories); a body part or a clip needs an ID.
    pub(crate) blank_removes: bool,
}

/// A slot's state: unticked, ticked and empty (remove), or an asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlotValue {
    Off,
    Remove,
    Asset(i64),
}

/// The value a ticked, empty row saves. Asset IDs are positive and 0 is
/// "nothing", so a negative one is the only spare sentinel; Studio's
/// encoding for it was not observable without publishing a place.
const REMOVE: i64 = -1;

impl Slot {
    pub(crate) fn read(&self, rules: &Rules, dom: &WeakDom) -> SlotValue {
        let current = self
            .current
            .map_or(0, |name| rules.int_of(dom, (self.rule, name)));
        match current {
            0 => {}
            REMOVE => return SlotValue::Remove,
            id => return SlotValue::Asset(id),
        }
        let Some((enabled, id)) = self.legacy else {
            return SlotValue::Off;
        };
        match (
            rules.bool_of(dom, (self.rule, enabled)),
            rules.int_of(dom, (self.rule, id)),
        ) {
            (false, _) => SlotValue::Off,
            (true, 0) => SlotValue::Remove,
            (true, id) => SlotValue::Asset(id),
        }
    }

    pub(crate) fn write(&self, value: SlotValue) -> Writes {
        let (current, enabled, id) = match value {
            SlotValue::Off => (0, false, 0),
            SlotValue::Remove => (REMOVE, true, 0),
            SlotValue::Asset(id) => (id, true, id),
        };
        let key = |name| (self.rule, name);
        let mut writes = Writes::new();
        if let Some(name) = self.current {
            writes.push((key(name), Variant::Int64(current)));
        }
        if let Some((enabled_name, id_name)) = self.legacy {
            writes.push((key(enabled_name), Variant::Bool(enabled)));
            writes.push((key(id_name), Variant::Int64(id)));
        }
        let also = if value == SlotValue::Remove {
            REMOVE
        } else {
            0
        };
        writes.extend(
            self.also
                .iter()
                .map(|&name| (key(name), Variant::Int64(also))),
        );
        writes
    }
}

const fn slot(
    label: &'static str,
    rule: Rule,
    current: Option<&'static str>,
    legacy: Option<(&'static str, &'static str)>,
    blank_removes: bool,
) -> Slot {
    Slot {
        label,
        rule,
        current,
        legacy,
        also: &[],
        blank_removes,
    }
}

/// Body › Custom Parts, in Studio's order. Face and Mood have no current
/// property; Studio's file only has the older pair for them.
pub(crate) const BODY_SLOTS: [Slot; 8] = [
    slot(
        "Face",
        Rule::Body,
        None,
        Some(("CustomFaceEnabled", "CustomFaceId")),
        false,
    ),
    slot(
        "Head",
        Rule::Body,
        Some("HeadOverride"),
        Some(("CustomHeadEnabled", "CustomHeadId")),
        false,
    ),
    slot(
        "Mood",
        Rule::Body,
        None,
        Some(("CustomMoodEnabled", "CustomMoodId")),
        false,
    ),
    slot(
        "Torso",
        Rule::Body,
        Some("TorsoOverride"),
        Some(("CustomTorsoEnabled", "CustomTorsoId")),
        false,
    ),
    slot(
        "Left Arm",
        Rule::Body,
        Some("LeftArmOverride"),
        Some(("CustomLeftArmEnabled", "CustomLeftArmId")),
        false,
    ),
    slot(
        "Right Arm",
        Rule::Body,
        Some("RightArmOverride"),
        Some(("CustomRightArmEnabled", "CustomRightArmId")),
        false,
    ),
    slot(
        "Left Leg",
        Rule::Body,
        Some("LeftLegOverride"),
        Some(("CustomLeftLegEnabled", "CustomLeftLegId")),
        false,
    ),
    slot(
        "Right Leg",
        Rule::Body,
        Some("RightLegOverride"),
        Some(("CustomRightLegEnabled", "CustomRightLegId")),
        false,
    ),
];

/// Clothing › Custom Clothing. Studio groups layered clothing the way the
/// avatar editor does: Tops (shirts, T-shirts, sweaters), Outerwear
/// (jackets), Bottoms (pants, shorts, dresses and skirts). An ID lands on
/// the group's main type; emptying a ticked group removes every type in it.
pub(crate) const CLOTHING_SLOTS: [Slot; 8] = [
    Slot {
        also: &["TShirtAccessoryOverride", "SweaterAccessoryOverride"],
        ..slot(
            "Tops",
            Rule::Clothing,
            Some("ShirtAccessoryOverride"),
            Some(("CustomShirtAccessoryEnabled", "CustomShirtAccessoryId")),
            true,
        )
    },
    slot(
        "Outerwear",
        Rule::Clothing,
        Some("JacketAccessoryOverride"),
        Some(("CustomJacketAccessoryEnabled", "CustomJacketAccessoryId")),
        true,
    ),
    Slot {
        also: &["ShortsAccessoryOverride", "DressSkirtAccessoryOverride"],
        ..slot(
            "Bottoms",
            Rule::Clothing,
            Some("PantsAccessoryOverride"),
            Some(("CustomPantsAccessoryEnabled", "CustomPantsAccessoryId")),
            true,
        )
    },
    slot(
        "Left Shoes",
        Rule::Clothing,
        Some("LeftShoesAccessoryOverride"),
        Some((
            "CustomLeftShoesAccessoryEnabled",
            "CustomLeftShoesAccessoryId",
        )),
        true,
    ),
    slot(
        "Right Shoes",
        Rule::Clothing,
        Some("RightShoesAccessoryOverride"),
        Some((
            "CustomRightShoesAccessoryEnabled",
            "CustomRightShoesAccessoryId",
        )),
        true,
    ),
    slot(
        "Classic Shirts",
        Rule::Clothing,
        Some("ClassicShirtsAccessoryOverride"),
        Some((
            "CustomClassicShirtsAccessoryEnabled",
            "CustomClassicShirtsAccessoryId",
        )),
        true,
    ),
    slot(
        "Classic T-Shirts",
        Rule::Clothing,
        Some("ClassicTShirtsAccessoryOverride"),
        Some((
            "CustomClassicTShirtsAccessoryEnabled",
            "CustomClassicTShirtsAccessoryId",
        )),
        true,
    ),
    slot(
        "Classic Pants",
        Rule::Clothing,
        Some("ClassicPantsAccessoryOverride"),
        Some((
            "CustomClassicPantsAccessoryEnabled",
            "CustomClassicPantsAccessoryId",
        )),
        true,
    ),
];

macro_rules! accessory {
    ($label:literal, $kind:literal) => {
        slot(
            $label,
            Rule::Accessory,
            Some(concat!($kind, "AccessoryOverride")),
            Some((
                concat!("Custom", $kind, "AccessoryEnabled"),
                concat!("Custom", $kind, "AccessoryId"),
            )),
            true,
        )
    };
}

/// Accessories › Custom Accessories.
pub(crate) const ACCESSORY_SLOTS: [Slot; 8] = [
    accessory!("Hair", "Hair"),
    accessory!("Head", "Head"),
    accessory!("Face", "Face"),
    accessory!("Neck", "Neck"),
    accessory!("Shoulder", "Shoulder"),
    accessory!("Front", "Front"),
    accessory!("Back", "Back"),
    accessory!("Waist", "Waist"),
];

macro_rules! clip {
    ($label:literal, $current:expr, $legacy:literal) => {
        slot(
            $label,
            Rule::Animation,
            $current,
            Some((
                concat!("Custom", $legacy, "AnimationEnabled"),
                concat!("Custom", $legacy, "AnimationId"),
            )),
            false,
        )
    };
}

/// Movement › Custom Clips. Studio lists two alternate idles; the current
/// schema has one property for the first and none for the second.
pub(crate) const CLIP_SLOTS: [Slot; 10] = [
    clip!("Run", Some("RunAnimationOverride"), "Run"),
    clip!("Walk", Some("WalkAnimationOverride"), "Walk"),
    clip!("Fall", Some("FallAnimationOverride"), "Fall"),
    clip!("Jump", Some("JumpAnimationOverride"), "Jump"),
    clip!("Idle", Some("IdleAnimationOverride"), "Idle"),
    clip!("Idle Alt 1", Some("IdleAltAnimationOverride"), "IdleAlt1"),
    clip!("Idle Alt 2", None, "IdleAlt2"),
    clip!("Swim", Some("SwimAnimationOverride"), "Swim"),
    clip!("Swim Idle", Some("SwimIdleAnimationOverride"), "SwimIdle"),
    clip!("Climb", Some("ClimbAnimationOverride"), "Climb"),
];
