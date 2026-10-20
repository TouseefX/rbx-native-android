use serde_json::json;

use super::super::tests::{place, run};

const GAME_LOOP: &str = "game.ServerScriptService.GameLoop";

#[test]
fn script_read_numbers_lines_as_studio_does_and_reads_a_range() {
    let dom = place();
    let (_, whole) = run(&dom, "script_read", json!({ "target_file": GAME_LOOP }));
    // Right-aligned to six, and the empty line after the last newline kept,
    // exactly as Studio printed TestPlace's scripts.
    assert_eq!(
        whole,
        "     1\u{2192}local rounds = 0\n     2\u{2192}print(\"Round start\")\n     \
         3\u{2192}rounds += 1\n     4\u{2192}"
    );
    let (_, middle) = run(
        &dom,
        "script_read",
        json!({
            "target_file": GAME_LOOP,
            "should_read_entire_file": false,
            "start_line_one_indexed": 2,
            "end_line_one_indexed_inclusive": 2,
        }),
    );
    assert_eq!(middle, "     2\u{2192}print(\"Round start\")");
    let (failed, _) = run(
        &dom,
        "script_read",
        json!({ "target_file": GAME_LOOP, "should_read_entire_file": false }),
    );
    assert!(failed, "a partial read needs both line numbers");
    let (failed, _) = run(
        &dom,
        "script_read",
        json!({ "target_file": GAME_LOOP, "start_line_one_indexed": 9 }),
    );
    assert!(failed);
}

#[test]
fn script_read_takes_an_unwritten_script_as_empty_and_refuses_the_rest() {
    let dom = place();
    assert_eq!(
        run(
            &dom,
            "script_read",
            json!({ "target_file": "game.ServerScriptService.Empty" })
        ),
        (false, String::new())
    );
    let (failed, text) = run(
        &dom,
        "script_read",
        json!({ "target_file": "game.Workspace.Tower.Door" }),
    );
    assert!(failed);
    assert!(text.contains("not a script"), "{text}");
    assert_eq!(
        run(
            &dom,
            "script_read",
            json!({ "target_file": "game.ServerScriptService.Nope" })
        ),
        (
            true,
            "Script not found at path: game.ServerScriptService.Nope".to_owned()
        )
    );
}

#[test]
fn script_search_ranks_closer_names_first_across_keywords() {
    let dom = place();
    assert_eq!(
        run(&dom, "script_search", json!({ "keywords": "round" })),
        (
            false,
            "Name: RoundConfig | Type: ModuleScript | Path: ServerScriptService.RoundConfig"
                .to_owned()
        )
    );
    let (_, fuzzy) = run(&dom, "script_search", json!({ "keywords": "glp" }));
    assert!(fuzzy.starts_with("Name: GameLoop |"), "{fuzzy}");
    let (_, either) = run(&dom, "script_search", json!({ "keywords": "zzz, empty" }));
    assert!(either.starts_with("Name: Empty |"), "{either}");
    assert_eq!(
        run(&dom, "script_search", json!({ "keywords": "zzz" })),
        (false, "No matches found for the given query.".to_owned())
    );
}

#[test]
fn script_grep_matches_luau_patterns_line_by_line() {
    let dom = place();
    let grep = |query: &str| run(&dom, "script_grep", json!({ "query": query })).1;
    assert_eq!(
        grep("rounds"),
        "Path: ServerScriptService.GameLoop | Line: 1 | local rounds = 0\n\
         Path: ServerScriptService.GameLoop | Line: 3 | rounds += 1"
    );
    let lines = |query: &str| -> Vec<String> {
        grep(query)
            .lines()
            .map(|row| row.split(" | ").take(2).collect::<Vec<_>>().join(" "))
            .collect()
    };
    let game_loop = |line: u32| format!("Path: ServerScriptService.GameLoop Line: {line}");
    // As in Luau, a plain string is case-sensitive.
    assert_eq!(lines("Round"), [game_loop(2)]);
    assert_eq!(lines("^local"), [game_loop(1)]);
    assert_eq!(
        lines("%d+"),
        [
            game_loop(1),
            game_loop(3),
            "Path: ServerScriptService.RoundConfig Line: 1".to_owned()
        ]
    );
    assert_eq!(lines("%+= 1"), [game_loop(3)]);
    assert_eq!(grep("zzz"), "No matches found for the given query.");

    let (failed, text) = run(&dom, "script_grep", json!({ "query": "%" }));
    assert!(failed);
    assert!(text.contains("malformed pattern"), "{text}");
}
