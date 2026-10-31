//! The client→server writers — the direction the reference never
//! implemented: Boblox's game code parses incoming `0x03` property
//! updates and `0x07` events, but its upstream-owned writes were
//! never built, and a receiver alone cannot tell the server anything.
//! The pinned Studio executable's own writers (see
//! `docs/research/replication-and-place-guide.md` §11.7) produce the
//! same item shapes its receiver parses, which is what this module
//! encodes:
//!
//! - `0x03` (the `ChangePropertyItem` path): the target reference,
//!   the member's global property id as `u16` LE, then the value in
//!   the member's wire shape. The value's tag is **not** on the wire
//!   — the receiver's schema supplies it, so the writer must be given
//!   the member's tag and encode accordingly.
//! - `0x07` (the `EventInvocationItem` path): the target reference,
//!   the event's global id as `u16` LE, then every argument in its
//!   own tag's shape, in the schema's order.
//! - The `0x83` chain envelope around them, ending in `00`.
//!
//! The reference itself, as the pinned writer emits it (§11.7.1): the
//! scope as an unsigned varint, then the object number as four raw
//! little-endian bytes — and for a null reference (or the scope
//! sentinel `0xffffffff`) a lone varint zero with no object word. A
//! zero scope is the replicator's default, never written as zero.
//!
//! What this module deliberately does **not** do: decide the
//! containing envelope, ordering, acknowledgement or echo lifecycle
//! of a real Team Create send (§11.6). Those are not established for
//! an unmodified Studio session, so nothing here is wired into the
//! live session — the encoders exist, are checked against the guide's
//! verified vectors, and wait on capture evidence for their envelope.

use crate::join_schema::Schema;

/// A network object reference: the scope (a per-replicator
/// namespace, varint on the wire) and the object number (four
/// little-endian bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetRef {
    /// The null reference: a lone varint zero, no object word.
    Null,
    Scope(u32, u32),
}

impl NetRef {
    pub fn new(scope: u32, object: u32) -> NetRef {
        NetRef::Scope(scope, object)
    }

    /// The scope with the replicator's default substituted for zero,
    /// the way the native writer does: a zero scope never reaches the
    /// wire as zero.
    pub fn scoped(scope: u32, object: u32, default_scope: u32) -> NetRef {
        if scope == 0 {
            NetRef::Scope(default_scope, object)
        } else {
            NetRef::Scope(scope, object)
        }
    }
}

/// One typed value, encoded in the wire shape the schema's tag asks
/// for — never with the tag itself on the wire.
#[derive(Clone, Debug)]
pub enum Value {
    /// Tag 9's single byte.
    Bool(bool),
    /// The varint integer family: plain for tags 7/34, zigzag for
    /// 10/44 (§5) — the schema member decides which.
    Int(i64),
    /// Tags 2/8: a varint byte length, then the bytes.
    Str(Vec<u8>),
    /// Tags 19/22/36: three little-endian f32s.
    Floats3([f32; 3]),
    /// Any shape this module does not model (a CFrame's 12+1(+6)
    /// bytes, a FontFace, …): written verbatim, caller-owned.
    Raw(Vec<u8>),
}

/// One `0x03` property-update item, the low-level form: the ids and
/// the tag are the caller's, the shape is the guide's.
pub fn property_update(
    target: NetRef,
    property: u16,
    tag: u8,
    value: &Value,
) -> Result<Vec<u8>, String> {
    let mut out = vec![0x03];
    write_ref(&mut out, target);
    out.extend_from_slice(&property.to_le_bytes());
    write_value(&mut out, tag, value)?;
    Ok(out)
}

/// One `0x07` event item, the low-level form: one tag per argument,
/// in the same order.
pub fn event_invocation(
    target: NetRef,
    event: u16,
    arg_tags: &[u8],
    args: &[Value],
) -> Result<Vec<u8>, String> {
    if arg_tags.len() != args.len() {
        return Err(format!(
            "{} arguments for {} argument tags",
            args.len(),
            arg_tags.len()
        ));
    }
    let mut out = vec![0x07];
    write_ref(&mut out, target);
    out.extend_from_slice(&event.to_le_bytes());
    for (tag, value) in arg_tags.iter().zip(args) {
        write_value(&mut out, *tag, value)?;
    }
    Ok(out)
}

/// The `0x83` chain envelope (§8.1): the items, then the end byte.
pub fn chain(items: &[&[u8]]) -> Vec<u8> {
    let mut out = vec![0x83];
    for item in items {
        out.extend_from_slice(item);
    }
    out.push(0x00);
    out
}

/// A property update for one member, everything derived from the
/// session's own schema by name — the global property id and the
/// member's value tag. This is the Collaborator-facing call
/// (`IsIdle`, `Status`, `CFrame`, …) with no id ever hardcoded.
pub fn member_update(
    schema: &Schema,
    class_name: &str,
    member: &str,
    target: NetRef,
    value: &Value,
) -> Result<Vec<u8>, String> {
    let class = schema
        .class(class_name)
        .ok_or_else(|| format!("no class {class_name} in this schema"))?;
    let slot = class
        .member_slot(member)
        .ok_or_else(|| format!("class {class_name} has no member {member}"))?;
    let member = class.member(slot).expect("the slot just resolved");
    let id = class.member_base + slot as u32;
    let id = u16::try_from(id).map_err(|_| format!("property id {id} over the u16 wire field"))?;
    property_update(target, id, member.tag, value)
}

/// An event invocation, ids and argument tags derived from the
/// session's own schema by name — the roster's lifecycle signals
/// (`CollaboratorInstanceCreatedSignal`, …) among them.
pub fn invoke_event(
    schema: &Schema,
    class_name: &str,
    event_name: &str,
    target: NetRef,
    args: &[Value],
) -> Result<Vec<u8>, String> {
    let class = schema
        .class(class_name)
        .ok_or_else(|| format!("no class {class_name} in this schema"))?;
    let event = class
        .event(event_name)
        .ok_or_else(|| format!("class {class_name} has no event {event_name}"))?;
    let id = class.global_event_id(event_name).expect("the event just resolved");
    let id = u16::try_from(id).map_err(|_| format!("event id {id} over the u16 wire field"))?;
    let tags: Vec<u8> = event.args.iter().map(|a| a.tag).collect();
    event_invocation(target, id, &tags, args)
}

/// The reference, as §11.7.1's emulation of the pinned writer
/// established it.
fn write_ref(out: &mut Vec<u8>, target: NetRef) {
    match target {
        // The native writes a lone varint zero for a null object and
        // for the scope sentinel, with no object word after it.
        NetRef::Null | NetRef::Scope(0xffff_ffff, _) => write_leb128(out, 0),
        NetRef::Scope(scope, object) => {
            write_leb128(out, u64::from(scope));
            out.extend_from_slice(&object.to_le_bytes());
        }
    }
}

/// One value in the shape `tag` asks for; a mismatch is an error,
/// never a coerced guess.
fn write_value(out: &mut Vec<u8>, tag: u8, value: &Value) -> Result<(), String> {
    match (tag, value) {
        (9, Value::Bool(v)) => out.push(u8::from(*v)),
        (7 | 34, Value::Int(v)) => write_leb128(out, *v as u64),
        (10 | 44, Value::Int(v)) => write_leb128(out, zigzag64(*v)),
        (2 | 8, Value::Str(bytes)) => {
            write_leb128(out, bytes.len() as u64);
            out.extend_from_slice(bytes);
        }
        (19 | 22 | 36, Value::Floats3(values)) => {
            for v in values {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        // Any tag this module does not model rides on Raw's bytes.
        (_, Value::Raw(bytes)) => out.extend_from_slice(bytes),
        _ => return Err(format!("tag {tag} does not carry this value")),
    }
    Ok(())
}

/// Unsigned LEB128, §3.1's varint.
fn write_leb128(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            return;
        }
    }
}

/// `signed = (unsigned >> 1) ^ -(unsigned & 1)` in reverse, so `-1`
/// is `1` on the wire (§3.1).
fn zigzag64(v: i64) -> u64 {
    (v.wrapping_shl(1) ^ (v >> 63)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LEB128, as the fixture below is built.
    fn v(mut val: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let mut b = (val & 0x7f) as u8;
            val >>= 7;
            if val != 0 {
                b |= 0x80;
            }
            out.push(b);
            if val == 0 {
                return out;
            }
        }
    }

    fn s(text: &str) -> Vec<u8> {
        let mut out = v(text.len() as u64);
        out.extend_from_slice(text.bytes());
        out
    }

    /// Part, Collaborator (at the guide's slots), and the service
    /// with the roster's lifecycle signals — the classes the writers
    /// are named against. Ids here are this fixture's own.
    fn fixture() -> Vec<u8> {
        let mut out = v(3);
        out.extend_from_slice(&15u64.to_le_bytes());
        out.extend_from_slice(&v(0)); // no enums
        out.extend_from_slice(&v(3)); // three classes
        out.extend_from_slice(&v(16)); // property bound
        out.extend_from_slice(&v(6)); // event bound
        out.extend_from_slice(&s("Part"));
        out.extend_from_slice(&v(3));
        out.extend_from_slice(&s("Anchored"));
        out.push(9);
        out.extend_from_slice(&s("Name"));
        out.push(49);
        out.extend_from_slice(&s("CFrame"));
        out.push(27);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&v(1));
        out.extend_from_slice(&s("AttributeChanged"));
        out.extend_from_slice(&v(1));
        out.extend_from_slice(&[2u8, 0, 0]);
        out.extend_from_slice(&s("Collaborator"));
        out.extend_from_slice(&v(9));
        for (name, tag) in [
            ("CFrame", 27u8),
            ("Slot1", 3),
            ("CollaboratorColor3", 19),
            ("CurDocGUID", 2),
            ("CurScriptLineNumber", 10),
            ("IsIdle", 9),
            ("Status", 7),
            ("UserId", 44),
            ("Username", 2),
        ] {
            out.extend_from_slice(&s(name));
            out.push(tag);
        }
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&v(0));
        out.extend_from_slice(&s("CollaboratorsService"));
        out.extend_from_slice(&v(0)); // no members
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&v(5));
        for (name, args) in [
            ("CollaboratorIdleUpdate", vec![(44u8, 0u8, 0u8), (9, 0, 0)]),
            ("CollaboratorInstanceCreatedSignal", vec![(44, 0, 0)]),
            ("CollaboratorInstanceDestroyedSignal", vec![(44, 0, 0)]),
            (
                "CollaboratorStatusUpdateRequestedSignal",
                vec![(44, 0, 0), (7, 0, 0)],
            ),
            ("CollaboratorStatusUpdatedSignal", vec![(44, 0, 0), (7, 0, 0)]),
        ] {
            out.extend_from_slice(&s(name));
            out.extend_from_slice(&v(args.len() as u64));
            for (tag, lo, hi) in args {
                out.extend_from_slice(&[tag, lo, hi]);
            }
        }
        out.extend_from_slice(&v(0)); // no url prefixes
        out.extend_from_slice(&v(0)); // no shared names
        out
    }

    /// §11.7.1's emulation table, byte for byte: the pinned writer's
    /// own header outputs for invented field `0x1234`.
    #[test]
    fn the_reference_writer_shapes_match_the_emulation() {
        let field = 0x1234;
        let none = Value::Raw(Vec::new());
        assert_eq!(
            property_update(NetRef::new(1, 201), field, 9, &none).unwrap(),
            [0x03, 0x01, 0xc9, 0x00, 0x00, 0x00, 0x34, 0x12]
        );
        assert_eq!(
            property_update(NetRef::new(300, 0x11223344), field, 9, &none).unwrap(),
            [0x03, 0xac, 0x02, 0x44, 0x33, 0x22, 0x11, 0x34, 0x12]
        );
        // A zero scope is the default, never a written zero.
        assert_eq!(
            property_update(NetRef::scoped(0, 201, 5), field, 9, &none).unwrap(),
            [0x03, 0x05, 0xc9, 0x00, 0x00, 0x00, 0x34, 0x12]
        );
        assert_eq!(NetRef::scoped(3, 7, 5), NetRef::Scope(3, 7));
        // Null and the scope sentinel both collapse to a lone zero.
        assert_eq!(
            property_update(NetRef::Null, field, 9, &none).unwrap(),
            [0x03, 0x00, 0x34, 0x12]
        );
        assert_eq!(
            property_update(NetRef::new(0xffff_ffff, 201), field, 9, &none).unwrap(),
            [0x03, 0x00, 0x34, 0x12]
        );
    }

    /// §7.4's worked update, whole: `Part.Anchored` id 5523, false.
    #[test]
    fn the_worked_update_chain_matches() {
        let item = property_update(NetRef::new(1, 101), 5523, 9, &Value::Bool(false)).unwrap();
        assert_eq!(
            item,
            [0x03, 0x01, 0x65, 0x00, 0x00, 0x00, 0x93, 0x15, 0x00]
        );
        assert_eq!(
            chain(&[&item]),
            [
                0x83, 0x03, 0x01, 0x65, 0x00, 0x00, 0x00, 0x93, 0x15, 0x00, 0x00,
            ]
        );
    }

    /// §10.7B's three updates for one Collaborator: a line number, an
    /// idle flip and a pose, each its own item in one chain.
    #[test]
    fn the_collaborator_updates_match() {
        let pose = Value::Raw(vec![
            0x00, 0x00, 0x30, 0x41, 0x00, 0x00, 0x80, 0x40, 0x00, 0x00, 0x00, 0xc0, 0x02,
        ]);
        let line = property_update(NetRef::new(1, 201), 2103, 10, &Value::Int(15)).unwrap();
        let idle = property_update(NetRef::new(1, 201), 2104, 9, &Value::Bool(true)).unwrap();
        let pose = property_update(NetRef::new(1, 201), 2099, 27, &pose).unwrap();
        assert_eq!(
            line,
            [0x03, 0x01, 0xc9, 0x00, 0x00, 0x00, 0x37, 0x08, 0x1e]
        );
        assert_eq!(
            idle,
            [0x03, 0x01, 0xc9, 0x00, 0x00, 0x00, 0x38, 0x08, 0x01]
        );
        assert_eq!(pose.len(), 8 + 13);
        assert_eq!(
            pose,
            [
                0x03, 0x01, 0xc9, 0x00, 0x00, 0x00, 0x33, 0x08, 0x00, 0x00, 0x30, 0x41, 0x00,
                0x00, 0x80, 0x40, 0x00, 0x00, 0x00, 0xc0, 0x02,
            ]
        );
        let mut expected = vec![0x83];
        for item in [&line, &idle, &pose] {
            expected.extend_from_slice(item);
        }
        expected.push(0x00);
        assert_eq!(chain(&[&line, &idle, &pose]), expected);
    }

    /// §8.4's worked event: `Part.AttributeChanged` id 3845, one
    /// string argument.
    #[test]
    fn the_worked_event_matches() {
        let item = event_invocation(
            NetRef::new(1, 101),
            3845,
            &[2],
            &[Value::Str(b"Test".to_vec())],
        )
        .unwrap();
        assert_eq!(
            item,
            [
                0x07, 0x01, 0x65, 0x00, 0x00, 0x00, 0x05, 0x0f, 0x04, 0x54, 0x65, 0x73, 0x74,
            ]
        );
        let mut expected = vec![0x83];
        expected.extend_from_slice(&item);
        expected.push(0x00);
        assert_eq!(chain(&[&item]), expected);
    }

    /// The value shapes §10.7A's record uses, and §3.1's zigzag.
    #[test]
    fn values_encode_in_their_tags_shapes() {
        let value = |tag: u8, v: &Value| {
            let mut out = Vec::new();
            write_value(&mut out, tag, v).unwrap();
            out
        };
        // UserId, tag 44: zigzag 42 is 0x54.
        assert_eq!(value(44, &Value::Int(42)), [0x54]);
        // CurScriptLineNumber, tag 10: zigzag 12 is 0x18.
        assert_eq!(value(10, &Value::Int(12)), [0x18]);
        // Status, tag 7: plain 2.
        assert_eq!(value(7, &Value::Int(2)), [0x02]);
        // IsIdle, tag 9.
        assert_eq!(value(9, &Value::Bool(false)), [0x00]);
        // Username, tag 2: length then bytes.
        assert_eq!(value(2, &Value::Str(b"Alex".to_vec())), [0x04, b'A', b'l', b'e', b'x']);
        // CollaboratorColor3, tag 19: three little-endian f32s.
        assert_eq!(
            value(19, &Value::Floats3([0.25, 0.5, 1.0])),
            [0x00, 0x00, 0x80, 0x3e, 0x00, 0x00, 0x00, 0x3f, 0x00, 0x00, 0x80, 0x3f]
        );
        // §3.1: encoded unsigned 1 represents -1.
        assert_eq!(value(10, &Value::Int(-1)), [0x01]);
        // A value its tag cannot carry is an error, not a guess.
        assert!(write_value(&mut Vec::new(), 9, &Value::Int(1)).is_err());
        assert!(write_value(&mut Vec::new(), 2, &Value::Bool(true)).is_err());
        assert!(write_value(&mut Vec::new(), 19, &Value::Str(vec![])).is_err());
    }

    /// The schema-keyed calls: ids and tags derived by name, the
    /// roster's own members and lifecycle signals among them.
    #[test]
    fn schema_keyed_writers_derive_ids_by_name() {
        let schema = Schema::parse(&fixture()).expect("fixture parses");
        // Collaborator's member base is Part's 3; IsIdle slot 5 → id 8.
        let idle = member_update(
            &schema,
            "Collaborator",
            "IsIdle",
            NetRef::new(1, 201),
            &Value::Bool(true),
        )
        .unwrap();
        assert_eq!(
            idle,
            [0x03, 0x01, 0xc9, 0x00, 0x00, 0x00, 0x08, 0x00, 0x01]
        );
        // CurScriptLineNumber slot 4 → id 7, tag 10 zigzag 15.
        let line = member_update(
            &schema,
            "Collaborator",
            "CurScriptLineNumber",
            NetRef::new(1, 201),
            &Value::Int(15),
        )
        .unwrap();
        assert_eq!(
            line,
            [0x03, 0x01, 0xc9, 0x00, 0x00, 0x00, 0x07, 0x00, 0x1e]
        );
        // A wire shape the member does not carry is refused.
        assert!(member_update(
            &schema,
            "Collaborator",
            "IsIdle",
            NetRef::new(1, 201),
            &Value::Int(1)
        )
        .is_err());
        assert!(member_update(
            &schema,
            "Collaborator",
            "Camera",
            NetRef::new(1, 1),
            &Value::Bool(false)
        )
        .is_err());
        assert!(member_update(
            &schema,
            "Humanoid",
            "Health",
            NetRef::new(1, 1),
            &Value::Int(1)
        )
        .is_err());
        // The service's events: Part's one event puts the base at 1,
        // so Created is id 2 with one tag-44 argument.
        let created = invoke_event(
            &schema,
            "CollaboratorsService",
            "CollaboratorInstanceCreatedSignal",
            NetRef::new(1, 200),
            &[Value::Int(42)],
        )
        .unwrap();
        assert_eq!(
            created,
            [0x07, 0x01, 0xc8, 0x00, 0x00, 0x00, 0x02, 0x00, 0x54]
        );
        // StatusUpdated: id 5, a tag-44 zigzag id and a tag-7 status.
        let status = invoke_event(
            &schema,
            "CollaboratorsService",
            "CollaboratorStatusUpdatedSignal",
            NetRef::new(1, 200),
            &[Value::Int(42), Value::Int(2)],
        )
        .unwrap();
        assert_eq!(
            status,
            [0x07, 0x01, 0xc8, 0x00, 0x00, 0x00, 0x05, 0x00, 0x54, 0x02]
        );
        // Wrong argument count is an error.
        assert!(invoke_event(
            &schema,
            "CollaboratorsService",
            "CollaboratorIdleUpdate",
            NetRef::Null,
            &[Value::Int(1)]
        )
        .is_err());
    }
}
