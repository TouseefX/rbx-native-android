//! `Headless::park` then `Parked::resume` from the same DOM must draw the
//! very frame it drew before, camera and selection included: that is what
//! an editor relies on to drop the scene of a place it is not showing.
//!
//! Needs a GPU, so it is `#[ignore]`d and run by hand:
//! `cargo test -p rbx_viewer --release --test park -- --ignored`.

use std::path::PathBuf;

use rbx_reflection::ReflectionDatabase;
use rbx_viewer::pick::Selected;
use rbx_viewer::Headless;

const SIZE: (u32, u32) = (640, 360);
/// Frames drawn before the compared one, so a fresh scene's texture uploads
/// are through (see `patch_parity`).
const DRAIN: usize = 40;

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

fn differing(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4).zip(b.chunks(4)).filter(|(a, b)| a != b).count()
}

#[test]
#[ignore = "needs a GPU"]
fn a_parked_viewer_resumes_to_the_same_frame() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/tests/TestPlace.rbxl");
    let dom = rbx_viewer::read_place(&path).expect("the fixture parses");
    let mut headless = Headless::load(&path, true).expect("the fixture loads");
    headless.open_at([20.0, 15.0, 20.0], [0.0, 0.0, 0.0], 70.0);
    let part = dom
        .root_refs()
        .iter()
        .flat_map(|&root| dom.get(root).unwrap().children().to_vec())
        .find(|&r| dom.get(r).is_some_and(|i| i.class() == "Part"))
        .expect("the fixture has a part under a service");
    headless.set_selection(&[Selected::read(&dom, &ReflectionDatabase::embedded(), part)]);
    let before = frame(&mut headless);

    let parked = headless.park();
    let mut headless = parked.resume(&dom).expect("the DOM rebuilds");
    let after = frame(&mut headless);
    assert_eq!(headless.pose().map(|pose| pose.fov_degrees), Some(70.0));
    assert_eq!(
        differing(&before, &after),
        0,
        "the resumed viewer drew a different frame"
    );
}
