//! Static resolution of instance paths in a script against the place.

use std::collections::HashMap;

use full_moon::ast::{Call, Expression, FunctionArgs, Index, Prefix, Suffix, Var};
use full_moon::tokenizer::{TokenReference, TokenType};
use rbx_dom::{Ref, WeakDom};

use crate::full_name;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Val {
    Game,
    Inst(Ref),
    /// A fully static path that leaves the place at `first`.
    Missing {
        path: String,
        first: String,
    },
    /// A service the file doesn't carry (`DataStoreService` never is).
    Service(String),
    LocalPlayer,
    /// `Instance.new(...)`: made at run time, never one of the place's.
    Created,
    Unknown,
}

/// Containers whose run-time copies gain children (a player's scripts,
/// character, GUI, backpack), or whose children only exist at run time.
const RUNTIME: [&str; 6] = [
    "Players",
    "StarterPlayerScripts",
    "StarterCharacterScripts",
    "StarterGui",
    "StarterPack",
    "StarterGear",
];

pub(crate) struct Resolver<'a> {
    pub dom: &'a WeakDom,
    pub script: Ref,
    /// Local and global names, flat for the whole script, last write wins.
    pub bindings: HashMap<String, Val>,
}

impl Resolver<'_> {
    pub fn expr(&self, expr: &Expression) -> Val {
        match expr {
            Expression::Parentheses { expression, .. }
            | Expression::TypeAssertion { expression, .. } => self.expr(expression),
            // `x:FindFirstChild("A") or error(...)`
            Expression::BinaryOperator { lhs, binop, .. }
                if binop.token().to_string().trim() == "or" =>
            {
                self.expr(lhs)
            }
            Expression::Var(Var::Name(name)) => self.name(ident(name)),
            Expression::Var(Var::Expression(var)) => {
                self.chain(var.prefix(), &var.suffixes().collect::<Vec<_>>())
            }
            Expression::FunctionCall(call) => {
                self.chain(call.prefix(), &call.suffixes().collect::<Vec<_>>())
            }
            _ => Val::Unknown,
        }
    }

    pub fn chain(&self, prefix: &Prefix, suffixes: &[&Suffix]) -> Val {
        let mut val = match prefix {
            Prefix::Name(name) => self.name(ident(name)),
            Prefix::Expression(expr) => self.expr(expr),
            _ => Val::Unknown,
        };
        for (i, suffix) in suffixes.iter().enumerate() {
            val = match suffix {
                Suffix::Call(Call::AnonymousCall(_))
                    if dotted(prefix, &suffixes[..i]).as_deref() == Some("Instance.new") =>
                {
                    Val::Created
                }
                Suffix::Index(Index::Dot { name, .. }) => self.member(val, ident(name)),
                Suffix::Index(Index::Brackets { expression, .. }) => match string(expression) {
                    Some(name) => self.child(val, name),
                    None => Val::Unknown,
                },
                Suffix::Call(Call::MethodCall(method)) => {
                    let arg = first_string(method.args());
                    match (ident(method.name()), arg) {
                        ("GetService" | "FindService", Some(class)) => self.service(&val, class),
                        ("WaitForChild" | "FindFirstChild", Some(name))
                            if arg_count(method.args()) == 1 || self.has_timeout(method.args()) =>
                        {
                            self.child(val, name)
                        }
                        ("FindFirstChildOfClass", Some(class)) => self.class_child(&val, class),
                        _ => Val::Unknown,
                    }
                }
                _ => Val::Unknown,
            };
        }
        val
    }

    /// `WaitForChild(name, timeout)` still names a direct child;
    /// `FindFirstChild(name, true)` searches descendants.
    fn has_timeout(&self, args: &FunctionArgs) -> bool {
        matches!(args, FunctionArgs::Parentheses { arguments, .. }
            if matches!(arguments.iter().nth(1), Some(Expression::Number(_))))
    }

    fn name(&self, name: &str) -> Val {
        if let Some(val) = self.bindings.get(name) {
            return val.clone();
        }
        match name {
            "game" | "Game" => Val::Game,
            "workspace" | "Workspace" => self.service(&Val::Game, "Workspace"),
            "script" => Val::Inst(self.script),
            _ => Val::Unknown,
        }
    }

    fn service(&self, val: &Val, class: &str) -> Val {
        if *val != Val::Game {
            return Val::Unknown;
        }
        self.dom
            .root_refs()
            .iter()
            .find(|r| self.dom.get(**r).is_some_and(|i| i.class() == class))
            .map_or_else(|| Val::Service(class.to_string()), |r| Val::Inst(*r))
    }

    /// `.name`: a child, or one of the few properties that lead to another
    /// instance.
    fn member(&self, val: Val, name: &str) -> Val {
        match (&val, name) {
            (Val::Inst(r), "Parent") => {
                let class = self.dom.get(*r).map_or("", |i| i.class());
                if RUNTIME.contains(&class) {
                    return Val::Unknown;
                }
                self.dom.parent(*r).map_or(Val::Game, Val::Inst)
            }
            (Val::Inst(r), "LocalPlayer")
                if self.dom.get(*r).is_some_and(|i| i.class() == "Players") =>
            {
                Val::LocalPlayer
            }
            (Val::Service(class), "LocalPlayer") if class == "Players" => Val::LocalPlayer,
            _ => self.child(val, name),
        }
    }

    fn child(&self, val: Val, name: &str) -> Val {
        match val {
            Val::Game => self
                .dom
                .root_refs()
                .iter()
                .find(|r| {
                    self.dom
                        .get(**r)
                        .is_some_and(|i| i.name() == name || i.class() == name)
                })
                .map_or_else(|| Val::Service(name.to_string()), |r| Val::Inst(*r)),
            Val::Inst(r) => {
                let Some(instance) = self.dom.get(r) else {
                    return Val::Unknown;
                };
                let found = instance
                    .children()
                    .iter()
                    .find(|c| self.dom.get(**c).is_some_and(|i| i.name() == name));
                match found {
                    Some(c) => Val::Inst(*c),
                    None if RUNTIME.contains(&instance.class()) => Val::Unknown,
                    None => Val::Missing {
                        path: format!("{}.{name}", full_name(self.dom, r)),
                        first: name.to_string(),
                    },
                }
            }
            Val::Missing { path, first } => Val::Missing {
                path: format!("{path}.{name}"),
                first,
            },
            _ => Val::Unknown,
        }
    }

    fn class_child(&self, val: &Val, class: &str) -> Val {
        let Val::Inst(r) = val else {
            return Val::Unknown;
        };
        self.dom
            .get(*r)
            .and_then(|i| {
                i.children()
                    .iter()
                    .find(|c| self.dom.get(**c).is_some_and(|i| i.class() == class))
            })
            .map_or(Val::Unknown, |c| Val::Inst(*c))
    }
}

pub(crate) fn ident(token: &TokenReference) -> &str {
    match token.token_type() {
        TokenType::Identifier { identifier } => identifier.as_str(),
        _ => "",
    }
}

pub(crate) fn string(expr: &Expression) -> Option<&str> {
    match expr {
        Expression::String(token) => match token.token_type() {
            TokenType::StringLiteral { literal, .. } => Some(literal.as_str()),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn args(args: &FunctionArgs) -> Vec<&Expression> {
    match args {
        FunctionArgs::Parentheses { arguments, .. } => arguments.iter().collect(),
        _ => Vec::new(),
    }
}

fn arg_count(args: &FunctionArgs) -> usize {
    match args {
        FunctionArgs::Parentheses { arguments, .. } => arguments.len(),
        _ => 1,
    }
}

/// The first argument as a string literal, in either call form:
/// `f("x")` or `f "x"`.
pub(crate) fn first_string(args: &FunctionArgs) -> Option<&str> {
    match args {
        FunctionArgs::String(token) => match token.token_type() {
            TokenType::StringLiteral { literal, .. } => Some(literal.as_str()),
            _ => None,
        },
        FunctionArgs::Parentheses { arguments, .. } => arguments.iter().next().and_then(string),
        _ => None,
    }
}

/// `a.b.c` for a chain of plain names, which is how a callee or a handler
/// is named.
pub(crate) fn dotted(prefix: &Prefix, suffixes: &[&Suffix]) -> Option<String> {
    let Prefix::Name(base) = prefix else {
        return None;
    };
    let mut name = ident(base).to_string();
    for suffix in suffixes {
        let Suffix::Index(Index::Dot { name: field, .. }) = suffix else {
            return None;
        };
        name.push('.');
        name.push_str(ident(field));
    }
    Some(name)
}
