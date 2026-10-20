//! A Loren project's Services and Controllers as the runtime will wire them:
//! each module's `Dependencies`, the boot (`LorenIgnite`) order per side, the
//! cycles, and the problems a boot would report (with Loren's own "Did you
//! mean" suggestions). Also the remote surface a security audit reads.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use rbx_dom::{Ref, Variant, WeakDom};

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
const LIST_MAX: usize = 12;

fn levenshtein(a: &[u8], b: &[u8]) -> usize {
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(sub);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// The closest candidate: a case-insensitive match, else Levenshtein <= 2 (ties: byte order).
fn suggest<'a>(name: &str, candidates: &'a [String]) -> Option<&'a str> {
    let lower = name.to_lowercase();
    let mut best: Option<&str> = None;
    let mut best_distance = 3;
    for candidate in candidates {
        let lc = candidate.to_lowercase();
        let d = if lc == lower {
            0
        } else if lc.len().abs_diff(lower.len()) <= 2 {
            levenshtein(lower.as_bytes(), lc.as_bytes())
        } else {
            3
        };
        if d < best_distance || (d == best_distance && best.is_some_and(|b| candidate.as_str() < b))
        {
            best = Some(candidate);
            best_distance = d;
        }
    }
    if best_distance <= 2 {
        best
    } else {
        None
    }
}

fn list_names(names: &[String]) -> String {
    let mut sorted = names.to_vec();
    sorted.sort();
    if sorted.len() <= LIST_MAX {
        return sorted.join(", ");
    }
    format!(
        "{}, ... ({} more)",
        sorted[..LIST_MAX].join(", "),
        sorted.len() - LIST_MAX
    )
}

fn suggestion(name: &str, candidates: &[String], label: &str) -> String {
    if let Some(s) = suggest(name, candidates) {
        format!(" Did you mean '{s}'?")
    } else if candidates.is_empty() {
        String::new()
    } else {
        format!(" {label}: {}.", list_names(candidates))
    }
}

struct Tarjan<'a> {
    known: HashSet<&'a str>,
    deps_of: &'a dyn Fn(&str) -> Vec<String>,
    index: HashMap<String, usize>,
    low: HashMap<String, usize>,
    on_stack: HashSet<String>,
    scc_stack: Vec<String>,
    path: Vec<String>,
    counter: usize,
    order: Vec<String>,
    cycles: Vec<Vec<String>>,
}

impl Tarjan<'_> {
    fn visit(&mut self, name: &str) {
        self.counter += 1;
        self.index.insert(name.to_string(), self.counter);
        self.low.insert(name.to_string(), self.counter);
        self.scc_stack.push(name.to_string());
        self.on_stack.insert(name.to_string());
        self.path.push(name.to_string());

        let mut deps: Vec<String> = Vec::new();
        for dep in (self.deps_of)(name) {
            if self.known.contains(dep.as_str()) && !deps.contains(&dep) {
                deps.push(dep);
            }
        }
        deps.sort();

        for dep in deps {
            match self.index.get(&dep).copied() {
                None => {
                    self.visit(&dep);
                    let ld = self.low[&dep];
                    if ld < self.low[name] {
                        self.low.insert(name.to_string(), ld);
                    }
                }
                Some(di) if self.on_stack.contains(&dep) => {
                    if di < self.low[name] {
                        self.low.insert(name.to_string(), di);
                    }
                    if let Some(from) = self.path.iter().position(|p| *p == dep) {
                        let mut cycle = self.path[from..].to_vec();
                        cycle.push(dep.clone());
                        self.cycles.push(cycle);
                    }
                }
                Some(_) => {}
            }
        }
        self.path.pop();

        if self.low[name] == self.index[name] {
            let mut members = Vec::new();
            while let Some(top) = self.scc_stack.pop() {
                self.on_stack.remove(&top);
                let done = top == name;
                members.push(top);
                if done {
                    break;
                }
            }
            if members.len() > 1 {
                members.sort();
            }
            self.order.extend(members);
        }
    }
}

/// Loren's `Topo.order`: the Ignite order (dependencies first) and each cycle found.
fn topo_order(
    names: &[String],
    deps_of: &dyn Fn(&str) -> Vec<String>,
) -> (Vec<String>, Vec<Vec<String>>) {
    let mut sorted: Vec<String> = Vec::new();
    for n in names {
        if !sorted.contains(n) {
            sorted.push(n.clone());
        }
    }
    sorted.sort();
    let mut t = Tarjan {
        known: sorted.iter().map(String::as_str).collect(),
        deps_of,
        index: HashMap::new(),
        low: HashMap::new(),
        on_stack: HashSet::new(),
        scc_stack: Vec::new(),
        path: Vec::new(),
        counter: 0,
        order: Vec::new(),
        cycles: Vec::new(),
    };
    for name in &sorted {
        if !t.index.contains_key(name) {
            t.visit(name);
        }
    }
    (t.order, t.cycles)
}

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
mod tests {
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
}
