//! A module's declared shape (`lint.js` lines 471-718: `analyzeLuau`, `isNetworked`).

use super::decl::{record_decl, statement_end, DECL_KEYS};
use super::syntax::{chain_before, table_fields, table_open_of, value_kind};
use super::tokenize::{is_kw, is_name, is_op, is_opener, structure, tokenize, Token, TokenKind};
use crate::constants::RESERVED_CLIENT_NAMES;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    pub name: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientMethod {
    pub name: String,
    pub line: usize,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientInfo {
    /// False when the Client table could not be read statically.
    pub known: bool,
    pub declared: bool,
    pub methods: Vec<ClientMethod>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameList {
    pub known: bool,
    pub names: Vec<Named>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceKey {
    /// `Spec` or `ClientEvents`.
    pub key: String,
    pub line: usize,
    pub modern: bool,
    pub empty: bool,
    pub unknown: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuauInfo {
    pub module_name: Option<String>,
    pub client: ClientInfo,
    pub events: NameList,
    pub signals: NameList,
    pub middleware: Vec<Named>,
    pub spec: Vec<Named>,
    pub spec_unknown: bool,
    pub service_keys: Vec<ServiceKey>,
    /// `{` token indices of Middleware tables.
    pub middleware_tables: Vec<usize>,
    /// rbx-native addition, read the same way as `signals` (no lint uses it):
    /// the module's `Dependencies = { "Name", ... }` list, for the editor's graph.
    pub dependencies: NameList,
}

fn new_info() -> LuauInfo {
    LuauInfo {
        module_name: None,
        client: ClientInfo {
            known: true,
            declared: false,
            methods: Vec::new(),
        },
        events: NameList {
            known: true,
            names: Vec::new(),
        },
        signals: NameList {
            known: true,
            names: Vec::new(),
        },
        middleware: Vec::new(),
        spec: Vec::new(),
        spec_unknown: false,
        service_keys: Vec::new(),
        middleware_tables: Vec::new(),
        dependencies: NameList {
            known: true,
            names: Vec::new(),
        },
    }
}

/// `tokens` when the caller already has them.
pub fn analyze_luau(text: &str, tokens: Option<&[Token]>) -> LuauInfo {
    let owned;
    let toks: &[Token] = match tokens {
        Some(t) => t,
        None => {
            owned = tokenize(text);
            &owned
        }
    };
    let s = &structure(toks);
    let mut info = new_info();

    // The module: `return X` or `return {` at the top level (the last one wins).
    let mut return_table: Option<usize> = None;
    for i in 0..toks.len() {
        if is_kw(toks.get(i), "return") && s.depth[i] == 0 {
            if is_name(toks.get(i + 1), None)
                && !is_op(toks.get(i + 2), ".")
                && !is_op(toks.get(i + 2), "(")
                && !is_op(toks.get(i + 2), ":")
            {
                info.module_name = Some(toks[i + 1].value.clone());
                return_table = None;
            } else if is_op(toks.get(i + 1), "{") {
                return_table = Some(i + 1);
                info.module_name = None;
            }
        }
    }
    let module_name = info.module_name.clone();
    let is_module = |name: &str| module_name.as_deref().is_none_or(|m| m == name);

    let mut module_tables: Vec<usize> = return_table.into_iter().collect();
    for i in 0..toks.len() {
        if s.depth[i] != 0 {
            continue;
        }
        let prev = i.checked_sub(1).and_then(|p| toks.get(p));
        let (name_idx, eq_idx);
        if is_kw(toks.get(i), "local") && is_name(toks.get(i + 1), None) {
            name_idx = i + 1;
            let mut k = i + 2;
            if is_op(toks.get(k), ":") {
                k += 1;
                while k < toks.len() && !is_op(toks.get(k), "=") && toks[k].kind != TokenKind::Kw {
                    if is_opener(toks.get(k)) && s.matching[k] > k as i32 {
                        k = s.matching[k] as usize;
                    }
                    k += 1;
                }
            }
            if !is_op(toks.get(k), "=") {
                continue;
            }
            eq_idx = k;
        } else if is_name(toks.get(i), None)
            && is_op(toks.get(i + 1), "=")
            && !is_op(prev, ".")
            && !is_op(prev, ":")
            && !is_op(prev, ",")
            && !is_kw(prev, "local")
        {
            name_idx = i;
            eq_idx = i + 1;
        } else {
            continue;
        }
        if !is_module(&toks[name_idx].value) {
            continue;
        }
        if let Some(open) = table_open_of(toks, eq_idx + 1, toks.len()) {
            module_tables.push(open);
        }
    }
    for open in module_tables {
        for f in table_fields(toks, s, open) {
            if let (Some(key), Some(ki)) = (&f.key, f.key_idx) {
                if DECL_KEYS.contains(&key.as_str()) {
                    record_decl(&mut info, toks, s, key, &toks[ki], f.val_start, f.val_end);
                }
            }
        }
    }

    for i in 0..toks.len() {
        let t = &toks[i];
        // function X.Client:Name( / function X.Middleware.Name( (at any depth)
        if is_kw(Some(t), "function") && is_name(toks.get(i + 1), None) {
            let mut parts = vec![toks[i + 1].value.clone()];
            let mut k = i + 2;
            while is_op(toks.get(k), ".") && is_name(toks.get(k + 1), None) {
                parts.push(toks[k + 1].value.clone());
                k += 2;
            }
            if is_op(toks.get(k), ":") && is_name(toks.get(k + 1), None) {
                parts.push(toks[k + 1].value.clone());
            }
            if parts.len() >= 3 && is_module(&parts[0]) {
                let owner = parts[parts.len() - 2].as_str();
                let name = parts[parts.len() - 1].clone();
                let line = t.line;
                if owner == "Client" && parts.len() == 3 {
                    info.client.declared = true;
                    info.client.methods.push(ClientMethod {
                        name,
                        line,
                        kind: "function".to_string(),
                    });
                } else if owner == "Middleware" && parts.len() == 3 {
                    info.middleware.push(Named { name, line });
                }
            }
            continue;
        }
        // X.Key = value / X.Key.Name = value (top level)
        if is_op(Some(t), "=") && s.depth[i] == 0 {
            let Some(chain) = chain_before(toks, i) else {
                continue;
            };
            if !is_module(&chain.parts[0]) || chain.parts.len() < 2 || chain.parts.len() > 3 {
                continue;
            }
            let before = chain.first.checked_sub(1).and_then(|p| toks.get(p));
            if is_op(before, ",") || is_kw(before, "local") {
                continue;
            }
            let end = statement_end(toks, s, i + 1);
            let key = chain.parts[1].as_str();
            if chain.parts.len() == 2 {
                if DECL_KEYS.contains(&key) {
                    record_decl(&mut info, toks, s, key, &toks[chain.first], i + 1, end);
                }
            } else {
                let name = chain.parts[2].clone();
                let line = toks[chain.first].line;
                let vk = value_kind(toks, i + 1, end);
                if key == "Client" && (vk == "function" || vk == "expr") {
                    info.client.declared = true;
                    info.client.methods.push(ClientMethod {
                        name,
                        line,
                        kind: vk.to_string(),
                    });
                } else if key == "Middleware" {
                    info.middleware.push(Named { name, line });
                } else if key == "Spec" {
                    info.spec.push(Named { name, line });
                }
            }
        }
    }
    info
}

/// True when Loren would network this Service (any Client method, Signal, ClientEvent or Spec).
pub fn is_networked(info: &LuauInfo) -> bool {
    info.client
        .methods
        .iter()
        .any(|m| !RESERVED_CLIENT_NAMES.contains(&m.name.as_str()))
        || !info.client.known
        || !info.signals.names.is_empty()
        || !info.signals.known
        || !info.events.names.is_empty()
        || !info.events.known
        || !info.spec.is_empty()
        || info.spec_unknown
}
