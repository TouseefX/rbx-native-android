use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Function, Lua, Table, Value};

use super::*;

type Acks = Rc<RefCell<Vec<(u64, bool, String)>>>;

struct Vm {
    lua: Lua,
    runners: Table,
    serialize: Function,
    acks: Acks,
}

fn vm() -> Vm {
    let lua = Lua::new();
    lua.load("task = { spawn = function(f, ...) assert(coroutine.resume(coroutine.create(f), ...)) end }")
        .exec()
        .unwrap();
    let loadstring = lua
        .create_function(|lua, (source, name): (String, String)| {
            Ok(match lua.load(&source).set_name(name).into_function() {
                Ok(function) => (Some(function), None),
                Err(error) => (None, Some(error.to_string())),
            })
        })
        .unwrap();
    let fiu: Table = lua
        .load(PROBE_FIU)
        .set_name("=ReplicatedStorage.__rbxnative_fiu")
        .eval()
        .unwrap();
    let serialize: Function = lua.load(PROBE_SERIALIZE).eval().unwrap();
    let runners: Table = lua
        .load(PROBE_EXEC)
        .call((serialize.clone(), fiu, loadstring))
        .unwrap();
    Vm {
        lua,
        runners,
        serialize,
        acks: Acks::default(),
    }
}

fn run(vm: &Vm, side: Side, source: &str) -> (bool, String) {
    let CommandBody::Exec {
        source: text,
        bytecode,
        ..
    } = command(side, source).unwrap()
    else {
        panic!("not an exec command");
    };
    let cmd = vm.lua.create_table().unwrap();
    cmd.set("id", 7).unwrap();
    cmd.set("source", text).unwrap();
    cmd.set("bytecode", bytecode).unwrap();
    let acks = vm.acks.clone();
    let ack = vm
        .lua
        .create_function(move |_, (id, ok, text): (u64, bool, String)| {
            acks.borrow_mut().push((id, ok, text));
            Ok(())
        })
        .unwrap();
    let runner = if side == Side::Client {
        "client"
    } else {
        "server"
    };
    let function: Function = vm.runners.get(runner).unwrap();
    function.call::<()>((cmd, ack)).unwrap();
    let (id, ok, text) = vm.acks.borrow_mut().pop().expect("an ack");
    assert_eq!(id, 7);
    (ok, text)
}

fn serialized(vm: &Vm, chunk: &str) -> String {
    let value: Value = vm.lua.load(chunk).eval().unwrap();
    vm.serialize.call(value).unwrap()
}

#[test]
fn client_expression_returns_its_value() {
    assert_eq!(run(&vm(), Side::Client, "1 + 2"), (true, "3".into()));
}

#[test]
fn client_statements_return_tables_as_json() {
    let source = "local t = {} for i = 1, 3 do t[i] = i * i end return t";
    assert_eq!(
        run(&vm(), Side::Client, source),
        (true, r#"{"1":1,"2":4,"3":9}"#.into())
    );
}

#[test]
fn client_error_is_not_ok() {
    let (ok, text) = run(&vm(), Side::Client, "local a = 1\nerror('boom')");
    assert!(!ok);
    assert_eq!(text, "AssistantCommand:2: boom");
}

#[test]
fn client_syntax_error_names_the_chunk_like_the_server() {
    let client = command(Side::Client, "local = 1").unwrap_err();
    let (ok, server) = run(&vm(), Side::Server, "local = 1");
    assert!(!ok);
    assert!(client.starts_with("AssistantCommand:1: "), "{client}");
    assert!(server.ends_with(&client), "{server} / {client}");
}

#[test]
fn server_uses_loadstring() {
    assert_eq!(run(&vm(), Side::Server, "return 'hi'"), (true, "hi".into()));
    assert_eq!(run(&vm(), Side::Server, "2 * 4"), (true, "8".into()));
}

#[test]
fn server_compile_error_is_not_ok() {
    assert!(!run(&vm(), Side::Server, "return +").0);
}

#[test]
fn server_errors_name_the_chunk_like_studio() {
    assert_eq!(
        run(&vm(), Side::Server, "error('boom')"),
        (false, "AssistantCommand:1: boom".into())
    );
}

#[test]
fn serializer_follows_studio_rules() {
    let vm = vm();
    assert_eq!(serialized(&vm, "return nil"), "nil");
    assert_eq!(serialized(&vm, "return true"), "true");
    assert_eq!(serialized(&vm, "return 1.5"), "1.5");
    assert_eq!(serialized(&vm, "return {a = 1}"), "{\"a\":1}");
    assert_eq!(
        serialized(&vm, "return {b = {2, 3}, a = 1, 'x'}"),
        r#"{"1":"x","a":1,"b":{"1":2,"2":3}}"#
    );
    assert_eq!(
        serialized(&vm, "local t = {} t.me = t return t"),
        "[Complex Table]"
    );
    let long = serialized(&vm, "return string.rep('x', 20000)");
    assert_eq!(long.len(), 10000 + "... (truncated)".len());
    assert!(long.ends_with("... (truncated)"));
}

#[test]
fn module_source_requires_its_siblings() {
    let source = exec_module_source();
    assert!(source.contains(SERIALIZE_NAME) && source.contains(FIU_NAME));
}
