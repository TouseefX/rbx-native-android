//! The FACS block of a v5–v7 mesh: per face bone, how far each facial
//! control moves and turns it.
//!
//! Layout as documented in the community "Roblox Mesh Format" reference
//! (<https://devforum.roblox.com/t/roblox-mesh-format/326114>): two
//! NUL-separated name buffers (face bones, then controls), six quantized
//! matrices (position X/Y/Z, then rotation X/Y/Z in degrees) with one row
//! per face bone and one column per control and corrective, then the two-
//! and three-control correctives as `u16` control indices. A corrective is
//! named by its controls joined with `_` and weighs the product of their
//! weights. Controls add up: each bone's offset is the weighted sum of its
//! columns, applied as `CFrame.new(position) * CFrame.Angles(rotation)` in
//! the bone's own frame, the way `Bone.Transform` is.

use crate::error::MeshError;
use crate::reader::Reader;

/// The 50 `FaceControls` properties and the short names a mesh's FACS
/// block stores them under (`JawDrop` is `c_JD`). Read off Roblox's own
/// dynamic heads; Studio's executable carries the same 50 short names. No
/// creator-docs page lists them.
pub const CONTROL_NAMES: [(&str, &str); 50] = [
    ("Corrugator", "c_COR"),
    ("LeftBrowLowerer", "l_BL"),
    ("RightBrowLowerer", "r_BL"),
    ("LeftInnerBrowRaiser", "l_IBR"),
    ("RightInnerBrowRaiser", "r_IBR"),
    ("LeftOuterBrowRaiser", "l_OBR"),
    ("RightOuterBrowRaiser", "r_OBR"),
    ("LeftNoseWrinkler", "l_NW"),
    ("RightNoseWrinkler", "r_NW"),
    ("EyesLookDown", "c_ELD"),
    ("EyesLookLeft", "c_ELL"),
    ("EyesLookRight", "c_ELR"),
    ("EyesLookUp", "c_ELU"),
    ("LeftEyeClosed", "l_EC"),
    ("RightEyeClosed", "r_EC"),
    ("LeftEyeUpperLidRaiser", "l_EULR"),
    ("RightEyeUpperLidRaiser", "r_EULR"),
    ("LeftCheekRaiser", "l_CHR"),
    ("RightCheekRaiser", "r_CHR"),
    ("JawDrop", "c_JD"),
    ("JawLeft", "c_JL"),
    ("JawRight", "c_JR"),
    ("ChinRaiser", "c_CR"),
    ("ChinRaiserUpperLip", "c_CRUL"),
    ("FlatPucker", "c_FP"),
    ("Funneler", "c_FN"),
    ("LeftCheekPuff", "l_CHP"),
    ("RightCheekPuff", "r_CHP"),
    ("LeftDimpler", "l_DM"),
    ("RightDimpler", "r_DM"),
    ("LeftLipCornerDown", "l_LCD"),
    ("RightLipCornerDown", "r_LCD"),
    ("LeftLipCornerPuller", "l_LCP"),
    ("RightLipCornerPuller", "r_LCP"),
    ("LeftLipStretcher", "l_LS"),
    ("RightLipStretcher", "r_LS"),
    ("LeftLowerLipDepressor", "l_LLD"),
    ("RightLowerLipDepressor", "r_LLD"),
    ("LeftUpperLipRaiser", "l_ULR"),
    ("RightUpperLipRaiser", "r_ULR"),
    ("LipPresser", "c_LP"),
    ("LipsTogether", "c_LPT"),
    ("LowerLipSuck", "c_LLS"),
    ("UpperLipSuck", "c_ULS"),
    ("MouthLeft", "c_ML"),
    ("MouthRight", "c_MR"),
    ("Pucker", "c_PK"),
    ("TongueDown", "c_TD"),
    ("TongueOut", "c_TO"),
    ("TongueUp", "c_TU"),
];

/// The `FaceControls` property a stored control name stands for; a name
/// outside [`CONTROL_NAMES`] is taken as already being one.
pub fn control_property(stored: &str) -> &str {
    CONTROL_NAMES
        .iter()
        .find(|(_, short)| *short == stored)
        .map_or(stored, |(full, _)| full)
}

/// The facial animation data of a skinned head.
#[derive(Debug, Clone, PartialEq)]
pub struct Facs {
    /// Face bone names, one per matrix row, matched to [`Skin::bones`] by name.
    pub bones: Vec<String>,
    /// The base controls as stored (`c_JD`, `l_EC`, …; see
    /// [`control_property`]), the first columns.
    pub controls: Vec<String>,
    /// The correctives, the columns after the controls, as indices into
    /// [`Facs::controls`].
    pub correctives: Vec<Vec<u16>>,
    /// Position deltas per axis, `rows × cols` row-major (rows are bones,
    /// columns the controls then the correctives).
    pub position: [Vec<f32>; 3],
    /// Rotation deltas per axis in degrees, same layout.
    pub rotation: [Vec<f32>; 3],
}

/// Below these a matrix entry is quantization noise, not motion: a
/// version 2 matrix cannot store an exact zero, and Roblox's own heads read
/// back about 2e-6 studs and 6e-4 degrees where a control leaves a bone
/// alone.
const MOVE_FLOOR: f32 = 1e-4;
const TURN_FLOOR: f32 = 0.01;

/// One face bone's offset for a set of control weights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FacePose {
    /// Row in [`Facs::bones`].
    pub bone: usize,
    pub position: [f32; 3],
    /// Degrees, `CFrame.Angles` order.
    pub rotation: [f32; 3],
}

impl Facs {
    fn columns(&self) -> usize {
        self.controls.len() + self.correctives.len()
    }

    fn column(&self, property: &str) -> Option<usize> {
        self.controls
            .iter()
            .position(|c| control_property(c) == property)
    }

    /// Whether the `FaceControls` property `property` is one of the mesh's
    /// base controls.
    pub fn has(&self, property: &str) -> bool {
        self.column(property).is_some()
    }

    /// Whether the control behind `property` moves or turns at least one
    /// bone.
    pub fn is_active(&self, property: &str) -> bool {
        let Some(col) = self.column(property) else {
            return false;
        };
        let cols = self.columns();
        let moves = |m: &[Vec<f32>; 3], floor: f32| {
            m.iter()
                .any(|m| (0..self.bones.len()).any(|row| m[row * cols + col].abs() > floor))
        };
        moves(&self.position, MOVE_FLOOR) || moves(&self.rotation, TURN_FLOOR)
    }

    /// Every face bone's offset with each control at `weight(property)`
    /// (0 to 1, keyed by `FaceControls` property name).
    pub fn pose(&self, weight: &dyn Fn(&str) -> f32) -> Vec<FacePose> {
        let base: Vec<f32> = self
            .controls
            .iter()
            .map(|c| weight(control_property(c)))
            .collect();
        let mut weights = base.clone();
        weights.extend(self.correctives.iter().map(|parts| {
            parts
                .iter()
                .map(|&i| base.get(i as usize).copied().unwrap_or(0.0))
                .product::<f32>()
        }));
        let cols = self.columns();
        (0..self.bones.len())
            .map(|row| {
                let sum = |m: &Vec<f32>| -> f32 {
                    weights
                        .iter()
                        .enumerate()
                        .filter(|(_, w)| **w != 0.0)
                        .map(|(col, w)| m[row * cols + col] * w)
                        .sum()
                };
                FacePose {
                    bone: row,
                    position: [0, 1, 2].map(|a| sum(&self.position[a])),
                    rotation: [0, 1, 2].map(|a| sum(&self.rotation[a])),
                }
            })
            .collect()
    }
}

fn f32_at(reader: &mut Reader<'_>) -> Result<f32, MeshError> {
    Ok(f32::from_bits(reader.u32()?))
}

fn names(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|&b| b == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
}

fn read_matrix(reader: &mut Reader<'_>, rows: usize, cols: usize) -> Result<Vec<f32>, MeshError> {
    let version = reader.u16()?;
    let (r, c) = (reader.u32()? as usize, reader.u32()? as usize);
    if (r, c) != (rows, cols) {
        return Err(MeshError::Facs(format!(
            "a {r}\u{d7}{c} matrix for {rows} bones and {cols} controls"
        )));
    }
    let count = rows.checked_mul(cols).ok_or(MeshError::TooLarge)?;
    match version {
        1 => {
            let raw = reader.take_records(count, 4)?;
            Ok(raw
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect())
        }
        2 => {
            let (min, max) = (f32_at(reader)?, f32_at(reader)?);
            let step = (max - min) / 65535.0;
            let raw = reader.take_records(count, 2)?;
            Ok(raw
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| min + f32::from(u16::from_le_bytes(*b)) * step)
                .collect())
        }
        other => Err(MeshError::Facs(format!("matrix version {other}"))),
    }
}

/// Reads one FACS block, the whole of `bytes`.
pub(crate) fn read(bytes: &[u8]) -> Result<Facs, MeshError> {
    let mut reader = Reader::new(bytes);
    let bone_names = reader.u32()? as usize;
    let control_names = reader.u32()? as usize;
    let transforms = u64::from(reader.u32()?) | (u64::from(reader.u32()?) << 32);
    let two = reader.u32()? as usize;
    let three = reader.u32()? as usize;
    let bones = names(reader.take(bone_names)?);
    let controls = names(reader.take(control_names)?);
    let transforms = usize::try_from(transforms).map_err(|_| MeshError::TooLarge)?;
    let mut matrices = Reader::new(reader.take(transforms)?);

    let indices = |raw: &[u8], arity: usize| -> Vec<Vec<u16>> {
        raw.chunks_exact(arity * 2)
            .map(|c| {
                c.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| u16::from_le_bytes(*b))
                    .collect()
            })
            .collect()
    };
    let mut correctives = indices(reader.take(two)?, 2);
    correctives.extend(indices(reader.take(three)?, 3));
    reader.expect_eof()?;

    let (rows, cols) = (bones.len(), controls.len() + correctives.len());
    let mut read = || read_matrix(&mut matrices, rows, cols);
    let position = [read()?, read()?, read()?];
    let rotation = [read()?, read()?, read()?];
    matrices.expect_eof()?;
    Ok(Facs {
        bones,
        controls,
        correctives,
        position,
        rotation,
    })
}

mod pose;

pub use pose::deform_positions;

#[cfg(test)]
#[path = "facs/tests.rs"]
pub(crate) mod tests;
