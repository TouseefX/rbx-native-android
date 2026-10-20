//! Runs a parsed query against a `WeakDom`.
//!
//! A property a comparison cannot find, or a component it cannot take, makes
//! that comparison false rather than an error: a query over a whole place
//! meets instances of every class, and most lack most properties.

use rbx_dom::{BrickColor, Content, Instance, Ref, Variant, Vector3Data, WeakDom};
use rbx_reflection::ReflectionDatabase;

use super::lex::Op;
use super::parse::{Ast, Expr, Target, Value};
use super::QueryError;

/// What a property or attribute reads as once converted for comparing.
enum Val {
    Str(String),
    Bool(bool),
    Num(f64),
    Item { value: f64, name: String },
    Other,
}

pub(super) fn select(
    ast: &Ast,
    dom: &WeakDom,
    db: &ReflectionDatabase,
) -> Result<Vec<Ref>, QueryError> {
    let class = match &ast.selector {
        Some(selector) => match &selector.class {
            Some(name) if db.class(name).is_none() => {
                return Err(QueryError {
                    message: format!("unknown class \"{name}\""),
                    span: selector.span.clone(),
                });
            }
            other => other.as_deref(),
        },
        None => None,
    };
    let scope: Vec<Ref> = match &ast.under {
        Some(under) => {
            let found = crate::explorer::resolve(dom, &under.path)
                .and_then(|reference| dom.get(reference))
                .ok_or_else(|| QueryError {
                    message: format!("nothing called \"{}\"", under.path),
                    span: under.span.clone(),
                })?;
            found.children().to_vec()
        }
        None => dom.root_refs().to_vec(),
    };
    let mut matches = Vec::new();
    visit(dom, db, &scope, class, ast.filter.as_ref(), &mut matches);
    Ok(matches)
}

fn visit(
    dom: &WeakDom,
    db: &ReflectionDatabase,
    references: &[Ref],
    class: Option<&str>,
    filter: Option<&Expr>,
    out: &mut Vec<Ref>,
) {
    for &reference in references {
        let Some(instance) = dom.get(reference) else {
            continue;
        };
        let class_ok = class.is_none_or(|name| {
            instance.class() == name || db.is_subclass_of(instance.class(), name)
        });
        if class_ok && filter.is_none_or(|expr| holds(expr, instance, db)) {
            out.push(reference);
        }
        visit(dom, db, instance.children(), class, filter, out);
    }
}

fn holds(expr: &Expr, instance: &Instance, db: &ReflectionDatabase) -> bool {
    match expr {
        Expr::Or(parts) => parts.iter().any(|part| holds(part, instance, db)),
        Expr::And(parts) => parts.iter().all(|part| holds(part, instance, db)),
        Expr::Not(inner) => !holds(inner, instance, db),
        Expr::HasTag(tag) => instance.tags().contains(&tag.as_str()),
        Expr::HasAttribute(name) => instance.attributes().contains_key(name),
        Expr::Test { target, compare } => {
            let found = match target {
                Target::Property(path) => property(instance, db, path),
                Target::Attribute(name) => instance
                    .attributes()
                    .get(name)
                    .map(|variant| convert(db, variant, None)),
            };
            match (found, compare) {
                (None, _) => false,
                (Some(val), None) => truthy(&val),
                (Some(val), Some((op, value))) => compare_values(*op, &val, value),
            }
        }
    }
}

fn property(instance: &Instance, db: &ReflectionDatabase, path: &[String]) -> Option<Val> {
    let (head, rest) = path.split_first()?;
    if rest.is_empty() {
        match head.as_str() {
            "Name" => return Some(Val::Str(instance.name().to_string())),
            "ClassName" => return Some(Val::Str(instance.class().to_string())),
            _ => {}
        }
    } else if matches!(head.as_str(), "Name" | "ClassName") {
        return None;
    }
    let position;
    let (key, variant) = if head == "Position" {
        match db.stored_or_default(instance, "CFrame") {
            Some((_, Variant::CFrame(frame))) => {
                position = Variant::Vector3(frame.position);
                ("Position", &position)
            }
            _ => db.stored_or_default(instance, head)?,
        }
    } else {
        db.stored_or_default(instance, head)?
    };
    if rest.is_empty() {
        // `key` may be a stored spelling the dump does not describe.
        let enum_type = db
            .resolve_property(instance.class(), db.canonical_name(instance.class(), key))
            .map(|descriptor| descriptor.value_type.as_str());
        return Some(convert(db, variant, enum_type));
    }
    let names: Vec<String> = rest.iter().map(|n| n.to_ascii_lowercase()).collect();
    component(variant, &names).map(Val::Num)
}

fn component(variant: &Variant, names: &[String]) -> Option<f64> {
    let (first, rest) = names.split_first()?;
    let first = first.as_str();
    let vector = |v: Vector3Data| match first {
        "x" => Some(f64::from(v.x)),
        "y" => Some(f64::from(v.y)),
        "z" => Some(f64::from(v.z)),
        _ => None,
    };
    match variant {
        Variant::Vector3(v) if rest.is_empty() => vector(*v),
        Variant::Vector3int16 { x, y, z } if rest.is_empty() => match first {
            "x" => Some(f64::from(*x)),
            "y" => Some(f64::from(*y)),
            "z" => Some(f64::from(*z)),
            _ => None,
        },
        Variant::Vector2(v) if rest.is_empty() => match first {
            "x" => Some(f64::from(v.x)),
            "y" => Some(f64::from(v.y)),
            _ => None,
        },
        Variant::CFrame(frame) if first == "position" => {
            component(&Variant::Vector3(frame.position), rest)
        }
        Variant::CFrame(frame) if rest.is_empty() => vector(frame.position),
        Variant::Color3(c) if rest.is_empty() => match first {
            "r" => Some(f64::from(c.r)),
            "g" => Some(f64::from(c.g)),
            "b" => Some(f64::from(c.b)),
            _ => None,
        },
        Variant::Color3uint8 { r, g, b } if rest.is_empty() => match first {
            "r" => Some(f64::from(*r) / 255.0),
            "g" => Some(f64::from(*g) / 255.0),
            "b" => Some(f64::from(*b) / 255.0),
            _ => None,
        },
        Variant::UDim(d) if rest.is_empty() => match first {
            "scale" => Some(f64::from(d.scale)),
            "offset" => Some(f64::from(d.offset)),
            _ => None,
        },
        Variant::UDim2(d) => match first {
            "x" => component(&Variant::UDim(d.x), rest),
            "y" => component(&Variant::UDim(d.y), rest),
            _ => None,
        },
        Variant::NumberRange(r) if rest.is_empty() => match first {
            "min" => Some(f64::from(r.min)),
            "max" => Some(f64::from(r.max)),
            _ => None,
        },
        Variant::Rect(r) => match first {
            "min" => component(&Variant::Vector2(r.min), rest),
            "max" => component(&Variant::Vector2(r.max), rest),
            _ => None,
        },
        _ => None,
    }
}

fn convert(db: &ReflectionDatabase, variant: &Variant, enum_type: Option<&str>) -> Val {
    match variant {
        Variant::String(text) => Val::Str(text.clone()),
        Variant::Bool(flag) => Val::Bool(*flag),
        Variant::Int32(n) => Val::Num(f64::from(*n)),
        Variant::Int64(n) => Val::Num(*n as f64),
        Variant::Float32(n) => Val::Num(f64::from(*n)),
        Variant::Float64(n) => Val::Num(*n),
        Variant::Enum(n) => Val::Item {
            value: f64::from(*n),
            name: enum_type
                .and_then(|kind| db.enum_name(kind, *n))
                .unwrap_or_default()
                .to_string(),
        },
        Variant::BrickColor(n) => Val::Item {
            value: f64::from(*n),
            name: BrickColor::from_number(*n)
                .map(|color| color.name.to_string())
                .unwrap_or_default(),
        },
        Variant::Content(Content::Uri(uri)) => Val::Str(uri.clone()),
        _ => Val::Other,
    }
}

fn truthy(val: &Val) -> bool {
    match val {
        Val::Bool(flag) => *flag,
        Val::Num(n) => *n != 0.0,
        Val::Str(text) => !text.is_empty(),
        Val::Item { value, .. } => *value != 0.0,
        Val::Other => false,
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-4 * a.abs().max(b.abs()).max(1.0)
}

fn equal(val: &Val, value: &Value) -> bool {
    match (val, value) {
        (Val::Num(a), Value::Num(b)) => close(*a, *b),
        (Val::Str(a), Value::Str(b) | Value::Word(b)) => a == b,
        (Val::Bool(a), Value::Bool(b)) => a == b,
        (Val::Item { name, .. }, Value::Str(b) | Value::Word(b)) => name.eq_ignore_ascii_case(b),
        (Val::Item { value, .. }, Value::Num(b)) => close(*value, *b),
        _ => false,
    }
}

fn compare_values(op: Op, val: &Val, value: &Value) -> bool {
    match op {
        Op::Eq => equal(val, value),
        Op::Ne => !equal(val, value),
        Op::Lt | Op::Le | Op::Gt | Op::Ge => match (val, value) {
            (Val::Num(a), Value::Num(b)) => match op {
                Op::Lt => a < b,
                Op::Le => a <= b,
                Op::Gt => a > b,
                _ => a >= b,
            },
            _ => false,
        },
        Op::Like => match (text_of(val), pattern_of(value)) {
            (Some(text), Some(pattern)) => like(&text, &pattern),
            _ => false,
        },
        Op::Contains => match (val, pattern_of(value)) {
            (Val::Str(text), Some(needle)) => text.contains(&needle),
            _ => false,
        },
    }
}

fn text_of(val: &Val) -> Option<String> {
    match val {
        Val::Str(text) => Some(text.clone()),
        Val::Item { name, .. } => Some(name.clone()),
        _ => None,
    }
}

fn pattern_of(value: &Value) -> Option<String> {
    match value {
        Value::Str(text) | Value::Word(text) => Some(text.clone()),
        _ => None,
    }
}

/// A pattern with `*` or `?` must match the whole text; a plain one need only
/// appear inside it.
fn like(text: &str, pattern: &str) -> bool {
    let text = text.to_lowercase();
    let pattern = pattern.to_lowercase();
    if pattern.contains(['*', '?']) {
        let text: Vec<char> = text.chars().collect();
        let pattern: Vec<char> = pattern.chars().collect();
        glob(&pattern, &text)
    } else {
        text.contains(&pattern)
    }
}

fn glob(pattern: &[char], text: &[char]) -> bool {
    let Some((&head, rest)) = pattern.split_first() else {
        return text.is_empty();
    };
    if head == '*' {
        return (0..=text.len()).any(|skip| glob(rest, &text[skip..]));
    }
    match text.split_first() {
        Some((&c, tail)) => (head == '?' || head == c) && glob(rest, tail),
        None => false,
    }
}
