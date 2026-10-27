use mlua::{Function, Lua, Table, Value};

const INPUT: &str = include_str!("../src/probe/input.luau");

fn module(lua: &Lua) -> Table {
    lua.load(INPUT)
        .set_name("input.luau")
        .eval()
        .expect("input.luau loads without a game")
}

fn insert(lua: &Lua, text: &str, cursor: i64, selection: i64, typed: &str) -> (String, i64) {
    let insert: Function = module(lua).get("insert").expect("insert");
    insert
        .call((text, cursor, selection, typed))
        .expect("insert runs")
}

#[test]
fn text_goes_in_at_the_caret_or_over_the_selection() {
    let lua = Lua::new();
    // CursorPosition 6 is after "Hello".
    assert_eq!(
        insert(&lua, "Hello World", 6, -1, ", big"),
        ("Hello, big World".to_owned(), 11)
    );
    // No caret yet: appended.
    assert_eq!(insert(&lua, "ab", -1, -1, "c"), ("abc".to_owned(), 4));
    // "World" selected, caret either end.
    assert_eq!(
        insert(&lua, "Hello World", 12, 7, "there"),
        ("Hello there".to_owned(), 12)
    );
    assert_eq!(
        insert(&lua, "Hello World", 7, 12, "there"),
        ("Hello there".to_owned(), 12)
    );
    // Byte offsets: "é" is two bytes.
    assert_eq!(insert(&lua, "é", 3, -1, "!"), ("é!".to_owned(), 4));
}

/// A stand-in instance tree: `node(name, children)` answers FindFirstChild
/// and GetChildren like an Instance.
const TREE: &str = r#"
local function node(name, children)
    local self = { Name = name, children = children or {} }
    function self:FindFirstChild(wanted)
        for _, c in self.children do
            if c.Name == wanted then return c end
        end
        return nil
    end
    function self:GetChildren()
        return self.children
    end
    return self
end
local button = node("Button")
local gui = node("Gui", { button, node("Config.v2", { node("Inner") }) })
local playerGui = node("PlayerGui", { gui })
local player = node("Chee", { playerGui })
local part = node("Target")
local workspace = node("Workspace", { part })
local game = node("Game", { workspace, node("Players", { player }) })
return game, player
"#;

fn resolve(lua: &Lua, path: &str) -> Option<String> {
    let (game, player): (Table, Table) = lua.load(TREE).eval().expect("tree");
    let resolve: Function = module(lua).get("resolveFrom").expect("resolveFrom");
    match resolve
        .call::<Value>((game, player, path))
        .expect("resolve runs")
    {
        Value::Table(found) => Some(found.get("Name").expect("name")),
        _ => None,
    }
}

#[test]
fn paths_start_at_game_or_local_player() {
    let lua = Lua::new();
    assert_eq!(
        resolve(&lua, "game.Workspace.Target").as_deref(),
        Some("Target")
    );
    assert_eq!(resolve(&lua, "Workspace.Target").as_deref(), Some("Target"));
    assert_eq!(
        resolve(&lua, "LocalPlayer.PlayerGui.Gui.Button").as_deref(),
        Some("Button")
    );
    assert_eq!(
        resolve(&lua, "game.Players.Chee.PlayerGui.Gui.Button").as_deref(),
        Some("Button")
    );
    // LocalPlayer is a property, not a child, as in Studio.
    assert_eq!(resolve(&lua, "game.Players.LocalPlayer.PlayerGui"), None);
}

#[test]
fn names_may_hold_dots_and_case_is_forgiven() {
    let lua = Lua::new();
    assert_eq!(
        resolve(&lua, "LocalPlayer.PlayerGui.Gui.Config.v2.Inner").as_deref(),
        Some("Inner")
    );
    assert_eq!(resolve(&lua, "workspace.target").as_deref(), Some("Target"));
    assert_eq!(resolve(&lua, "Workspace.Nope"), None);
    assert_eq!(resolve(&lua, ""), None);
}
