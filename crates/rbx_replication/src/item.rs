//! The live `0x83` chain of guide §8: typed items carrying the session's
//! incremental changes, each walked until an unknown item must stop it.

use crate::cursor::{Cursor, ParseError, ParseResult, Ref};
use crate::message::{parse_batch, Batch};
use crate::schema::Schema;
use crate::value::Value;

/// One live chain item **[GAME]**.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// `0x02`, in its no-pad form — the form the reference chain walker
    /// passes. A spawn: its two property loops (the guide's A and B) are
    /// merged here; their prefix-mode difference only shows in the asset
    /// and content tags this stage rejects anyway.
    Spawn(Spawn),
    /// `0x03`: one property update, addressed by the schema-derived global
    /// property ID.
    PropertyUpdate(PropertyUpdate),
    /// `0x07`: one event with its schema-described arguments.
    Event(Event),
    /// `0x01`: a single reference the reference receiver parses but does
    /// not apply as a deletion. Its semantics are unknown; keeping the item
    /// makes the walk continue without claiming a meaning.
    Reference(Ref),
    /// `0x23`: a `u32`-shaped clock/tick item, meaning not established.
    Clock(u32),
    /// `0x16` as the message's first item: the whole rest of the body is
    /// one compressed batch, the same special case the reference runtime
    /// makes (guide §8.1). A batch anywhere else cannot be walked past —
    /// its trailer's extent is unresolved — and fails closed.
    Batch(Batch),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Spawn {
    pub id: Ref,
    pub class_index: u16,
    pub class: String,
    pub properties: Vec<(String, Value)>,
    pub parent: Option<Ref>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyUpdate {
    pub target: Ref,
    pub property: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub target: Ref,
    pub name: String,
    pub arguments: Vec<Value>,
}

/// Parse one logical delta message **[GAME]**: the `0x83` byte, then typed
/// items until the `00` terminator or the buffer's clean end (guide §8.1:
/// the reference walker accepts both).
pub fn parse_message(data: &[u8], schema: &Schema) -> ParseResult<Vec<Item>> {
    let mut c = Cursor::new(data);
    if c.byte()? != 0x83 {
        return Err(ParseError::Malformed("expected a 0x83 delta chain".into()));
    }
    let mut items = Vec::new();
    loop {
        if c.done() {
            break;
        }
        let kind = c.byte()?;
        if kind == 0x00 {
            break;
        }
        // A batch in the message head owns the whole rest of the body,
        // trailer and all; the chain walk cannot resume past it (guide
        // §8.1's special case).
        if kind == 0x16 && items.is_empty() {
            let body = &data[c.pos() - 1..];
            items.push(Item::Batch(parse_batch(body, schema)?));
            break;
        }
        // Anywhere else a batch cannot be walked past: its trailer's extent
        // is unresolved, so guessing where the next item starts is exactly
        // what the guide forbids.
        if kind == 0x16 {
            return Err(ParseError::Unsupported(
                "0x16 batch item outside the message head".into(),
            ));
        }
        items.push(match kind {
            0x02 => Item::Spawn(parse_spawn(&mut c, schema)?),
            0x03 => Item::PropertyUpdate(parse_update(&mut c, schema)?),
            0x07 => Item::Event(parse_event(&mut c, schema)?),
            0x01 => Item::Reference(c.instance()?),
            0x23 => Item::Clock(c.u32_le()?),
            // Every item this decoder has no established shape for: stop,
            // keeping the offset, rather than guessing where the next item
            // starts.
            other => return Err(ParseError::Unsupported(format!("chain item {other:#04x}"))),
        });
    }
    Ok(items)
}

fn parse_spawn(c: &mut Cursor<'_>, schema: &Schema) -> ParseResult<Spawn> {
    let id = c.instance()?;
    let class_index = c.u16_le()?;
    let class = schema.class(class_index)?;
    let mut properties = Vec::new();
    // Two property loops, both required even when one is empty: a reader
    // that stops at the first `ff` mistakes the second for the parent
    // reference and desynchronizes (guide §8.3).
    for _ in 0..2 {
        loop {
            let slot = c.byte()?;
            if slot == 0xff {
                break;
            }
            let member = class.member(slot).ok_or_else(|| {
                ParseError::Malformed(format!("slot {slot} out of range for class {}", class.name))
            })?;
            let value = crate::value::read_value(c, member.tag, schema)?;
            properties.push((member.name.clone(), value));
        }
    }
    let parent = c.reference()?;
    Ok(Spawn {
        id,
        class_index,
        class: class.name.clone(),
        properties,
        parent,
    })
}

fn parse_update(c: &mut Cursor<'_>, schema: &Schema) -> ParseResult<PropertyUpdate> {
    let target = c.instance()?;
    let global = c.u16_le()?;
    let (_, member) = schema.resolve_property(global).ok_or_else(|| {
        ParseError::Malformed(format!("global property ID {global} not in schema"))
    })?;
    let value = crate::value::read_value(c, member.tag, schema)?;
    Ok(PropertyUpdate {
        target,
        property: member.name.clone(),
        value,
    })
}

fn parse_event(c: &mut Cursor<'_>, schema: &Schema) -> ParseResult<Event> {
    let target = c.instance()?;
    let global = c.u16_le()?;
    let (_, descriptor) = schema
        .resolve_event(global)
        .ok_or_else(|| ParseError::Malformed(format!("global event ID {global} not in schema")))?;
    // The wire carries no per-argument tag: the event descriptor names each
    // argument's shape (guide §8.4).
    let mut arguments = Vec::with_capacity(descriptor.parameters.len());
    for parameter in &descriptor.parameters {
        arguments.push(crate::value::read_value(c, parameter.tag, schema)?);
    }
    Ok(Event {
        target,
        name: descriptor.name.clone(),
        arguments,
    })
}
