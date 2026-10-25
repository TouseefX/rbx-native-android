use serde_json::{json, Value};

use super::super::resolve;
use super::super::tests::{json_of, place, run};

#[test]
fn search_game_tree_starts_at_the_place_and_walks_in_explorer_order() {
    let dom = place();
    let all = json_of(&dom, "search_game_tree", json!({}));
    let paths: Vec<&str> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["fullPath"].as_str().unwrap())
        .collect();
    assert_eq!(
        &paths[..5],
        [
            "Arena.rbxl",
            "Workspace",
            "Workspace.Tower",
            "Workspace.Tower.Door",
            "Workspace.Config.v2"
        ]
    );
    // As real Studio answers: the place is the DataModel, the services'
    // parent, and has no parent of its own.
    assert_eq!(
        all[0],
        json!({ "name": "Arena.rbxl", "className": "DataModel", "fullPath": "Arena.rbxl" })
    );
    assert_eq!(all[1]["parentName"], "Arena.rbxl");
    assert_eq!(
        all[2],
        json!({
            "name": "Tower",
            "className": "Model",
            "fullPath": "Workspace.Tower",
            "parentName": "Workspace",
        })
    );
}

#[test]
fn search_game_tree_summarizes_children_past_the_depth_limit() {
    let dom = place();
    let shallow = json_of(
        &dom,
        "search_game_tree",
        json!({ "path": "Workspace", "max_depth": 0 }),
    );
    assert_eq!(
        shallow,
        json!([{
            "name": "Workspace",
            "className": "Workspace",
            "fullPath": "Workspace",
            "parentName": "Arena.rbxl",
            "childSummary": "2 children (1 Folder, 1 Model)",
            "unexploredChildCount": 2,
        }])
    );
    // Studio says "children" even for one.
    let tower = json_of(
        &dom,
        "search_game_tree",
        json!({ "path": "Workspace.Tower", "max_depth": 0 }),
    );
    assert_eq!(tower[0]["childSummary"], "1 children (1 Part)");
    // Studio caps the depth at 10; a larger one is not an error.
    let deep = json_of(&dom, "search_game_tree", json!({ "max_depth": 99 }));
    assert_eq!(deep.as_array().unwrap().len(), 9);
}

#[test]
fn search_game_tree_filters_by_class_and_by_any_keyword() {
    let dom = place();
    let scripts = json_of(
        &dom,
        "search_game_tree",
        json!({ "instance_type": "LuaSourceContainer" }),
    );
    assert_eq!(scripts.as_array().unwrap().len(), 3);
    let parts = json_of(
        &dom,
        "search_game_tree",
        json!({ "instance_type": "BasePart" }),
    );
    assert_eq!(parts[0]["fullPath"], "Workspace.Tower.Door");
    let named = json_of(
        &dom,
        "search_game_tree",
        json!({ "keywords": "ROUND, door" }),
    );
    let names: Vec<&str> = named
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Door", "RoundConfig"]);

    // At the depth limit, only children the filters would keep count as
    // unexplored, as in Studio.
    let filtered = json_of(
        &dom,
        "search_game_tree",
        json!({ "path": "Workspace", "max_depth": 0, "keywords": "work" }),
    );
    assert_eq!(
        filtered[0]["childSummary"],
        "2 children (1 Folder, 1 Model)"
    );
    assert!(filtered[0].get("unexploredChildCount").is_none());
}

#[test]
fn search_game_tree_words_a_cut_off_or_empty_result_as_studio_does() {
    let dom = place();
    let (failed, capped) = run(&dom, "search_game_tree", json!({ "head_limit": 2 }));
    assert!(!failed);
    let (note, rest) = capped.split_once("\n\n").unwrap();
    assert_eq!(note, "Note: Output limited to 2 nodes (results truncated)");
    let rows: Value = serde_json::from_str(rest).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);

    assert_eq!(
        run(
            &dom,
            "search_game_tree",
            json!({ "instance_type": "Sound" })
        ),
        (false, "No instances found matching type 'Sound'".to_owned())
    );
    let (failed, text) = run(
        &dom,
        "search_game_tree",
        json!({ "path": "Workspace.Nope" }),
    );
    assert!(failed);
    assert_eq!(text, "Could not find instance at path 'Workspace.Nope'");
    let (failed, _) = run(
        &dom,
        "search_game_tree",
        json!({ "datamodel_type": "Server" }),
    );
    assert!(failed, "only the Edit data model is reachable");
}

#[test]
fn search_game_tree_reads_a_live_tree_and_refuses_a_missing_one() {
    use crate::mcp::{PlayState, Session, Studio};
    use crate::properties::Properties;
    use rbx_dom::WeakDom;
    use rbx_reflection::ReflectionDatabase;

    let dom = place();
    let mut client = WeakDom::new();
    let workspace = client.new_instance("Workspace", "Workspace", None);
    client.new_instance("Part", "LiveOnly", Some(workspace));
    let properties = Properties::new(ReflectionDatabase::embedded());
    let studio = Studio {
        play: PlayState::Playing,
        ..super::super::tests::studio()
    };
    let mut session = Session::new(&dom, &properties, &studio);
    session.client = Some(&client);
    let found = super::search(
        &json!({ "keywords": "LiveOnly", "datamodel_type": "Client" }),
        &session,
    )
    .unwrap();
    let rows: Value = serde_json::from_str(found.as_str().unwrap()).unwrap();
    assert_eq!(rows[0]["fullPath"], "Workspace.LiveOnly");
    assert_eq!(
        super::inspect(
            &json!({ "path": "Workspace.LiveOnly", "datamodel_type": "Client" }),
            &session
        )
        .map(|_| ()),
        Ok(())
    );
    assert_eq!(
        super::search(&json!({ "datamodel_type": "Server" }), &session),
        Err("Server datamodel is not available in Play mode".to_owned())
    );
    assert_eq!(
        super::search(&json!({ "datamodel_type": "Nope" }), &session),
        Err("Invalid datamodel_type: Nope".to_owned())
    );
}

#[test]
fn inspect_instance_reports_studios_fields_with_typed_values() {
    let dom = place();
    let tower = json_of(
        &dom,
        "inspect_instance",
        json!({ "path": "game.Workspace.Tower" }),
    );
    assert_eq!(tower["className"], "Model");
    assert_eq!(tower["path"], "Workspace.Tower");
    assert_eq!(
        tower["children"],
        json!({
            "immediateChildren": [{ "className": "Part", "path": "Workspace.Tower.Door" }],
            "childrenCount": 1,
            "totalDescendants": 1,
        })
    );
    let door = json_of(
        &dom,
        "inspect_instance",
        json!({ "path": "workspace.tower.door" }),
    );
    assert_eq!(door["name"], "Door", "the path ignores case");
    // A boolean is a JSON boolean, as Studio returns it, not "true".
    assert_eq!(door["properties"]["Anchored"], true);
    let (failed, text) = run(
        &dom,
        "inspect_instance",
        json!({ "path": "Workspace.NoSuchThing" }),
    );
    assert!(failed);
    assert_eq!(
        text,
        "Could not find any instances at path 'Workspace.NoSuchThing'"
    );
}

#[test]
fn inspect_instance_returns_every_sibling_a_path_names() {
    let mut dom = place();
    let tower = resolve(&dom, "Workspace.Tower").unwrap();
    dom.new_instance("Part", "Door", Some(tower));
    let both = json_of(
        &dom,
        "inspect_instance",
        json!({ "path": "Workspace.Tower.Door" }),
    );
    assert_eq!(both["count"], 2);
    assert_eq!(both["totalFound"], 2);
    assert_eq!(both["matches"].as_array().unwrap().len(), 2);
    assert!(both.get("warning").is_none());

    for _ in 0..25 {
        dom.new_instance("Part", "Door", Some(tower));
    }
    let many = json_of(
        &dom,
        "inspect_instance",
        json!({ "path": "Workspace.Tower.Door" }),
    );
    assert_eq!(many["count"], 20);
    assert_eq!(many["totalFound"], 27);
    assert!(many["warning"].is_string());
}

#[test]
fn inspect_instance_lists_what_a_script_reads_in_studios_formats() {
    use rbx_dom::{CFrameData, Variant, Vector3Data, WeakDom};

    // A part as a place file stores it: `size`, `Color3uint8`, a raw
    // `Material` ordinal and serialization-only fields.
    let mut dom = WeakDom::new();
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    let part = dom.new_instance("Part", "Baseplate", Some(workspace));
    let set = |dom: &mut WeakDom, name: &str, value: Variant| {
        dom.set_property(part, name, value).unwrap();
    };
    set(
        &mut dom,
        "size",
        Variant::Vector3(Vector3Data {
            x: 2048.0,
            y: 16.0,
            z: 2048.0,
        }),
    );
    set(
        &mut dom,
        "CFrame",
        Variant::CFrame(CFrameData {
            position: Vector3Data {
                x: 0.0,
                y: -8.0,
                z: 0.0,
            },
            rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        }),
    );
    set(
        &mut dom,
        "Color3uint8",
        Variant::Color3uint8 {
            r: 91,
            g: 91,
            b: 91,
        },
    );
    set(&mut dom, "Material", Variant::Enum(256));
    set(
        &mut dom,
        "AttributesSerialize",
        Variant::String(String::new()),
    );

    // Every value below is what real Studio answered for TestPlace's
    // Baseplate.
    let report = json_of(
        &dom,
        "inspect_instance",
        json!({ "path": "Workspace.Baseplate" }),
    );
    let properties = &report["properties"];
    assert_eq!(properties["Size"], "2048, 16, 2048");
    assert!(
        properties.get("size").is_none(),
        "named as a script reads it"
    );
    assert_eq!(properties["CFrame.Position"], "0, -8, 0");
    assert_eq!(properties["CFrame.Rotation"], "1, 0, 0, 0, 1, 0, 0, 0, 1");
    assert!(properties.get("CFrame").is_none());
    assert_eq!(properties["Material"], "Enum.Material.Plastic");
    assert_eq!(
        properties["Color"],
        "0.35686275362968445, 0.35686275362968445, 0.35686275362968445"
    );
    assert!(
        properties.get("AttributesSerialize").is_none(),
        "a field only the file holds is not a property"
    );
    // Defaults a script reads even though the file never wrote them.
    assert_eq!(properties["Archivable"], true);
    // What Studio leaves out: the identity given above, what cannot be set,
    // and empty tags and attributes.
    assert!(properties.get("Name").is_none() && properties.get("Parent").is_none());
    assert!(properties.get("Mass").is_none(), "read-only");
    assert!(
        properties.get("Origin.Position").is_none(),
        "the panel's pivot row is not a property"
    );
    assert!(report.get("tags").is_none() && report.get("attributes").is_none());
}
