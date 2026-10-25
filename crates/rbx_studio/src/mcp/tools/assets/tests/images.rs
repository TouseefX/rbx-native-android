use serde_json::{json, Value};

use super::super::super::tests::{place, run};
use super::{json_body, mock, INTROSPECT, MOCK_ROOT, PNG};

#[test]
fn upload_image_maps_each_path_to_its_asset_or_why_not() {
    let seen = mock(|target, body| match target {
        t if t.contains("/api-keys/v1/introspect") => (200, INTROSPECT.as_bytes().to_vec()),
        t if t.contains("/img/") && t.contains("broken") => (404, Vec::new()),
        t if t.contains("/img/") => (200, PNG.to_vec()),
        t if t.starts_with("POST /assets/v1/assets") => {
            assert!(String::from_utf8_lossy(body).contains("\"sky\""));
            json_body(json!({
                "path": "operations/op1",
                "done": true,
                "response": { "assetId": "71", "moderationResult": { "moderationState": "Approved" } },
            }))
        }
        _ => (404, Vec::new()),
    });
    let root = MOCK_ROOT.with(|root| root.borrow().clone().unwrap());
    let sky = format!("{root}/img/sky.png?v=2");
    let broken = format!("{root}/img/broken.png");
    let local = "C:\\sky.png";
    let (error, text) = run(
        &place(),
        "upload_image",
        json!({ "imagePaths": [sky, broken, local] }),
    );
    assert!(!error, "{text}");
    let answer: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(answer[&sky], "rbxassetid://71");
    assert!(answer[&broken]
        .as_str()
        .unwrap()
        .starts_with("Error Upload: Unable to read file at path:"));
    assert_eq!(
        answer[local],
        "Error Upload: Only http/https URLs are supported"
    );
    let uploads = seen
        .lock()
        .unwrap()
        .iter()
        .filter(|(target, _)| target.starts_with("POST /assets/v1/assets"))
        .count();
    assert_eq!(uploads, 1);
}

#[test]
fn an_empty_upload_batch_is_an_empty_list() {
    assert_eq!(
        run(&place(), "upload_image", json!({ "imagePaths": [] })),
        (false, "[]".to_owned())
    );
}

#[test]
fn store_image_keeps_a_local_image_under_a_handle() {
    let dir = std::env::temp_dir().join(format!("rbx-mcp-store-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("logo.png");
    std::fs::write(&file, PNG).unwrap();
    let empty = dir.join("empty.jpg");
    std::fs::write(&empty, b"").unwrap();
    let place_file = dir.join("place.rbxl");
    std::fs::write(&place_file, b"<roblox/>").unwrap();
    let dom = place();
    let (error, first) = run(&dom, "store_image", json!({ "filePath": file }));
    let (_, second) = run(&dom, "store_image", json!({ "filePath": file }));
    let (_, wrong) = run(&dom, "store_image", json!({ "filePath": place_file }));
    // Studio reads before it checks the name, so a missing file of the wrong
    // kind is unreadable rather than refused.
    let (_, missing) = run(
        &dom,
        "store_image",
        json!({ "filePath": dir.join("gone.gif") }),
    );
    let (_, blank) = run(&dom, "store_image", json!({ "filePath": empty }));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!error, "{first}");
    assert!(
        first.starts_with("IMAGEID_") && first.len() == 8 + 36,
        "{first}"
    );
    assert_ne!(first, second);
    assert_eq!(
        wrong,
        "Error loading image: Unsupported image file extension \".rbxl\". Supported: jpg, jpeg, png."
    );
    assert!(blank.starts_with("Error loading image: File is empty or could not be read:"));
    assert!(missing.starts_with("Error loading image: File is empty or could not be read:"));
}
