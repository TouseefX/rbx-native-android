//! Dynamic head rules: the `FaceControls`, the 17 FACS poses, the eye and
//! mouth landmarks in the outer cage, the 5 facial actions that must move
//! them, the head staying in its box while it emotes, and the Marketplace
//! part settings.
//!
//! <https://create.roblox.com/docs/avatar/dynamic-heads/specifications>,
//! <https://create.roblox.com/docs/avatar/dynamic-heads/validate>.
//!
//! The crate has no mesh reader, so the caller hands over the head already
//! posed: [`Head::poses`] holds the mesh deformed for each of [`ACTIONS`]
//! and for every control on its own (see [`pose_list`]).

use crate::{Finding, Geometry, Severity, Value};

mod landmarks;
mod rules;

pub use landmarks::{find as find_landmarks, Hit, Landmark, LandmarkPoints};

const SPEC: &str = "https://create.roblox.com/docs/avatar/dynamic-heads/specifications";
const VALIDATE: &str = "https://create.roblox.com/docs/avatar/dynamic-heads/validate";

/// The 17 FACS poses every dynamic head needs, in the page's order.
pub const REQUIRED_POSES: [&str; 17] = [
    "LeftEyeClosed",
    "RightEyeClosed",
    "EyesLookDown",
    "JawDrop",
    "Pucker",
    "LeftLipCornerPuller",
    "RightLipCornerPuller",
    "ChinRaiser",
    "ChinRaiserUpperLip",
    "LeftCheekRaiser",
    "RightCheekRaiser",
    "LeftInnerBrowRaiser",
    "RightInnerBrowRaiser",
    "LeftLipCornerDown",
    "RightLipCornerDown",
    "LeftLowerLipDepressor",
    "RightLowerLipDepressor",
];

/// One of the 5 action unit tests: the controls it sets and the landmark
/// that has to move.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Action {
    pub name: &'static str,
    pub controls: &'static [(&'static str, f32)],
    pub landmark: Landmark,
}

/// The page's action unit table, values as written there.
pub const ACTIONS: [Action; 5] = [
    Action {
        name: "Left eye blink",
        controls: &[("LeftEyeClosed", 1.0), ("EyesLookDown", 1.0)],
        landmark: Landmark::LeftEye,
    },
    Action {
        name: "Right eye blink",
        controls: &[("RightEyeClosed", 1.0), ("EyesLookDown", 1.0)],
        landmark: Landmark::RightEye,
    },
    Action {
        name: "Mouth opens",
        controls: &[("JawDrop", 1.0)],
        landmark: Landmark::Mouth,
    },
    Action {
        name: "Happy expression",
        controls: &[
            ("Pucker", 1.0),
            ("LeftLipCornerPuller", 1.0),
            ("RightLipCornerPuller", 1.0),
        ],
        landmark: Landmark::Mouth,
    },
    Action {
        name: "Sad expression",
        controls: &[
            ("ChinRaiser", 1.0),
            ("ChinRaiserUpperLip", 1.0),
            ("LeftCheekRaiser", 0.85),
            ("RightCheekRaiser", 0.85),
            ("LeftInnerBrowRaiser", 1.0),
            ("RightInnerBrowRaiser", 1.0),
            ("LeftLipCornerDown", 1.0),
            ("RightLipCornerDown", 1.0),
            ("LeftLowerLipDepressor", 1.0),
            ("RightLowerLipDepressor", 1.0),
        ],
        landmark: Landmark::Mouth,
    },
];

/// The poses [`Head::poses`] should hold, by name: each action's name with
/// its control values, then each of `controls` at 1 on its own, named after
/// the control.
pub fn pose_list(controls: &[String]) -> Vec<(String, Vec<(String, f32)>)> {
    let actions = ACTIONS.iter().map(|a| {
        let values = a.controls.iter().map(|(c, v)| (c.to_string(), *v));
        (a.name.to_string(), values.collect())
    });
    let singles = controls.iter().map(|c| (c.clone(), vec![(c.clone(), 1.0)]));
    actions.chain(singles).collect()
}

/// The FACS side of a head mesh.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facs {
    /// `FaceControls` property names the mesh has a pose for.
    pub controls: Vec<String>,
    /// Those whose pose moves at least one bone.
    pub active: Vec<String>,
    /// The mesh posed per [`pose_list`] name, positions parallel to
    /// [`Head::mesh`].
    pub poses: Vec<(String, Vec<[f32; 3]>)>,
}

/// A dynamic head as read from the place.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Head {
    /// The head part's name, for messages.
    pub name: String,
    /// Whether a `FaceControls` sits under the head part.
    pub face_controls: bool,
    /// The head mesh in studs, in the part's space; `None` when it did not
    /// load (the item's own unreadable note covers it).
    pub mesh: Option<Geometry>,
    /// `None` when the mesh has no FACS data at all.
    pub facs: Option<Facs>,
    /// The `WrapTarget` outer cage in the head part's space, UVs as stored.
    pub cage: Option<Geometry>,
    /// The part's `Material` name, `Transparency` and `VertexColor`.
    pub material: String,
    pub transparency: f32,
    pub vertex_color: [f32; 3],
    /// Scripts and plain parts in the head's model, by name.
    pub extraneous: Vec<String>,
}

/// The dynamic head checks, each its own row in a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HeadRule {
    FaceControls,
    FacsPoses,
    Landmarks,
    FacialAction,
    EmoteBounds,
    PartSettings,
    Extraneous,
}

impl HeadRule {
    pub fn title(self) -> &'static str {
        match self {
            HeadRule::FaceControls => "FaceControls",
            HeadRule::FacsPoses => "FACS poses",
            HeadRule::Landmarks => "Face landmarks",
            HeadRule::FacialAction => "Facial actions",
            HeadRule::EmoteBounds => "Head size while emoting",
            HeadRule::PartSettings => "Head part settings",
            HeadRule::Extraneous => "Extra objects",
        }
    }

    pub fn source(self) -> &'static str {
        match self {
            HeadRule::Landmarks | HeadRule::FacialAction => VALIDATE,
            _ => SPEC,
        }
    }
}

fn finding(rule: HeadRule, subject: &str, message: String) -> Finding {
    Finding {
        rule: crate::Rule::Head(rule),
        severity: Severity::Error,
        subject: subject.to_string(),
        measured: None,
        limit: None,
        message,
    }
}

fn measured(mut f: Finding, measured: Value, limit: Value) -> Finding {
    f.measured = Some(measured);
    f.limit = Some(limit);
    f
}

/// Every dynamic head finding for `head`.
pub fn check(head: &Head) -> Vec<Finding> {
    let mut out = Vec::new();
    rules::face_controls(head, &mut out);
    rules::poses(head, &mut out);
    rules::part_settings(head, &mut out);
    rules::emote_bounds(head, &mut out);
    landmarks::check(head, &mut out);
    out
}

#[cfg(test)]
mod tests;
