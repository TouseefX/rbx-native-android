//! The dock's numeric ranges: the Build rows, in percent, and Custom
//! Scale's height, in studs, each written with its older twin.

use rbx_dom::WeakDom;

use super::{Key, Rule, Rules, Writes};

/// A Build row: its label, its property and older twin, and the bounds
/// Studio's slider spans, in the percent it shows.
pub(crate) struct BuildRange {
    pub(crate) label: &'static str,
    pub(crate) current: &'static str,
    legacy: &'static str,
    pub(crate) bounds: (f32, f32),
}

pub(crate) const BUILD_RANGES: [BuildRange; 5] = [
    BuildRange {
        label: "Height",
        current: "CustomBuildHeight",
        legacy: "CustomHeightScale",
        bounds: (90.0, 105.0),
    },
    BuildRange {
        label: "Width",
        current: "CustomBuildWidth",
        legacy: "CustomWidthScale",
        bounds: (70.0, 100.0),
    },
    BuildRange {
        label: "Head",
        current: "CustomBuildHead",
        legacy: "CustomHeadScale",
        bounds: (95.0, 100.0),
    },
    BuildRange {
        label: "Body Type",
        current: "CustomBuildBodyType",
        legacy: "CustomBodyTypeScale",
        bounds: (0.0, 100.0),
    },
    BuildRange {
        label: "Proportions",
        current: "CustomBuildProportions",
        legacy: "CustomProportionsScale",
        bounds: (0.0, 100.0),
    },
];

impl BuildRange {
    /// The row's range in percent, to a hundredth: stored as a fraction,
    /// 1.05 would read back as 104.99999.
    pub(crate) fn read(&self, rules: &Rules, dom: &WeakDom) -> (f32, f32) {
        let (min, max) = rules.range_of(dom, (Rule::Body, self.current));
        let percent = |v: f32| (v * 10_000.0).round() / 100.0;
        (percent(min), percent(max))
    }

    /// Clamped to the slider's bounds and ordered, as Studio's fields do.
    pub(crate) fn write(&self, min: f32, max: f32) -> Writes {
        let clamp = |v: f32| v.clamp(self.bounds.0, self.bounds.1) / 100.0;
        let (min, max) = (clamp(min.min(max)), clamp(min.max(max)));
        vec![
            ((Rule::Body, self.current), super::range(min, max)),
            ((Rule::Body, self.legacy), super::range(min, max)),
        ]
    }
}

/// Body › Custom Scale's height slider, in studs.
pub(crate) const HEIGHT_RANGE: (f32, f32) = (2.0, 10.0);

/// What Studio's height boxes accept, in studs: past the slider's end, a
/// typed 50 comes back as 25.
const HEIGHT_LIMIT: (f32, f32) = (2.0, 25.0);

/// The absolute height range (min equals max for one height) and the
/// older property Studio mirrors it into.
pub(crate) const WORLD_HEIGHT: Key = (Rule::Body, "CustomWorldHeight");

pub(crate) fn write_height(min: f32, max: f32) -> Writes {
    let clamp = |v: f32| v.clamp(HEIGHT_LIMIT.0, HEIGHT_LIMIT.1);
    let (min, max) = (clamp(min.min(max)), clamp(min.max(max)));
    vec![
        (WORLD_HEIGHT, super::range(min, max)),
        ((Rule::Body, "CustomHeight"), super::range(min, max)),
    ]
}
