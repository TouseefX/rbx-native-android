//! Every CLI prints its usage on stdout and exits 0 for `--help` / `-h`.

use std::process::Command;

fn check(exe: &str, name: &str) {
    for flag in ["--help", "-h"] {
        let output = Command::new(exe).arg(flag).output().expect("run binary");
        assert!(
            output.status.success(),
            "{name} {flag}: {:?}",
            output.status
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("usage:"), "{name} {flag}: {stdout}");
        assert!(output.stderr.is_empty(), "{name} {flag} wrote to stderr");
    }
}

#[test]
fn rbxdump_help() {
    check(env!("CARGO_BIN_EXE_rbxdump"), "rbxdump");
}

#[test]
fn rbxlua_help() {
    check(env!("CARGO_BIN_EXE_rbxlua"), "rbxlua");
}

#[test]
fn rbxmerge_help() {
    check(env!("CARGO_BIN_EXE_rbxmerge"), "rbxmerge");
}

#[test]
fn rbxaudit_help() {
    check(env!("CARGO_BIN_EXE_rbxaudit"), "rbxaudit");
}
