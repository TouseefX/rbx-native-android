//! The replication wire format a Boblox-style receiver speaks, decoded.
//!
//! This is the first stage of the Team Create port described by
//! `replication-and-place-guide.md` at the repository root: the byte-level
//! layouts that the public Boblox game-session receiver (commit `8c46b6dc`,
//! its `solve/repl_snapshot.cpp`, `solve/repl_stream.cpp` and
//! `solve/repl_core.cpp`) was observed to use, re-derived from that guide's
//! evidence and reproduced here as a decoder with the guide's worked examples
//! as tests.
//!
//! Evidence labels, carried over from the guide:
//!
//! * **[GAME]** — observed in that public game receiver. This crate
//!   implements only `[GAME]` layouts.
//! * **[UNKNOWN]** — whether Studio's Team Create sessions use them. Nothing
//!   here proves that; the decoder is a prerequisite for finding out, not a
//!   finished Team Create client.
//!
//! What is in (all against bytes already uncompressed by whatever carries
//! them — the zstd layers of the `0x97` schema message and the `0x83`/`0x16`
//! batch item are the next stage):
//!
//! * the binary primitives of guide §3: LEB128 varints, zigzag, length-
//!   prefixed strings, and the two-word instance reference with its null
//!   sentinel;
//! * the schema prefix of guide §4.3: enums, classes, ordered members with
//!   value tags, events with argument descriptors, URL prefixes, shared
//!   names, and the retained opaque tail — plus the derived global property
//!   and event ID tables of §4.4;
//! * the value reader of guide §5 for the tags whose extent is unambiguous,
//!   keeping length-known-but-semantically-unresolved values as raw bytes;
//! * the initial instance records of guide §7.1;
//! * the live `0x83` chain items of guide §8 the guide gives exact shapes
//!   for: `0x02` spawn (no-pad form), `0x03` property update, `0x07` event,
//!   `0x23` clock and `0x01`'s single reference (semantics unknown; the
//!   guide is explicit it is not a proven delete).
//!
//! Everything else fails closed with [`ParseError::Unsupported`]: per the
//! guide's rule, an unknown item or value must stop the walk rather than
//! guess the next boundary. Unknown tags include the context-dependent
//! asset/content forms (33, 61), the NetAsset form (60), tuples and maps
//! (29–32) and the optional CFrame (51) — each needs context the decoder
//! does not yet model.

#![forbid(unsafe_code)]

pub mod cursor;
pub mod item;
pub mod record;
pub mod schema;
pub mod value;

#[cfg(test)]
mod guide_tests;

pub use cursor::{Cursor, ParseError, ParseResult, Ref};
pub use item::{Event, Item, PropertyUpdate, Spawn};
pub use record::{parse_records, Record, World};
pub use schema::{Class, EventDescriptor, Member, MemberExtra, Parameter, Schema};
pub use value::Value;
