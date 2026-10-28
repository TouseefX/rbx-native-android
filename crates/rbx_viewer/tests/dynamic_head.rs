//! A dynamic head moves with its `FaceControls`: a ribbon skinned half to a
//! `skull` bone and half to a `jaw` face bone drops its lower half when
//! `JawDrop` is written, as a patch through `Headless::apply_changes`, not a
//! reload, and rests again when the control returns to 0 or is removed.
//!
//! The mesh is written here as `version 5.00` bytes (the v4 layout behind a
//! 32-byte header, with the FACS block at the end) and served from the local
//! asset cache as `rbxasset://rbx-native-local/...` native content.
//!
//! Needs a GPU. Without one the test says so and passes, rather than being
//! `#[ignore]`d: a software rasteriser (Mesa's lavapipe,
//! `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`) is enough.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use rbx_dom::{CFrameData, Instance, Ref, Variant, Vector3Data, WeakDom};
use rbx_viewer::{Applied, Headless};

const SIZE: (u32, u32) = (160, 120);
const DRAIN: usize = 8;
const IDENTITY: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
/// `Enum.Material.SmoothPlastic`: no maps to fetch, so nothing waits on the
/// network, and the one material layer the opened place already loaded.
const SMOOTH_PLASTIC: u32 = 272;
const NATIVE_PATH: &str = "rbx-native-local/tests/rbx_viewer-dynamic-head.mesh";

const WORKSPACE: u32 = 1;
const HEAD: u32 = 2;
const FACE_CONTROLS: u32 = 3;

/// Rows of the ribbon, top to bottom; the joint is the middle one.
const ROWS: [f32; 5] = [3.0, 1.5, 0.0, -1.5, -3.0];
/// How far the jaw bone moves down at `JawDrop` 1, in studs.
const DROP: f32 = 1.5;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/tests/TestPlace.rbxl")
}

fn bone_record(out: &mut Vec<u8>, name_offset: u32, parent: Option<u16>) {
    out.extend(name_offset.to_le_bytes());
    let parent = parent.unwrap_or(0xFFFF);
    out.extend(parent.to_le_bytes());
    out.extend(parent.to_le_bytes());
    out.extend(0f32.to_le_bytes()); // culling
    for value in IDENTITY.iter().chain(&[0.0; 3]) {
        out.extend(value.to_le_bytes());
    }
}

/// A version 1 FACS matrix of one bone by one control.
fn matrix(value: f32) -> Vec<u8> {
    let mut out = 1u16.to_le_bytes().to_vec();
    out.extend(1u32.to_le_bytes());
    out.extend(1u32.to_le_bytes());
    out.extend(value.to_le_bytes());
    out
}

/// The FACS block: face bone `jaw`, control `c_JD` (`JawDrop`), which moves
/// the bone `DROP` studs down Y and does nothing else.
fn facs() -> Vec<u8> {
    let bones = b"jaw\0";
    let controls = b"c_JD\0";
    let mut transforms = Vec::new();
    for value in [0.0, -DROP, 0.0, 0.0, 0.0, 0.0] {
        transforms.extend(matrix(value));
    }
    let mut out = Vec::new();
    out.extend((bones.len() as u32).to_le_bytes());
    out.extend((controls.len() as u32).to_le_bytes());
    out.extend((transforms.len() as u64).to_le_bytes());
    out.extend(0u32.to_le_bytes()); // two-control correctives
    out.extend(0u32.to_le_bytes()); // three-control correctives
    out.extend(bones);
    out.extend(controls);
    out.extend(transforms);
    out
}

/// A ribbon one stud wide and six long down native Y, facing −Z: the top
/// half weighted to `skull`, the bottom half to `jaw`, the joint row split.
fn ribbon() -> Vec<u8> {
    let mut vertices = Vec::new();
    let mut envelopes = Vec::new();
    let mut faces = Vec::new();
    for (row, &y) in ROWS.iter().enumerate() {
        for x in [-0.5f32, 0.5] {
            vertices.push(([x, y, 0.0], [0.0f32, 0.0, -1.0]));
            let weights: [u8; 4] = match y {
                y if y > 0.0 => [255, 0, 0, 0],
                0.0 => [128, 127, 0, 0],
                _ => [0, 255, 0, 0],
            };
            envelopes.push(([0u8, 1, 0, 0], weights));
        }
        if row + 1 < ROWS.len() {
            let top = row as u32 * 2;
            faces.push([top, top + 1, top + 2]);
            faces.push([top + 1, top + 3, top + 2]);
        }
    }
    let names = b"skull\0jaw\0";
    let facs = facs();

    let mut out = b"version 5.00\n".to_vec();
    out.extend(32u16.to_le_bytes());
    out.extend(0u16.to_le_bytes()); // lod_type
    out.extend((vertices.len() as u32).to_le_bytes());
    out.extend((faces.len() as u32).to_le_bytes());
    out.extend(0u16.to_le_bytes()); // num_lods
    out.extend(2u16.to_le_bytes()); // num_bones
    out.extend((names.len() as u32).to_le_bytes());
    out.extend(1u16.to_le_bytes()); // num_subsets
    out.push(0); // num_high_quality_lods
    out.push(0); // unused
    out.extend(1u32.to_le_bytes()); // facs_data_format
    out.extend((facs.len() as u32).to_le_bytes());

    for (position, normal) in &vertices {
        for value in position.iter().chain(normal).chain(&[0.0, 0.0]) {
            out.extend(value.to_le_bytes());
        }
        out.extend([0, 0, 0, 127]); // tangent
        out.extend([255; 4]); // color
    }
    for (slots, weights) in &envelopes {
        out.extend(slots);
        out.extend(weights);
    }
    for face in &faces {
        for index in face {
            out.extend(index.to_le_bytes());
        }
    }
    bone_record(&mut out, 0, None);
    bone_record(&mut out, 6, Some(0));
    out.extend(names);
    for word in [0, faces.len() as u32, 0, vertices.len() as u32, 2] {
        out.extend(u32::to_le_bytes(word));
    }
    for slot in 0..26u16 {
        let bone = if slot < 2 { slot } else { 0xFFFF };
        out.extend(bone.to_le_bytes());
    }
    out.extend(facs);
    out
}

fn vector3([x, y, z]: [f32; 3]) -> Variant {
    Variant::Vector3(Vector3Data { x, y, z })
}

/// A place of a lone `Head` under `Workspace` with a `FaceControls` whose
/// `JawDrop` is `jaw_drop`.
fn place(jaw_drop: f32) -> WeakDom {
    let mut dom = WeakDom::new();
    dom.insert(Instance::new(Ref::new(WORKSPACE), "Workspace", "Workspace"));
    dom.set_parent(Ref::new(WORKSPACE), None);

    let mut head = Instance::new(Ref::new(HEAD), "MeshPart", "Head");
    let properties = head.properties_mut();
    properties.insert(
        "MeshId".to_string(),
        Variant::String(format!("rbxasset://{NATIVE_PATH}")),
    );
    properties.insert("size".to_string(), vector3([1.0, 6.0, 0.2]));
    properties.insert("InitialSize".to_string(), vector3([1.0, 6.0, 0.2]));
    properties.insert(
        "CFrame".to_string(),
        Variant::CFrame(CFrameData {
            position: Vector3Data {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            rotation: IDENTITY,
        }),
    );
    properties.insert("Material".to_string(), Variant::Enum(SMOOTH_PLASTIC));
    properties.insert(
        "Color3uint8".to_string(),
        Variant::Color3uint8 { r: 0, g: 255, b: 0 },
    );
    dom.insert(head);

    let mut face = Instance::new(Ref::new(FACE_CONTROLS), "FaceControls", "FaceControls");
    face.properties_mut()
        .insert("JawDrop".to_string(), Variant::Float32(jaw_drop));
    dom.insert(face);

    dom.set_parent(Ref::new(HEAD), Some(Ref::new(WORKSPACE)));
    dom.set_parent(Ref::new(FACE_CONTROLS), Some(Ref::new(HEAD)));
    dom.take_changes();
    dom
}

fn headless() -> Option<Headless> {
    // Once per run: the tests run side by side, and two writes of the same
    // cache entry at once trip over each other's temporary file.
    static CACHED: std::sync::Once = std::sync::Once::new();
    CACHED.call_once(|| {
        let cache = rbx_assets::AssetCache::new(None).expect("the asset cache opens");
        cache
            .put_native(NATIVE_PATH, &ribbon())
            .expect("the mesh is cached");
    });
    match Headless::load(&fixture(), false) {
        Ok(headless) => Some(headless),
        Err(why) => {
            println!("skipped: no GPU to render with ({why})");
            None
        }
    }
}

fn settle(headless: &mut Headless) {
    let until = Instant::now() + Duration::from_secs(20);
    while Instant::now() < until {
        let swapped = headless.swap_assets();
        if headless.assets_in_flight() == 0 && !swapped {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the mesh never landed");
}

fn frame(headless: &mut Headless) -> Vec<u8> {
    for _ in 0..DRAIN {
        headless.render_frame(SIZE.0, SIZE.1).expect("a frame");
    }
    headless.take_frame().expect("the last frame");
    headless.render_frame(SIZE.0, SIZE.1).expect("a frame");
    headless
        .take_frame()
        .expect("the compared frame")
        .expect("one frame in flight")
        .pixels
}

/// The green mesh, however it is lit.
fn green(pixel: &[u8]) -> bool {
    let [r, g, b] = [pixel[0], pixel[1], pixel[2]].map(u16::from);
    g > 30 && g > 2 * r + 8 && g > 2 * b + 8
}

/// Green pixels whose row, as a fraction of the frame, passes `inside`.
fn count(pixels: &[u8], inside: impl Fn(f32) -> bool) -> usize {
    let (width, height) = (SIZE.0 as usize, SIZE.1 as usize);
    (0..height)
        .flat_map(|row| (0..width).map(move |column| (row, column)))
        .filter(|&(row, column)| {
            let at = (row * width + column) * 4;
            green(&pixels[at..at + 4]) && inside((row as f32 + 0.5) / height as f32)
        })
        .count()
}

/// Rows below where the resting ribbon ends (world y −3 seen from nine
/// studs off lands at row 0.74 or 0.82, by the field of view's axis) and
/// above where the dropped one ends.
fn below_the_rest_end(row: f32) -> bool {
    row > 0.83
}

fn assert_closed(pixels: &[u8]) {
    assert!(count(pixels, |_| true) > 200, "the ribbon is drawn");
    assert_eq!(
        count(pixels, below_the_rest_end),
        0,
        "at rest nothing hangs below the ribbon's end"
    );
}

fn assert_open(pixels: &[u8]) {
    assert!(
        count(pixels, below_the_rest_end) > 20,
        "the dropped jaw reaches below where the ribbon ended"
    );
    assert!(
        count(pixels, |row| row < 0.4) > 20,
        "the skull stays where it was"
    );
}

fn write_jaw_drop(headless: &mut Headless, dom: &mut WeakDom, value: f32) {
    dom.set_property(Ref::new(FACE_CONTROLS), "JawDrop", Variant::Float32(value))
        .expect("a FaceControls takes JawDrop");
    let changes = dom.take_changes();
    assert_eq!(
        headless.apply_changes(dom, &changes).expect("applies"),
        Applied::Patched,
        "a control write is a patch, not a reload"
    );
    settle(headless);
}

#[test]
fn writing_jaw_drop_opens_the_jaw_as_a_patch() {
    let Some(mut headless) = headless() else {
        return;
    };
    let mut dom = place(0.0);
    headless.reload(&dom).expect("the DOM rebuilds");
    settle(&mut headless);
    headless.open_at([0.0, 0.0, -9.0], [0.0, 0.0, 0.0], 70.0);

    let rest = frame(&mut headless);
    assert_closed(&rest);
    write_jaw_drop(&mut headless, &mut dom, 1.0);
    assert_open(&frame(&mut headless));
    // And back: the rest frame exactly.
    write_jaw_drop(&mut headless, &mut dom, 0.0);
    assert_eq!(frame(&mut headless), rest, "the rest face draws as before");
}

#[test]
fn removing_face_controls_rests_the_face() {
    let Some(mut headless) = headless() else {
        return;
    };
    let mut dom = place(1.0);
    headless.reload(&dom).expect("the DOM rebuilds");
    settle(&mut headless);
    headless.open_at([0.0, 0.0, -9.0], [0.0, 0.0, 0.0], 70.0);
    assert_open(&frame(&mut headless));

    dom.remove(Ref::new(FACE_CONTROLS));
    let changes = dom.take_changes();
    assert_eq!(
        headless.apply_changes(&dom, &changes).expect("applies"),
        Applied::Patched,
        "removing the controls is a patch"
    );
    settle(&mut headless);
    let removed = frame(&mut headless);
    assert_closed(&removed);

    headless.reload(&place(0.0)).expect("the DOM rebuilds");
    settle(&mut headless);
    headless.open_at([0.0, 0.0, -9.0], [0.0, 0.0, 0.0], 70.0);
    assert_eq!(
        frame(&mut headless),
        removed,
        "a face without controls draws as one reloaded at rest"
    );
}
