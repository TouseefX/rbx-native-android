use rbx_dom::WeakDom;
use rbx_reflection::ReflectionDatabase;
use serde_json::{json, Value};

use super::super::tests::{place, studio};
use super::super::{call, resolve};
use super::{apply, ScriptEdit, Target};
use crate::mcp::{PlayState, Session, Studio};
use crate::properties::Properties;
use crate::script_editor::source;

const GAME_LOOP: &str = "game.ServerScriptService.GameLoop";

/// Whether the call failed, its result, and the edit it planned.
fn edit_with(
    dom: &WeakDom,
    studio: &Studio,
    arguments: Value,
) -> (bool, Value, Option<ScriptEdit>) {
    let properties = Properties::new(ReflectionDatabase::embedded());
    let mut session = Session::new(dom, &properties, studio);
    let result = call(
        &json!({ "name": "multi_edit", "arguments": arguments }),
        &mut session,
    )
    .unwrap();
    let text = result["content"][0]["text"].as_str().unwrap();
    let value = serde_json::from_str(text).unwrap_or(Value::String(text.to_owned()));
    (result["isError"].as_bool().unwrap(), value, session.planned)
}

fn edit(dom: &WeakDom, arguments: Value) -> (bool, Value, Option<ScriptEdit>) {
    edit_with(dom, &studio(), arguments)
}

fn replacement(old: &str, new: &str) -> Value {
    json!({ "old_string": old, "new_string": new })
}

#[test]
fn edits_apply_in_order_each_on_what_the_last_left() {
    let mut dom = place();
    let script = resolve(&dom, GAME_LOOP).unwrap();
    let (failed, result, planned) = edit(
        &dom,
        json!({
            "path": GAME_LOOP,
            "edits": [
                replacement("rounds = 0", "rounds = 1"),
                replacement("rounds = 1\n", "rounds = 1 -- first round\n"),
                replacement("Round start", "Go"),
            ],
        }),
    );
    assert!(!failed, "{result}");
    assert_eq!(result["created"], false);
    assert_eq!(result["changed"], true);
    assert_eq!(result["lines"], 3);
    let planned = planned.unwrap();
    assert_eq!(planned.target, Target::Existing(script));
    assert!(
        source::read(&dom, script).unwrap().contains("Round start"),
        "the tool only plans; the DOM is the shell's to write"
    );
    assert_eq!(apply(&mut dom, &planned), Some(script));
    assert_eq!(
        source::read(&dom, script).unwrap(),
        "local rounds = 1 -- first round\nprint(\"Go\")\nrounds += 1\n"
    );
}

#[test]
fn one_edit_that_cannot_apply_leaves_the_script_untouched() {
    let dom = place();
    let (failed, result, planned) = edit(
        &dom,
        json!({
            "path": GAME_LOOP,
            "edits": [replacement("rounds = 0", "rounds = 5"), replacement("no such text", "x")],
        }),
    );
    assert!(failed);
    assert!(planned.is_none(), "all or none");
    assert!(result.as_str().unwrap().starts_with("edit 2:"), "{result}");
}

#[test]
fn text_that_appears_twice_needs_replace_all() {
    let dom = place();
    let (failed, result, _) = edit(
        &dom,
        json!({ "path": GAME_LOOP, "edits": [replacement("rounds", "laps")] }),
    );
    assert!(failed);
    assert!(result.as_str().unwrap().contains("2 times"), "{result}");

    let (failed, _, planned) = edit(
        &dom,
        json!({
            "path": GAME_LOOP,
            "edits": [{ "old_string": "rounds", "new_string": "laps", "replace_all": true }],
        }),
    );
    assert!(!failed);
    assert_eq!(
        planned.unwrap().source,
        "local laps = 0\nprint(\"Round start\")\nlaps += 1\n"
    );
}

#[test]
fn an_edit_that_changes_nothing_plans_nothing() {
    let dom = place();
    let (failed, result, planned) = edit(
        &dom,
        json!({ "path": GAME_LOOP, "edits": [replacement("rounds += 1", "rounds += 1")] }),
    );
    assert!(!failed);
    assert_eq!(result["changed"], false);
    assert!(planned.is_none(), "no undo step that reverts nothing");
}

#[test]
fn a_missing_script_is_created_under_its_parent() {
    let mut dom = place();
    let service = resolve(&dom, "game.ServerScriptService").unwrap();
    let (failed, result, planned) = edit(
        &dom,
        json!({
            "path": "game.ServerScriptService.Shop",
            "class_name": "ModuleScript",
            "edits": [replacement("", "return {}\n")],
        }),
    );
    assert!(!failed, "{result}");
    assert_eq!(result["created"], true);
    assert_eq!(result["path"], "game.ServerScriptService.Shop");
    let planned = planned.unwrap();
    assert!(planned.creates());
    let shop = apply(&mut dom, &planned).unwrap();
    assert_eq!(dom.parent(shop), Some(service));
    let instance = dom.get(shop).unwrap();
    assert_eq!(
        (instance.class(), instance.name()),
        ("ModuleScript", "Shop")
    );
    assert_eq!(source::read(&dom, shop).as_deref(), Some("return {}\n"));
}

#[test]
fn a_new_script_defaults_to_script_and_needs_a_parent_that_exists() {
    let dom = place();
    let (_, _, planned) = edit(
        &dom,
        json!({ "path": "ServerScriptService.Boot", "edits": [replacement("", "print(1)")] }),
    );
    assert!(matches!(
        planned.unwrap().target,
        Target::New {
            class: "Script",
            ..
        }
    ));

    // Only the last level is new: a typo above it is not a script name.
    let (failed, result, planned) = edit(
        &dom,
        json!({ "path": "game.ServerScriptServic.Boot", "edits": [replacement("", "x")] }),
    );
    assert!(failed && planned.is_none(), "{result}");

    let (failed, result, _) = edit(
        &dom,
        json!({
            "path": "game.ServerScriptService.Boot",
            "class_name": "Part",
            "edits": [replacement("", "x")],
        }),
    );
    assert!(failed, "{result}");
}

#[test]
fn an_empty_old_string_only_fills_an_empty_script() {
    let dom = place();
    let (failed, _, _) = edit(
        &dom,
        json!({ "path": GAME_LOOP, "edits": [replacement("", "-- header\n")] }),
    );
    assert!(failed);
    let (failed, _, planned) = edit(
        &dom,
        json!({
            "path": "game.ServerScriptService.Empty",
            "edits": [replacement("", "print(1)")],
        }),
    );
    assert!(!failed);
    assert_eq!(planned.unwrap().source, "print(1)");
}

#[test]
fn what_is_not_a_script_or_not_the_edit_data_model_is_refused() {
    let dom = place();
    let (failed, result, _) = edit(
        &dom,
        json!({ "path": "game.Workspace.Tower.Door", "edits": [replacement("a", "b")] }),
    );
    assert!(failed);
    assert!(
        result.as_str().unwrap().contains("not a script"),
        "{result}"
    );

    let (failed, _, _) = edit(
        &dom,
        json!({
            "path": GAME_LOOP,
            "datamodel_type": "Server",
            "edits": [replacement("rounds = 0", "x")],
        }),
    );
    assert!(failed);

    let (failed, _, _) = edit(&dom, json!({ "path": GAME_LOOP, "edits": [] }));
    assert!(failed, "an empty list is a mistake, not a no-op");
    let (failed, _, _) = edit(
        &dom,
        json!({ "path": GAME_LOOP, "edits": [{ "old_string": "x" }] }),
    );
    assert!(failed);
}

#[test]
fn nothing_is_edited_while_a_live_playtest_is_shown() {
    let dom = place();
    let showing = Studio {
        play: PlayState::ShowingLive,
        ..studio()
    };
    let (failed, result, planned) = edit_with(
        &dom,
        &showing,
        json!({ "path": GAME_LOOP, "edits": [replacement("rounds = 0", "rounds = 1")] }),
    );
    assert!(failed && planned.is_none(), "{result}");
}

#[test]
fn applying_to_a_script_deleted_meanwhile_writes_nothing() {
    let mut dom = place();
    let script = resolve(&dom, GAME_LOOP).unwrap();
    let (_, _, planned) = edit(
        &dom,
        json!({ "path": GAME_LOOP, "edits": [replacement("rounds = 0", "rounds = 1")] }),
    );
    dom.remove(script);
    assert_eq!(apply(&mut dom, &planned.unwrap()), None);
}
