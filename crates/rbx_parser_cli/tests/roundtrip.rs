//! Integration tests for `rbxdump --roundtrip`: invoked as a real subprocess so
//! exit codes and stdout/stderr are covered, not just the library glue.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/tests")
        .join(name)
}

fn run_roundtrip(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rbxdump"))
        .arg("--roundtrip")
        .arg(path)
        .output()
        .expect("spawn rbxdump")
}

#[test]
fn test_place_round_trips_clean_through_both_formats() {
    let output = run_roundtrip(&fixture("TestPlace.rbxl"));
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("binary round-trip"), "stdout: {stdout}");
    assert!(stdout.contains("xml round-trip"), "stdout: {stdout}");
    assert!(!stdout.contains("FAIL"), "stdout: {stdout}");
}

// FPS.rbxm is a model file (a ModuleScript subtree), not a place, but neither
// serializer is place-specific (no DataModel/service requirement in either
// `rbx_binary::serialize` or `rbx_xml::serialize`), so it is a valid input here
// too and exercises a differently-shaped tree than TestPlace.rbxl.
#[test]
fn fps_model_round_trips_clean_through_both_formats() {
    let output = run_roundtrip(&fixture("FPS.rbxm"));
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!stdout.contains("FAIL"), "stdout: {stdout}");
}

// A byte-truncated file can't even be deserialized once, so it never reaches
// the round-trip logic: this exercises the load_dom error path (exit code 2),
// not a round-trip FAIL (exit code 1), and above all must not panic.
#[test]
fn truncated_file_reports_a_clean_read_error_not_a_panic() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("truncated.rbxl");
    let original = std::fs::read(fixture("TestPlace.rbxl")).expect("read fixture");
    std::fs::write(&path, &original[..200]).expect("write truncated copy");

    let output = run_roundtrip(&path);

    assert_eq!(
        output.status.code(),
        Some(2),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.starts_with(b"error: "),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // A panic would print "thread 'main' panicked" to stderr instead of the
    // clean `error: ...` message asserted above; belt-and-braces check.
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
}

#[test]
fn missing_path_argument_exits_with_usage_code() {
    let output = Command::new(env!("CARGO_BIN_EXE_rbxdump"))
        .arg("--roundtrip")
        .output()
        .expect("spawn rbxdump");

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
}

// The editor saves binary files through `serialize_with_defaults`, filling the
// column a part lacks with the class default. A file written that way has to
// come back through `--roundtrip` clean.
#[test]
fn editor_saved_file_round_trips_clean() {
    use rbx_dom::{Variant, WeakDom};

    let mut dom = WeakDom::new();
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    dom.new_instance("Part", "Plain", Some(workspace));
    let ghost = dom.new_instance("Part", "Ghost", Some(workspace));
    dom.get_mut(ghost)
        .unwrap()
        .properties_mut()
        .insert("CanCollide".into(), Variant::Bool(false));

    let database = rbx_parser_cli::default_reflection_database();
    let bytes = rbx_binary::serialize_with_defaults(&dom, |class, key| {
        database.stored_default(class, key).cloned()
    })
    .expect("editor-style save");
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("editor.rbxl");
    std::fs::write(&path, bytes).expect("write");

    let output = run_roundtrip(&path);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "stdout: {stdout}");
    assert!(!stdout.contains("FAIL"), "stdout: {stdout}");
}

// An editor-saved .rbxlx leaves a column out of the parts that don't hold it. The
// binary check has to fill that gap the way the editor's own save does (class
// default), not with a zero, or `CanCollide` reads back `false` and it FAILs.
#[test]
fn sparse_editor_xml_round_trips_clean_through_the_binary_check() {
    use rbx_dom::{Variant, WeakDom};

    let mut dom = WeakDom::new();
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    dom.new_instance("Part", "Plain", Some(workspace));
    let ghost = dom.new_instance("Part", "Ghost", Some(workspace));
    dom.get_mut(ghost)
        .unwrap()
        .properties_mut()
        .insert("CanCollide".into(), Variant::Bool(false));

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("editor.rbxlx");
    std::fs::write(&path, rbx_xml::serialize(&dom).expect("xml save")).expect("write");

    let output = run_roundtrip(&path);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "stdout: {stdout}");
    assert!(!stdout.contains("FAIL"), "stdout: {stdout}");
}
