//! Dependency ordering (Loren's `Topo.order`) and the "Did you mean"
//! suggestions that accompany unknown-dependency problems.

use std::collections::{HashMap, HashSet};

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

pub(super) fn suggestion(name: &str, candidates: &[String], label: &str) -> String {
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
pub(super) fn topo_order(
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
