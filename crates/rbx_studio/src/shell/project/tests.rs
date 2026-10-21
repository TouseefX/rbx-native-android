use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rbx_project_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn child(dom: &WeakDom, parent: Ref, name: &str) -> Ref {
    dom.get(parent)
        .unwrap()
        .children()
        .iter()
        .copied()
        .find(|&c| dom.get(c).unwrap().name() == name)
        .unwrap_or_else(|| panic!("no {name}"))
}

fn root(dom: &WeakDom, class: &str) -> Ref {
    dom.root_refs()
        .iter()
        .copied()
        .find(|&r| dom.get(r).unwrap().class() == class)
        .unwrap()
}

#[test]
fn a_rojo_style_project_imports_with_its_file_conventions() {
    let dir = scratch("import");
    std::fs::create_dir_all(dir.join("src/server/Util")).unwrap();
    std::fs::create_dir_all(dir.join("src/shared")).unwrap();
    std::fs::write(dir.join("src/server/main.server.luau"), "print(1)").unwrap();
    std::fs::write(dir.join("src/server/Util/init.luau"), "return {}").unwrap();
    std::fs::write(dir.join("src/server/Util/Child.lua"), "return 2").unwrap();
    std::fs::write(dir.join("src/shared/hello.client.luau"), "print(2)").unwrap();
    std::fs::write(dir.join("src/shared/Note.txt"), "hi").unwrap();
    std::fs::write(
        dir.join("src/shared/Box.model.json"),
        r#"{"ClassName":"Part","Properties":{"Anchored":true,"Size":[4,1,2]}}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("default.project.json"),
        r#"{"name":"Game","tree":{"$className":"DataModel",
            "ServerScriptService":{"$path":"src/server"},
            "ReplicatedStorage":{"$className":"ReplicatedStorage","$path":"src/shared"},
            "Lighting":{"$className":"Lighting","$properties":{"Brightness":3,"Technology":"ShadowMap"}}}}"#,
    )
    .unwrap();

    let dom = import(&dir.join("default.project.json")).unwrap();

    let server = root(&dom, "ServerScriptService");
    let main = child(&dom, server, "main");
    assert_eq!(dom.get(main).unwrap().class(), "Script");
    let util = child(&dom, server, "Util");
    assert_eq!(dom.get(util).unwrap().class(), "ModuleScript");
    assert_eq!(dom.get(child(&dom, util, "Child")).unwrap().class(), "ModuleScript");
    let shared = root(&dom, "ReplicatedStorage");
    assert_eq!(dom.get(child(&dom, shared, "hello")).unwrap().class(), "LocalScript");
    assert_eq!(
        dom.get(child(&dom, shared, "Note")).unwrap().properties().get("Value"),
        Some(&Variant::String("hi".into()))
    );
    let part = dom.get(child(&dom, shared, "Box")).unwrap();
    assert_eq!(part.class(), "Part");
    assert_eq!(part.properties().get("Anchored"), Some(&Variant::Bool(true)));
    let lighting = dom.get(root(&dom, "Lighting")).unwrap().properties();
    assert_eq!(lighting.get("Brightness"), Some(&Variant::Float32(3.0)));
    assert_eq!(lighting.get("Technology"), Some(&Variant::Enum(3)));
}

#[test]
fn an_exported_place_imports_back_to_the_same_tree() {
    let mut dom = WeakDom::new();
    let sss = dom.new_instance("ServerScriptService", "ServerScriptService", None);
    let script = dom.new_instance("Script", "Main", Some(sss));
    dom.set_property(script, "Source", Variant::String("print('x')".into())).unwrap();
    let module = dom.new_instance("ModuleScript", "Lib", Some(sss));
    dom.new_instance("ModuleScript", "Inner", Some(module));
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    let part = dom.new_instance("Part", "Wall/1", Some(workspace));
    dom.set_property(part, "Anchored", Variant::Bool(true)).unwrap();
    dom.new_instance("Folder", "Empty", Some(workspace));
    dom.new_instance("Folder", "empty", Some(workspace));

    let dir = scratch("export");
    let project = export(&dom, &dir, "Round").unwrap();
    assert!(dir.join("src/ServerScriptService/Main.server.luau").exists());
    assert!(dir.join("src/ServerScriptService/Lib/init.luau").exists());
    assert!(dir.join("src/Workspace/Wall_1.rbxm").exists());

    let back = import(&project).unwrap();
    let sss = root(&back, "ServerScriptService");
    assert_eq!(
        back.get(child(&back, sss, "Main")).unwrap().properties().get("Source"),
        Some(&Variant::String("print('x')".into()))
    );
    let lib = child(&back, sss, "Lib");
    assert_eq!(back.get(child(&back, lib, "Inner")).unwrap().class(), "ModuleScript");
    let workspace = root(&back, "Workspace");
    // The file name is the name, as in Rojo: `/` can't be in one.
    let wall = back.get(child(&back, workspace, "Wall_1")).unwrap();
    assert_eq!(wall.properties().get("Anchored"), Some(&Variant::Bool(true)));
    // Two folders whose names differ only in case both survive.
    assert_eq!(back.get(workspace).unwrap().children().len(), 3);
}

#[test]
fn exported_models_carry_tags_as_a_string_argon_can_read() {
    let mut dom = WeakDom::new();
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    let part = dom.new_instance("Part", "Tagged", Some(workspace));
    dom.set_property(
        part,
        "Tags",
        Variant::Unknown {
            type_id: 0x1C,
            raw: b"a\0b".to_vec(),
        },
    )
    .unwrap();

    let dir = scratch("tags");
    export(&dom, &dir, "Tags").unwrap();
    let bytes = std::fs::read(dir.join("src/Workspace/Tagged.rbxm")).unwrap();
    let model = rbx_binary::deserialize(&bytes).unwrap();
    let tags = model.get(model.root_refs()[0]).unwrap().properties().get("Tags").cloned();

    assert!(
        !matches!(tags, Some(Variant::Unknown { type_id: 0x1C, .. })),
        "{tags:?}"
    );
}
