use super::*;

fn svc(name: &str, src: &str) -> ModuleSource {
    ModuleSource {
        name: name.into(),
        kind: Kind::Service,
        source: src.into(),
    }
}
fn ctl(name: &str, src: &str) -> ModuleSource {
    ModuleSource {
        name: name.into(),
        kind: Kind::Controller,
        source: src.into(),
    }
}

#[test]
fn order_puts_dependencies_first() {
    let g = analyze(&[
        svc("A", "local A = { Dependencies = { \"B\" } }\nreturn A\n"),
        svc("B", "local B = { Dependencies = { \"C\" } }\nreturn B\n"),
        svc("C", "local C = {}\nreturn C\n"),
    ]);
    assert_eq!(g.server_order, vec!["C", "B", "A"]);
    assert!(g.cycles.is_empty());
    assert!(g.problems.is_empty());
}

#[test]
fn cycles_are_listed() {
    let g = analyze(&[
        svc("A", "local A = { Dependencies = { \"B\" } }\nreturn A\n"),
        svc("B", "local B = { Dependencies = { \"A\" } }\nreturn B\n"),
    ]);
    assert_eq!(
        g.cycles,
        vec![vec!["A".to_string(), "B".into(), "A".into()]]
    );
    assert_eq!(g.server_order, vec!["A", "B"]);
}

#[test]
fn unknown_service_suggests() {
    let g = analyze(&[
        svc("DataService", "local D = {}\nreturn D\n"),
        svc(
            "A",
            "local A = { Dependencies = { \"DataServce\" } }\nreturn A\n",
        ),
    ]);
    assert_eq!(g.problems.len(), 1);
    assert_eq!(g.problems[0].severity, Severity::Error);
    assert_eq!(
        g.problems[0].message,
        "A.Dependencies: 'DataServce' is not a registered Service. Did you mean 'DataService'?"
    );
}

#[test]
fn controller_rules() {
    let g = analyze(&[
        svc(
            "Net",
            "local N = { Client = {} }\nfunction N.Client:Ping(p) end\nreturn N\n",
        ),
        svc("Quiet", "local Q = {}\nreturn Q\n"),
        ctl(
            "Ui",
            "local U = { Dependencies = { \"Net\", \"Quiet\" } }\nreturn U\n",
        ),
    ]);
    assert_eq!(g.client_order, vec!["Ui"]);
    assert_eq!(g.problems.len(), 1);
    assert_eq!(g.problems[0].severity, Severity::Warning);
    assert!(g.problems[0]
        .message
        .starts_with("Ui.Dependencies: 'Quiet' is not a Controller"));
}

#[test]
fn surface_lists_methods_and_events() {
    let g = analyze(&[svc(
        "Shop",
        "local S = { Client = {}, ClientEvents = { \"Poke\" } }\nfunction S.Client:Buy(p) end\nreturn S\n",
    )]);
    let routes = remote_surface(&g);
    assert!(routes
        .iter()
        .any(|r| r.name == "Buy" && r.kind == RouteKind::Method));
    assert!(routes
        .iter()
        .any(|r| r.name == "Poke" && r.kind == RouteKind::ClientEvent));
}
