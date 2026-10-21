//! The audit over a fixture place whose scripts mark every line that should
//! be flagged with `-- expect: <check-id>`; every other line is a guarded or
//! benign case that must not be. Precision and recall are printed (run with
//! `--nocapture`) and must both be 100%.

use std::collections::BTreeSet;

use rbx_audit::{audit, Check, Severity};
use rbx_dom::{CFrameData, Ref, Variant, Vector3Data, WeakDom};

const SCRIPTS: [(&str, &str, &str, &str); 6] = [
    (
        "ServerScriptService",
        "Script",
        "Shop",
        include_str!("fixtures/Shop.server.luau"),
    ),
    (
        "ServerScriptService",
        "Script",
        "Combat",
        include_str!("fixtures/Combat.server.luau"),
    ),
    (
        "ServerScriptService",
        "Script",
        "Admin",
        include_str!("fixtures/Admin.server.luau"),
    ),
    (
        "ServerScriptService",
        "Script",
        "Data",
        include_str!("fixtures/Data.server.luau"),
    ),
    (
        "ServerScriptService",
        "Script",
        "Confused",
        include_str!("fixtures/Confused.server.luau"),
    ),
    (
        "StarterPlayer.StarterPlayerScripts",
        "LocalScript",
        "Client",
        include_str!("fixtures/Client.client.luau"),
    ),
];

const REMOTE_EVENTS: [&str; 30] = [
    "AddCoins",
    "BuyItem",
    "Donate",
    "SetTitle",
    "SetBio",
    "Prompt",
    "PromptSafe",
    "Notify",
    "Damage",
    "Hit",
    "Teleport",
    "TeleportSafe",
    "Drop",
    "DropSafe",
    "Equip",
    "AdminCommand",
    "LoadModule",
    "Spawn",
    "RunCommand",
    "SpawnSafe",
    "SaveData",
    "SaveSettings",
    "SaveLevel",
    "Gift",
    "Vote",
    "Score",
    "Ping",
    "Feedback",
    "Orphan",
    "Trade",
];
const REMOTE_FUNCTIONS: [&str; 3] = ["GetPrice", "GetSettings", "ClientOnly"];

/// Findings about a remote rather than a line.
const PLACE_EXPECTED: [(&str, Check, Severity); 4] = [
    (
        "ReplicatedStorage.Remotes.ClientOnly",
        Check::NoServerListener,
        Severity::High,
    ),
    (
        "ReplicatedStorage.Remotes.Feedback",
        Check::NoServerListener,
        Severity::Medium,
    ),
    (
        "ReplicatedStorage.Remotes.Orphan",
        Check::NoServerListener,
        Severity::Low,
    ),
    (
        "ReplicatedStorage.Remotes.Trade",
        Check::NoServerListener,
        Severity::Low,
    ),
];

fn add(dom: &mut WeakDom, class: &str, name: &str, parent: Option<Ref>) -> Ref {
    dom.new_instance(class, name, parent)
}

/// Finds or makes the folder-like path `a.b.c` from a service down.
fn path(dom: &mut WeakDom, path: &str) -> Ref {
    let mut parts = path.split('.');
    let service = parts.next().unwrap();
    let mut at = dom
        .root_refs()
        .iter()
        .copied()
        .find(|r| dom.get(*r).unwrap().name() == service)
        .unwrap_or_else(|| add(dom, service, service, None));
    for part in parts {
        let existing = dom
            .get(at)
            .unwrap()
            .children()
            .iter()
            .copied()
            .find(|c| dom.get(*c).unwrap().name() == part);
        at = existing.unwrap_or_else(|| add(dom, part, part, Some(at)));
    }
    at
}

pub fn fixture() -> WeakDom {
    let mut dom = WeakDom::new();
    for service in [
        "Workspace",
        "Players",
        "ServerScriptService",
        "ServerStorage",
    ] {
        add(&mut dom, service, service, None);
    }
    let storage = add(&mut dom, "ReplicatedStorage", "ReplicatedStorage", None);
    let remotes = add(&mut dom, "Folder", "Remotes", Some(storage));
    for name in REMOTE_EVENTS {
        add(&mut dom, "RemoteEvent", name, Some(remotes));
    }
    for name in REMOTE_FUNCTIONS {
        add(&mut dom, "RemoteFunction", name, Some(remotes));
    }
    let shared = add(&mut dom, "ModuleScript", "Shared", Some(storage));
    let _ = dom.set_property(shared, "Source", Variant::String("return {}\n".into()));

    let server = path(&mut dom, "ServerScriptService");
    let commands = add(&mut dom, "ModuleScript", "Commands", Some(server));
    let _ = dom.set_property(commands, "Source", Variant::String("return {}\n".into()));
    add(&mut dom, "Folder", "Config", Some(server));
    let lib = path(&mut dom, "ServerStorage");
    add(&mut dom, "Folder", "Lib", Some(lib));
    let player = path(&mut dom, "StarterPlayer");
    add(
        &mut dom,
        "StarterPlayerScripts",
        "StarterPlayerScripts",
        Some(player),
    );

    for (parent, class, name, source) in SCRIPTS {
        let parent = path(&mut dom, parent);
        let script = add(&mut dom, class, name, Some(parent));
        let _ = dom.set_property(script, "Source", Variant::String(source.into()));
    }
    dom
}

#[test]
fn the_fixture_place_is_flagged_exactly_where_marked() {
    let mut expected = BTreeSet::new();
    for (parent, _, name, source) in SCRIPTS {
        for (index, line) in source.lines().enumerate() {
            if let Some((_, id)) = line.split_once("-- expect: ") {
                expected.insert((
                    format!("{parent}.{name}"),
                    Some(index + 1),
                    id.trim().to_string(),
                ));
            }
        }
    }
    for (path, check, _) in PLACE_EXPECTED {
        expected.insert((path.to_string(), None, check.id().to_string()));
    }

    let findings = audit(&fixture());
    let found: BTreeSet<_> = findings
        .iter()
        .map(|f| (f.path.clone(), f.line, f.check.id().to_string()))
        .collect();

    let true_positives = found.intersection(&expected).count();
    let false_positives: Vec<_> = found.difference(&expected).collect();
    let false_negatives: Vec<_> = expected.difference(&found).collect();
    let precision = true_positives as f64 / found.len() as f64;
    let recall = true_positives as f64 / expected.len() as f64;
    println!(
        "fixtures: {} expected, {} found, {true_positives} true positives, {} false positives, {} false negatives, precision {:.1}%, recall {:.1}%",
        expected.len(),
        found.len(),
        false_positives.len(),
        false_negatives.len(),
        precision * 100.,
        recall * 100.,
    );
    for check in [
        Check::UnvalidatedArgument,
        Check::PlayerArgument,
        Check::NoServerListener,
        Check::MissingRemote,
        Check::BadRequire,
        Check::ServerApiOnClient,
        Check::InvokeClient,
    ] {
        let hits = found.iter().filter(|f| f.2 == check.id()).count();
        let right = found
            .intersection(&expected)
            .filter(|f| f.2 == check.id())
            .count();
        println!("  {:<22} {right}/{hits} correct", check.id());
    }
    assert!(
        false_positives.is_empty(),
        "false positives: {false_positives:#?}"
    );
    assert!(false_negatives.is_empty(), "missed: {false_negatives:#?}");
    // Each finding is reported once, even for a named handler walked twice.
    assert_eq!(findings.len(), found.len());
}

#[test]
fn place_findings_carry_their_severity() {
    let findings = audit(&fixture());
    for (path, check, severity) in PLACE_EXPECTED {
        let finding = findings
            .iter()
            .find(|f| f.path == path && f.check == check)
            .unwrap();
        assert_eq!(finding.severity, severity, "{path}");
        assert_eq!(finding.line, None);
    }
    // Highest severity first.
    assert!(findings.windows(2).all(|w| w[0].severity <= w[1].severity));
}

#[test]
fn an_unresolved_listener_lowers_no_listener_findings() {
    let mut dom = WeakDom::new();
    let storage = add(&mut dom, "ReplicatedStorage", "ReplicatedStorage", None);
    let remote = add(&mut dom, "RemoteEvent", "Fire", Some(storage));
    let server = add(&mut dom, "ServerScriptService", "ServerScriptService", None);
    let script = add(&mut dom, "Script", "Wire", Some(server));
    let source = "for _, r in game.ReplicatedStorage:GetChildren() do\n\tr.OnServerEvent:Connect(print)\nend\n";
    let _ = dom.set_property(script, "Source", Variant::String(source.into()));
    let client = add(&mut dom, "LocalScript", "Caller", Some(storage));
    let call = "game.ReplicatedStorage.Fire:FireServer()\n";
    let _ = dom.set_property(client, "Source", Variant::String(call.into()));

    let findings = audit(&dom);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].instance, remote);
    assert_eq!(findings[0].severity, Severity::Low);
}

/// `RBX_AUDIT_FIXTURE_OUT=path.rbxlx cargo test -p rbx_audit -- --ignored`
/// writes the fixture place, for `rbxaudit` and the editor's dock.
#[test]
#[ignore]
fn write_fixture_place() {
    let out = std::env::var("RBX_AUDIT_FIXTURE_OUT").expect("RBX_AUDIT_FIXTURE_OUT");
    let mut dom = fixture();
    // Something for the viewport to draw, or the editor won't open it.
    let workspace = path(&mut dom, "Workspace");
    let baseplate = add(&mut dom, "Part", "Baseplate", Some(workspace));
    let v = |x, y, z| Vector3Data { x, y, z };
    let _ = dom.set_property(baseplate, "size", Variant::Vector3(v(64., 1., 64.)));
    let cframe = CFrameData {
        position: v(0., -0.5, 0.),
        rotation: [1., 0., 0., 0., 1., 0., 0., 0., 1.],
    };
    let _ = dom.set_property(baseplate, "CFrame", Variant::CFrame(cframe));
    std::fs::write(out, rbx_xml::serialize(&dom).unwrap()).unwrap();
}
