//! Reflection-backed schema queries for the Luau intelligence engine.
//!
//! The engine asks two kinds of "what does Roblox say about this class"
//! questions. Member-shaped ones (methods, events, statics, signatures) are
//! answered by [`super::api_data`]'s generated tables; property-shaped ones
//! are answered here, against the official API dump embedded in
//! `rbx_reflection` — the same dump the rest of the editor reads.

use rbx_reflection::ReflectionDatabase;
use std::collections::BTreeMap;

pub fn class_exists(class_name: &str) -> bool {
    ReflectionDatabase::shared().class(class_name).is_some()
}

pub fn class_is_service(class_name: &str) -> bool {
    ReflectionDatabase::shared().is_service(class_name)
}

/// Like the engine's original home: a class only counts as directly creatable
/// when it is neither tagged `NotCreatable` nor a `Service` singleton.
pub fn class_is_creatable(class_name: &str) -> bool {
    let database = ReflectionDatabase::shared();
    database.is_creatable(class_name) && !database.is_service(class_name)
}

/// `Some(false)` names a real enum carrying no such item; `None` names no enum
/// at all.
pub fn enum_item_exists(enum_name: &str, item_name: &str) -> Option<bool> {
    ReflectionDatabase::shared()
        .enum_items(enum_name)
        .map(|items| items.iter().any(|(name, _)| name.as_str() == item_name))
}

/// Every reflected property of `class_name` and its ancestors, property name
/// to the dump's own type spelling (`"Vector3"`, `"bool"`, `"PartType"`).
pub fn get_class_schema_properties(class_name: &str) -> BTreeMap<String, String> {
    let database = ReflectionDatabase::shared();
    let mut out = BTreeMap::new();
    let mut current = database.class(class_name);
    while let Some(descriptor) = current {
        for property in &descriptor.properties {
            out.entry(property.name.clone())
                .or_insert_with(|| property.value_type.clone());
        }
        current = descriptor
            .superclass
            .as_deref()
            .and_then(|name| database.class(name));
    }
    out
}

/// The dump's type for a property, walking superclasses and retrying
/// case-insensitively (scripts write `size` where the dump says `Size`).
///
/// The name is capitalized the way the engine's previous reflection crate
/// debug-printed its `DataType`, because callers match on substrings like
/// `"Bool"` and `"String"`: the dump spells primitives lowercase (`bool`,
/// `string`) and those checks must keep firing.
pub fn resolve_property_type(class_name: &str, prop_name: &str) -> Option<String> {
    let database = ReflectionDatabase::shared();
    let mut current = database.class(class_name);
    while let Some(descriptor) = current {
        if let Some(property) = descriptor
            .properties
            .iter()
            .find(|property| property.name == prop_name)
            .or_else(|| {
                descriptor
                    .properties
                    .iter()
                    .find(|property| property.name.eq_ignore_ascii_case(prop_name))
            })
        {
            return Some(capitalized(&property.value_type));
        }
        current = descriptor
            .superclass
            .as_deref()
            .and_then(|name| database.class(name));
    }
    None
}

/// `"bool"` → `"Bool"`; datatypes and enum names already start uppercase and
/// pass through unchanged.
fn capitalized(name: &str) -> String {
    let mut characters = name.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}
