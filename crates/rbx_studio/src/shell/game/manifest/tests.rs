use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rbx_studio_manifest_test_{}_{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn entry(path: &str) -> PlaceEntry {
    PlaceEntry {
        path: PathBuf::from(path),
        place_id: None,
        name: None,
    }
}

fn manifest(places: Vec<PlaceEntry>) -> Manifest {
    Manifest {
        name: "G".into(),
        universe_id: None,
        places,
    }
}

#[test]
fn is_manifest_matches_extension_case_insensitively() {
    assert!(is_manifest(Path::new("a/b.rbxgame")));
    assert!(is_manifest(Path::new("b.RBXGAME")));
    assert!(!is_manifest(Path::new("b.rbxl")));
    assert!(!is_manifest(Path::new("rbxgame")));
}

#[test]
fn write_then_read_round_trips() {
    let dir = scratch("round_trip");
    let path = dir.join("g.rbxgame");
    let mut original = manifest(vec![entry("a.rbxl")]);
    original.universe_id = Some(7);
    original.places[0].place_id = Some(9);
    original.places[0].name = Some("Lobby".into());
    write(&path, &original).unwrap();
    assert_eq!(read(&path).unwrap(), original);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn read_rejects_bad_json_and_empty_places() {
    let dir = scratch("errors");
    let bad = dir.join("bad.rbxgame");
    std::fs::write(&bad, "not json").unwrap();
    assert!(read(&bad).unwrap_err().contains("bad.rbxgame"));
    let empty = dir.join("empty.rbxgame");
    std::fs::write(&empty, r#"{"name":"x","places":[]}"#).unwrap();
    assert!(read(&empty).unwrap_err().contains("empty.rbxgame"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn resolve_joins_relative_and_keeps_absolute() {
    let abs = std::env::temp_dir().join("abs.rbxl");
    let m = manifest(vec![
        entry("a.rbxl"),
        PlaceEntry {
            path: abs.clone(),
            place_id: None,
            name: None,
        },
    ]);
    let resolved = m.resolve(Path::new("/games/g/g.rbxgame"));
    assert_eq!(resolved[0], PathBuf::from("/games/g/a.rbxl"));
    assert_eq!(resolved[1], abs);
}

#[test]
fn label_falls_back_to_file_stem() {
    let mut m = manifest(vec![entry("dir/lobby.rbxl"), entry("b.rbxl")]);
    m.places[1].name = Some("Arena".into());
    assert_eq!(m.label(0), "lobby");
    assert_eq!(m.label(1), "Arena");
}

#[test]
fn for_files_names_manifest_after_folder_and_stores_relative() {
    let dir = PathBuf::from("/games/obby");
    let other = PathBuf::from("/elsewhere/x.rbxl");
    let (path, m) = Manifest::for_files(&[dir.join("a.rbxl"), other.clone()], None);
    assert_eq!(path, dir.join("obby.rbxgame"));
    assert_eq!(m.name, "obby");
    assert_eq!(m.places[0].path, PathBuf::from("a.rbxl"));
    assert_eq!(m.places[1].path, other);
}

#[test]
fn for_files_merges_into_existing_appending_only_missing() {
    let dir = PathBuf::from("/games/obby");
    let mut existing = manifest(vec![entry("a.rbxl")]);
    existing.places[0].name = Some("Kept".into());
    let (_, m) = Manifest::for_files(&[dir.join("a.rbxl"), dir.join("b.rbxl")], Some(existing));
    assert_eq!(m.name, "G");
    assert_eq!(m.places.len(), 2);
    assert_eq!(m.places[0].name.as_deref(), Some("Kept"));
    assert_eq!(m.places[1].path, PathBuf::from("b.rbxl"));
}

#[test]
fn for_files_lists_duplicates_once() {
    let dir = PathBuf::from("/games/obby");
    let (_, m) = Manifest::for_files(&[dir.join("a.rbxl"), dir.join("a.rbxl")], None);
    assert_eq!(m.places.len(), 1);
}
