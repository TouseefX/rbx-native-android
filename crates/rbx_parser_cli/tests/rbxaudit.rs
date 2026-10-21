//! Integration tests for the `rbxaudit` binary: invoked as a real subprocess
//! so exit codes and stdout/stderr are covered.

use std::path::Path;
use std::process::{Command, Output};

use rbx_dom::{Variant, WeakDom};

const UNVALIDATED: &str =
    "game.ReplicatedStorage.R.OnServerEvent:Connect(function(p, n) p.leaderstats.Coins.Value = n end)";

/// A place with one server script; `remote` adds ReplicatedStorage.R.
fn place_with_script(source: &str, remote: bool) -> WeakDom {
    let mut dom = WeakDom::new();
    let storage = dom.new_instance("ReplicatedStorage", "ReplicatedStorage", None);
    if remote {
        dom.new_instance("RemoteEvent", "R", Some(storage));
    }
    let service = dom.new_instance("ServerScriptService", "ServerScriptService", None);
    let script = dom.new_instance("Script", "Script", Some(service));
    dom.set_property(script, "Source", Variant::String(source.to_string()))
        .expect("set Source");
    dom
}

fn audit(dom: &WeakDom) -> Output {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("place.rbxlx");
    std::fs::write(&path, rbx_xml::serialize(dom).expect("serialize")).expect("write place");
    run(&path)
}

fn run(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rbxaudit"))
        .arg(path)
        .output()
        .expect("spawn rbxaudit")
}

#[test]
fn a_high_finding_exits_one() {
    let output = audit(&place_with_script(UNVALIDATED, true));

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with("high") && l.contains("unvalidated-argument")),
        "stdout: {stdout}"
    );
}

#[test]
fn a_clean_place_exits_zero() {
    let output = audit(&place_with_script("print(1)", false));

    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("no findings"));
}

#[test]
fn a_missing_file_exits_two() {
    let dir = tempfile::tempdir().expect("temp dir");
    let output = run(&dir.path().join("missing.rbxlx"));

    assert_eq!(output.status.code(), Some(2));
    assert!(!output.stderr.is_empty());
}
