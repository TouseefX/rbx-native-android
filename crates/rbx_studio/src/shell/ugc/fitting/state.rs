//! What the Fitting Tool is set to, apart from the window: the step, the
//! kind of item, where a rigid item sits on its attachment, the
//! `WrapLayer` a layered one gets, and the test body and animation.

use glam::{EulerRot, Mat3};
use rbx_ugc::{AssetType, BodyScale};

use crate::shell::rig::{self, Cf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    Item,
    Fit,
    Test,
    Generate,
}

impl Step {
    pub(super) const ALL: [Step; 4] = [Step::Item, Step::Fit, Step::Test, Step::Generate];

    pub(super) fn label(self) -> &'static str {
        match self {
            Step::Item => "Item",
            Step::Fit => "Fit",
            Step::Test => "Test",
            Step::Generate => "Generate",
        }
    }

    pub(super) fn index(self) -> usize {
        Step::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    /// Every step past Item works on the item, so it needs one.
    pub(super) fn reachable(self, has_source: bool) -> bool {
        self == Step::Item || has_source
    }
}

/// Studio's two kinds: a rigid accessory welded to an attachment, or
/// layered clothing that wraps the body through its cages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Accessory,
    Clothing,
}

/// Studio's Accessory list, in its order. Eyebrows and eyelashes are rigid
/// here, as Studio lists them, though the Marketplace files them with
/// layered items.
const RIGID: [AssetType; 10] = [
    AssetType::Back,
    AssetType::Eyebrow,
    AssetType::Eyelash,
    AssetType::Face,
    AssetType::Front,
    AssetType::Hair,
    AssetType::Hat,
    AssetType::Neck,
    AssetType::Shoulder,
    AssetType::Waist,
];

/// Studio's Clothing list, in its order; its one Shoes entry is the two
/// Marketplace types here.
const LAYERED: [AssetType; 9] = [
    AssetType::DressSkirt,
    AssetType::Jacket,
    AssetType::LeftShoe,
    AssetType::RightShoe,
    AssetType::Pants,
    AssetType::Shirt,
    AssetType::Shorts,
    AssetType::Sweater,
    AssetType::TShirt,
];

impl Mode {
    pub(super) fn label(self) -> &'static str {
        match self {
            Mode::Accessory => "Accessory",
            Mode::Clothing => "Clothing",
        }
    }

    pub(super) fn types(self) -> &'static [AssetType] {
        match self {
            Mode::Accessory => &RIGID,
            Mode::Clothing => &LAYERED,
        }
    }

    fn default_type(self) -> AssetType {
        match self {
            Mode::Accessory => AssetType::Hat,
            Mode::Clothing => AssetType::Jacket,
        }
    }
}

/// The kind Studio lists `asset` under.
pub(super) fn mode_of(asset: AssetType) -> Mode {
    if LAYERED.contains(&asset) {
        Mode::Clothing
    } else {
        Mode::Accessory
    }
}

/// The attachments an item of `asset` may sit on. Shoes have no row on the
/// spec pages; they go on their own foot's attachment, the one R15 feet carry.
pub(super) fn attachments(asset: AssetType) -> &'static [&'static str] {
    match asset {
        AssetType::LeftShoe => &["LeftFootAttachment"],
        AssetType::RightShoe => &["RightFootAttachment"],
        other => other.attachments(),
    }
}

/// "RightShoulderAttachment" as Studio's picker shows it: "Right Shoulder".
pub(super) fn attachment_label(name: &str) -> String {
    let stem = name.strip_suffix("Attachment").unwrap_or(name);
    let mut out = String::new();
    for c in stem.chars() {
        if c.is_uppercase() && !out.is_empty() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// The body part a layered item of `asset` is modelled on, which its handle
/// sits on with no offset (see `rig::avatar`'s layered weld).
pub(super) fn layered_part(asset: AssetType) -> &'static str {
    match asset {
        AssetType::Pants | AssetType::Shorts | AssetType::DressSkirt => "LowerTorso",
        AssetType::LeftShoe => "LeftFoot",
        AssetType::RightShoe => "RightFoot",
        _ => "UpperTorso",
    }
}

/// Studio's three body scale labels.
pub(super) fn scale_label(scale: BodyScale) -> &'static str {
    match scale {
        BodyScale::Classic => "Classic Blocky",
        BodyScale::Normal => "Realistic Normal",
        BodyScale::Slender => "Realistic Slender",
    }
}

pub(super) fn rig_scale(scale: BodyScale) -> rig::BodyScale {
    match scale {
        BodyScale::Classic => rig::BodyScale::Classic,
        BodyScale::Normal => rig::BodyScale::RthroNormal,
        BodyScale::Slender => rig::BodyScale::RthroSlender,
    }
}

/// Where a rigid item sits relative to its body attachment: studs along the
/// attachment's axes, degrees about them (X, then Y, then Z, as
/// `CFrame.Angles`), and a uniform scale of the handle's size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Placement {
    pub(super) offset: [f32; 3],
    pub(super) rotation: [f32; 3],
    pub(super) scale: f32,
}

impl Default for Placement {
    fn default() -> Self {
        Placement {
            offset: [0.0; 3],
            rotation: [0.0; 3],
            scale: 1.0,
        }
    }
}

impl Placement {
    /// The handle relative to the body attachment.
    pub(super) fn frame(&self) -> Cf {
        let [x, y, z] = self.rotation.map(f32::to_radians);
        let m = Mat3::from_euler(EulerRot::XYZ, x, y, z);
        // `CFrameData` is row-major; glam is column-major.
        Cf::with(self.offset, m.transpose().to_cols_array())
    }
}

/// `WrapLayer.AutoSkin` (`Enum.WrapLayerAutoSkin`, by value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AutoSkin {
    Disabled,
    EnabledPreserve,
    EnabledOverride,
}

impl AutoSkin {
    pub(super) const ALL: [AutoSkin; 3] = [
        AutoSkin::Disabled,
        AutoSkin::EnabledPreserve,
        AutoSkin::EnabledOverride,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            AutoSkin::Disabled => "Disabled",
            AutoSkin::EnabledPreserve => "Enabled Preserve",
            AutoSkin::EnabledOverride => "Enabled Override",
        }
    }

    pub(super) fn value(self) -> u32 {
        self as u32
    }

    pub(super) fn from_value(value: u32) -> AutoSkin {
        AutoSkin::ALL
            .into_iter()
            .find(|a| a.value() == value)
            .unwrap_or(AutoSkin::Disabled)
    }
}

/// The `WrapLayer` a layered item gets, Studio's defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Wrap {
    pub(super) order: i32,
    pub(super) puffiness: f32,
    pub(super) shrink: f32,
    pub(super) auto_skin: AutoSkin,
}

impl Default for Wrap {
    fn default() -> Self {
        Wrap {
            order: 1,
            puffiness: 1.0,
            shrink: 0.0,
            auto_skin: AutoSkin::Disabled,
        }
    }
}

/// Everything the item step and the fit step set.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Settings {
    pub(super) mode: Mode,
    pub(super) asset: AssetType,
    pub(super) attachment: &'static str,
    pub(super) scale: BodyScale,
    pub(super) placement: Placement,
    pub(super) wrap: Wrap,
}

impl Default for Settings {
    fn default() -> Self {
        let asset = Mode::Accessory.default_type();
        Settings {
            mode: Mode::Accessory,
            asset,
            attachment: attachments(asset)[0],
            scale: BodyScale::Classic,
            placement: Placement::default(),
            wrap: Wrap::default(),
        }
    }
}

impl Settings {
    pub(super) fn layered(&self) -> bool {
        self.mode == Mode::Clothing
    }

    pub(super) fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.set_asset(mode.default_type());
        }
    }

    /// A new type keeps the attachment only when it may still use it, and
    /// starts from the attachment itself, as a fresh Studio fit does.
    pub(super) fn set_asset(&mut self, asset: AssetType) {
        if self.asset == asset {
            return;
        }
        self.asset = asset;
        let allowed = attachments(asset);
        if !allowed.contains(&self.attachment) {
            self.attachment = allowed[0];
        }
        self.placement = Placement::default();
    }
}

/// The bodies the item is tried on: a stock body, or a downloaded avatar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Body {
    Preset(BodyScale),
    Player(u64),
    MyAvatar,
}

/// The stock `Animate` states the test step plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Motion {
    Idle,
    Walk,
    Run,
    Jump,
    Climb,
    Fall,
}

impl Motion {
    pub(super) const ALL: [Motion; 6] = [
        Motion::Idle,
        Motion::Walk,
        Motion::Run,
        Motion::Jump,
        Motion::Climb,
        Motion::Fall,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Motion::Idle => "Idle",
            Motion::Walk => "Walk",
            Motion::Run => "Run",
            Motion::Jump => "Jump",
            Motion::Climb => "Climb",
            Motion::Fall => "Fall",
        }
    }

    /// The state's name in the `Animate` script.
    pub(super) fn state(self) -> &'static str {
        match self {
            Motion::Idle => "idle",
            Motion::Walk => "walk",
            Motion::Run => "run",
            Motion::Jump => "jump",
            Motion::Climb => "climb",
            Motion::Fall => "fall",
        }
    }
}

#[cfg(test)]
#[path = "state/tests.rs"]
mod tests;
