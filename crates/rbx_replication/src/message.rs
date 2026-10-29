//! The compressed envelopes of guide §4.2 and §6.1: the Zstandard layer
//! between the transport and stage one's decoder.

use crate::cursor::{Cursor, ParseError, ParseResult};
use crate::record::{parse_records, Record};
use crate::schema::Schema;

/// The cap on a claimed decompressed size, checked before anything is
/// allocated from it: a claimed size is untrusted input (guide §17).
const MAX_DECOMPRESSED: u32 = 256 * 1024 * 1024;

/// A parsed `0x97` schema message **[GAME]**: the two observed build/magic
/// bytes — `50 d3` and `a3 cf` in the public samples, meaning not
/// established (guide §4.2) — and the schema its frame carried.
#[derive(Debug)]
pub struct SchemaMessage {
    pub magic: [u8; 2],
    pub schema: Schema,
}

/// Parse one complete, already-unframed logical `0x97` message (guide
/// §4.2): the `97` byte, two magic bytes, two zero bytes, the
/// little-endian decompressed length, and one Zstandard frame running to
/// the end of the message.
pub fn parse_schema_message(data: &[u8]) -> ParseResult<SchemaMessage> {
    let mut c = Cursor::new(data);
    if c.byte()? != 0x97 {
        return Err(ParseError::Malformed(
            "expected a 0x97 schema message".into(),
        ));
    }
    let mut magic = [0u8; 2];
    magic.copy_from_slice(c.take(2)?);
    if c.take(2)? != [0, 0] {
        return Err(ParseError::Malformed(
            "the 0x97 header's zero bytes are not zero".into(),
        ));
    }
    let decompressed_len = c.u32_le()?;
    if decompressed_len > MAX_DECOMPRESSED {
        return Err(ParseError::Malformed(format!(
            "claimed decompressed size {decompressed_len} exceeds the cap"
        )));
    }
    let body = inflate(c.rest(), decompressed_len)?;
    Ok(SchemaMessage {
        magic,
        schema: Schema::parse_prefix(&body)?,
    })
}

/// One keyed shared-string entry of a flag-1 batch **[GAME]** (guide §6.3).
///
/// Distinct from the schema's ordered shared-**name** table (guide §4.6):
/// these are keyed blobs whose consumer is not established; keep them by
/// their full 16-byte key.
#[derive(Clone, Debug, PartialEq)]
pub struct SharedStringEntry {
    pub key: [u8; 16],
    pub marker: u8,
    pub value: Vec<u8>,
}

/// One inflated `0x83`/`0x16` batch **[GAME]** (guide §6.1): flags 2, 3
/// and 4 decode as instance records, flag 1 as keyed shared strings, and
/// flag 5's NetAsset-shaped data is not implemented at this stage. The
/// trailer past the frame is retained verbatim and unvalidated — eight of
/// the nine public samples opened with the record count then zeros, and
/// the ninth did not, so its semantics stay unresolved (guide §6.2).
#[derive(Clone, Debug, PartialEq)]
pub struct Batch {
    pub flags: u8,
    pub records: Vec<Record>,
    pub shared_strings: Vec<SharedStringEntry>,
    pub trailer: Vec<u8>,
}

/// Parse one `0x16` batch item, `16` byte first, from a body that runs to
/// the message's end: the flags byte, the compressed and decompressed
/// lengths, the frame of exactly the compressed length, and the trailer.
pub fn parse_batch(data: &[u8], schema: &Schema) -> ParseResult<Batch> {
    let mut c = Cursor::new(data);
    if c.byte()? != 0x16 {
        return Err(ParseError::Malformed("expected a 0x16 batch item".into()));
    }
    let flags = c.byte()?;
    let compressed_len = c.u32_le()?;
    let decoded_len = c.u32_le()?;
    if decoded_len > MAX_DECOMPRESSED {
        return Err(ParseError::Malformed(format!(
            "claimed decompressed size {decoded_len} exceeds the cap"
        )));
    }
    let frame = c.take(compressed_len as usize)?;
    let trailer = c.rest().to_vec();
    let decoded = inflate(frame, decoded_len)?;
    match flags {
        2..=4 => Ok(Batch {
            flags,
            records: parse_records(&decoded, schema)?,
            shared_strings: Vec::new(),
            trailer,
        }),
        1 => {
            let mut entries = Vec::new();
            let mut c = Cursor::new(&decoded);
            while !c.done() {
                let mut key = [0u8; 16];
                key.copy_from_slice(c.take(16)?);
                let marker = c.byte()?;
                let len = c.length()?;
                let value = c.take(len)?.to_vec();
                entries.push(SharedStringEntry { key, marker, value });
            }
            Ok(Batch {
                flags,
                records: Vec::new(),
                shared_strings: entries,
                trailer,
            })
        }
        5 => Err(ParseError::Unsupported(
            "flag-5 NetAsset-shaped batch".into(),
        )),
        other => Err(ParseError::Unsupported(format!("batch flag {other}"))),
    }
}

/// Inflate one frame to exactly the claimed length, or say why not.
fn inflate(frame: &[u8], expected: u32) -> ParseResult<Vec<u8>> {
    let decoded = zstd::bulk::decompress(frame, expected as usize)
        .map_err(|error| ParseError::Malformed(format!("decompression failed: {error}")))?;
    if decoded.len() != expected as usize {
        return Err(ParseError::Malformed(format!(
            "decompressed to {} bytes, the header claimed {expected}",
            decoded.len()
        )));
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::parse_message;

    fn frame(data: &[u8]) -> Vec<u8> {
        zstd::bulk::compress(data, 3).expect("compressing test data")
    }

    fn u32_bytes(value: u32) -> [u8; 4] {
        value.to_le_bytes()
    }

    #[test]
    fn schema_message_roundtrips() {
        let body = crate::guide_tests::guide_schema_bytes();
        let compressed = frame(&body);
        let mut message = vec![0x97, 0x50, 0xd3, 0x00, 0x00];
        message.extend_from_slice(&u32_bytes(body.len() as u32));
        message.extend_from_slice(&compressed);

        let parsed = parse_schema_message(&message).unwrap();
        assert_eq!(parsed.magic, [0x50, 0xd3]);
        assert_eq!(parsed.schema.version, 3);
        assert_eq!(parsed.schema.classes.len(), 359);
        assert_eq!(parsed.schema.class(224).unwrap().name, "Folder");
        assert_eq!(parsed.schema.class(358).unwrap().name, "Part");
        assert_eq!(parsed.schema.tail, [0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn schema_message_rejects_wrong_claimed_length() {
        let body = crate::guide_tests::guide_schema_bytes();
        let compressed = frame(&body);
        let mut message = vec![0x97, 0x50, 0xd3, 0x00, 0x00];
        message.extend_from_slice(&u32_bytes(body.len() as u32 + 1));
        message.extend_from_slice(&compressed);
        match parse_schema_message(&message) {
            Err(ParseError::Malformed(what)) => {
                assert!(what.contains("claimed"), "{what}");
            }
            other => panic!("expected a malformed error, got {other:?}"),
        }
    }

    #[test]
    fn batch_message_carries_records_and_trailer() {
        let schema = crate::guide_tests::guide_schema();
        let mut body = crate::guide_tests::ROOM_RECORD.to_vec();
        body.extend_from_slice(crate::guide_tests::BLOCK_RECORD);

        let compressed = frame(&body);
        let mut message = vec![0x83, 0x16, 0x03];
        message.extend_from_slice(&u32_bytes(compressed.len() as u32));
        message.extend_from_slice(&u32_bytes(body.len() as u32));
        message.extend_from_slice(&compressed);
        // The observed trailer shape: the record count, then zeros (guide
        // §6.2) — retained, not validated.
        message.extend_from_slice(&[0x02, 0x00, 0x00, 0x00, 0x00, 0x00]);

        let items = parse_message(&message, &schema).unwrap();
        assert_eq!(items.len(), 1);
        match &items[0] {
            crate::item::Item::Batch(batch) => {
                assert_eq!(batch.flags, 3);
                assert_eq!(batch.records.len(), 2);
                assert_eq!(batch.records[0].class, "Folder");
                assert_eq!(batch.records[1].class, "Part");
                assert_eq!(
                    batch.records[1].parent,
                    Some(crate::guide_tests::room_ref())
                );
                assert_eq!(batch.trailer, [0x02, 0x00, 0x00, 0x00, 0x00, 0x00]);
            }
            other => panic!("expected a batch, got {other:?}"),
        }
    }

    #[test]
    fn batch_flag_one_carries_shared_strings() {
        let schema = crate::guide_tests::guide_schema();
        let mut body = Vec::new();
        body.extend_from_slice(&[1u8; 16]);
        body.push(0);
        body.push(5);
        body.extend_from_slice(b"hello");
        body.extend_from_slice(&[2u8; 16]);
        body.push(1);
        body.push(0);

        let compressed = frame(&body);
        let mut message = vec![0x83, 0x16, 0x01];
        message.extend_from_slice(&u32_bytes(compressed.len() as u32));
        message.extend_from_slice(&u32_bytes(body.len() as u32));
        message.extend_from_slice(&compressed);

        let items = parse_message(&message, &schema).unwrap();
        match &items[0] {
            crate::item::Item::Batch(batch) => {
                assert_eq!(batch.flags, 1);
                assert!(batch.records.is_empty());
                assert_eq!(batch.shared_strings.len(), 2);
                assert_eq!(batch.shared_strings[0].key, [1u8; 16]);
                assert_eq!(batch.shared_strings[0].marker, 0);
                assert_eq!(batch.shared_strings[0].value, b"hello");
                assert_eq!(batch.shared_strings[1].key, [2u8; 16]);
                assert_eq!(batch.shared_strings[1].marker, 1);
                assert!(batch.shared_strings[1].value.is_empty());
            }
            other => panic!("expected a batch, got {other:?}"),
        }
    }

    #[test]
    fn batch_not_first_fails_closed() {
        let schema = crate::guide_tests::guide_schema();
        // A clock item, then a batch: the trailer's extent is unresolved,
        // so a batch anywhere but the message head cannot be walked past.
        let message: &[u8] = &[
            0x83, 0x23, 0x00, 0x00, 0x00, 0x00, 0x16, 0x03, 0x00, 0x00, 0x00, 0x00,
        ];
        match parse_message(message, &schema) {
            Err(ParseError::Unsupported(what)) => {
                assert!(what.contains("batch"), "{what}");
            }
            other => panic!("expected an unsupported error, got {other:?}"),
        }
    }
}
