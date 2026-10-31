//! The `0x97` runtime dictionary: the schema every instance record
//! and live update in this session is decoded against. Ported from
//! the reference's own pure parser (see
//! `docs/research/replication-and-place-guide.md` §4, live-verified
//! there against two public game schemas), with the roster's interest
//! first-class: the `Collaborator` class and `CollaboratorsService`
//! events it names are the players in the Team Create, so the class
//! table is keyed by name and never by a sample's numeric ids — the
//! two public schemas already disagree (`Collaborator` is 141 in the
//! `50 d3` sample and 140 in `a3 cf`, its `UserId` global id 2106
//! there and 2045 in the other). Parse each session's own.
//!
//! Wire (the *decompressed* `0x97` body, §4.3 — `varint` is unsigned
//! LEB128, `string` is a varint byte length then the bytes):
//!
//! ```text
//! varint  schema_version          (observed 3, the only layout known)
//! u64 LE  schema_flags            (observed 15, retained not enforced)
//! varint  enum_count, then per enum: string name, u8 marker (07)
//! varint  class_count
//! varint  property_bound          (a preallocation bound, not a property)
//! varint  event_bound
//!   per class: string name, varint member_count,
//!     per member: string name, u8 tag,
//!       tag 7 → u16 LE extra (enum table index),
//!       tag 60 → varint extra (descriptor metadata),
//!     then u16 LE class flags, varint event_count,
//!     per event: string name, u8 arg count, then per arg
//!       u8 tag, u8 extra_low, u8 extra_high
//! varint  url_prefix_count, then strings
//! varint  shared_name_count, then strings
//! the rest is an opaque tail, retained unread
//! ```
//!
//! Class-local slots versus global ids (§4.4): a record's property
//! byte is an absolute slot in its class's member list, while a live
//! `0x03` update carries a global `u16` id built by concatenating
//! every class's members in order — `member_base + slot`, where
//! `member_base` is the sum of all earlier classes' member counts.
//! Events concatenate the same way. The property table is not
//! deduplicated by name across classes (inherited-looking members
//! recur), so lookups are always (class, member).

/// Schema version the layout below is known for; another version's
/// prefix could mean anything, so it fails closed naming itself.
const KNOWN_VERSION: u64 = 3;

/// Caps that make a corrupt or hostile count fail before it
/// allocates: the real samples are far inside every one (606 enums,
/// 636 classes, bounds 10301/7003), and a schema is a few hundred
/// kilobytes at most, never gigabytes.
const MAX_TABLE: u64 = 100_000;
const MAX_PER_CLASS: u64 = 10_000;
const MAX_EVENT_ARGS: u64 = 256;
const MAX_STRING: u64 = 4096;

/// The whole dictionary, plus the derived lookup tables.
#[derive(Debug)]
pub struct Schema {
    pub version: u64,
    pub flags: u64,
    pub enums: Vec<SchemaEnum>,
    pub classes: Vec<Class>,
    /// The schema's own preallocation bound; ids stay under it.
    pub property_bound: u64,
    pub event_bound: u64,
    pub url_prefixes: Vec<String>,
    /// The ordered shared-name table the tag-49 Name format indexes
    /// (§4.6) — a different thing from the keyed shared strings of a
    /// flag-1 batch.
    pub shared_names: Vec<String>,
    /// The decompressed bytes past the tables: retained unread, the
    /// reference does the same.
    pub tail: Vec<u8>,
}

#[derive(Debug)]
pub struct SchemaEnum {
    pub name: String,
    /// Observed `07`; kept, not interpreted.
    pub marker: u8,
}

/// One class: its ordered member list is the slot table a record's
/// property bytes index, and its event list is part of the global
/// event-id concatenation.
#[derive(Debug)]
pub struct Class {
    pub name: String,
    pub members: Vec<Member>,
    pub events: Vec<Event>,
    /// Observed flags, retained (the reference does not interpret
    /// them either).
    pub flags: u16,
    /// The global property id of member slot 0: every earlier class's
    /// member count summed. `member_base + slot` is the `u16` a live
    /// `0x03` property update carries.
    pub member_base: u32,
    /// The same concatenation over class event lists: `event_base +
    /// index` is the global event id a `0x07` item carries.
    pub event_base: u32,
    member_by_name: std::collections::HashMap<String, usize>,
    event_by_name: std::collections::HashMap<String, usize>,
}

#[derive(Debug)]
pub struct Member {
    pub name: String,
    /// The value tag that decides the wire shape (§5); a record's
    /// reader consumes the value by this, in the member's context.
    pub tag: u8,
    pub extra: Extra,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Extra {
    /// Tags other than 7 and 60 carry nothing.
    None,
    /// Tag 7: an enum table index (`Collaborator.Status`'s is what
    /// names `CollaboratorStatus` in the public sample).
    Enum(u16),
    /// Tag 60: descriptor metadata, retained unread.
    Descriptor(u64),
}

#[derive(Debug)]
pub struct Event {
    pub name: String,
    pub args: Vec<EventArg>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct EventArg {
    pub tag: u8,
    /// Two per-argument bytes the reference retains without
    /// interpreting; kept for the same reason.
    pub extra_low: u8,
    pub extra_hi: u8,
}

impl Schema {
    /// Parses the *decompressed* `0x97` body (what `replication`'s
    /// join-data inflate produced). Fails closed on an unsupported
    /// version, a truncated or overflowing table, or a string that
    /// runs past its end — the tail is the only thing kept unread.
    pub fn parse(inner: &[u8]) -> Result<Schema, String> {
        let mut r = Reader { b: inner, at: 0 };
        let version = r.varint("schema version")?;
        if version != KNOWN_VERSION {
            return Err(format!("unsupported schema version {version}"));
        }
        let flags = r.u64_le("schema flags")?;
        let enum_count = r.capped_count("enum count", MAX_TABLE)?;
        let mut enums = Vec::new();
        for _ in 0..enum_count {
            let name = r.string("enum name")?;
            let marker = r.u8("enum marker")?;
            enums.push(SchemaEnum { name, marker });
        }
        let class_count = r.capped_count("class count", MAX_TABLE)?;
        let property_bound = r.varint("property bound")?;
        let event_bound = r.varint("event bound")?;
        let mut classes = Vec::new();
        for _ in 0..class_count {
            let name = r.string("class name")?;
            let member_count = r.capped_count("member count", MAX_PER_CLASS)? as usize;
            let mut members = Vec::with_capacity(member_count);
            for _ in 0..member_count {
                let name = r.string("member name")?;
                let tag = r.u8("member tag")?;
                let extra = match tag {
                    7 => Extra::Enum(r.u16_le("enum extra")?),
                    60 => Extra::Descriptor(r.varint("descriptor extra")?),
                    _ => Extra::None,
                };
                members.push(Member { name, tag, extra });
            }
            let flags = r.u16_le("class flags")?;
            let event_count = r.capped_count("event count", MAX_PER_CLASS)? as usize;
            let mut events = Vec::with_capacity(event_count);
            for _ in 0..event_count {
                let name = r.string("event name")?;
                let arg_count = r.capped_count("event arg count", MAX_EVENT_ARGS)? as usize;
                let mut args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    let tag = r.u8("argument tag")?;
                    let extra_low = r.u8("argument extra low")?;
                    let extra_hi = r.u8("argument extra high")?;
                    args.push(EventArg {
                        tag,
                        extra_low,
                        extra_hi,
                    });
                }
                events.push(Event { name, args });
            }
            classes.push(Class {
                member_by_name: by_name(members.iter().map(|m| m.name.clone())),
                event_by_name: by_name(events.iter().map(|e| e.name.clone())),
                name,
                members,
                events,
                flags,
                // Filled once the counts are final, below.
                member_base: 0,
                event_base: 0,
            });
        }
        let url_prefix_count = r.capped_count("url prefix count", MAX_TABLE)?;
        let mut url_prefixes = Vec::new();
        for _ in 0..url_prefix_count {
            url_prefixes.push(r.string("url prefix")?);
        }
        let shared_name_count = r.capped_count("shared name count", MAX_TABLE)?;
        let mut shared_names = Vec::new();
        for _ in 0..shared_name_count {
            shared_names.push(r.string("shared name")?);
        }
        // The concatenated-id bases: every earlier class's members
        // (events) summed, per §4.4. u32 is generous — the bound is
        // ten thousand-ish in every sample seen.
        let mut member_base = 0u32;
        let mut event_base = 0u32;
        for class in &mut classes {
            class.member_base = member_base;
            class.event_base = event_base;
            member_base = member_base.saturating_add(class.members.len() as u32);
            event_base = event_base.saturating_add(class.events.len() as u32);
        }
        Ok(Schema {
            version,
            flags,
            enums,
            classes,
            property_bound,
            event_bound,
            url_prefixes,
            shared_names,
            tail: r.rest().to_vec(),
        })
    }

    /// A class by name — the way everything should look ids up, never
    /// by a sample's numeric index.
    pub fn class(&self, name: &str) -> Option<&Class> {
        self.classes.iter().find(|c| c.name == name)
    }

    /// The class's index in the schema's ordered class table.
    pub fn class_index(&self, name: &str) -> Option<u32> {
        self.classes.iter().position(|c| c.name == name).map(|i| i as u32)
    }
}

impl Member {
    /// The enum table index a tag-7 member names (`Status`'s is what
    /// resolves to `CollaboratorStatus`); `None` for other tags.
    pub fn enum_index(&self) -> Option<u16> {
        match self.extra {
            Extra::Enum(i) => Some(i),
            _ => None,
        }
    }
}

impl Class {
    /// The class-local slot a record's property byte would carry for
    /// this member. A slot past 254 can never appear in a record (the
    /// loop's `ff` ends it) but still counts toward global ids.
    pub fn member_slot(&self, name: &str) -> Option<usize> {
        self.member_by_name.get(name).copied()
    }

    /// The member at a class-local slot.
    pub fn member(&self, slot: usize) -> Option<&Member> {
        self.members.get(slot)
    }

    /// The global property id a live `0x03` update carries for this
    /// member: `member_base + slot`.
    pub fn global_property_id(&self, name: &str) -> Option<u32> {
        self.member_slot(name)
            .map(|slot| self.member_base + slot as u32)
    }

    /// The global event id a `0x07` item carries for this event.
    pub fn global_event_id(&self, name: &str) -> Option<u32> {
        self.event_by_name.get(name).map(|i| self.event_base + *i as u32)
    }

    /// An event by name, with its argument descriptors — what a
    /// `0x07` invocation's arguments are encoded against.
    pub fn event(&self, name: &str) -> Option<&Event> {
        self.event_by_name.get(name).map(|i| &self.events[*i])
    }
}

/// First-wins name → position map, so a duplicated name inside one
/// class stays addressable instead of panicking or shadowing later.
fn by_name(names: impl Iterator<Item = String>) -> std::collections::HashMap<String, usize> {
    let mut out = std::collections::HashMap::new();
    for (i, name) in names.enumerate() {
        out.entry(name).or_insert(i);
    }
    out
}

/// A bounded cursor: every read names what it ran out of, and a count
/// is capped before it can allocate.
struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self, what: &str) -> Result<u8, String> {
        let at = self.take(1, what)?;
        Ok(self.b[at])
    }

    fn u16_le(&mut self, what: &str) -> Result<u16, String> {
        let at = self.take(2, what)?;
        Ok(u16::from_le_bytes([self.b[at], self.b[at + 1]]))
    }

    fn u64_le(&mut self, what: &str) -> Result<u64, String> {
        let at = self.take(8, what)?;
        Ok(u64::from_le_bytes(self.b[at..at + 8].try_into().unwrap()))
    }

    /// Unsigned LEB128, rejecting a 10-byte overlong encoding the way
    /// the reference's reader does.
    fn varint(&mut self, what: &str) -> Result<u64, String> {
        let mut v = 0u64;
        for k in 0..10 {
            let b = self.u8(what)?;
            v |= u64::from(b & 0x7f) << (7 * k);
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(format!("{what}: varint overlong"))
    }

    fn string(&mut self, what: &str) -> Result<String, String> {
        let len = self.varint(what)?;
        if len > MAX_STRING {
            return Err(format!("{what}: string length {len} over cap"));
        }
        let at = self.take(len as usize, what)?;
        // Lossy: names are looked up by their proper spelling, so one
        // odd byte making a name unfindable beats losing the whole
        // session's dictionary over it.
        Ok(String::from_utf8_lossy(&self.b[at..at + len as usize]).into_owned())
    }

    fn capped_count(&mut self, what: &str, cap: u64) -> Result<u64, String> {
        let v = self.varint(what)?;
        if v > cap {
            return Err(format!("{what}: {v} over cap {cap}"));
        }
        Ok(v)
    }

    fn take(&mut self, n: usize, what: &str) -> Result<usize, String> {
        let end = self.at.checked_add(n).ok_or(format!("{what}: overflow"))?;
        if end > self.b.len() {
            return Err(format!("{what}: truncated at {}", self.at));
        }
        let at = self.at;
        self.at = end;
        Ok(at)
    }

    fn rest(&self) -> &'a [u8] {
        &self.b[self.at.min(self.b.len())..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LEB128, as the tests build their fixtures.
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
        out.extend_from_slice(text.as_bytes());
        out
    }

    /// A member: name, tag, and the tag's extra when it has one.
    fn m(name: &str, tag: u8, extra: &[u8]) -> Vec<u8> {
        let mut out = s(name);
        out.push(tag);
        out.extend_from_slice(extra);
        out
    }

    /// An event: name, then (tag) argument triples.
    fn e(name: &str, args: &[(u8, u8, u8)]) -> Vec<u8> {
        let mut out = s(name);
        out.extend_from_slice(&v(args.len() as u64));
        for (tag, lo, hi) in args {
            out.extend_from_slice(&[*tag, *lo, *hi]);
        }
        out
    }

    /// The guide's own Collaborator facts as a schema: the classes
    /// the roster lives off, one enum-extra member (`Status`), one
    /// descriptor-extra member (`Capabilities`), and an event list
    /// whose global ids the CollaboratorsService signals occupy.
    /// Numeric ids here are this fixture's own — the two public
    /// schemas disagree with each other, which is the point of
    /// looking everything up by name.
    fn fixture() -> Vec<u8> {
        let mut out = v(3); // version
        out.extend_from_slice(&15u64.to_le_bytes()); // flags
        out.extend_from_slice(&v(2)); // two enums
        out.extend_from_slice(&s("FormFactor"));
        out.push(0x07);
        out.extend_from_slice(&s("CollaboratorStatus"));
        out.push(0x07);
        out.extend_from_slice(&v(4)); // four classes
        out.extend_from_slice(&v(15)); // property bound
        out.extend_from_slice(&v(5)); // event bound
        // Folder: the §7 worked example's shape.
        out.extend_from_slice(&s("Folder"));
        out.extend_from_slice(&v(2));
        out.extend_from_slice(&m("Archivable", 9, b""));
        out.extend_from_slice(&m("Name", 49, b""));
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&v(0)); // no events
        // Collaborator: the players in the session, at the same slots
        // the public 50 d3 sample's table gives them — including its
        // unlisted slot-1 member, named here as a placeholder because
        // the guide does not name it. That member's presence is what
        // shifts every later slot, which is why nothing downstream
        // may hardcode the numbers.
        out.extend_from_slice(&s("Collaborator"));
        out.extend_from_slice(&v(9));
        out.extend_from_slice(&m("CFrame", 27, b""));
        out.extend_from_slice(&m("Slot1", 3, b""));
        out.extend_from_slice(&m("CollaboratorColor3", 19, b""));
        out.extend_from_slice(&m("CurDocGUID", 2, b""));
        out.extend_from_slice(&m("CurScriptLineNumber", 10, b""));
        out.extend_from_slice(&m("IsIdle", 9, b""));
        out.extend_from_slice(&m("Status", 7, &1u16.to_le_bytes()));
        out.extend_from_slice(&m("UserId", 44, b""));
        out.extend_from_slice(&m("Username", 2, b""));
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&v(0));
        // CollaboratorsService: the roster's lifecycle signals, tags
        // straight out of the guide's sample.
        out.extend_from_slice(&s("CollaboratorsService"));
        out.extend_from_slice(&v(0)); // no members
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&v(5));
        out.extend_from_slice(&e(
            "CollaboratorIdleUpdate",
            &[(44, 0, 0), (9, 0, 0)],
        ));
        out.extend_from_slice(&e("CollaboratorInstanceCreatedSignal", &[(44, 0, 0)]));
        out.extend_from_slice(&e("CollaboratorInstanceDestroyedSignal", &[(44, 0, 0)]));
        out.extend_from_slice(&e(
            "CollaboratorStatusUpdateRequestedSignal",
            &[(44, 0, 0), (7, 0, 0)],
        ));
        out.extend_from_slice(&e(
            "CollaboratorStatusUpdatedSignal",
            &[(44, 0, 0), (7, 0, 0)],
        ));
        // Player: exercises the tag-60 descriptor extra.
        out.extend_from_slice(&s("Player"));
        out.extend_from_slice(&v(4));
        out.extend_from_slice(&m("DisplayName", 2, b""));
        out.extend_from_slice(&m("UserId", 44, b""));
        out.extend_from_slice(&m("Name", 49, b""));
        out.extend_from_slice(&m("Capabilities", 60, &v(7)));
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&v(0));
        out.extend_from_slice(&v(2)); // two url prefixes
        out.extend_from_slice(&s("rbxasset://"));
        out.extend_from_slice(&s("rbxassetid://"));
        out.extend_from_slice(&v(2)); // two shared names
        out.extend_from_slice(&s("Room"));
        out.extend_from_slice(&s("Block"));
        out.extend_from_slice(&[0xab; 12]); // opaque tail
        out
    }

    #[test]
    fn the_dictionary_parses_and_answers_by_name() {
        let schema = Schema::parse(&fixture()).expect("fixture parses");
        assert_eq!(schema.version, 3);
        assert_eq!(schema.flags, 15);
        assert_eq!(schema.enums.len(), 2);
        assert_eq!(schema.enums[1].name, "CollaboratorStatus");
        assert_eq!(schema.property_bound, 15);
        assert_eq!(schema.event_bound, 5);
        assert_eq!(schema.url_prefixes, ["rbxasset://", "rbxassetid://"]);
        assert_eq!(schema.shared_names, ["Room", "Block"]);
        assert_eq!(schema.tail, [0xab; 12]);

        // Every class is found by name, in table order.
        assert_eq!(schema.class_index("Folder"), Some(0));
        assert_eq!(schema.class_index("Collaborator"), Some(1));
        assert_eq!(schema.class_index("CollaboratorsService"), Some(2));
        assert_eq!(schema.class_index("Player"), Some(3));
        assert_eq!(schema.class_index("Humanoid"), None);

        // Slots are positions in the member list, per §4.4 — the
        // guide's own Collaborator slots, unlisted slot-1 member
        // included.
        let collaborator = schema.class("Collaborator").unwrap();
        assert_eq!(collaborator.member_slot("CFrame"), Some(0));
        assert_eq!(collaborator.member_slot("Status"), Some(6));
        assert_eq!(collaborator.member_slot("UserId"), Some(7));
        assert_eq!(collaborator.member_slot("Parent"), None);
        assert_eq!(collaborator.member(0).unwrap().tag, 27);
        // Status names its enum through the tag-7 extra.
        assert_eq!(collaborator.member(6).unwrap().extra, Extra::Enum(1));
        let status = collaborator.member(6).unwrap();
        assert_eq!(status.enum_index(), Some(1));
        assert_eq!(schema.enums[status.enum_index().unwrap() as usize].name, "CollaboratorStatus");

        // Global ids: every earlier class's members summed, plus the
        // slot. Folder's 2 members put Collaborator's base at 2.
        assert_eq!(collaborator.member_base, 2);
        assert_eq!(collaborator.global_property_id("UserId"), Some(2 + 7));
        // CFrame is slot 0, so its global id is the base itself.
        assert_eq!(collaborator.global_property_id("CFrame"), Some(2));
        // Player's base counts Folder, Collaborator and the service's
        // zero members too.
        let player = schema.class("Player").unwrap();
        assert_eq!(player.member_base, 11);
        assert_eq!(player.global_property_id("Name"), Some(11 + 2));
        // The tag-60 descriptor extra survives.
        assert_eq!(player.member(3).unwrap().extra, Extra::Descriptor(7));

        // Events concatenate the same way; CollaboratorsService is the
        // first class with any, so its ids start at 0.
        let service = schema.class("CollaboratorsService").unwrap();
        assert_eq!(service.event_base, 0);
        assert_eq!(
            service.global_event_id("CollaboratorInstanceCreatedSignal"),
            Some(1)
        );
        assert_eq!(
            service.global_event_id("CollaboratorStatusUpdatedSignal"),
            Some(4)
        );
        let created = &service.events[1];
        assert_eq!(created.name, "CollaboratorInstanceCreatedSignal");
        assert_eq!(created.args, vec![EventArg { tag: 44, extra_low: 0, extra_hi: 0 }]);
    }

    #[test]
    fn a_version_or_shape_we_do_not_know_fails_naming_itself() {
        let mut other = fixture();
        other[0] = 4; // version 4
        let err = Schema::parse(&other).unwrap_err();
        assert!(err.contains("version 4"), "{err}");

        // Truncated in the middle of the class list.
        let full = fixture();
        let err = Schema::parse(&full[..full.len() - 30]).unwrap_err();
        assert!(err.contains("truncated"), "{err}");

        // A string length that runs past the end.
        let mut runaway = fixture();
        let at = runaway
            .windows(4)
            .position(|w| w == [0x04, b'R', b'o', b'o'])
            .expect("the Room shared name is in the fixture");
        runaway[at] = 0x7f; // 127-byte string from a 4-byte one
        let err = Schema::parse(&runaway).unwrap_err();
        assert!(err.contains("truncated") || err.contains("over cap"), "{err}");

        // A member count past the cap, before it can allocate. The
        // Collaborator member count is the varint just before the
        // CFrame member string.
        let mut flood = fixture();
        let at = flood
            .windows(7)
            .position(|w| w == [0x06, b'C', b'F', b'r', b'a', b'm', b'e'])
            .expect("the CFrame member is in the fixture")
            - 1;
        flood[at] = 0xff; // 8 becomes the start of a huge varint
        flood.insert(at + 1, 0xff);
        flood.insert(at + 2, 0x01);
        let err = Schema::parse(&flood).unwrap_err();
        assert!(err.contains("over cap") || err.contains("truncated"), "{err}");
    }
}
