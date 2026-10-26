//! The place's avatar rules as Studio's Avatar Settings dock edits them:
//! `AvatarSettings` › `AvatarRules` › one `Avatar*Rules` child per area,
//! read with Studio's defaults where a file stores nothing, and written as
//! plain property writes the shell applies through its undo path.
//!
//! Studio still saves each rule's deprecated twin beside the current one
//! (a Custom Scale place carries `CustomHeight` equal to
//! `CustomWorldHeight`), so every write here keeps both in step, and a
//! read falls back to the old spelling for a file saved before the new one
//! existed.

use rbx_dom::{NumberRange, Ref, Variant, Vector3Data, WeakDom};

mod presets;
mod ranges;
mod schema;

pub(crate) use presets::*;
pub(crate) use ranges::{write_height, BUILD_RANGES, HEIGHT_RANGE, WORLD_HEIGHT};
pub(crate) use schema::{
    asset_type, Slot, SlotValue, ACCESSORY_SLOTS, BODY_SLOTS, CLIP_SLOTS, CLOTHING_SLOTS,
};

/// The service, as Studio names it in every place it saves.
pub(crate) const SERVICE: &str = "AvatarSettings";
const RULES_CLASS: &str = "AvatarRules";
const RULES_NAME: &str = "DefaultAvatarRules";

/// `AvatarRules` itself, and the six areas it splits into, one child
/// instance each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rule {
    Root,
    Body,
    Clothing,
    Accessory,
    Animation,
    Collision,
    Ability,
}

impl Rule {
    const ALL: [Rule; 6] = [
        Rule::Body,
        Rule::Clothing,
        Rule::Accessory,
        Rule::Animation,
        Rule::Collision,
        Rule::Ability,
    ];

    pub(crate) fn class(self) -> &'static str {
        match self {
            Rule::Root => RULES_CLASS,
            Rule::Body => "AvatarBodyRules",
            Rule::Clothing => "AvatarClothingRules",
            Rule::Accessory => "AvatarAccessoryRules",
            Rule::Animation => "AvatarAnimationRules",
            Rule::Collision => "AvatarCollisionRules",
            Rule::Ability => "AvatarAbilityRules",
        }
    }
}

/// One property of one rule instance.
pub(crate) type Key = (Rule, &'static str);

/// What a control asks for: properties and their new values. Applying them
/// is the shell's job, so it can snapshot for undo first.
pub(crate) type Writes = Vec<(Key, Variant)>;

/// The rule instances a place has, found by class under the service.
#[derive(Debug, Clone, Default)]
pub(crate) struct Rules {
    found: Vec<(Rule, Ref)>,
}

impl Rules {
    pub(crate) fn find(dom: &WeakDom) -> Rules {
        let child = |parent: Ref, class: &str| {
            dom.get(parent)?
                .children()
                .iter()
                .copied()
                .find(|&r| dom.get(r).is_some_and(|i| i.class() == class))
        };
        let found = service(dom)
            .and_then(|service| child(service, RULES_CLASS))
            .map(|rules| {
                let children = Rule::ALL
                    .into_iter()
                    .filter_map(|rule| Some((rule, child(rules, rule.class())?)));
                std::iter::once((Rule::Root, rules))
                    .chain(children)
                    .collect()
            })
            .unwrap_or_default();
        Rules { found }
    }

    pub(crate) fn get(&self, rule: Rule) -> Option<Ref> {
        self.found
            .iter()
            .find_map(|&(r, reference)| (r == rule).then_some(reference))
    }

    /// The stored value, Studio's default where the file has none.
    pub(crate) fn value(&self, dom: &WeakDom, (rule, name): Key) -> Variant {
        self.get(rule)
            .and_then(|r| dom.get(r)?.properties().get(name).cloned())
            .unwrap_or_else(|| schema::default(name))
    }

    pub(crate) fn enum_of(&self, dom: &WeakDom, key: Key) -> u32 {
        match self.value(dom, key) {
            Variant::Enum(value) => value,
            _ => 0,
        }
    }

    pub(crate) fn bool_of(&self, dom: &WeakDom, key: Key) -> bool {
        matches!(self.value(dom, key), Variant::Bool(true))
    }

    pub(crate) fn int_of(&self, dom: &WeakDom, key: Key) -> i64 {
        match self.value(dom, key) {
            Variant::Int64(value) => value,
            Variant::Int32(value) => value.into(),
            _ => 0,
        }
    }

    pub(crate) fn range_of(&self, dom: &WeakDom, key: Key) -> (f32, f32) {
        match self.value(dom, key) {
            Variant::NumberRange(range) => (range.min, range.max),
            _ => (0.0, 0.0),
        }
    }

    pub(crate) fn vector_of(&self, dom: &WeakDom, key: Key) -> [f32; 3] {
        match self.value(dom, key) {
            Variant::Vector3(v) => [v.x, v.y, v.z],
            _ => [0.0; 3],
        }
    }

    /// The writes in `writes` that would change something, so an edit that
    /// lands on the value already there costs no undo step.
    pub(crate) fn effective(&self, dom: &WeakDom, writes: Writes) -> Writes {
        writes
            .into_iter()
            .filter(|(key, value)| {
                let stored = self
                    .get(key.0)
                    .and_then(|r| dom.get(r)?.properties().get(key.1));
                stored != Some(value)
            })
            .collect()
    }
}

fn service(dom: &WeakDom) -> Option<Ref> {
    dom.root_refs()
        .iter()
        .copied()
        .find(|&r| dom.get(r).is_some_and(|i| i.class() == SERVICE))
}

/// Creates whatever part of the service, `AvatarRules` and its six
/// children the place lacks (an old file can hold the bare service), the
/// way Studio fills them in on load, and writes `writes`.
pub(crate) fn apply(dom: &mut WeakDom, writes: &Writes) {
    let service = service(dom).unwrap_or_else(|| dom.new_instance(SERVICE, SERVICE, None));
    let rules = Rules::find(dom)
        .get(Rule::Root)
        .unwrap_or_else(|| dom.new_instance(RULES_CLASS, RULES_NAME, Some(service)));
    for ((rule, name), value) in writes {
        let target = match Rules::find(dom).get(*rule) {
            Some(target) => target,
            None => dom.new_instance(rule.class(), rule.class(), Some(rules)),
        };
        // Only fails for an unknown referent, and `target` was just found.
        let _ = dom.set_property(target, name, value.clone());
    }
}

pub(crate) fn range(min: f32, max: f32) -> Variant {
    Variant::NumberRange(NumberRange { min, max })
}

pub(crate) fn vector(v: [f32; 3]) -> Variant {
    Variant::Vector3(Vector3Data {
        x: v[0],
        y: v[1],
        z: v[2],
    })
}

/// `GameAvatarType`, which the "…" menu sets.
pub(crate) const AVATAR_TYPE: Key = (Rule::Body, "AvatarType");

/// The avatar type writes: Studio saves it on the body rules now and on
/// `AvatarRules` itself before that, so both carry it.
pub(crate) fn set_avatar_type(value: u32) -> Writes {
    vec![
        (AVATAR_TYPE, Variant::Enum(value)),
        ((Rule::Root, "AvatarType"), Variant::Enum(value)),
    ]
}

#[cfg(test)]
mod tests;
