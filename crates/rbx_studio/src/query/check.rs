//! Checks a query against the API dump before it runs, so a misspelt enum
//! item is an error instead of a query that quietly matches nothing.

use std::collections::BTreeSet;

use rbx_reflection::ReflectionDatabase;

use super::lex::Op;
use super::parse::{Expr, Target, Value};
use super::QueryError;

/// Every `Property = Name` (or `!=`) whose property is an enum on every class
/// the query can meet must name one of that enum's items.
pub(super) fn enum_items(
    expr: &Expr,
    class: Option<&str>,
    db: &ReflectionDatabase,
) -> Result<(), QueryError> {
    match expr {
        Expr::Or(parts) | Expr::And(parts) => parts
            .iter()
            .try_for_each(|part| enum_items(part, class, db)),
        Expr::Not(inner) => enum_items(inner, class, db),
        Expr::Test {
            target: Target::Property(path),
            compare: Some((Op::Eq | Op::Ne, Value::Word(item) | Value::Str(item), span)),
        } if path.len() == 1 => {
            let Some(enums) = enum_types(&path[0], class, db) else {
                return Ok(());
            };
            let items: Vec<&str> = enums
                .iter()
                .filter_map(|name| db.enum_items(name))
                .flatten()
                .map(|(name, _)| name.as_str())
                .collect();
            if items.iter().any(|name| name.eq_ignore_ascii_case(item)) {
                return Ok(());
            }
            let kind = enums.iter().copied().collect::<Vec<_>>().join(" or ");
            let mut message = format!("\"{item}\" is not a {kind} item");
            let lower = item.to_lowercase();
            if let Some(near) = items
                .iter()
                .min_by_key(|name| distance(&name.to_lowercase(), &lower))
            {
                message.push_str(&format!("; did you mean \"{near}\"?"));
            }
            Err(QueryError {
                message,
                span: span.clone(),
            })
        }
        _ => Ok(()),
    }
}

/// The enums `property` holds on the classes a query over `class` can meet
/// (the class, the ones it inherits from and its subclasses), or `None` when
/// any of them holds something other than an enum, or none has it at all.
fn enum_types<'a>(
    property: &str,
    class: Option<&str>,
    db: &'a ReflectionDatabase,
) -> Option<BTreeSet<&'a str>> {
    let mut enums = BTreeSet::new();
    for name in db.class_names() {
        let reachable = class
            .is_none_or(|class| db.is_subclass_of(name, class) || db.is_subclass_of(class, name));
        if !reachable {
            continue;
        }
        let declared = db.class(name)?.properties.iter();
        for descriptor in declared.filter(|descriptor| descriptor.name == property) {
            let kind = descriptor.value_type.as_str();
            db.enum_items(kind)?;
            enums.insert(kind);
        }
    }
    (!enums.is_empty()).then_some(enums)
}

/// Levenshtein distance, over chars.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(ca != cb));
            diagonal = above;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use rbx_dom::WeakDom;
    use rbx_reflection::ReflectionDatabase;

    use super::super::{parse, QueryError};
    use super::distance;

    fn check(source: &str) -> Result<usize, QueryError> {
        let mut dom = WeakDom::new();
        dom.new_instance("Workspace", "Workspace", None);
        let db = ReflectionDatabase::embedded();
        parse(source)
            .unwrap()
            .select(&dom, &db)
            .map(|found| found.len())
    }

    #[test]
    fn a_misspelt_enum_item_is_an_error_that_suggests_the_closest() {
        let error = check("Part where Material = Plastc").unwrap_err();
        assert_eq!(
            error.message,
            "\"Plastc\" is not a Material item; did you mean \"Plastic\"?"
        );
        assert_eq!(error.span, 22..28);
    }

    #[test]
    fn the_check_covers_every_class_without_a_selector() {
        let error = check("* where Name = x or not Material != \"Neo\"").unwrap_err();
        assert_eq!(
            error.message,
            "\"Neo\" is not a Material item; did you mean \"Neon\"?"
        );
        assert_eq!(error.span, 36..41);
    }

    #[test]
    fn an_enum_path_is_checked_by_its_last_segment() {
        let error = check("BasePart where Material = Enum.Material.Wod").unwrap_err();
        assert!(error.message.ends_with("did you mean \"Wood\"?"));
        assert_eq!(error.span, 26..43);
    }

    #[test]
    fn real_items_numbers_and_patterns_are_left_alone() {
        for source in [
            "Part where Material = plastic",
            "Part where Material = Enum.Material.Neon",
            "Part where Material = 256",
            "Part where Material ~ \"Plas*\"",
            "Part where Shape = Ball",
            "* where Name = Plastc",
        ] {
            assert_eq!(check(source), Ok(0), "{source}");
        }
    }

    #[test]
    fn a_property_that_is_not_an_enum_everywhere_is_not_checked() {
        assert_eq!(check("* where Value = Plastc"), Ok(0));
    }

    #[test]
    fn distance_counts_single_char_edits() {
        assert_eq!(distance("plastc", "plastic"), 1);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("kitten", "sitting"), 3);
    }
}
