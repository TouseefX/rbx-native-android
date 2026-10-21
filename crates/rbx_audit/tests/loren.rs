//! The audit on a real Loren project: scaffolded from Loren's own template
//! the way the editor's Home does it, with a Service whose client routes
//! mark every line that should be flagged with `-- expect: <check-id>`.
//! Loren's runtime is in the place too, and must come out clean.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use rbx_audit::{audit, Severity};
use rbx_dom::{CFrameData, Variant, Vector3Data, WeakDom};

const SHOP: &str = include_str!("loren/ShopService.luau");

fn project() -> WeakDom {
    // Tests run in parallel: each call gets a folder of its own.
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("rbx-audit-loren-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let template = rbx_loren::template::template_dir().unwrap();
    rbx_loren::commands::init::write_scaffold(
        &template,
        &dir,
        "AuditGame",
        rbx_loren::ctx::Tool::Argon,
    )
    .unwrap();
    std::fs::write(dir.join("src/server/Services/ShopService.luau"), SHOP).unwrap();
    let mut dom = WeakDom::new();
    rbx_loren::place::apply_project(&mut dom, &dir).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    dom
}

#[test]
fn loren_routes_are_remote_entry_points() {
    let findings = audit(&project());
    let got: BTreeSet<(usize, &str)> = findings
        .iter()
        .filter(|f| f.path.ends_with("Services.ShopService"))
        .map(|f| (f.line.unwrap(), f.check.id()))
        .collect();
    let want: BTreeSet<(usize, &str)> = SHOP
        .lines()
        .enumerate()
        .filter_map(|(i, l)| Some((i + 1, l.split("-- expect: ").nth(1)?.trim())))
        .collect();
    assert_eq!(got, want);
    let others: Vec<_> = findings
        .iter()
        .filter(|f| !f.path.ends_with("Services.ShopService"))
        .collect();
    assert!(
        others.is_empty(),
        "Loren's own runtime flagged: {others:#?}"
    );
}

#[test]
fn findings_name_the_route_and_a_gate_lowers_them() {
    let findings = audit(&project());
    let at = |line: usize| {
        findings
            .iter()
            .find(|f| f.line == Some(line))
            .unwrap_or_else(|| panic!("nothing on line {line}"))
    };
    let line_of = |needle: &str| SHOP.lines().position(|l| l.contains(needle)).unwrap() + 1;
    let grant = at(line_of("coins(player).Value += amount"));
    assert!(
        grant.message.contains("`ShopService.Client.Grant`"),
        "{}",
        grant.message
    );
    assert_eq!(grant.severity, Severity::High);
    let tip = at(line_of("coins(player).Value -= n"));
    assert!(
        tip.message.contains("`ShopService.ClientEvents.Tip`"),
        "{}",
        tip.message
    );
    let volume = at(line_of("Volume.Value = volume"));
    assert_eq!(volume.severity, Severity::Low);
    assert!(
        volume
            .message
            .contains("`ShopService.Spec.SetVolume` runs first"),
        "{}",
        volume.message
    );
}

/// `RBX_AUDIT_LOREN_OUT=dir cargo test -p rbx_audit --test loren -- --ignored`
/// writes the project's place, for `rbxaudit`, the editor and Studio.
#[test]
#[ignore]
fn write_loren_place() {
    let out = PathBuf::from(std::env::var("RBX_AUDIT_LOREN_OUT").expect("RBX_AUDIT_LOREN_OUT"));
    let mut dom = project();
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    // Something for the viewport to draw, or the editor won't open it.
    let part = dom.new_instance("Part", "Baseplate", Some(workspace));
    let v = |x, y, z| Vector3Data { x, y, z };
    let cframe = CFrameData {
        position: v(0., -0.5, 0.),
        rotation: [1., 0., 0., 0., 1., 0., 0., 0., 1.],
    };
    for (key, value) in [
        ("Anchored", Variant::Bool(true)),
        ("size", Variant::Vector3(v(64., 1., 64.))),
        ("CFrame", Variant::CFrame(cframe)),
    ] {
        dom.set_property(part, key, value).unwrap();
    }
    std::fs::write(
        out.join("LorenAudit.rbxlx"),
        rbx_xml::serialize(&dom).unwrap(),
    )
    .unwrap();
}
