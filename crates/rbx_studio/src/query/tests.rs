//! The query language against a small place: Workspace › Map › (Door,
//! DoorFrame, LavaPit, Runner), plus a loose part beside Map.

use std::collections::BTreeMap;

use rbx_dom::{CFrameData, Color3Data, Ref, Variant, Vector3Data, WeakDom};
use rbx_reflection::ReflectionDatabase;

use super::{parse, QueryError};

fn set(dom: &mut WeakDom, reference: Ref, name: &str, value: Variant) {
    dom.set_property(reference, name, value).unwrap();
}

fn place() -> WeakDom {
    let mut dom = WeakDom::new();
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    let map = dom.new_instance("Model", "Map", Some(workspace));
    let door = dom.new_instance("Part", "Door", Some(map));
    set(&mut dom, door, "Anchored", Variant::Bool(false));
    set(&mut dom, door, "Material", Variant::Enum(256));
    set(
        &mut dom,
        door,
        "size",
        Variant::Vector3(Vector3Data {
            x: 4.0,
            y: 0.5,
            z: 2.0,
        }),
    );
    let frame = dom.new_instance("MeshPart", "DoorFrame", Some(map));
    set(&mut dom, frame, "Anchored", Variant::Bool(true));
    let lava = dom.new_instance("Part", "LavaPit", Some(map));
    set(&mut dom, lava, "Tags", Variant::String("Lava".to_string()));
    set(
        &mut dom,
        lava,
        "Color",
        Variant::Color3(Color3Data {
            r: 1.0,
            g: 0.0,
            b: 0.0,
        }),
    );
    let mut attributes = BTreeMap::new();
    attributes.insert("Health".to_string(), Variant::Float64(75.0));
    let blob = rbx_dom::attributes::encode(&attributes, |_| None).unwrap();
    set(
        &mut dom,
        lava,
        "AttributesSerialize",
        Variant::Unknown {
            type_id: 0,
            raw: blob,
        },
    );
    let script = dom.new_instance("Script", "Runner", Some(map));
    set(
        &mut dom,
        script,
        "Source",
        Variant::String("while true do wait(1) end".to_string()),
    );
    let loose = dom.new_instance("Part", "Loose", Some(workspace));
    set(
        &mut dom,
        loose,
        "CFrame",
        Variant::CFrame(CFrameData {
            position: Vector3Data {
                x: 0.0,
                y: 5.0,
                z: 0.0,
            },
            rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        }),
    );
    dom
}

fn names(source: &str) -> Vec<String> {
    let dom = place();
    let db = ReflectionDatabase::embedded();
    parse(source)
        .unwrap()
        .select(&dom, &db)
        .unwrap()
        .into_iter()
        .map(|reference| dom.get(reference).unwrap().name().to_string())
        .collect()
}

fn parse_error(source: &str) -> QueryError {
    parse(source).unwrap_err()
}

fn select_error(source: &str) -> QueryError {
    let dom = place();
    let db = ReflectionDatabase::embedded();
    parse(source).unwrap().select(&dom, &db).unwrap_err()
}

#[test]
fn a_selector_with_conditions_filters_by_class_and_property() {
    assert_eq!(
        names("Part where Material = Plastic and Anchored = false"),
        ["Door", "LavaPit", "Loose"]
    );
}

#[test]
fn a_base_class_matches_its_subclasses() {
    assert_eq!(names("BasePart"), ["Door", "DoorFrame", "LavaPit", "Loose"]);
}

#[test]
fn a_vector_component_is_compared_as_a_number() {
    assert_eq!(names("BasePart where Size.Y < 1"), ["Door"]);
}

#[test]
fn a_lowercase_stored_name_is_found_by_its_property_name() {
    assert_eq!(
        names("BasePart where size.x >= 4 and size.y < 1 and size.z <= 2"),
        ["Door"]
    );
}

#[test]
fn a_float_stored_value_equals_its_decimal_spelling() {
    assert_eq!(names("Part where Size.Y = 0.5"), ["Door"]);
}

#[test]
fn a_match_pattern_without_wildcards_is_a_substring_test() {
    assert_eq!(
        names("* where Name ~ \"door\" under Workspace.Map"),
        ["Door", "DoorFrame"]
    );
}

#[test]
fn a_glob_must_match_the_whole_name() {
    assert_eq!(names("* where Name ~ \"Door*\""), ["Door", "DoorFrame"]);
    assert_eq!(names("* where Name ~ \"*Frame\""), ["DoorFrame"]);
    assert_eq!(names("* where Name ~ \"Doo?\""), ["Door"]);
    assert!(names("* where Name ~ \"oor*\"").is_empty());
}

#[test]
fn contains_is_a_case_sensitive_substring_test() {
    assert_eq!(names("Script where Source contains \"wait(\""), ["Runner"]);
    assert!(names("Script where Source contains \"WAIT(\"").is_empty());
}

#[test]
fn has_tag_finds_exactly_the_tagged_instances() {
    assert_eq!(names("has tag Lava"), ["LavaPit"]);
    assert_eq!(names("has tag \"Lava\""), ["LavaPit"]);
    assert!(names("has tag Lav").is_empty());
}

#[test]
fn attributes_are_compared_and_tested_for_presence() {
    assert_eq!(names("attr Health > 50"), ["LavaPit"]);
    assert!(names("attr Health > 80").is_empty());
    assert_eq!(names("attr Health = 75"), ["LavaPit"]);
    assert_eq!(names("has attr Health"), ["LavaPit"]);
    assert_eq!(names("attr Health"), ["LavaPit"]);
    assert!(names("has attr Mana").is_empty());
}

#[test]
fn and_binds_tighter_than_or() {
    assert_eq!(
        names("Name = Door or Name = Loose and ClassName = MeshPart"),
        ["Door"]
    );
    assert_eq!(
        names("Name = Door or Name = Loose and ClassName = Part"),
        ["Door", "Loose"]
    );
}

#[test]
fn parentheses_override_precedence() {
    assert_eq!(
        names("(Name = Door or Name = DoorFrame) and ClassName = MeshPart"),
        ["DoorFrame"]
    );
}

#[test]
fn not_negates_the_condition_after_it() {
    assert_eq!(
        names("BasePart where not Anchored"),
        ["Door", "LavaPit", "Loose"]
    );
    assert_eq!(names("BasePart where not not Anchored"), ["DoorFrame"]);
}

#[test]
fn keywords_ignore_case() {
    assert_eq!(names("Part WHERE Name = Door AND NOT Anchored"), ["Door"]);
}

#[test]
fn an_enum_path_compares_by_its_last_segment() {
    assert_eq!(
        names("Part where Name = Door and Material = Enum.Material.Plastic"),
        ["Door"]
    );
    assert_eq!(names("Part where Name = Door and Material = 256"), ["Door"]);
    assert_eq!(
        names("Part where Name = Door and Material = plastic"),
        ["Door"]
    );
}

#[test]
fn not_equal_is_false_when_the_property_is_missing() {
    assert!(names("* where Bogus != 1").is_empty());
    assert!(names("* where Bogus.X != 1").is_empty());
}

#[test]
fn not_equal_is_true_for_a_different_kind_of_value() {
    assert_eq!(names("Part where Name = Door and Name != 5"), ["Door"]);
}

#[test]
fn a_color_component_is_a_zero_to_one_number() {
    assert_eq!(names("Part where Color.R > 0.9"), ["LavaPit"]);
}

#[test]
fn position_reads_the_cframe_translation() {
    assert_eq!(names("Part where Position.Y > 4"), ["Loose"]);
    assert_eq!(names("Part where CFrame.Position.Y = 5"), ["Loose"]);
}

#[test]
fn an_unstored_property_compares_as_its_class_default() {
    assert_eq!(
        names("Part where Name = Loose and Anchored = false"),
        ["Loose"]
    );
    assert_eq!(
        names("Part where Name = Loose and Material = Plastic"),
        ["Loose"]
    );
}

#[test]
fn results_come_depth_first_in_file_order() {
    assert_eq!(
        names("*"),
        [
            "Workspace",
            "Map",
            "Door",
            "DoorFrame",
            "LavaPit",
            "Runner",
            "Loose"
        ]
    );
}

#[test]
fn under_matches_strict_descendants_only() {
    assert_eq!(
        names("under Workspace.Map"),
        ["Door", "DoorFrame", "LavaPit", "Runner"]
    );
    assert_eq!(names("Model under Workspace"), ["Map"]);
    assert!(names("Model under Workspace.Map").is_empty());
}

#[test]
fn a_bare_property_is_a_truthiness_test() {
    assert_eq!(names("where Anchored"), ["DoorFrame"]);
    assert_eq!(names("Script where Source"), ["Runner"]);
}

#[test]
fn an_unterminated_string_spans_to_the_end() {
    let error = parse_error("Name = \"abc");
    assert_eq!(error.message, "unterminated string");
    assert_eq!(error.span, 7..11);
}

#[test]
fn a_missing_value_names_the_operator() {
    let error = parse_error("Name =");
    assert_eq!(error.message, "expected a value after \"=\"");
    assert_eq!(error.span, 5..6);
}

#[test]
fn an_unbalanced_paren_points_at_the_opening_one() {
    let error = parse_error("(Name = x");
    assert_eq!(error.message, "expected \")\" to close this \"(\"");
    assert_eq!(error.span, 0..1);
}

#[test]
fn trailing_garbage_is_reported_where_it_starts() {
    let error = parse_error("Part where Name = x foo");
    assert_eq!(error.message, "unexpected \"foo\" after the query");
    assert_eq!(error.span, 20..23);
}

#[test]
fn has_must_be_followed_by_tag_or_attr() {
    let error = parse_error("has foo");
    assert_eq!(
        error.message,
        "\"has\" must be followed by \"tag\" or \"attr\""
    );
    assert_eq!(error.span, 0..3);
}

#[test]
fn an_unknown_character_is_an_error_at_it() {
    let error = parse_error("Name # 1");
    assert_eq!(error.message, "unexpected \"#\"");
    assert_eq!(error.span, 5..6);
}

#[test]
fn an_unknown_class_is_an_error_at_the_selector() {
    let error = select_error("Prat where Name = x");
    assert_eq!(error.message, "unknown class \"Prat\"");
    assert_eq!(error.span, 0..4);
}

#[test]
fn an_unresolved_under_path_is_an_error_at_the_path() {
    let error = select_error("* under Workspace.Mpa");
    assert_eq!(error.message, "nothing called \"Workspace.Mpa\"");
    assert_eq!(error.span, 8..21);
}

#[test]
fn an_empty_query_asks_for_one() {
    for source in ["", "   "] {
        let error = parse_error(source);
        assert_eq!(error.message, "type a query");
        assert_eq!(error.span, 0..0);
    }
}

#[test]
fn an_error_displays_its_column() {
    let error = parse_error("Name # 1");
    assert_eq!(error.to_string(), "unexpected \"#\" (column 6)");
}

fn walk(dom: &WeakDom, references: &[Ref], out: &mut Vec<Ref>, db: &ReflectionDatabase) {
    for &reference in references {
        let instance = dom.get(reference).unwrap();
        if db.is_subclass_of(instance.class(), "BasePart") {
            out.push(reference);
        }
        walk(dom, instance.children(), out, db);
    }
}

#[test]
fn base_part_in_the_test_place_matches_a_manual_walk() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/tests/TestPlace.rbxl"
    );
    let dom = rbx_binary::deserialize(&std::fs::read(path).unwrap()).unwrap();
    let db = ReflectionDatabase::embedded();
    let mut expected = Vec::new();
    walk(&dom, dom.root_refs(), &mut expected, &db);
    let found = parse("BasePart").unwrap().select(&dom, &db).unwrap();
    assert!(!found.is_empty());
    assert_eq!(found, expected);
}
