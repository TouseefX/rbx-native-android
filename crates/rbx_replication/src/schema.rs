//! The runtime schema of guide §4: the dictionary a session's records and
//! live items are meaningless without.

use crate::cursor::{Cursor, ParseError, ParseResult};

/// Bounds that reject absurd inputs before anything is allocated from a
/// count. The observed public samples sat far below every one of these
/// (guide §4.3: 606 enums, 636 classes, bounds 10,301/7,003).
const MAX_ENTRIES: u64 = 65_536;
const MAX_MEMBERS_PER_CLASS: u64 = 4_096;
const MAX_PARAMETERS_PER_EVENT: u64 = 64;

/// One class member: a name, the value tag that shapes its wire form, and
/// the tag's schema metadata when it has any **[GAME]**.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub tag: u8,
    pub extra: Option<MemberExtra>,
}

/// The metadata a member carries for the two tags that have it: the enum
/// table index for tag 7, a descriptor index for tag 60 (guide §4.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberExtra {
    Enum(u16),
    Descriptor(u64),
}

/// One event argument: its value tag and the two retained metadata bytes,
/// whose meaning is per-tag and not established (guide §4.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub tag: u8,
    pub extra_low: u8,
    pub extra_high: u8,
}

/// One event of a class, with its argument descriptors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventDescriptor {
    pub name: String,
    pub parameters: Vec<Parameter>,
}

/// One class: an ordered member list — snapshot property slots index it
/// directly — plus flags and events **[GAME]**.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Class {
    pub name: String,
    pub members: Vec<Member>,
    pub flags: u16,
    pub events: Vec<EventDescriptor>,
}

impl Class {
    /// The member a class-local slot addresses, `None` when the slot is out
    /// of range for this class.
    pub fn member(&self, slot: u8) -> Option<&Member> {
        self.members.get(usize::from(slot))
    }
}

/// A parsed session schema **[GAME]**.
///
/// Built from the decompressed body of a `0x97` schema message's understood
/// prefix (guide §4.3); the opaque tail beyond it is retained verbatim.
/// Class indices, member slots and the derived global property/event IDs of
/// §4.4 are session-scoped: nothing here is a stable Roblox API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schema {
    pub version: u64,
    pub flags: u64,
    pub enums: Vec<String>,
    pub classes: Vec<Class>,
    pub property_bound: u64,
    pub event_bound: u64,
    pub url_prefixes: Vec<String>,
    /// The ordered shared-**name** table of tag 49 (guide §4.6); distinct
    /// from the keyed shared-**string** batches of `0x83/0x16` flag 1.
    pub shared_names: Vec<String>,
    /// The schema's bytes past the understood prefix, kept verbatim.
    pub tail: Vec<u8>,
    /// Global property ID `(class index, member slot)` pairs, in ID order:
    /// the concatenation of every class's ordered member list (§4.4).
    property_table: Vec<(usize, usize)>,
    /// Global event ID `(class index, event index)` pairs, in ID order.
    event_table: Vec<(usize, usize)>,
}

impl Schema {
    /// Parse the decompressed schema prefix (guide §4.3). The input starts
    /// after the `0x97` outer header's Zstandard frame — see that message's
    /// shape in §4.2; inflating it is the next stage of the port.
    pub fn parse_prefix(data: &[u8]) -> ParseResult<Self> {
        let mut c = Cursor::new(data);
        let version = c.varint()?;
        let flags = c.u64_le()?;

        // Each enum name carries a one-byte marker, observed 07, whose
        // meaning is not established (guide §4.3); it is read and dropped.
        let enum_count = count(&mut c, "enum")?;
        let mut enums = Vec::new();
        for _ in 0..enum_count {
            let name = text(&mut c, "enum name")?;
            let _marker = c.byte()?;
            enums.push(name);
        }

        let class_count = count(&mut c, "class")?;
        let property_bound = c.varint()?;
        let event_bound = c.varint()?;

        let mut classes = Vec::with_capacity(usize::try_from(class_count).unwrap_or(0));
        for _ in 0..class_count {
            let name = text(&mut c, "class name")?;
            let member_count = count_bounded(&mut c, "member", MAX_MEMBERS_PER_CLASS)?;
            let mut members = Vec::new();
            for _ in 0..member_count {
                let member_name = text(&mut c, "member name")?;
                let tag = c.byte()?;
                let extra = match tag {
                    7 => Some(MemberExtra::Enum(c.u16_le()?)),
                    60 => Some(MemberExtra::Descriptor(c.varint()?)),
                    _ => None,
                };
                members.push(Member {
                    name: member_name,
                    tag,
                    extra,
                });
            }
            let class_flags = c.u16_le()?;
            let event_count = count_bounded(&mut c, "event", MAX_MEMBERS_PER_CLASS)?;
            let mut events = Vec::new();
            for _ in 0..event_count {
                let event_name = text(&mut c, "event name")?;
                let parameter_count =
                    count_bounded(&mut c, "event parameter", MAX_PARAMETERS_PER_EVENT)?;
                let mut parameters = Vec::new();
                for _ in 0..parameter_count {
                    parameters.push(Parameter {
                        tag: c.byte()?,
                        extra_low: c.byte()?,
                        extra_high: c.byte()?,
                    });
                }
                events.push(EventDescriptor {
                    name: event_name,
                    parameters,
                });
            }
            classes.push(Class {
                name,
                members,
                flags: class_flags,
                events,
            });
        }

        let url_prefixes = names(&mut c, "URL prefix")?;
        let shared_names = names(&mut c, "shared name")?;

        // The derived tables of §4.4: global property and event IDs are the
        // concatenation of every class's ordered lists.
        let mut property_table = Vec::new();
        let mut event_table = Vec::new();
        for (class_index, class) in classes.iter().enumerate() {
            for member_index in 0..class.members.len() {
                property_table.push((class_index, member_index));
            }
            for event_index in 0..class.events.len() {
                event_table.push((class_index, event_index));
            }
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
            tail: data[c.pos()..].to_vec(),
            property_table,
            event_table,
        })
    }

    /// The class a class index names.
    pub fn class(&self, index: u16) -> ParseResult<&Class> {
        self.classes
            .get(usize::from(index))
            .ok_or_else(|| ParseError::Malformed(format!("class index {index} out of range")))
    }

    /// The member a global property ID names, with its class (guide §4.4:
    /// the ID counts members of all earlier classes first).
    pub fn resolve_property(&self, global: u16) -> Option<(&Class, &Member)> {
        let (class_index, member_index) = *self.property_table.get(usize::from(global))?;
        let class = &self.classes[class_index];
        Some((class, &class.members[member_index]))
    }

    /// The global property ID a class-local slot resolves to.
    pub fn global_property_id(&self, class: usize, slot: usize) -> Option<u16> {
        self.property_table
            .iter()
            .position(|(c, m)| *c == class && *m == slot)
            .and_then(|index| u16::try_from(index).ok())
    }

    /// The event a global event ID names, with its class (guide §4.4).
    pub fn resolve_event(&self, global: u16) -> Option<(&Class, &EventDescriptor)> {
        let (class_index, event_index) = *self.event_table.get(usize::from(global))?;
        let class = &self.classes[class_index];
        Some((class, &class.events[event_index]))
    }
}

fn count(c: &mut Cursor<'_>, what: &str) -> ParseResult<u64> {
    count_bounded(c, what, MAX_ENTRIES)
}

fn count_bounded(c: &mut Cursor<'_>, what: &str, bound: u64) -> ParseResult<u64> {
    let n = c.varint()?;
    if n > bound {
        return Err(ParseError::Malformed(format!(
            "{what} count {n} exceeds the {bound} entry bound"
        )));
    }
    Ok(n)
}

fn text(c: &mut Cursor<'_>, what: &str) -> ParseResult<String> {
    String::from_utf8(c.string()?)
        .map_err(|_| ParseError::Malformed(format!("{what} is not UTF-8; the schema is text")))
}

fn names(c: &mut Cursor<'_>, what: &str) -> ParseResult<Vec<String>> {
    let n = count(c, what)?;
    let mut out = Vec::new();
    for _ in 0..n {
        out.push(text(c, what)?);
    }
    Ok(out)
}
