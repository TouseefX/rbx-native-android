//! The initial instance records of guide §7.1, and the staging world that
//! applies the guide's worked live items to them.

use crate::cursor::{Cursor, ParseError, ParseResult, Ref};
use crate::item::Item;
use crate::schema::Schema;
use crate::value::Value;

/// One instance from a snapshot batch **[GAME]**: its session reference, the
/// class the schema names, the properties its slots addressed, and its
/// parent — which may name an instance that has not arrived yet (guide
/// §5.2's delayed resolution).
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub id: Ref,
    pub class_index: u16,
    pub class: String,
    pub properties: Vec<(String, Value)>,
    pub parent: Option<Ref>,
}

impl Record {
    /// One named property's latest value, if the record carried it.
    pub fn property(&self, name: &str) -> Option<&Value> {
        self.properties
            .iter()
            .rev()
            .find(|(property, _)| property == name)
            .map(|(_, value)| value)
    }
}

/// Parse concatenated, already-decompressed snapshot records (guide §7.1):
/// reference, class index, `u8` class-local member slots each followed by
/// its schema-shaped value until `ff`, then the parent reference. There is
/// no per-record length: the member tags' extents carry the walk, so an
/// unknown tag must fail closed.
pub fn parse_records(data: &[u8], schema: &Schema) -> ParseResult<Vec<Record>> {
    let mut c = Cursor::new(data);
    let mut records = Vec::new();
    while !c.done() {
        let id = c.instance()?;
        let class_index = c.u16_le()?;
        let class = schema.class(class_index)?;
        let mut properties = Vec::new();
        loop {
            let slot = c.byte()?;
            if slot == 0xff {
                break;
            }
            let member = class.member(slot).ok_or_else(|| {
                ParseError::Malformed(format!("slot {slot} out of range for class {}", class.name))
            })?;
            let value = crate::value::read_value(&mut c, member.tag, schema)?;
            properties.push((member.name.clone(), value));
        }
        let parent = c.reference()?;
        records.push(Record {
            id,
            class_index,
            class: class.name.clone(),
            properties,
            parent,
        });
    }
    Ok(records)
}

/// A staging world over decoded records: the guide §9.2 application
/// algorithm's first slice — records land in arrival order, property
/// updates find their target by session reference, and parents stay as
/// unresolved references until a later pass resolves them.
///
/// This is deliberately not a DOM: identity is the session reference, and
/// nothing here has been proven against a live Team Create session.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    pub records: Vec<Record>,
}

impl World {
    pub fn from_records(records: Vec<Record>) -> Self {
        World { records }
    }

    fn find(&self, id: Ref) -> Option<&Record> {
        self.records.iter().find(|record| record.id == id)
    }

    fn find_mut(&mut self, id: Ref) -> Option<&mut Record> {
        self.records.iter_mut().find(|record| record.id == id)
    }

    /// Apply one live item: a spawn appends its record, a property update
    /// replaces (or appends) the named property on its target, and a
    /// batch's records land in arrival order. A batch's shared strings
    /// stay on the item for the consumer that will eventually need them;
    /// events, references with unknown semantics and clocks are left to
    /// the caller, per the guide's rule that an unapplied item must be
    /// reported, not guessed at.
    pub fn apply(&mut self, item: &Item) -> ParseResult<()> {
        match item {
            Item::Spawn(spawn) => {
                self.records.push(Record {
                    id: spawn.id,
                    class_index: spawn.class_index,
                    class: spawn.class.clone(),
                    properties: spawn.properties.clone(),
                    parent: spawn.parent,
                });
                Ok(())
            }
            Item::PropertyUpdate(update) => {
                let target = update.target;
                let record = self.find_mut(target).ok_or_else(|| {
                    ParseError::Malformed(format!(
                        "property update for unknown reference ({}, {})",
                        target.lo, target.id
                    ))
                })?;
                match record
                    .properties
                    .iter_mut()
                    .rev()
                    .find(|(name, _)| *name == update.property)
                {
                    Some(slot) => slot.1 = update.value.clone(),
                    None => record
                        .properties
                        .push((update.property.clone(), update.value.clone())),
                }
                Ok(())
            }
            Item::Batch(batch) => {
                self.records.extend(batch.records.iter().cloned());
                Ok(())
            }
            Item::Event(_) | Item::Reference(_) | Item::Clock(_) => Ok(()),
        }
    }

    /// The record a parent reference names, for tree assembly once every
    /// record has arrived.
    pub fn parent_of(&self, record: &Record) -> Option<&Record> {
        record.parent.and_then(|parent| self.find(parent))
    }
}
