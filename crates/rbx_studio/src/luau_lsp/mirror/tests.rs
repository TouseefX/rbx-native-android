use std::fs;
use std::path::PathBuf;

use lsp_types::FileChangeType;
use rbx_dom::{Ref, Variant, WeakDom};
use rbx_reflection::ReflectionDatabase;
use serde_json::{json, Value};

use super::{Mirror, SOURCEMAP};

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("rbx-luau-mirror-{}-{name}", std::process::id()))
}

fn place() -> (WeakDom, Ref, Ref) {
    let mut dom = WeakDom::new();
    let storage = dom.new_instance("ReplicatedStorage", "ReplicatedStorage", None);
    let module = dom.new_instance("ModuleScript", "Mod", Some(storage));
    let _ = dom.set_property(module, "Source", Variant::String("return 1".into()));
    let _ = dom.new_instance("Folder", "Assets", Some(storage));
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    let script = dom.new_instance("Script", "Main", Some(workspace));
    (dom, module, script)
}

fn read_sourcemap(mirror: &Mirror) -> Value {
    serde_json::from_str(&fs::read_to_string(mirror.root().join(SOURCEMAP)).unwrap()).unwrap()
}

#[test]
fn writes_every_script_and_a_rojo_shaped_sourcemap() {
    let (dom, module, script) = place();
    let db = ReflectionDatabase::embedded();
    let mut mirror = Mirror::new(scratch("first"));
    let changes = mirror.sync(&dom, &db).unwrap();

    assert_eq!(
        changes.len(),
        3,
        "two scripts and the sourcemap: {changes:?}"
    );
    assert_eq!(
        fs::read_to_string(mirror.path_of(module)).unwrap(),
        "return 1"
    );
    // No `Source` yet reads as an empty script, not a missing one.
    assert_eq!(fs::read_to_string(mirror.path_of(script)).unwrap(), "");

    let module_file = format!("{}.luau", module.value());
    assert_eq!(
        read_sourcemap(&mirror)["children"][0],
        json!({
            "name": "ReplicatedStorage",
            "className": "ReplicatedStorage",
            "children": [
                {"name": "Mod", "className": "ModuleScript", "filePaths": [module_file]},
                {"name": "Assets", "className": "Folder"},
            ],
        })
    );
    assert_eq!(read_sourcemap(&mirror)["className"], "DataModel");
}

#[test]
fn a_second_sync_reports_only_what_moved() {
    let (mut dom, module, script) = place();
    let db = ReflectionDatabase::embedded();
    let mut mirror = Mirror::new(scratch("second"));
    mirror.sync(&dom, &db).unwrap();
    assert!(mirror.sync(&dom, &db).unwrap().is_empty());

    let _ = dom.set_property(module, "Source", Variant::String("return 2".into()));
    assert_eq!(
        mirror.sync(&dom, &db).unwrap(),
        vec![(mirror.path_of(module), FileChangeType::CHANGED)]
    );

    // A rename leaves every script's text alone and only moves the map.
    dom.set_name(script, "Renamed").unwrap();
    assert_eq!(
        mirror.sync(&dom, &db).unwrap(),
        vec![(mirror.root().join(SOURCEMAP), FileChangeType::CHANGED)]
    );

    dom.remove(script);
    let changes = mirror.sync(&dom, &db).unwrap();
    assert!(changes.contains(&(mirror.path_of(script), FileChangeType::DELETED)));
    assert!(!mirror.path_of(script).exists());
}

#[test]
fn maps_a_file_back_to_its_script_and_nothing_else() {
    let (dom, module, _) = place();
    let db = ReflectionDatabase::embedded();
    let mut mirror = Mirror::new(scratch("lookup"));
    mirror.sync(&dom, &db).unwrap();

    assert_eq!(mirror.script_at(&mirror.path_of(module)), Some(module));
    assert_eq!(mirror.script_at(&mirror.root().join(SOURCEMAP)), None);
    assert_eq!(mirror.script_at(&mirror.root().join("999999.luau")), None);
}

#[test]
fn dropping_the_mirror_removes_its_folder() {
    let (dom, _, _) = place();
    let db = ReflectionDatabase::embedded();
    let root = scratch("drop");
    let mut mirror = Mirror::new(root.clone());
    mirror.sync(&dom, &db).unwrap();
    drop(mirror);
    assert!(!root.exists());
}

#[test]
fn a_place_nested_deeper_than_any_call_stack_still_syncs() {
    // The mirror's walk used to recurse per instance, and built a tree of
    // JSON values that nested as deep on top of that: a joined place with a
    // deep chain in it overflowed the phone's smaller stack the moment a
    // script opened — the editor's silent crash. The sourcemap is a stream
    // over a work stack now, and a hundred thousand levels is just text.
    let mut dom = WeakDom::new();
    let mut current = dom.new_instance("DataModel", "game", None);
    for index in 0..100_000 {
        current = dom.new_instance("Model", &format!("M{index}"), Some(current));
    }
    let script = dom.new_instance("ModuleScript", "Deep", Some(current));
    let _ = dom.set_property(script, "Source", Variant::String("return 1".into()));

    let db = ReflectionDatabase::embedded();
    let mut mirror = Mirror::new(scratch("deep"));
    let changes = mirror.sync(&dom, &db).unwrap();
    assert_eq!(
        changes.len(),
        2,
        "the script and the sourcemap: {changes:?}"
    );
    assert_eq!(
        fs::read_to_string(mirror.path_of(script)).unwrap(),
        "return 1"
    );

    // Not parsed into a `Value` on purpose: a JSON reader nests as deep as
    // the text does, and this text is a hundred thousand levels deep. What
    // is checked is what any reader would find on the way down.
    let sourcemap = fs::read_to_string(mirror.root().join(SOURCEMAP)).unwrap();
    assert!(sourcemap.starts_with("{\"name\":\"game\",\"className\":\"DataModel\""));
    assert!(sourcemap.contains("\"Deep\""));
    assert_eq!(
        sourcemap.matches('{').count(),
        sourcemap.matches('}').count()
    );
    assert_eq!(
        sourcemap.matches('[').count(),
        sourcemap.matches(']').count()
    );
}
