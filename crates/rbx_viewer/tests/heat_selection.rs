//! A selected part keeps its Performance view tint: the heat fills it and
//! the selection's outline is drawn on top, rather than one replacing the
//! other.
//!
//! Needs a GPU. Without one the test says so and passes, the same way
//! `isolation.rs` does.

use rbx_dom::{CFrameData, Instance, Ref, Variant, Vector3Data, WeakDom};
use rbx_reflection::ReflectionDatabase;
use rbx_viewer::perf::HeatBin;
use rbx_viewer::pick::Selected;
use rbx_viewer::Headless;

const SIZE: (u32, u32) = (160, 120);
const DRAIN: usize = 8;
const WORKSPACE: u32 = 1;
const BLOCK: u32 = 2;

/// `Workspace { Block }`: one white block in the middle of the view.
fn place() -> WeakDom {
    let mut dom = WeakDom::new();
    dom.insert(Instance::new(Ref::new(WORKSPACE), "Workspace", "Workspace"));
    let mut block = Instance::new(Ref::new(BLOCK), "Part", "Block");
    let properties = block.properties_mut();
    properties.insert(
        "size".to_string(),
        Variant::Vector3(Vector3Data {
            x: 4.0,
            y: 4.0,
            z: 4.0,
        }),
    );
    properties.insert(
        "CFrame".to_string(),
        Variant::CFrame(CFrameData {
            position: Vector3Data {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        }),
    );
    properties.insert(
        "Color3uint8".to_string(),
        Variant::Color3uint8 {
            r: 255,
            g: 255,
            b: 255,
        },
    );
    dom.insert(block);
    dom.set_parent(Ref::new(WORKSPACE), None);
    dom.set_parent(Ref::new(BLOCK), Some(Ref::new(WORKSPACE)));
    dom.take_changes();
    dom
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

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 3] {
    let at = ((y * SIZE.0 + x) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2]]
}

/// The heat ramp's hottest step, viridis yellow, at 70% over a white part:
/// well short of white in blue only.
fn yellow([r, g, b]: [u8; 3]) -> bool {
    r > 200 && g > 200 && b + 60 < r
}

/// The selection's own blue.
fn blue([r, _, b]: [u8; 3]) -> bool {
    b > 150 && u16::from(r) * 2 < u16::from(b)
}

#[test]
fn a_selected_part_keeps_its_heat_tint_under_the_selection_outline() {
    let mut headless = match Headless::from_dom(&place(), false) {
        Ok(headless) => headless,
        Err(why) => {
            println!("skipped: no GPU to render with ({why})");
            return;
        }
    };
    headless.open_at([0.0, 0.0, -12.0], [0.0, 0.0, 0.0], 70.0);
    let to_linear = |c: u8| {
        let c = f32::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    headless.set_heat(vec![HeatBin {
        color: [0xfd, 0xe7, 0x25].map(to_linear),
        alpha: 0.7,
        parts: vec![Ref::new(BLOCK)],
    }]);
    let heat = frame(&mut headless);
    let (cx, cy) = (SIZE.0 / 2, SIZE.1 / 2);
    assert!(yellow(pixel(&heat, cx, cy)), "{:?}", pixel(&heat, cx, cy));

    let dom = place();
    let selected = Selected::read(&dom, ReflectionDatabase::shared(), Ref::new(BLOCK));
    headless.set_selection(&[selected]);
    let both = frame(&mut headless);

    // The middle of the part is still the tint, untouched by the selection.
    assert_eq!(pixel(&both, cx, cy), pixel(&heat, cx, cy));
    // And the selection outline runs around it: walking out from the middle
    // along the centre row meets the blue edge before the backdrop.
    let edge = (cx..SIZE.0).find(|&x| blue(pixel(&both, x, cy)));
    assert!(edge.is_some(), "no selection outline on the centre row");
    assert!(
        (cx..SIZE.0).all(|x| !blue(pixel(&heat, x, cy))),
        "the outline is the selection's, not the heat's"
    );
}
