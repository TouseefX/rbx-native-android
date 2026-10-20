use rbx_dom::{Variant, WeakDom};
use rbx_reflection::ReflectionDatabase;
use serde_json::{json, Value};

use super::{call, resolve};
use crate::mcp::{PlayState, Session, Studio};
use crate::properties::Properties;

pub(super) fn studio() -> Studio {
    Studio {
        id: "a1".to_owned(),
        name: "Arena".to_owned(),
        place_id: None,
        play: PlayState::Edit,
    }
}

pub(super) fn place() -> WeakDom {
    let mut dom = WeakDom::new();
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    let model = dom.new_instance("Model", "Tower", Some(workspace));
    let part = dom.new_instance("Part", "Door", Some(model));
    dom.set_property(part, "Anchored", Variant::Bool(true))
        .unwrap();
    dom.new_instance("Folder", "Config.v2", Some(workspace));
    let service = dom.new_instance("ServerScriptService", "ServerScriptService", None);
    let main = dom.new_instance("Script", "GameLoop", Some(service));
    dom.set_property(
        main,
        "Source",
        Variant::String("local rounds = 0\nprint(\"Round start\")\nrounds += 1\n".to_owned()),
    )
    .unwrap();
    let module = dom.new_instance("ModuleScript", "RoundConfig", Some(service));
    dom.set_property(
        module,
        "Source",
        Variant::String("return { length = 60 } -- round length".to_owned()),
    )
    .unwrap();
    dom.new_instance("LocalScript", "Empty", Some(service));
    dom
}

fn run(dom: &WeakDom, tool: &str, arguments: Value) -> (bool, String) {
    let properties = Properties::new(ReflectionDatabase::embedded());
    let studio = studio();
    let mut session = Session::new(dom, &properties, &studio);
    let result = call(
        &json!({ "name": tool, "arguments": arguments }),
        &mut session,
    )
    .unwrap();
    (
        result["isError"].as_bool().unwrap(),
        result["content"][0]["text"].as_str().unwrap().to_owned(),
    )
}

fn json_of(dom: &WeakDom, tool: &str, arguments: Value) -> Value {
    let (failed, text) = run(dom, tool, arguments);
    assert!(!failed, "{tool}: {text}");
    serde_json::from_str(&text).unwrap()
}

#[test]
fn paths_resolve_with_or_without_game_and_through_dotted_names() {
    let dom = place();
    let door = resolve(&dom, "game.Workspace.Tower.Door").unwrap();
    assert_eq!(resolve(&dom, "Workspace.Tower.Door"), Some(door));
    assert_eq!(super::path_of(&dom, door), "game.Workspace.Tower.Door");
    let folder = resolve(&dom, "game.Workspace.Config.v2").unwrap();
    assert_eq!(dom.get(folder).unwrap().name(), "Config.v2");
    assert_eq!(resolve(&dom, "game.Workspace.Tower.Window"), None);
    assert_eq!(resolve(&dom, "game"), None);
}

#[test]
fn search_game_tree_walks_in_explorer_order_down_to_the_depth_asked() {
    let dom = place();
    let all = json_of(&dom, "search_game_tree", json!({}));
    let paths: Vec<&str> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        &paths[..4],
        [
            "game.Workspace",
            "game.Workspace.Tower",
            "game.Workspace.Tower.Door",
            "game.Workspace.Config.v2"
        ]
    );

    let shallow = json_of(
        &dom,
        "search_game_tree",
        json!({ "path": "game.Workspace", "max_depth": 0 }),
    );
    assert_eq!(
        shallow,
        json!([{ "path": "game.Workspace", "class": "Workspace", "children": 2 }])
    );
}

#[test]
fn search_game_tree_filters_by_class_as_is_a_does_and_by_name() {
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
    assert_eq!(parts[0]["path"], "game.Workspace.Tower.Door");
    let named = json_of(&dom, "search_game_tree", json!({ "keywords": "ROUND" }));
    assert_eq!(named[0]["path"], "game.ServerScriptService.RoundConfig");
    assert_eq!(named.as_array().unwrap().len(), 1);
}

#[test]
fn inspect_instance_reports_properties_children_and_descendants() {
    let dom = place();
    let tower = json_of(
        &dom,
        "inspect_instance",
        json!({ "path": "game.Workspace.Tower" }),
    );
    assert_eq!(tower["class"], "Model");
    assert_eq!(
        tower["children"],
        json!([{ "name": "Door", "class": "Part" }])
    );
    assert_eq!(tower["descendant_count"], 1);
    let door = json_of(
        &dom,
        "inspect_instance",
        json!({ "path": "Workspace.Tower.Door" }),
    );
    assert_eq!(door["properties"]["Anchored"], "true");
    let script = json_of(
        &dom,
        "inspect_instance",
        json!({ "path": "game.ServerScriptService.GameLoop" }),
    );
    assert!(script.get("source").is_some());
}

#[test]
fn script_read_numbers_lines_and_reads_a_range() {
    let dom = place();
    let (_, whole) = run(
        &dom,
        "script_read",
        json!({ "path": "game.ServerScriptService.GameLoop" }),
    );
    assert_eq!(
        whole,
        "1\tlocal rounds = 0\n2\tprint(\"Round start\")\n3\trounds += 1"
    );
    let (_, middle) = run(
        &dom,
        "script_read",
        json!({ "path": "game.ServerScriptService.GameLoop", "start_line": 2, "end_line": 2 }),
    );
    assert_eq!(middle, "2\tprint(\"Round start\")");
    let (failed, _) = run(
        &dom,
        "script_read",
        json!({ "path": "game.ServerScriptService.GameLoop", "start_line": 9 }),
    );
    assert!(failed);
}

#[test]
fn script_read_takes_an_unwritten_script_as_empty_and_refuses_a_part() {
    let dom = place();
    assert_eq!(
        run(
            &dom,
            "script_read",
            json!({ "path": "game.ServerScriptService.Empty" })
        ),
        (false, String::new())
    );
    let (failed, text) = run(
        &dom,
        "script_read",
        json!({ "path": "game.Workspace.Tower.Door" }),
    );
    assert!(failed);
    assert!(text.contains("not a script"), "{text}");
}

#[test]
fn script_search_ranks_closer_names_first() {
    let dom = place();
    let found = json_of(&dom, "script_search", json!({ "query": "round" }));
    assert_eq!(
        found,
        json!([{ "path": "game.ServerScriptService.RoundConfig", "class": "ModuleScript" }])
    );
    let fuzzy = json_of(&dom, "script_search", json!({ "query": "glp" }));
    assert_eq!(fuzzy[0]["path"], "game.ServerScriptService.GameLoop");
    let none = json_of(&dom, "script_search", json!({ "query": "zzz" }));
    assert_eq!(none, json!([]));
}

#[test]
fn script_grep_finds_text_literally_ignoring_case_unless_asked() {
    let dom = place();
    let found = json_of(&dom, "script_grep", json!({ "pattern": "ROUND" }));
    let lines: Vec<(String, u64)> = found
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["path"].as_str().unwrap().to_owned(),
                m["line"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        lines,
        [
            ("game.ServerScriptService.GameLoop".to_owned(), 1),
            ("game.ServerScriptService.GameLoop".to_owned(), 2),
            ("game.ServerScriptService.GameLoop".to_owned(), 3),
            ("game.ServerScriptService.RoundConfig".to_owned(), 1),
        ]
    );
    let exact = json_of(
        &dom,
        "script_grep",
        json!({ "pattern": "Round", "case_sensitive": true }),
    );
    assert_eq!(exact.as_array().unwrap().len(), 1);
    // A Luau pattern's magic characters are only text here.
    let literal = json_of(&dom, "script_grep", json!({ "pattern": "+= 1" }));
    assert_eq!(literal[0]["line"], 3);
}

#[test]
fn get_studio_state_names_this_editor_and_what_it_can_reach() {
    let dom = place();
    let state = json_of(&dom, "get_studio_state", json!({}));
    assert_eq!(state["studio_id"], "a1");
    assert_eq!(state["name"], "Arena");
    assert!(
        state.get("place_id").is_none(),
        "an unlinked place has none"
    );
    assert_eq!(state["play_state"], "Edit");
    assert_eq!(state["datamodel_types"], json!(["Edit"]));
    assert_eq!(state["editable"], true);
}

#[test]
fn list_roblox_studios_on_one_editor_lists_just_it() {
    let dom = place();
    assert_eq!(
        json_of(&dom, "list_roblox_studios", json!({})),
        json!([{ "studio_id": "a1", "name": "Arena" }])
    );
}

#[test]
fn a_call_for_another_studio_is_refused_by_this_one() {
    let dom = place();
    let (failed, text) = run(
        &dom,
        "script_read",
        json!({ "path": "game.ServerScriptService.GameLoop", "studio_id": "b2" }),
    );
    assert!(failed);
    assert!(text.contains("a1") && text.contains("b2"), "{text}");
    let (failed, _) = run(
        &dom,
        "script_read",
        json!({ "path": "game.ServerScriptService.GameLoop", "studio_id": "a1" }),
    );
    assert!(!failed);
}
