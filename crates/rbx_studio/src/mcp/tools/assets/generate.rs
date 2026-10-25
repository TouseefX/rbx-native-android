//! `generate_mesh`, `generate_material`, `generate_procedural_model`,
//! `segment_mesh`, `generate_texture` and `wait_job_finished`, listed with Studio's arguments. Studio runs the
//! generation on Roblox's own AI services for its Assistant, and nothing
//! public reaches them:
//!
//! - Open Cloud (creator-docs `reference/cloud/openapi.json`) has no
//!   generation endpoint; its only `generate` routes make speech assets,
//!   thumbnails and translations.
//! - `GenerationService` (`reference/engine/classes/GenerationService.yaml`)
//!   runs Cube 3D only inside a running experience: `GenerateMeshAsync`
//!   "must be called from server scripts", and a mesh it makes is not an
//!   asset a place can keep.
//! - The Material Generator (`studio/material-generator.md`) is a Studio
//!   window, with no API or engine class behind it.
//!
//! So each call answers why it cannot run here rather than pretending to,
//! and no generation job ever exists for `wait_job_finished` to wait on.

use serde_json::Value;

use rbx_dom::{Ref, WeakDom};

use super::super::{resolve, text_argument, Outcome};

pub(super) const MESH: &str = "generate_mesh";
pub(super) const MATERIAL: &str = "generate_material";
pub(super) const PROCEDURAL: &str = "generate_procedural_model";
pub(super) const SEGMENT: &str = "segment_mesh";
pub(super) const TEXTURE: &str = "generate_texture";
pub(super) const WAIT: &str = "wait_job_finished";

pub(super) fn refuse(name: &str) -> Outcome {
    let why = match name {
        MESH => {
            "Roblox generates meshes for Studio's Assistant on its own servers, and no public \
             API reaches that service: Open Cloud has no generation endpoint, and \
             GenerationService:GenerateMeshAsync only runs from server scripts in a live \
             experience, where the mesh it makes is not an asset a place can keep."
        }
        MATERIAL => {
            "Roblox generates materials only in Studio's Material Generator and Assistant, on \
             its own servers, and no public API or engine class reaches that service."
        }
        SEGMENT => {
            "Roblox segments meshes for Studio's Assistant on its own servers, and no public \
             API or engine class reaches that service: Open Cloud has no mesh-segmentation \
             endpoint."
        }
        TEXTURE => {
            "Roblox generates textures for Studio's Assistant on its own servers, and no \
             public API reaches that service: Open Cloud has no texture-generation endpoint, \
             and GenerationService only runs from server scripts in a live experience."
        }
        _ => {
            "Roblox generates procedural models for Studio's Assistant on its own servers, and \
             no public API reaches that service: Open Cloud has no generation endpoint, and \
             GenerationService:GenerateModelAsync only runs from server scripts in a live \
             experience."
        }
    };
    Err(format!(
        "{name} is not available in rbx-native. {why} Build it from parts instead, or insert \
         an existing asset with search_asset and insert_asset."
    ))
}

/// The MeshPart or Model a mesh tool works on: `instance_path`, or without
/// it the editor's selection, which must then be exactly one MeshPart or
/// Model, as Studio's schema says.
fn source(dom: &WeakDom, selection: &[Ref], arguments: &Value) -> Result<(), String> {
    let Some(path) = text_argument(arguments, "instance_path") else {
        let mesh_or_model = |one: &Ref| {
            dom.get(*one)
                .is_some_and(|found| matches!(found.class(), "MeshPart" | "Model"))
        };
        return match selection {
            [one] if mesh_or_model(one) => Ok(()),
            _ => Err(format!(
                "No instance_path was given and the current Studio selection is not exactly \
                 one MeshPart or Model ({} selected).",
                selection.len()
            )),
        };
    };
    let found = resolve(dom, path)
        .and_then(|found| dom.get(found))
        .ok_or_else(|| format!("No instance found at path: {path}"))?;
    match found.class() {
        "MeshPart" | "Model" => Ok(()),
        class => Err(format!(
            "The instance at {path} is a {class}, not a MeshPart or Model."
        )),
    }
}

/// Checks the arguments as Studio does, then refuses as the generation
/// tools do: a bad call is told what is wrong before it is told why.
pub(super) fn mesh_edit(
    name: &str,
    dom: &WeakDom,
    selection: &[Ref],
    arguments: &Value,
) -> Outcome {
    if name == SEGMENT {
        let parts = arguments
            .get("parts")
            .and_then(Value::as_array)
            .filter(|parts| !parts.is_empty() && parts.iter().all(Value::is_string))
            .ok_or("missing the `parts` argument: a list of part names")?;
        if parts.len() > 5 {
            return Err(format!(
                "`parts` takes at most 5 part names, got {}.",
                parts.len()
            ));
        }
    } else if text_argument(arguments, "textPrompt").is_none()
        && !arguments.get("hintImage").is_some_and(Value::is_object)
    {
        return Err("missing the `textPrompt` argument".to_owned());
    }
    source(dom, selection, arguments)?;
    refuse(name)
}

/// No generation job can start here, so no id names one.
pub(super) fn wait(arguments: &Value) -> Outcome {
    let job = text_argument(arguments, "jobId").unwrap_or_default();
    Err(format!(
        "No job found with id: {job}. rbx-native never starts one: the generation tools have \
         no public backend outside Roblox Studio."
    ))
}
