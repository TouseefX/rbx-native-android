//! Hold-out precision/recall measurement for the audit engine.
//!
//! The scripts under `tests/holdout/` were written from the check descriptions
//! alone, each flagged line carrying a trailing `-- expect: <check-id>`. The
//! test reports how the engine's findings line up with those marks; it measures
//! and only asserts a loose floor, so it must not be tuned to pass.

use std::collections::BTreeSet;

use rbx_dom::{Ref, Variant, WeakDom};

/// (file stem, is a LocalScript, source)
const SCRIPTS: &[(&str, bool, &str)] = &[
    ("Shop", false, include_str!("holdout/Shop.server.lua")),
    ("Combat", false, include_str!("holdout/Combat.server.lua")),
    ("Trading", false, include_str!("holdout/Trading.server.lua")),
    ("Pets", false, include_str!("holdout/Pets.server.lua")),
    ("Admin", false, include_str!("holdout/Admin.server.lua")),
    (
        "UiController",
        true,
        include_str!("holdout/UiController.client.lua"),
    ),
    (
        "Inventory",
        true,
        include_str!("holdout/Inventory.client.lua"),
    ),
];

const REMOTE_EVENTS: &[&str] = &[
    "BuyItem",
    "SellItem",
    "Donate",
    "Tip",
    "BuyProduct",
    "Redeem",
    "Bonus",
    "Refund",
    "Gift",
    "Notify",
    "Damage",
    "Attack",
    "Heal",
    "Haste",
    "Dash",
    "Blink",
    "Sprint",
    "TradeRequest",
    "TradeCancel",
    "GiftPet",
    "StealPet",
    "TradeNotice",
    "Splash",
    "EquipPet",
    "DeletePet",
    "ReleasePet",
    "ReparentPet",
    "SpawnItem",
    "DropItem",
    "FeedPet",
    "RunCode",
    "LoadModule",
    "Kick",
    "AddStat",
    "Ban",
    "Announce",
    "SaveNote",
    "MoveItem",
];

const REMOTE_FUNCTIONS: &[&str] = &["GetPrice", "Ping", "Confirm"];

/// Place-level findings a reviewer expects: clients fire these remotes but
/// no server script listens.
const PLACE_EXPECTED: &[(&str, &str)] = &[
    ("ReplicatedStorage.Remotes.SaveNote", "no-server-listener"),
    ("ReplicatedStorage.Remotes.MoveItem", "no-server-listener"),
];

fn child(dom: &mut WeakDom, parent: Ref, class: &str, name: &str) -> Ref {
    dom.new_instance(class, name, Some(parent))
}

fn script(dom: &mut WeakDom, parent: Ref, class: &str, name: &str, source: &str) {
    let r = child(dom, parent, class, name);
    dom.set_property(r, "Source", Variant::String(source.to_string()))
        .unwrap();
}

fn build_place() -> WeakDom {
    let mut dom = WeakDom::new();
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    let _ = workspace;
    let replicated = dom.new_instance("ReplicatedStorage", "ReplicatedStorage", None);
    let sss = dom.new_instance("ServerScriptService", "ServerScriptService", None);
    dom.new_instance("ServerStorage", "ServerStorage", None);
    let starter = dom.new_instance("StarterPlayer", "StarterPlayer", None);
    let player_scripts = child(&mut dom, starter, "StarterPlayerScripts", "StarterPlayerScripts");

    let remotes = child(&mut dom, replicated, "Folder", "Remotes");
    for name in REMOTE_EVENTS {
        child(&mut dom, remotes, "RemoteEvent", name);
    }
    for name in REMOTE_FUNCTIONS {
        child(&mut dom, remotes, "RemoteFunction", name);
    }

    let shared = child(&mut dom, replicated, "Folder", "Shared");
    script(
        &mut dom,
        shared,
        "ModuleScript",
        "Util",
        "local Util = {}\nfunction Util.log(...)\n\tprint(...)\nend\nreturn Util\n",
    );
    child(&mut dom, shared, "Folder", "Settings");
    child(&mut dom, replicated, "Folder", "Items");

    for (name, local, source) in SCRIPTS {
        if *local {
            script(&mut dom, player_scripts, "LocalScript", name, source);
        } else {
            script(&mut dom, sss, "Script", name, source);
        }
    }
    dom
}

fn expected() -> BTreeSet<(String, usize, String)> {
    let mut set = BTreeSet::new();
    for (name, local, source) in SCRIPTS {
        let path = if *local {
            format!("StarterPlayer.StarterPlayerScripts.{name}")
        } else {
            format!("ServerScriptService.{name}")
        };
        for (i, line) in source.lines().enumerate() {
            if let Some((_, id)) = line.split_once("-- expect: ") {
                set.insert((path.clone(), i + 1, id.trim().to_string()));
            }
        }
    }
    for (path, id) in PLACE_EXPECTED {
        set.insert((path.to_string(), 0, id.to_string()));
    }
    set
}

#[test]
fn holdout_precision_and_recall() {
    let dom = build_place();
    let findings = rbx_audit::audit(&dom);

    let expect = expected();
    let mut found = BTreeSet::new();
    let mut messages = std::collections::BTreeMap::new();
    for f in &findings {
        let key = (
            f.path.clone(),
            f.line.unwrap_or(0),
            f.check.id().to_string(),
        );
        messages.insert(key.clone(), f.message.clone());
        found.insert(key);
    }

    let tp = expect.intersection(&found).count();
    let fp: Vec<_> = found.difference(&expect).collect();
    let fn_: Vec<_> = expect.difference(&found).collect();
    let precision = if found.is_empty() {
        1.0
    } else {
        tp as f64 / found.len() as f64
    };
    let recall = if expect.is_empty() {
        1.0
    } else {
        tp as f64 / expect.len() as f64
    };

    println!(
        "holdout: {} expected, {} found, {} TP, {} FP, {} FN, precision {:.0}%, recall {:.0}%",
        expect.len(),
        found.len(),
        tp,
        fp.len(),
        fn_.len(),
        precision * 100.0,
        recall * 100.0
    );
    for key in &fp {
        println!("  FP {}:{} [{}] {}", key.0, key.1, key.2, messages[*key]);
    }
    for key in &fn_ {
        println!("  FN {}:{} [{}]", key.0, key.1, key.2);
    }

    assert!(precision >= 0.5, "precision {precision:.2} below 0.5");
    assert!(recall >= 0.5, "recall {recall:.2} below 0.5");
}
