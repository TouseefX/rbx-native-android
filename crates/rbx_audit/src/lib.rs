//! A whole-place security and consistency audit of a place's scripts — a
//! pass Roblox Studio itself doesn't have.
//!
//! Every script is parsed with `full_moon` and walked once, resolving
//! instance paths (`game:GetService`, `WaitForChild`, `FindFirstChild`, dot
//! chains, `script.Parent`) against the place where that can be done
//! statically. On top of that walk:
//!
//! 1. **Unvalidated remote arguments**: inside an `OnServerEvent` /
//!    `OnServerInvoke` handler, the client-supplied parameters are tainted and
//!    followed through locals into sensitive sinks — currency and leaderstats
//!    `Value`s, `Health`, `CFrame`/`Position`, `Parent`, `:Destroy()`,
//!    `Instance.new`, DataStore writes, `MarketplaceService`, `require`,
//!    `loadstring`. A name mentioned in an `if`/`assert` condition counts as
//!    validated inside that branch, and for the rest of the block when the
//!    branch exits (`if not x then return end`); `tonumber`, `math.clamp`,
//!    `typeof`/`type` and a lookup into a server table (`Items[id]`) produce
//!    clean values.
//! 2. **Player-argument confusion**: a handler whose first parameter (always
//!    the sending `Player`) is named or used as data, and a client passing
//!    `LocalPlayer` as `FireServer`'s first argument.
//! 3. **Remote wiring**: remotes a client calls (or nothing uses) with no
//!    server listener, and calls or listeners on remotes the place doesn't
//!    have.
//! 4. **`require` targets** that aren't a `ModuleScript`, or aren't there.
//! 5. **Server-only APIs on the client** (`DataStoreService`, `ServerStorage`,
//!    `ServerScriptService`) and `RemoteFunction:InvokeClient` on the server.
//!
//! It is a heuristic, not a type checker: taint is per handler (it does not
//! follow calls into other functions), bindings are one flat table per
//! script, and a path that only exists at run time is unknown rather than
//! missing — except a name some script assigns to `.Name`, which is assumed
//! to be created at run time and never reported missing.

mod resolve;
mod walk;

use std::collections::HashSet;

use rbx_dom::{Ref, Variant, WeakDom};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    High,
    Medium,
    Low,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::High => "high",
            Severity::Medium => "medium",
            Severity::Low => "low",
        }
    }

    fn lower(self) -> Severity {
        match self {
            Severity::High => Severity::Medium,
            _ => Severity::Low,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Check {
    UnvalidatedArgument,
    PlayerArgument,
    NoServerListener,
    MissingRemote,
    BadRequire,
    ServerApiOnClient,
    InvokeClient,
}

impl Check {
    /// Stable, for CI output and the fixtures' `-- expect:` comments.
    pub fn id(self) -> &'static str {
        match self {
            Check::UnvalidatedArgument => "unvalidated-argument",
            Check::PlayerArgument => "player-argument",
            Check::NoServerListener => "no-server-listener",
            Check::MissingRemote => "missing-remote",
            Check::BadRequire => "bad-require",
            Check::ServerApiOnClient => "server-api-on-client",
            Check::InvokeClient => "invoke-client",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Check::UnvalidatedArgument => "Unvalidated remote argument",
            Check::PlayerArgument => "Player argument confusion",
            Check::NoServerListener => "Remote without a server listener",
            Check::MissingRemote => "Remote not in the place",
            Check::BadRequire => "require target is not a ModuleScript",
            Check::ServerApiOnClient => "Server-only API on the client",
            Check::InvokeClient => "Server trusts a client callback",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub check: Check,
    pub severity: Severity,
    /// The script the finding is in, or the remote for one about a remote.
    pub instance: Ref,
    /// `instance`'s full name, as `Instance:GetFullName()` gives it.
    pub path: String,
    /// 1-based; `None` for a finding about an instance rather than a line.
    pub line: Option<usize>,
    pub message: String,
}

/// Which side of the network a script runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Server,
    Client,
    /// A ModuleScript somewhere both sides can require it from.
    Shared,
}

/// What the walk learned about remotes across every script.
#[derive(Default)]
pub(crate) struct Usage {
    listened: HashSet<Ref>,
    /// Listeners on remotes whose path couldn't be resolved: any remote
    /// could be one of them.
    unresolved_listeners: usize,
    client_calls: HashSet<Ref>,
    server_calls: HashSet<Ref>,
}

const REMOTES: [&str; 3] = ["RemoteEvent", "UnreliableRemoteEvent", "RemoteFunction"];

/// Audits every script in `dom`. Findings come sorted by severity, then
/// path, then line.
pub fn audit(dom: &WeakDom) -> Vec<Finding> {
    let mut scripts = Vec::new();
    let mut remotes = Vec::new();
    let mut stack: Vec<Ref> = dom.root_refs().to_vec();
    while let Some(r) = stack.pop() {
        let Some(instance) = dom.get(r) else { continue };
        stack.extend(instance.children().iter().rev());
        if REMOTES.contains(&instance.class()) {
            remotes.push(r);
        }
        let Some(side) = side(dom, r) else { continue };
        if let Some(Variant::String(source)) = instance.properties().get("Source") {
            let ast = full_moon::parse_fallible(source, full_moon::LuaVersion::luau()).into_ast();
            scripts.push((r, side, ast));
        }
    }

    let runtime_names: HashSet<String> = scripts
        .iter()
        .flat_map(|(_, _, ast)| walk::runtime_names(ast))
        .collect();
    let mut usage = Usage::default();
    let mut findings = Vec::new();
    for (script, side, ast) in &scripts {
        findings.extend(walk::script(
            dom,
            *script,
            *side,
            ast,
            &runtime_names,
            &mut usage,
        ));
    }

    for remote in remotes {
        if usage.listened.contains(&remote) {
            continue;
        }
        let class = dom.get(remote).map_or("", |i| i.class());
        let (severity, mut message) = if usage.client_calls.contains(&remote) {
            if class == "RemoteFunction" {
                (
                    Severity::High,
                    "A client invokes this RemoteFunction, but no server script sets OnServerInvoke: InvokeServer yields forever.".to_string(),
                )
            } else {
                (
                    Severity::Medium,
                    "A client fires this remote, but no server script listens on OnServerEvent: the events are dropped.".to_string(),
                )
            }
        } else if usage.server_calls.contains(&remote) {
            continue;
        } else {
            (
                Severity::Low,
                "No script listens on or fires this remote.".to_string(),
            )
        };
        let severity = if usage.unresolved_listeners > 0 {
            message.push_str(&format!(
                " ({} server listener(s) are connected through paths the audit couldn't resolve, so this may be one of them.)",
                usage.unresolved_listeners
            ));
            severity.lower()
        } else {
            severity
        };
        findings.push(Finding {
            check: Check::NoServerListener,
            severity,
            instance: remote,
            path: full_name(dom, remote),
            line: None,
            message,
        });
    }

    findings.sort_by(|a, b| {
        (a.severity, &a.path, a.line, a.check, &a.message)
            .cmp(&(b.severity, &b.path, b.line, b.check, &b.message))
    });
    // A named handler is walked once where it's declared and again, with
    // its parameters tainted, where it's connected.
    findings.dedup();
    findings
}

/// The side `r` runs on, or `None` when it isn't a script.
fn side(dom: &WeakDom, r: Ref) -> Option<Side> {
    let instance = dom.get(r)?;
    match instance.class() {
        "LocalScript" => Some(Side::Client),
        // RunContext: 0 Legacy, 1 Server, 2 Client, 3 Plugin.
        "Script" => match instance.properties().get("RunContext") {
            Some(Variant::Enum(2)) => Some(Side::Client),
            _ => Some(Side::Server),
        },
        "ModuleScript" => {
            let mut at = dom.parent(r);
            while let Some(a) = at {
                match dom.get(a).map(|i| i.class()) {
                    Some("ServerScriptService" | "ServerStorage") => return Some(Side::Server),
                    Some("StarterPlayer" | "StarterGui" | "StarterPack" | "ReplicatedFirst") => {
                        return Some(Side::Client)
                    }
                    _ => at = dom.parent(a),
                }
            }
            Some(Side::Shared)
        }
        _ => None,
    }
}

/// `Instance:GetFullName()`: the dotted path from the service down.
pub fn full_name(dom: &WeakDom, r: Ref) -> String {
    let mut names = Vec::new();
    let mut at = Some(r);
    while let Some(reference) = at {
        let Some(instance) = dom.get(reference) else {
            break;
        };
        names.push(instance.name());
        at = dom.parent(reference);
    }
    names.reverse();
    names.join(".")
}
