//! The guide's worked examples, as tests.
//!
//! The schema built here is synthetic but reproduces the exact indices the
//! guide's `50 d3` sample assigns: class 224 `Folder` (slot 2 `Archivable`,
//! slot 6 `Name`), class 358 `Part` (slot 2 `Anchored`, slot 59 `Name`),
//! global property ID 5523 = `Part.Anchored`, global event ID 3845 =
//! `Part.AttributeChanged`. Filler classes pad the member and event totals
//! to those numbers, exactly as the concatenated tables of §4.4 derive
//! them. The record, update, spawn and event bytes below are copied
//! verbatim from guide §7.2, §7.3, §7.4, §8.3 and §8.4.

use crate::cursor::Ref;
use crate::item::parse_message;
use crate::record::{parse_records, World};
use crate::schema::Schema;
use crate::value::Value;

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn put_string(out: &mut Vec<u8>, text: &str) {
    put_varint(out, u64::try_from(text.len()).unwrap());
    out.extend_from_slice(text.as_bytes());
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// One filler class: `members` members and `events` events, none of which
/// the examples address.
fn put_filler_class(out: &mut Vec<u8>, index: usize, members: u64, events: u64) {
    put_string(out, &format!("Filler{index}"));
    put_varint(out, members);
    for member in 0..members {
        put_string(out, &format!("m{member}"));
        out.push(9);
    }
    put_u16(out, 0);
    put_varint(out, events);
    for event in 0..events {
        put_string(out, &format!("e{event}"));
        out.push(0);
    }
}

/// The synthetic schema, laid out per guide §4.3.
pub(crate) fn guide_schema_bytes() -> Vec<u8> {
    let mut out = Vec::new();
    put_varint(&mut out, 3);
    out.extend_from_slice(&15u64.to_le_bytes());

    put_varint(&mut out, 2);
    put_string(&mut out, "FormFactor");
    out.push(7);
    put_string(&mut out, "Material");
    out.push(7);

    // 359 classes: fillers up to 224, Folder, fillers to 358, Part last.
    put_varint(&mut out, 359);
    // Part's Anchored sits at slot 2, so the classes before Part must carry
    // exactly 5521 members: Folder's 7 plus the fillers'.
    put_varint(&mut out, 5581);
    // Part.AttributeChanged sits at event index 0, so they carry exactly
    // 3845 events, all Folder-free since Folder has none.
    put_varint(&mut out, 3846);

    // Fillers 0..=223: 159 classes of 16 members and 65 of 15
    // (2544 + 975 = 3519 members); events 155 of 11 and 69 of 10
    // (1705 + 690 = 2395).
    for index in 0..224 {
        let (members, events) = if index < 159 {
            (16, if index < 155 { 11 } else { 10 })
        } else {
            (15, if index < 155 { 11 } else { 10 })
        };
        put_filler_class(&mut out, index, members, events);
    }

    // Class 224: Folder, seven members — slot 2 Archivable, slot 6 Name.
    put_string(&mut out, "Folder");
    put_varint(&mut out, 7);
    for (slot, name) in ["f0", "f1", "Archivable", "f3", "f4", "f5", "Name"]
        .into_iter()
        .enumerate()
    {
        put_string(&mut out, name);
        out.push(if slot == 6 { 49 } else { 9 });
    }
    put_u16(&mut out, 0);
    put_varint(&mut out, 0);

    // Fillers 225..=357: 133 classes of 15 members (1995, so the classes
    // before Part carry 3519 + 1995 + Folder's 7 = 5521 members, making
    // Part.Anchored's slot 2 land on global ID 5523); events 120 of 11 and
    // 13 of 10 (1320 + 130, so Part's first event lands on global ID 3845).
    for index in 225..358 {
        let events = if index < 345 { 11 } else { 10 };
        put_filler_class(&mut out, index, 15, events);
    }

    // Class 358: Part, sixty members — slot 2 Anchored, slot 59 Name — and
    // one event, AttributeChanged, with one tag-2 string argument.
    put_string(&mut out, "Part");
    put_varint(&mut out, 60);
    for slot in 0..60u16 {
        let filler = format!("p{slot}");
        let (name, tag): (&str, u8) = match slot {
            2 => ("Anchored", 9),
            59 => ("Name", 49),
            _ => (filler.as_str(), 9),
        };
        put_string(&mut out, name);
        out.push(tag);
    }
    put_u16(&mut out, 0);
    put_varint(&mut out, 1);
    put_string(&mut out, "AttributeChanged");
    out.push(1);
    out.push(2);
    out.push(0);
    out.push(0);

    put_varint(&mut out, 1);
    put_string(&mut out, "rbxasset://");
    put_varint(&mut out, 2);
    put_string(&mut out, "Room");
    put_string(&mut out, "Block");

    // The opaque tail the prefix parser retains but does not decode.
    out.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    out
}

pub(crate) fn guide_schema() -> Schema {
    let bytes = guide_schema_bytes();
    Schema::parse_prefix(&bytes).expect("the synthetic guide schema parses")
}

#[test]
fn schema_prefix_parses_with_the_guides_indices() {
    let schema = guide_schema();
    assert_eq!(schema.version, 3);
    assert_eq!(schema.flags, 15);
    assert_eq!(schema.enums, ["FormFactor", "Material"]);
    assert_eq!(schema.classes.len(), 359);

    let folder = schema.class(224).unwrap();
    assert_eq!(folder.name, "Folder");
    assert_eq!(folder.member(2).unwrap().name, "Archivable");
    assert_eq!(folder.member(2).unwrap().tag, 9);
    assert_eq!(folder.member(6).unwrap().name, "Name");
    assert_eq!(folder.member(6).unwrap().tag, 49);

    let part = schema.class(358).unwrap();
    assert_eq!(part.name, "Part");
    assert_eq!(part.member(2).unwrap().name, "Anchored");
    assert_eq!(part.member(59).unwrap().name, "Name");
    assert_eq!(part.member(59).unwrap().tag, 49);

    // The derived tables of §4.4: `Part.Anchored` is global ID 5523 and
    // `Part.AttributeChanged` is global event ID 3845, the exact numbers
    // the guide's worked update and event carry.
    assert_eq!(schema.global_property_id(358, 2), Some(5523));
    let (class, member) = schema.resolve_property(5523).unwrap();
    assert_eq!(class.name, "Part");
    assert_eq!(member.name, "Anchored");

    let (class, event) = schema.resolve_event(3845).unwrap();
    assert_eq!(class.name, "Part");
    assert_eq!(event.name, "AttributeChanged");
    assert_eq!(event.parameters.len(), 1);
    assert_eq!(event.parameters[0].tag, 2);

    assert_eq!(schema.url_prefixes, ["rbxasset://"]);
    assert_eq!(schema.shared_names, ["Room", "Block"]);
    assert_eq!(schema.tail, [0xde, 0xad, 0xbe, 0xef]);
}

/// Guide §7.2 and §7.3: the `Room` folder and the `Block` part under it,
/// verbatim.
pub(crate) const ROOM_RECORD: &[u8] = &[
    0x01, 0x64, 0x00, 0x00, 0x00, 0xe0, 0x00, 0x02, 0x01, 0x06, 0x04, b'R', b'o', b'o', b'm', 0xff,
    0x00,
];
pub(crate) const BLOCK_RECORD: &[u8] = &[
    0x01, 0x65, 0x00, 0x00, 0x00, 0x66, 0x01, 0x02, 0x01, 0x3b, 0x05, b'B', b'l', b'o', b'c', b'k',
    0xff, 0x01, 0x64, 0x00, 0x00, 0x00,
];

pub(crate) fn room_ref() -> Ref {
    Ref { lo: 1, id: 100 }
}

pub(crate) fn block_ref() -> Ref {
    Ref { lo: 1, id: 101 }
}

#[test]
fn worked_records_room_and_block() {
    let schema = guide_schema();
    let mut data = ROOM_RECORD.to_vec();
    data.extend_from_slice(BLOCK_RECORD);
    let records = parse_records(&data, &schema).unwrap();
    assert_eq!(records.len(), 2);

    assert_eq!(records[0].id, room_ref());
    assert_eq!(records[0].class, "Folder");
    assert_eq!(records[0].property("Archivable"), Some(&Value::Bool(true)));
    assert_eq!(
        records[0].property("Name"),
        Some(&Value::Name("Room".into()))
    );
    assert_eq!(records[0].parent, None);

    assert_eq!(records[1].id, block_ref());
    assert_eq!(records[1].class, "Part");
    assert_eq!(records[1].property("Anchored"), Some(&Value::Bool(true)));
    assert_eq!(
        records[1].property("Name"),
        Some(&Value::Name("Block".into()))
    );
    assert_eq!(records[1].parent, Some(room_ref()));

    // The guide §13 example's graph: DataModel → Room → Block.
    let world = World::from_records(records);
    let block = &world.records[1];
    assert_eq!(
        world.parent_of(block).unwrap().property("Name"),
        Some(&Value::Name("Room".into()))
    );
}

/// Guide §7.4: one property update flipping `Block.Anchored` to false,
/// addressed by the global property ID the schema derives.
#[test]
fn worked_update_unanchors_block() {
    let schema = guide_schema();
    let mut data = ROOM_RECORD.to_vec();
    data.extend_from_slice(BLOCK_RECORD);
    let records = parse_records(&data, &schema).unwrap();
    let mut world = World::from_records(records);

    let message: &[u8] = &[
        0x83, 0x03, 0x01, 0x65, 0x00, 0x00, 0x00, 0x93, 0x15, 0x00, 0x00,
    ];
    let items = parse_message(message, &schema).unwrap();
    assert_eq!(items.len(), 1);
    match &items[0] {
        crate::item::Item::PropertyUpdate(update) => {
            assert_eq!(update.target, block_ref());
            assert_eq!(update.property, "Anchored");
            assert_eq!(update.value, Value::Bool(false));
        }
        other => panic!("expected a property update, got {other:?}"),
    }

    world.apply(&items[0]).unwrap();
    let block = world.records.iter().find(|r| r.id == block_ref()).unwrap();
    assert_eq!(block.property("Anchored"), Some(&Value::Bool(false)));
    assert_eq!(block.property("Name"), Some(&Value::Name("Block".into())));
}

/// Guide §8.3: a spawn with two property loops — the first carries the
/// properties, the second is empty, and both terminators are required.
#[test]
fn worked_spawn_two_property_loops() {
    let schema = guide_schema();
    let mut data = ROOM_RECORD.to_vec();
    data.extend_from_slice(BLOCK_RECORD);
    let records = parse_records(&data, &schema).unwrap();
    let mut world = World::from_records(records);

    let message: &[u8] = &[
        0x83, 0x02, 0x01, 0x66, 0x00, 0x00, 0x00, 0x66, 0x01, 0x02, 0x01, 0x3b, 0x05, b'C', b'u',
        b'b', b'e', b'2', 0xff, 0xff, 0x01, 0x64, 0x00, 0x00, 0x00, 0x00,
    ];
    let items = parse_message(message, &schema).unwrap();
    assert_eq!(items.len(), 1);
    world.apply(&items[0]).unwrap();

    let cube = world
        .records
        .iter()
        .find(|r| r.property("Name") == Some(&Value::Name("Cube2".into())))
        .unwrap();
    assert_eq!(cube.class, "Part");
    assert_eq!(cube.property("Anchored"), Some(&Value::Bool(true)));
    assert_eq!(cube.parent, Some(room_ref()));
    assert_eq!(
        world.parent_of(cube).unwrap().property("Name"),
        Some(&Value::Name("Room".into()))
    );
}

/// Guide §8.4: an event whose argument tag comes from the schema's event
/// descriptor, not from the wire.
#[test]
fn worked_event_attribute_changed() {
    let schema = guide_schema();
    let message: &[u8] = &[
        0x83, 0x07, 0x01, 0x65, 0x00, 0x00, 0x00, 0x05, 0x0f, 0x04, b'T', b'e', b's', b't', 0x00,
    ];
    let items = parse_message(message, &schema).unwrap();
    assert_eq!(items.len(), 1);
    match &items[0] {
        crate::item::Item::Event(event) => {
            assert_eq!(event.target, block_ref());
            assert_eq!(event.name, "AttributeChanged");
            assert_eq!(event.arguments, [Value::Bytes(b"Test".to_vec())]);
        }
        other => panic!("expected an event, got {other:?}"),
    }
}

/// The fail-closed rule: an item this decoder has no shape for stops the
/// walk instead of guessing the next boundary.
#[test]
fn unknown_items_and_values_fail_closed() {
    let schema = guide_schema();

    // A `0x16` batch in the message head is an envelope stage two opens;
    // its flag byte is still part of the walk's contract, and a flag with
    // no established shape stops it. Flag 6 carrying one empty frame.
    let mut batch = vec![0x83_u8, 0x16, 0x06];
    let empty = zstd::bulk::compress(&[], 3).expect("compressing empty test data");
    batch.extend_from_slice(&(empty.len() as u32).to_le_bytes());
    batch.extend_from_slice(&0u32.to_le_bytes());
    batch.extend_from_slice(&empty);
    match parse_message(&batch, &schema) {
        Err(crate::cursor::ParseError::Unsupported(what)) => {
            assert!(what.contains("batch flag"), "{what}");
        }
        other => panic!("expected an unsupported error, got {other:?}"),
    }

    // A record addressing a slot the class does not have.
    let bad_slot: &[u8] = &[
        0x01, 0x64, 0x00, 0x00, 0x00, 0xe0, 0x00, 0x09, 0x01, 0xff, 0x00,
    ];
    assert!(parse_records(bad_slot, &schema).is_err());

    // Truncated inside the class index.
    let truncated: &[u8] = &[0x01, 0x64, 0x00, 0x00, 0x00, 0xe0];
    assert_eq!(
        parse_records(truncated, &schema),
        Err(crate::cursor::ParseError::Truncated)
    );
}
