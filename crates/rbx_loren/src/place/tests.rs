use super::*;

fn project_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/loren/project")
}

fn at(dom: &WeakDom, start: Ref, path: &[&str]) -> Ref {
    let mut cur = start;
    for part in path {
        cur = find_child(dom, cur, part).unwrap_or_else(|| panic!("missing {part}"));
    }
    cur
}

fn built() -> WeakDom {
    let mut dom = WeakDom::new();
    apply_project(&mut dom, &project_dir()).unwrap();
    dom
}

fn service(dom: &WeakDom, name: &str) -> Ref {
    dom.root_refs()
        .iter()
        .copied()
        .find(|r| dom.get(*r).unwrap().class() == name)
        .unwrap()
}

fn class(dom: &WeakDom, r: Ref) -> String {
    dom.get(r).unwrap().class().to_string()
}

#[test]
fn builds_runtime_and_packages() {
    let dom = built();
    let rs = service(&dom, "ReplicatedStorage");
    let runtime = at(&dom, rs, &["LorenRuntime"]);
    assert_eq!(class(&dom, runtime), "ModuleScript");
    at(&dom, runtime, &["Codec"]);
    let promise = at(&dom, rs, &["LorenPackages", "Promise"]);
    assert_eq!(class(&dom, promise), "ModuleScript");
}

#[test]
fn builds_server_and_client() {
    let dom = built();
    let server = at(
        &dom,
        service(&dom, "ServerScriptService"),
        &["Server", "Server"],
    );
    assert_eq!(class(&dom, server), "Script");
    let client = at(
        &dom,
        service(&dom, "StarterPlayer"),
        &["StarterPlayerScripts", "Client", "Client"],
    );
    assert_eq!(class(&dom, client), "LocalScript");
}

#[test]
fn source_matches_file() {
    let dom = built();
    let example = at(
        &dom,
        service(&dom, "ServerScriptService"),
        &["Server", "Services", "ExampleService"],
    );
    let expected = std::fs::read_dir(project_dir().join("src/server/Services"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_stem()
                .is_some_and(|s| s.to_string_lossy().starts_with("ExampleService"))
        })
        .unwrap();
    let text = std::fs::read_to_string(expected).unwrap();
    assert_eq!(
        dom.get(example).unwrap().properties().get("Source"),
        Some(&Variant::String(text))
    );
}

#[test]
fn reuses_existing_services() {
    let mut dom = WeakDom::new();
    let existing = dom.new_instance("ReplicatedStorage", "ReplicatedStorage", None);
    apply_project(&mut dom, &project_dir()).unwrap();
    let count = dom
        .root_refs()
        .iter()
        .filter(|r| dom.get(**r).unwrap().class() == "ReplicatedStorage")
        .count();
    assert_eq!(count, 1);
    assert_eq!(service(&dom, "ReplicatedStorage"), existing);
}
