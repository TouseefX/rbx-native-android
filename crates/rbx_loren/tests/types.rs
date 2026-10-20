mod common;

use std::fs;
use std::path::Path;

use rbx_loren::ctx::{Names, TypesOptions};
use rbx_loren::fsutil::{rel, write_text};
use rbx_loren::lint::{analyze_luau, is_networked};
use rbx_loren::types::{collect_names, generate_types, list_modules, shared_source};
use regex::Regex;

// Left out: the luau-lsp type-check test (needs an installed luau-lsp and its definitions file).

fn golden(name: &str) -> String {
    common::read(&common::fixtures_dir().join("golden").join(name)).replace("\r\n", "\n")
}

fn networked(src: &str) -> bool {
    is_networked(&analyze_luau(src, None))
}

fn w(dir: &Path, rel_path: &str, text: &str) {
    write_text(&dir.join(rel_path), text).expect("write");
}

fn matches(pattern: &str, text: &str) -> bool {
    Regex::new(&format!("(?m){pattern}"))
        .expect("regex")
        .is_match(text)
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn golden_output_for_the_151_fixture_shared_and_server_files() {
    let p = common::project(Some("v151"));
    let dir = p.path();
    let res = generate_types(dir, &TypesOptions::default()).unwrap();
    let written: Vec<String> = res.written.iter().map(|f| rel(dir, f)).collect();
    assert_eq!(
        written,
        [
            "src/shared/LorenTypes.luau",
            "src/server/LorenServerTypes.luau"
        ]
    );
    assert_eq!(
        common::read(&dir.join("src/shared/LorenTypes.luau")),
        golden("v151.LorenTypes.luau")
    );
    assert_eq!(
        common::read(&dir.join("src/server/LorenServerTypes.luau")),
        golden("v151.LorenServerTypes.luau")
    );
    assert_eq!(
        res.names,
        Names {
            services: strs(&["DataService", "Inventory", "ShopService", "TradeService"]),
            controllers: strs(&["HUD", "UIController"]),
            networked: strs(&["Inventory", "ShopService", "TradeService"]),
        }
    );
}

#[test]
fn both_files_end_with_return_table_freeze_and_never_return_nil_dx_01() {
    for name in [Some("v151"), Some("v2"), None] {
        let p = common::project(name);
        let dir = p.path();
        fs::create_dir_all(dir.join("src/server")).unwrap();
        generate_types(dir, &TypesOptions::default()).unwrap();
        for f in [
            "src/shared/LorenTypes.luau",
            "src/server/LorenServerTypes.luau",
        ] {
            let text = common::read(&dir.join(f));
            assert!(
                text.ends_with("return table.freeze({})\n"),
                "{f} ends with the frozen table"
            );
            assert!(!text.contains("return nil"), "{f} never returns nil");
            assert!(
                text.starts_with("--!strict\n-- AUTO-GENERATED"),
                "{f} has the header"
            );
            assert!(!matches(r"\brequire\s*\(", &text), "{f} requires nothing");
        }
    }
}

#[test]
fn empty_projects_fall_back_to_string_and_string_any() {
    let p = common::project(None);
    let dir = p.path();
    fs::create_dir_all(dir.join("src/server")).unwrap();
    generate_types(dir, &TypesOptions::default()).unwrap();
    let shared = common::read(&dir.join("src/shared/LorenTypes.luau"));
    assert!(matches(
        r"^export type ServiceName = string -- none yet$",
        &shared
    ));
    assert!(matches(
        r"^export type ControllerName = string -- none yet$",
        &shared
    ));
    assert!(matches(
        r"^export type NetworkedServiceName = string -- none yet$",
        &shared
    ));
    assert!(matches(
        r"^export type ClientDependencyName = ControllerName \| NetworkedServiceName$",
        &shared
    ));
    assert!(matches(
        r"^export type ClientDeps = \{ \[string\]: any \} -- none yet$",
        &shared
    ));
    assert!(matches(
        r"^export type ServerDeps = \{ \[string\]: any \} -- none yet$",
        &shared
    ));
    let server = common::read(&dir.join("src/server/LorenServerTypes.luau"));
    assert!(matches(
        r"^export type ServerDeps = \{ \[string\]: any \} -- none yet$",
        &server
    ));
}

#[test]
fn client_deps_networked_services_are_service_proxy_controllers_any_clashes_flagged() {
    let p = common::project(None);
    let dir = p.path();
    w(
        dir,
        "src/server/Services/Chat.luau",
        "local Chat = { ClientEvents = { \"Say\" } }\nreturn Chat\n",
    );
    w(
        dir,
        "src/server/Services/Hidden.luau",
        "return { Client = {} }\n",
    );
    w(
        dir,
        "src/server/Services/Map.luau",
        "local Map = {}\nfunction Map.Client:Get(p) end\nreturn Map\n",
    );
    w(dir, "src/client/Controllers/Map.luau", "return {}\n");
    w(dir, "src/client/Controllers/Camera.luau", "return {}\n");
    generate_types(dir, &TypesOptions::default()).unwrap();
    let shared = common::read(&dir.join("src/shared/LorenTypes.luau"));
    assert!(shared.contains("export type NetworkedServiceName = \"Chat\" | \"Map\"\n"));
    assert!(shared.contains(
        "export type ClientDeps = {\n\tCamera: any,\n\tChat: ServiceProxy,\n\tMap: any, -- a Controller and a networked Service: Loren refuses this at boot, rename one\n}"
    ));
    assert!(
        shared.contains("export type ServerDeps = {\n\tChat: any,\n\tHidden: any,\n\tMap: any,\n}")
    );
}

#[test]
fn names_that_are_not_identifiers_become_quoted_keys_long_unions_wrap() {
    let p = common::project(None);
    let dir = p.path();
    w(
        dir,
        "src/server/Services/My-Odd.luau",
        "return { Signals = { \"A\" } }\n",
    );
    w(dir, "src/server/Services/end.luau", "return {}\n");
    for i in 0..8 {
        w(
            dir,
            &format!("src/server/Services/LongServiceName{i}.luau"),
            "return {}\n",
        );
    }
    fs::create_dir_all(dir.join("src/client")).unwrap();
    generate_types(dir, &TypesOptions::default()).unwrap();
    let server = common::read(&dir.join("src/server/LorenServerTypes.luau"));
    assert!(server.contains("\t[\"My-Odd\"]: any,\n"));
    assert!(server.contains("\t[\"end\"]: any,\n"));
    assert!(server
        .contains("export type ServiceName =\n\t\"LongServiceName0\"\n\t| \"LongServiceName1\"\n"));
}

#[test]
fn control_characters_in_names_become_3_digit_escapes_a_following_digit_stays_a_digit() {
    let names = Names {
        services: vec!["A\u{1}2".to_string()],
        ..Names::default()
    };
    let text = shared_source(&names);
    assert!(matches(r#"^export type ServiceName = "A\\0012"$"#, &text));
}

#[test]
fn list_modules_follows_the_runtime_direct_module_scripts_and_folder_modules_only() {
    let p = common::project(None);
    let dir = p.path();
    let s = "src/server/Services";
    w(dir, &format!("{s}/A.luau"), "return {}");
    w(dir, &format!("{s}/B.lua"), "return {}");
    w(dir, &format!("{s}/Folder/init.luau"), "return {}");
    w(dir, &format!("{s}/Folder/Child.luau"), "return {}");
    w(dir, &format!("{s}/LuaFolder/init.lua"), "return {}");
    w(dir, &format!("{s}/ArgonFolder/.src.luau"), "return {}");
    w(dir, &format!("{s}/PlainFolder/Nested.luau"), "return {}");
    w(
        dir,
        &format!("{s}/ScriptFolder/init.server.luau"),
        "print(1)",
    );
    w(dir, &format!("{s}/Boot.server.luau"), "print(1)");
    w(dir, &format!("{s}/Ui.client.lua"), "print(1)");
    w(dir, &format!("{s}/init.luau"), "return {}");
    w(dir, &format!("{s}/notes.txt"), "x");
    w(dir, &format!("{s}/Dup.luau"), "return {}");
    w(dir, &format!("{s}/Dup.lua"), "return {}");
    let mods = list_modules(&dir.join(s));
    let mut keys: Vec<&str> = mods.iter().map(|(n, _)| n.as_str()).collect();
    keys.sort();
    assert_eq!(
        keys,
        ["A", "ArgonFolder", "B", "Dup", "Folder", "LuaFolder"]
    );
    let folder = mods.iter().find(|(n, _)| n == "Folder").unwrap();
    assert_eq!(folder.1.file_name().unwrap(), "init.luau");
    assert_eq!(list_modules(&dir.join("missing")).len(), 0);
}

#[test]
fn networked_detection_matches_the_runtime_client_methods_signals_client_events_spec() {
    assert!(
        !networked("local S = { Client = {}, Signals = {}, Middleware = {} }\nreturn S"),
        "the scaffold template is not networked"
    );
    assert!(networked(
        "local S = { Client = { Ping = function() end } }\nreturn S"
    ));
    assert!(networked(
        "local S = { Client = {} }\nfunction S.Client:Ping(p) end\nreturn S"
    ));
    assert!(networked(
        "local S = { Client = {} }\nS.Client.Ping = function(self, p) end\nreturn S"
    ));
    assert!(networked(
        "local S = {}\nS.Signals = { \"Changed\" }\nreturn S"
    ));
    assert!(networked("return { ClientEvents = { \"Say\" } }"));
    assert!(networked("return { Spec = { Ping = Loren.Method() } }"));
    assert!(
        !networked("local S = { Client = { Try = function() end } }\nfunction S.Client:Signals() end\nreturn S"),
        "reserved names are not networked"
    );
    assert!(networked(
        "local S = { Client = { Try = function() end, Buy = function() end } }\nreturn S"
    ));
    assert!(!networked(
        "local S = { Signals = {} :: { string } }\nreturn S"
    ));
    assert!(!networked(
        "local S: { Client: any } = { Client = ({}) }\nreturn S"
    ));
    assert!(
        networked("local API = require(script.API)\nlocal S = { Client = API }\nreturn S"),
        "unknown counts as networked"
    );
    assert!(
        !networked("local S = { Client = {} }\n-- function S.Client:Ping(p) end\nreturn S"),
        "comments are ignored"
    );
    assert!(
        !networked(
            "local S = { Client = {} }\nlocal t = \"S.Client.Ping = function() end\"\nreturn S"
        ),
        "strings are ignored"
    );
    assert!(
        !networked(
            "local S = { Client = {} }\nlocal t = [[\nfunction S.Client:Ping() end\n]]\nreturn S"
        ),
        "long strings are ignored"
    );
    assert!(
        !networked("local Other = { Client = { X = function() end } }\nlocal S = {}\nreturn S"),
        "only the returned table"
    );
    assert!(!networked("local S = {}\nfunction S:Helper() local t = { Client = { X = function() end } } end\nreturn S"));
}

#[test]
fn idempotent_a_second_run_writes_nothing_dry_run_never_writes() {
    let p = common::project(Some("v151"));
    let dir = p.path();
    let dry = generate_types(
        dir,
        &TypesOptions {
            dry_run: true,
            ..TypesOptions::default()
        },
    )
    .unwrap();
    assert_eq!(dry.written.len(), 0);
    assert_eq!(dry.planned.len(), 2);
    assert!(
        common::read(&dir.join("src/shared/LorenTypes.luau")).contains("return nil"),
        "dry run left the old file"
    );
    generate_types(dir, &TypesOptions::default()).unwrap();
    let again = generate_types(dir, &TypesOptions::default()).unwrap();
    assert!(again.written.is_empty());
    assert!(again.planned.is_empty());
    assert_eq!(again.unchanged.len(), 2);
    let file = dir.join("src/shared/LorenTypes.luau");
    fs::write(&file, common::read(&file).replace('\n', "\r\n")).unwrap();
    assert!(generate_types(dir, &TypesOptions::default())
        .unwrap()
        .written
        .is_empty());
}

#[test]
fn custom_layout_folders_from_options_no_server_file_without_a_server_folder() {
    let p = common::project(None);
    let dir = p.path();
    w(
        dir,
        "game/server/Services/Alpha.luau",
        "return { Signals = { \"X\" } }",
    );
    w(dir, "game/client/Controllers/Beta.luau", "return {}");
    let opts = TypesOptions {
        shared_dir: Some("game/shared".into()),
        server_dir: Some("game/server".into()),
        client_dir: Some("game/client".into()),
        ..TypesOptions::default()
    };
    let res = generate_types(dir, &opts).unwrap();
    assert_eq!(res.written.len(), 2);
    assert!(common::read(&dir.join("game/shared/LorenTypes.luau"))
        .contains("ClientDependencyName = ControllerName | NetworkedServiceName"));
    assert_eq!(
        collect_names(dir, "game/shared", "game/server", "game/client", None),
        Names {
            services: strs(&["Alpha"]),
            controllers: strs(&["Beta"]),
            networked: strs(&["Alpha"])
        }
    );
    let bare = common::project(None);
    let only = generate_types(bare.path(), &TypesOptions::default()).unwrap();
    let names: Vec<String> = only
        .written
        .iter()
        .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["LorenTypes.luau"]);
}

#[test]
fn without_options_the_folders_come_from_default_project_json() {
    let p = common::project(None);
    let dir = p.path();
    w(
        dir,
        "default.project.json",
        r#"{"name":"x","tree":{"$className":"DataModel","ReplicatedStorage":{"Shared":{"$path":"code/shared"}},"ServerScriptService":{"Server":{"$path":{"optional":"code/server"}}},"StarterPlayer":{"StarterPlayerScripts":{"Client":{"$path":"code/client"}}}}}"#,
    );
    w(
        dir,
        "code/server/Services/Gamma.luau",
        "return { Signals = { \"X\" } }",
    );
    w(dir, "code/client/Controllers/Delta.luau", "return {}");
    let res = generate_types(dir, &TypesOptions::default()).unwrap();
    assert_eq!(
        res.names,
        Names {
            services: strs(&["Gamma"]),
            controllers: strs(&["Delta"]),
            networked: strs(&["Gamma"])
        }
    );
    assert!(dir.join("code/shared/LorenTypes.luau").exists());
    assert!(dir.join("code/server/LorenServerTypes.luau").exists());
    assert!(
        !dir.join("src").exists(),
        "nothing written to the default folders"
    );
}
