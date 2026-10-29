//! The tagged value reader of guide §5.
//!
//! A tag fixes a value's byte extent; the class/member context and the
//! receiver's per-property overrides fix what those bytes mean. This reader
//! resolves the tags whose form is unambiguous and keeps every other
//! extent-known value as raw bytes, so a walk can continue without guessing
//! semantics. Tags whose extent itself needs unmodelled context fail closed
//! with [`ParseError::Unsupported`].

use crate::cursor::{Cursor, ParseError, ParseResult, Ref};
use crate::schema::Schema;

/// One decoded property or argument value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Tag 9, strictly 0 or 1.
    Bool(bool),
    /// Tags 2 and 8: raw bytes of a string-shaped value; not assumed UTF-8.
    Bytes(Vec<u8>),
    /// Tag 3: a zero-width value.
    Zero,
    /// Tags 7 and 34: the raw varint of an enum/integer-family value; the
    /// schema's enum table and a semantic mapper give it meaning.
    Enum(u64),
    /// Tags 10 and 44: a zigzag varint.
    Int(i64),
    /// Tags 28 and 69: an instance reference, resolved after the whole
    /// snapshot is in (guide §5.2).
    Ref(Option<Ref>),
    /// Tag 49: a Name, from its literal or shared form.
    Name(String),
    /// A tag whose extent is known but whose meaning this decoder does not
    /// resolve: the value's exact bytes, kept for later analysis.
    Opaque { tag: u8, bytes: Vec<u8> },
}

/// Read one value of the given tag **[GAME]**.
pub fn read_value(c: &mut Cursor<'_>, tag: u8, schema: &Schema) -> ParseResult<Value> {
    match tag {
        // Boolean/u8 family; the guide's demo accepts only 0 and 1.
        9 => match c.byte()? {
            0 => Ok(Value::Bool(false)),
            1 => Ok(Value::Bool(true)),
            other => Err(ParseError::InvalidBool(other)),
        },
        // String/binary-string family: preserve the bytes as-is.
        2 | 8 => Ok(Value::Bytes(c.string()?)),
        // Zero-width.
        3 => Ok(Value::Zero),
        // Enum/integer family: the raw varint.
        7 | 34 => Ok(Value::Enum(c.varint()?)),
        // Signed zigzag family.
        10 | 44 => Ok(Value::Int(c.zigzag()?)),
        // Reference family.
        28 | 69 => Ok(Value::Ref(c.reference()?)),
        // Name, tag 49 (guide §5.1).
        49 => read_name(c, schema),
        // Fixed-extent shapes whose semantics stay unresolved: four-byte
        // (f32/u32-shaped), eight-byte (UDim, Vector2, NumberRange,
        // Capabilities-shaped), and sixteen-byte (UDim2/Rect and the
        // mixed 6/46/47 family, which includes the opaque `Script.Source`
        // extent — not editable source, guide §5).
        6 | 46 | 47 | 14 | 40 | 67 | 52 => opaque(c, tag, 16),
        11 | 16 | 17 => opaque(c, tag, 4),
        12 | 48 | 55 | 62 | 13 | 21 | 37 => opaque(c, tag, 8),
        15 => opaque(c, tag, 24),
        18 => opaque(c, tag, 2),
        19 | 22 | 36 => opaque(c, tag, 12),
        20 => opaque(c, tag, 3),
        24 => opaque(c, tag, 4),
        25 => opaque(c, tag, 6),
        39 => opaque(c, tag, 20),
        65 => opaque(c, tag, 13),
        66 => opaque(c, tag, 3),
        // CFrame-B (guide §5, tag 27): 12-byte position, a kind byte, then a
        // 6-byte compressed rotation only when the kind is zero. Nonzero
        // kinds use orientation lookup logic this decoder does not model;
        // keeping 19 bytes is not the same as reconstructing the rotation.
        27 => {
            let mut bytes = c.take(13)?.to_vec();
            let kind = bytes[12];
            if kind == 0 {
                bytes.extend_from_slice(c.take(6)?);
            }
            Ok(Value::Opaque { tag, bytes })
        }
        // CFrame-A (tag 26): the same shape with a 36-byte matrix instead.
        26 => {
            let mut bytes = c.take(13)?.to_vec();
            let kind = bytes[12];
            if kind == 0 {
                bytes.extend_from_slice(c.take(36)?);
            }
            Ok(Value::Opaque { tag, bytes })
        }
        // Ray-shaped: a u32 count (bounded to 20 by the reference receiver)
        // of 12-byte components (tag 35), or of 20-byte ones (tag 38).
        35 => bounded_repeat(c, tag, 12),
        38 => bounded_repeat(c, tag, 20),
        // Physical-properties: a flags byte, and only when its low bit is
        // set the further extent the guide marks conditional.
        41 => {
            let flags = c.byte()?;
            if flags & 1 != 0 {
                Err(ParseError::Unsupported(
                    "tag 41 with the extended physical-properties form".into(),
                ))
            } else {
                Ok(Value::Opaque {
                    tag,
                    bytes: vec![flags],
                })
            }
        }
        // FontFace: u16, u8, then two length-prefixed strings.
        54 => {
            let mut bytes = c.take(3)?.to_vec();
            bytes.extend_from_slice(&c.string()?);
            bytes.extend_from_slice(&c.string()?);
            Ok(Value::Opaque { tag, bytes })
        }
        // A bulk blob: two varint lengths and the compressed bytes.
        56 => {
            let compressed_len = c.length()?;
            let _decoded_len = c.varint()?;
            Ok(Value::Opaque {
                tag,
                bytes: c.take(compressed_len)?.to_vec(),
            })
        }
        // 7-byte header, a count byte, and that many bytes.
        58 => {
            let mut bytes = c.take(7)?.to_vec();
            let n = c.byte()?;
            bytes.extend_from_slice(c.take(usize::from(n))?);
            Ok(Value::Opaque { tag, bytes })
        }
        // Context-dependent asset/content and NetAsset forms, the optional
        // CFrame, tuples and maps, and the user shape: extents that need
        // context or per-element decoding this stage does not model.
        33 | 61 | 60 | 51 | 29 | 30 | 31 | 32 | 68 => Err(ParseError::Unsupported(format!(
            "value tag {tag}"
        ))),
        other => Err(ParseError::Unsupported(format!(
            "unknown value tag {other}"
        ))),
    }
}

/// Guide §5.1: a literal length under 127, `127` plus a varint length, or
/// the high bit selecting a shared name from the schema's table. The
/// extended shared form (high bit with low bits 127) is not established and
/// is rejected, as the reference reader rejects it.
fn read_name(c: &mut Cursor<'_>, schema: &Schema) -> ParseResult<Value> {
    let selector = c.byte()?;
    let bytes = if selector & 0x80 == 0 {
        if selector == 127 {
            let len = c.varint()?;
            let len = usize::try_from(len).map_err(|_| ParseError::VarintOverflow)?;
            c.take(len)?.to_vec()
        } else {
            c.take(usize::from(selector))?.to_vec()
        }
    } else {
        if selector & 0x7f == 127 {
            return Err(ParseError::Unsupported(
                "extended shared Name form".into(),
            ));
        }
        let index = usize::from(selector & 0x7f);
        schema
            .shared_names
            .get(index)
            .ok_or_else(|| {
                ParseError::Malformed(format!("shared name {index} out of range"))
            })?
            .as_bytes()
            .to_vec()
    };
    String::from_utf8(bytes)
        .map(Value::Name)
        .map_err(|_| ParseError::Malformed("Name is not UTF-8".into()))
}

fn opaque(c: &mut Cursor<'_>, tag: u8, len: usize) -> ParseResult<Value> {
    Ok(Value::Opaque {
        tag,
        bytes: c.take(len)?.to_vec(),
    })
}

fn bounded_repeat(c: &mut Cursor<'_>, tag: u8, element: usize) -> ParseResult<Value> {
    let n = c.u32_le()?;
    if n > 20 {
        return Err(ParseError::Malformed(format!(
            "tag {tag} repeat count {n} exceeds the reference receiver's bound of 20"
        )));
    }
    let len = usize::try_from(n)
        .checked_mul(element)
        .ok_or_else(|| ParseError::Malformed("repeat length overflows".into()))?;
    opaque(c, tag, len)
}
