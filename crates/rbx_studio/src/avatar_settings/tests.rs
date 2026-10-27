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
fn a_slot_round_trips_through_the_pair_studio_saves() {
    let hair = &ACCESSORY_SLOTS[0];
    for value in [
        SlotValue::Asset(63690008),
        SlotValue::Remove,
        SlotValue::Off,
    ] {
        let dom = applied(test_place(), hair.write(value, None));
        let rules = Rules::find(&dom);
        assert_eq!(hair.read(&rules, &dom), value);
        let enabled = rules.bool_of(&dom, (Rule::Accessory, "CustomHairAccessoryEnabled"));
        assert_eq!(enabled, value != SlotValue::Off);
        // A ticked, empty row is the pair with no id, never a sentinel.
        let id = rules.int_of(&dom, (Rule::Accessory, "CustomHairAccessoryId"));
        assert_eq!(
            id,
            if let SlotValue::Asset(id) = value {
                id
            } else {
                0
            }
        );
        assert_eq!(
            rules.int_of(&dom, (Rule::Accessory, "HairAccessoryOverride")),
            0
        );
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
fn emptying_tops_ticks_every_top_kind() {
    let dom = applied(
        test_place(),
        CLOTHING_SLOTS[0].write(SlotValue::Remove, None),
    );
    let rules = Rules::find(&dom);
    for (enabled, id) in [
        ("CustomShirtAccessoryEnabled", "CustomShirtAccessoryId"),
        ("CustomTShirtAccessoryEnabled", "CustomTShirtAccessoryId"),
        ("CustomSweaterAccessoryEnabled", "CustomSweaterAccessoryId"),
    ] {
        assert!(rules.bool_of(&dom, (Rule::Clothing, enabled)), "{enabled}");
        assert_eq!(rules.int_of(&dom, (Rule::Clothing, id)), 0, "{id}");
    }
    assert!(!rules.bool_of(&dom, (Rule::Clothing, "CustomPantsAccessoryEnabled")));
    assert_eq!(CLOTHING_SLOTS[0].read(&rules, &dom), SlotValue::Remove);
}

#[test]
fn a_tops_id_lands_on_its_own_kind_like_studio() {
    // Studio filed a T-shirt typed into Tops under the T-shirt pair.
    let tops = &CLOTHING_SLOTS[0];
    let dom = applied(
        test_place(),
        tops.write(SlotValue::Asset(7178736794), Some(64)),
    );
    let rules = Rules::find(&dom);
    let id = |name| rules.int_of(&dom, (Rule::Clothing, name));
    assert_eq!(id("CustomTShirtAccessoryId"), 7178736794);
    assert_eq!(id("CustomShirtAccessoryId"), 0);
    assert!(rules.bool_of(&dom, (Rule::Clothing, "CustomSweaterAccessoryEnabled")));
    assert_eq!(tops.read(&rules, &dom), SlotValue::Asset(7178736794));
    // An unknown type falls back to the row's first kind.
    let dom = applied(test_place(), tops.write(SlotValue::Asset(5), None));
    let rules = Rules::find(&dom);
    assert_eq!(
        rules.int_of(&dom, (Rule::Clothing, "CustomShirtAccessoryId")),
        5
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

    let dom = applied(dom, BODY_SLOTS[3].write(SlotValue::Asset(86500008), None));
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
    // Studio's boxes go past the slider's 10 studs, up to 25.
    let writes = write_height(3.5, 50.0);
    assert_eq!(writes[0].1, range(3.5, 25.0));
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
    writes.extend(CLIP_SLOTS[5].write(SlotValue::Asset(507766388), None));
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
