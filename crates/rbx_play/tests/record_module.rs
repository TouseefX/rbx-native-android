use mlua::{Function, Lua, Table};

const RECORD: &str = include_str!("../src/probe/record.luau");

fn damage_taken(last: f64, health: f64, dead: bool) -> Option<f64> {
    let lua = Lua::new();
    lua.load("game = { GetService = function() return {} end }")
        .exec()
        .unwrap();
    let module: Table = lua
        .load(RECORD)
        .set_name("record.luau")
        .eval()
        .expect("record.luau loads");
    let f: Function = module.get("damageTaken").unwrap();
    f.call((last, health, dead)).unwrap()
}

#[test]
fn a_hit_is_the_health_lost() {
    assert_eq!(damage_taken(100.0, 70.0, false), Some(30.0));
}

#[test]
fn a_reset_to_zero_is_the_death_not_damage() {
    assert_eq!(damage_taken(100.0, 0.0, false), None);
    assert_eq!(damage_taken(40.0, -5.0, false), None);
}

#[test]
fn healing_and_hits_after_death_are_not_damage() {
    assert_eq!(damage_taken(50.0, 80.0, false), None);
    assert_eq!(damage_taken(50.0, 50.0, false), None);
    assert_eq!(damage_taken(30.0, 10.0, true), None);
}
