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

use serde_json::{json, Value};

use rbx_dom::WeakDom;

use super::super::{resolve, string, text_argument, tool, Outcome};

pub(super) const MESH: &str = "generate_mesh";
pub(super) const MATERIAL: &str = "generate_material";
pub(super) const PROCEDURAL: &str = "generate_procedural_model";
pub(super) const SEGMENT: &str = "segment_mesh";
pub(super) const TEXTURE: &str = "generate_texture";
pub(super) const WAIT: &str = "wait_job_finished";

/// `generate_material`'s `baseMaterial` values, as Studio lists them.
const BASE_MATERIALS: [&str; 40] = [
    "Plastic",
    "SmoothPlastic",
    "Wood",
    "WoodPlanks",
    "Marble",
    "Basalt",
    "Slate",
    "CrackedLava",
    "Concrete",
    "Limestone",
    "Granite",
    "Pavement",
    "Brick",
    "Pebble",
    "Cobblestone",
    "Rock",
    "Sandstone",
    "CorrodedMetal",
    "DiamondPlate",
    "Foil",
    "Metal",
    "Grass",
    "LeafyGrass",
    "Sand",
    "Fabric",
    "Snow",
    "Mud",
    "Ground",
    "Asphalt",
    "Salt",
    "Ice",
    "Glacier",
    "Cardboard",
    "Carpet",
    "CeramicTiles",
    "ClayRoofTiles",
    "RoofShingles",
    "Leather",
    "Plaster",
    "Rubber",
];

/// Studio's own wording, which `list` keeps on the tools that carry it.
pub(super) const STUDIO_ID: &str =
    "Selects Roblox Studio instance, use the list_roblox_studios tool to get available instances";

const UNAVAILABLE: &str = " Not available in rbx-native: Roblox runs this generation only for \
    Studio, and no public API reaches it. The call answers why.";

fn asynchronous() -> Value {
    json!({
        "type": "boolean",
        "description": "Run in the background and return a jobId for wait_job_finished.",
    })
}

fn segmentation() -> Value {
    json!({
        "type": "string",
        "enum": ["auto", "none", "explicit"],
        "description": "How the result is broken into parts: derived automatically, one \
            piece, or the parts named in partNames.",
    })
}

fn part_names() -> Value {
    string("Part names, comma-separated or a JSON array; needed with segmentation='explicit'. At most 8.")
}

pub(super) fn mesh_schema() -> Value {
    tool(
        MESH,
        &format!("Generates a textured mesh from a prompt using AI.{UNAVAILABLE}"),
        json!({
            "textPrompt": string("The text prompt describing the mesh to generate."),
            "size": {
                "type": "object",
                "description": "The bounding box the generation tries to fit within.",
                "properties": {
                    "x": { "type": "number", "description": "X dimension scalar." },
                    "y": { "type": "number", "description": "Y dimension scalar." },
                    "z": { "type": "number", "description": "Z dimension scalar." },
                },
                "required": ["x", "y", "z"],
            },
            "maxTriangles": { "type": "number", "description": "Between 12 and 20000." },
            "segmentation": segmentation(),
            "partNames": part_names(),
            "async": asynchronous(),
        }),
        &["textPrompt"],
    )
}

pub(super) fn material_schema() -> Value {
    tool(
        MATERIAL,
        &format!(
            "Generates a MaterialVariant and returns its BaseMaterial and Name, to set as a \
             BasePart's Material and MaterialVariant.{UNAVAILABLE}"
        ),
        json!({
            "materialPattern": { "type": "string", "enum": ["Regular", "Organic"] },
            "materialId": { "type": "string" },
            "baseMaterial": { "type": "string", "enum": &BASE_MATERIALS[..] },
            "materialDescription": { "type": "string" },
            "async": asynchronous(),
        }),
        &[
            "materialPattern",
            "materialId",
            "baseMaterial",
            "materialDescription",
        ],
    )
}

pub(super) fn procedural_schema() -> Value {
    tool(
        PROCEDURAL,
        &format!(
            "Creates a 3D object built from primitive parts as a ProceduralModel with \
             configurable attributes, from a prompt and an optional reference image.{UNAVAILABLE}"
        ),
        json!({
            "prompt": string("What to create and which attributes to expose, in the user's own words."),
            "attachedImageUri": string("A reference image's IMAGEID_<id> URI, from store_image."),
            "segmentation": segmentation(),
            "partNames": part_names(),
            "async": asynchronous(),
        }),
        &["prompt"],
    )
}

const INSTANCE_PATH: &str = "Full Studio path to the source MeshPart or Model, e.g. \
    \"Workspace.Model1.Part2\". Resolved directly \u{2014} no live Studio selection required. \
    If omitted, falls back to the current Studio selection (which must be exactly one MeshPart \
    or Model).";

/// Studio lists these two with a title and annotations; `studio_id` is
/// already in their schemas, so `list` leaves it as Studio words it.
fn titled(mut schema: Value, title: &str) -> Value {
    schema["title"] = json!(title);
    schema["annotations"] = json!({
        "title": title,
        "readOnlyHint": false,
        "destructiveHint": true,
        "idempotentHint": false,
        "openWorldHint": true,
    });
    schema
}

pub(super) fn segment_schema() -> Value {
    let schema = tool(
        SEGMENT,
        "Segments an existing mesh into named sub-parts using AI. Requires a source MeshPart \
         or Model and up to 5 part names (e.g. [\"head\", \"body\", \"tail\"]). Returns a new \
         Model containing the segmented parts, inserted as a sibling of the source.",
        json!({
            "instance_path": string(INSTANCE_PATH),
            "parts": {
                "type": "array",
                "items": { "type": "string" },
                "description": "List of part names to segment the mesh into (max 5). Example: \
                    [\"head\", \"body\", \"tail\"].",
            },
            "studio_id": string(STUDIO_ID),
        }),
        &["parts", "studio_id"],
    );
    titled(schema, "Mesh Segmentation")
}

pub(super) fn texture_schema() -> Value {
    let schema = tool(
        TEXTURE,
        "Re-textures an existing mesh using AI, given a text prompt describing the desired \
         look. Requires a source MeshPart or Model. Replaces the source in place.",
        json!({
            "hintImage": {
                "type": "object",
                "description": "Optional reference image guiding the texture style. When \
                    provided, textPrompt may be an empty string.",
            },
            "instance_path": string(INSTANCE_PATH),
            "model": {
                "type": "string",
                "enum": ["Fast", "Quality"],
                "description": "Generation mode. \"Fast\" (default) trades quality for speed. \
                    \"Quality\" is a research preview and can vary in results; it first \
                    generates a reference image from an automatically selected view, so it \
                    takes longer.",
            },
            "studio_id": string(STUDIO_ID),
            "textPrompt": string("Text description of the desired texture/appearance, e.g. \"rusty metal\"."),
        }),
        &["textPrompt", "studio_id"],
    );
    titled(schema, "Texture Generation")
}

pub(super) fn wait_schema() -> Value {
    tool(
        WAIT,
        "Waits for a background job, started by a generation tool called with async, to reach \
         a terminal state (Completed, Failed or Cancelled) and returns its final status.",
        json!({
            "jobId": string("The jobId a tool called with async returned."),
            "timeout": { "type": "number", "description": "Most seconds to wait. Defaults to 600." },
        }),
        &["jobId"],
    )
}

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

/// The MeshPart or Model a mesh tool works on; Studio falls back to its
/// selection without a path, which an MCP call cannot see here.
fn source(dom: &WeakDom, arguments: &Value) -> Result<(), String> {
    let Some(path) = text_argument(arguments, "instance_path") else {
        return Err(
            "instance_path is required: the editor's selection is not visible to MCP \
            tools here, so pass the full path of a MeshPart or Model."
                .to_owned(),
        );
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
pub(super) fn mesh_edit(name: &str, dom: &WeakDom, arguments: &Value) -> Outcome {
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
    source(dom, arguments)?;
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
