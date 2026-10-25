use rbx_dom::{Variant, WeakDom};

use super::*;
use crate::save::Format;

fn test_place() -> WeakDom {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/tests/TestPlace.rbxl"
    );
    rbx_binary::deserialize(&std::fs::read(path).unwrap()).unwrap()
}

fn applied(mut dom: WeakDom, writes: Writes) -> WeakDom {
    apply(&mut dom, &writes);
    dom
}

#[test]
fn a_studio_saved_place_reads_as_player_choice_r15() {
    let dom = test_place();
    let rules = Rules::find(&dom);
    for rule in Rule::ALL {
        assert!(rules.get(rule).is_some(), "{rule:?}");
    }
    assert_eq!(rules.enum_of(&dom, AVATAR_TYPE), 1);
    assert_eq!(Preset::detect(&rules, &dom), Some(Preset::PlayerChoice));
    // The file predates `CustomBuildHeight`; Studio's default stands in.
    assert_eq!(BUILD_RANGES[0].read(&rules, &dom), (90.0, 105.0));
}

#[test]
fn a_bare_service_gets_its_rules_on_the_first_write() {
    let mut dom = WeakDom::new();
    dom.new_instance(SERVICE, SERVICE, None);
    let dom = applied(dom, vec![(COLLISION_MODE, Variant::Enum(2))]);
    let rules = Rules::find(&dom);
    let collision = rules.get(Rule::Collision).unwrap();
    let parent = dom.parent(collision).unwrap();
    assert_eq!(dom.get(parent).unwrap().name(), "DefaultAvatarRules");
    assert_eq!(rules.enum_of(&dom, COLLISION_MODE), 2);
    assert_eq!(dom.root_refs().len(), 1);
}

#[test]
fn a_slot_round_trips_and_keeps_its_older_twin_in_step() {
    let hair = &ACCESSORY_SLOTS[0];
    for value in [
        SlotValue::Asset(63690008),
        SlotValue::Remove,
        SlotValue::Off,
    ] {
        let dom = applied(test_place(), hair.write(value));
        let rules = Rules::find(&dom);
        assert_eq!(hair.read(&rules, &dom), value);
        let legacy = rules.bool_of(&dom, (Rule::Accessory, "CustomHairAccessoryEnabled"));
        assert_eq!(legacy, value != SlotValue::Off);
    }
}

#[test]
fn a_file_with_only_the_older_pair_still_reads() {
    let dom = applied(
        test_place(),
        vec![
            (
                (Rule::Animation, "CustomRunAnimationEnabled"),
                Variant::Bool(true),
            ),
            (
                (Rule::Animation, "CustomRunAnimationId"),
                Variant::Int64(913376220),
            ),
        ],
    );
    let rules = Rules::find(&dom);
    assert_eq!(
        CLIP_SLOTS[0].read(&rules, &dom),
        SlotValue::Asset(913376220)
    );
}

#[test]
fn emptying_tops_removes_every_top_kind() {
    let dom = applied(test_place(), CLOTHING_SLOTS[0].write(SlotValue::Remove));
    let rules = Rules::find(&dom);
    for name in [
        "ShirtAccessoryOverride",
        "TShirtAccessoryOverride",
        "SweaterAccessoryOverride",
    ] {
        assert_eq!(rules.int_of(&dom, (Rule::Clothing, name)), -1, "{name}");
    }
    assert_eq!(
        rules.int_of(&dom, (Rule::Clothing, "PantsAccessoryOverride")),
        0
    );
}

#[test]
fn consistent_gameplay_is_detected_and_a_custom_part_breaks_it() {
    let dom = applied(test_place(), Preset::ConsistentGameplay.writes());
    let rules = Rules::find(&dom);
    assert_eq!(
        Preset::detect(&rules, &dom),
        Some(Preset::ConsistentGameplay)
    );
    assert_eq!(rules.range_of(&dom, WORLD_HEIGHT), (5.5, 5.5));
    assert_eq!(
        rules.range_of(&dom, (Rule::Body, "CustomHeight")),
        (5.5, 5.5)
    );
    assert_eq!(rules.vector_of(&dom, COLLIDER_SIZE), [2.0, 3.0, 1.0]);

    let dom = applied(dom, BODY_SLOTS[3].write(SlotValue::Asset(86500008)));
    assert_eq!(Preset::detect(&Rules::find(&dom), &dom), None);

    let dom = applied(dom, Preset::PlayerChoice.writes());
    assert_eq!(
        Preset::detect(&Rules::find(&dom), &dom),
        Some(Preset::PlayerChoice)
    );
}

#[test]
fn ranges_clamp_to_the_sliders_and_come_out_ordered() {
    let writes = BUILD_RANGES[0].write(120.0, 80.0);
    assert_eq!(writes[0].1, range(0.9, 1.05));
    let writes = write_height(7.0, 1.0);
    assert_eq!(writes[0].1, range(HEIGHT_RANGE.0, 7.0));
}

#[test]
fn only_real_changes_count_as_writes() {
    let dom = test_place();
    let rules = Rules::find(&dom);
    assert!(rules
        .effective(&dom, vec![(SCALE_MODE, Variant::Enum(0))])
        .is_empty());
    assert_eq!(
        rules
            .effective(&dom, vec![(SCALE_MODE, Variant::Enum(1))])
            .len(),
        1
    );
}

#[test]
fn avatar_type_lands_on_both_spellings() {
    let dom = applied(test_place(), set_avatar_type(2));
    let rules = Rules::find(&dom);
    assert_eq!(rules.enum_of(&dom, AVATAR_TYPE), 2);
    assert_eq!(rules.enum_of(&dom, (Rule::Root, "AvatarType")), 2);
}

/// What the dock writes survives our own save in both formats, which is
/// what Studio then loads.
#[test]
fn edits_survive_a_save_and_reload() {
    let mut writes = Preset::ConsistentGameplay.writes();
    writes.extend(CLIP_SLOTS[5].write(SlotValue::Asset(507766388)));
    writes.extend(BUILD_RANGES[1].write(75.0, 95.0));
    writes.push((CLOTHING_MODE, Variant::Enum(1)));
    writes.push(((Rule::Clothing, "LimitBounds"), vector([1.0, 2.0, 3.0])));
    let dom = applied(test_place(), writes);
    for format in [Format::Binary, Format::Xml] {
        let bytes = format.encode(&dom).unwrap();
        let reloaded = match format {
            Format::Binary => rbx_binary::deserialize(&bytes).unwrap(),
            Format::Xml => rbx_xml::deserialize(std::str::from_utf8(&bytes).unwrap()).unwrap(),
        };
        let rules = Rules::find(&reloaded);
        assert_eq!(Preset::detect(&rules, &reloaded), None, "{format:?}");
        assert_eq!(
            CLIP_SLOTS[5].read(&rules, &reloaded),
            SlotValue::Asset(507766388)
        );
        assert_eq!(rules.range_of(&reloaded, WORLD_HEIGHT), (5.5, 5.5));
        let (min, max) = BUILD_RANGES[1].read(&rules, &reloaded);
        assert!((min - 75.0).abs() < 1e-3 && (max - 95.0).abs() < 1e-3);
        assert_eq!(rules.enum_of(&reloaded, CLOTHING_MODE), 1);
        assert_eq!(
            rules.vector_of(&reloaded, (Rule::Clothing, "LimitBounds")),
            [1.0, 2.0, 3.0]
        );
    }
}
