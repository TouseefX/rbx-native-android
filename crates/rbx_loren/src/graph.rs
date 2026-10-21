//! A Loren project's Services and Controllers as the runtime will wire them:
//! each module's `Dependencies`, the boot (`LorenIgnite`) order per side, the
//! cycles, and the problems a boot would report (with Loren's own "Did you
//! mean" suggestions). Also the remote surface a security audit reads.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use rbx_dom::{Ref, Variant, WeakDom};

mod topo;
use topo::{suggestion, topo_order};

use crate::constants::RESERVED_CLIENT_NAMES;
use crate::fsutil::read_text;
use crate::lint::{analyze_luau, is_networked};
use crate::project::project_paths;
use crate::types::list_modules;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Service,
    Controller,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSource {
    pub name: String,
    pub kind: Kind,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub name: String,
    pub kind: Kind,
    pub dependencies: Vec<String>,
    /// False when `Dependencies` is not a literal list of strings.
    pub dependencies_known: bool,
    pub networked: bool,
    pub client_methods: Vec<String>,
    pub client_events: Vec<String>,
    pub signals: Vec<String>,
    pub middleware: Vec<String>,
    pub spec: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The boot fails.
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub module: String,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Graph {
    /// Services first, then Controllers, each by name.
    pub nodes: Vec<Node>,
    /// `LorenIgnite` order on the server and on the client (Loren's `Topo.order`).
    pub server_order: Vec<String>,
    pub client_order: Vec<String>,
    /// Each dependency cycle, as Loren lists them with `CycleWarnings`.
    pub cycles: Vec<Vec<String>>,
    pub problems: Vec<Problem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteKind {
    /// `Service.Client:Method(player, ...)`: a call the client awaits.
    Method,
    /// A name in `Service.ClientEvents`: fire-and-forget from the client.
    ClientEvent,
}

/// One way a client reaches server code through Loren.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub service: String,
    pub name: String,
    pub kind: RouteKind,
    /// `Service.Middleware.<name>` gates it (runs first, with the Player).
    pub middleware: bool,
    /// `Service.Spec.<name>` types its arguments (Loren rejects bad packets before the handler).
    pub spec: bool,
}

const NOT_NETWORKED: &str =
    "(a Service is networked when it has a Client method, ClientEvent or Signal)";
fn node_of(module: &ModuleSource) -> Node {
    let info = analyze_luau(&module.source, None);
    let names =
        |list: &[crate::lint::Named]| list.iter().map(|n| n.name.clone()).collect::<Vec<_>>();
    Node {
        name: module.name.clone(),
        kind: module.kind,
        dependencies: names(&info.dependencies.names),
        dependencies_known: info.dependencies.known,
        networked: is_networked(&info),
        client_methods: info.client.methods.iter().map(|m| m.name.clone()).collect(),
        client_events: names(&info.events.names),
        signals: names(&info.signals.names),
        middleware: names(&info.middleware),
        spec: names(&info.spec),
    }
}

fn problem(module: &str, severity: Severity, message: String) -> Problem {
    Problem {
        module: module.to_string(),
        severity,
        message,
    }
}

pub fn analyze(modules: &[ModuleSource]) -> Graph {
    let mut services: BTreeMap<String, Node> = BTreeMap::new();
    let mut controllers: BTreeMap<String, Node> = BTreeMap::new();
    for module in modules {
        let side = match module.kind {
            Kind::Service => &mut services,
            Kind::Controller => &mut controllers,
        };
        if !side.contains_key(&module.name) {
            side.insert(module.name.clone(), node_of(module));
        }
    }
    let mut problems = Vec::new();
    for node in services.values().chain(controllers.values()) {
        if !node.dependencies_known {
            problems.push(problem(
                &node.name,
                Severity::Warning,
                "Dependencies is not a literal list of names; the graph cannot show its edges"
                    .to_string(),
            ));
        }
    }

    let service_names: Vec<String> = services.keys().cloned().collect();
    for (name, node) in &services {
        for dep in &node.dependencies {
            if !services.contains_key(dep) {
                let message = format!("{name}.Dependencies: '{dep}' is not a registered Service.")
                    + &suggestion(dep, &service_names, "Services");
                problems.push(problem(name, Severity::Error, message));
            }
        }
    }
    let (server_order, mut cycles) = topo_order(&service_names, &|n| {
        services
            .get(n)
            .map(|s| s.dependencies.clone())
            .unwrap_or_default()
    });

    let controller_names: Vec<String> = controllers.keys().cloned().collect();
    let proxy_names: Vec<String> = services
        .values()
        .filter(|s| s.networked)
        .map(|s| s.name.clone())
        .collect();
    let candidates: Vec<String> = controller_names
        .iter()
        .chain(&proxy_names)
        .cloned()
        .collect();
    let mut depended_on: HashSet<&str> = HashSet::new();
    for (name, node) in &controllers {
        for dep in &node.dependencies {
            let ctrl = controllers.contains_key(dep);
            let proxy = proxy_names.contains(dep);
            if ctrl && proxy {
                depended_on.insert(dep);
                problems.push(problem(
                    name,
                    Severity::Error,
                    format!("{name}.Dependencies: '{dep}' is both a Controller and a networked Service; rename one of them"),
                ));
            } else if !ctrl && !proxy {
                let message = format!(
                    "{name}.Dependencies: '{dep}' is not a Controller or a networked Service {NOT_NETWORKED}; it resolves to a placeholder that raises when used."
                ) + &suggestion(dep, &candidates, "Available");
                problems.push(problem(name, Severity::Warning, message));
            }
        }
    }
    for pname in &proxy_names {
        if controllers.contains_key(pname) && !depended_on.contains(pname.as_str()) {
            problems.push(problem(
                pname,
                Severity::Warning,
                format!("'{pname}' is both a Controller and a networked Service; GetController and GetService return different objects"),
            ));
        }
    }
    let (client_order, client_cycles) = topo_order(&controller_names, &|n| {
        controllers
            .get(n)
            .map(|c| c.dependencies.clone())
            .unwrap_or_default()
    });
    cycles.extend(client_cycles);

    Graph {
        nodes: services
            .into_values()
            .chain(controllers.into_values())
            .collect(),
        server_order,
        client_order,
        cycles,
        problems,
    }
}

/// Services and Controllers on disk (`src/server/Services`, `src/client/Controllers`, or the project's own mapping).
pub fn project_modules(root: &Path) -> Vec<ModuleSource> {
    let paths = project_paths(root);
    let mut out = Vec::new();
    for (dir, kind) in [
        (&paths.services, Kind::Service),
        (&paths.controllers, Kind::Controller),
    ] {
        for (name, file) in list_modules(dir) {
            if let Ok(source) = read_text(&file) {
                out.push(ModuleSource { name, kind, source });
            }
        }
    }
    out
}

fn child_named(dom: &WeakDom, parent: Ref, name: &str) -> Option<Ref> {
    dom.get(parent)?
        .children()
        .iter()
        .copied()
        .find(|c| dom.get(*c).is_some_and(|i| i.name() == name))
}

fn modules_under(
    dom: &WeakDom,
    service: &str,
    path: &[&str],
    kind: Kind,
    out: &mut Vec<ModuleSource>,
) {
    let Some(mut at) = dom
        .root_refs()
        .iter()
        .copied()
        .find(|r| dom.get(*r).is_some_and(|i| i.class() == service))
    else {
        return;
    };
    for name in path {
        let Some(next) = child_named(dom, at, name) else {
            return;
        };
        at = next;
    }
    let Some(folder) = dom.get(at) else { return };
    for child in folder.children() {
        let Some(inst) = dom.get(*child) else {
            continue;
        };
        if inst.class() != "ModuleScript" {
            continue;
        }
        let source = match inst.properties().get("Source") {
            Some(Variant::String(s)) => s.clone(),
            _ => String::new(),
        };
        out.push(ModuleSource {
            name: inst.name().to_string(),
            kind,
            source,
        });
    }
}

/// Services under `ServerScriptService.Server.Services` and Controllers under
/// `StarterPlayer.StarterPlayerScripts.Client.Controllers` in an open place.
pub fn dom_modules(dom: &WeakDom) -> Vec<ModuleSource> {
    let mut out = Vec::new();
    modules_under(
        dom,
        "ServerScriptService",
        &["Server", "Services"],
        Kind::Service,
        &mut out,
    );
    modules_under(
        dom,
        "StarterPlayer",
        &["StarterPlayerScripts", "Client", "Controllers"],
        Kind::Controller,
        &mut out,
    );
    out
}

/// Every client-reachable route of the networked Services.
pub fn remote_surface(graph: &Graph) -> Vec<Route> {
    let mut routes = Vec::new();
    for node in graph
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Service && n.networked)
    {
        let route = |name: &String, kind| Route {
            service: node.name.clone(),
            name: name.clone(),
            kind,
            middleware: node.middleware.contains(name),
            spec: node.spec.contains(name),
        };
        for m in &node.client_methods {
            if !RESERVED_CLIENT_NAMES.contains(&m.as_str()) {
                routes.push(route(m, RouteKind::Method));
            }
        }
        for e in &node.client_events {
            routes.push(route(e, RouteKind::ClientEvent));
        }
    }
    routes
}

#[cfg(test)]
mod tests;
