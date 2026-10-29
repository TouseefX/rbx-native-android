//! The binary primitives of guide §3, over one already-delivered logical
//! message.

use std::fmt;

/// An instance reference as read off the wire **[GAME]**: a varint `lo` word
/// and, when `lo != 0`, a little-endian `u32` id. A null reference is the
/// `lo == 0` sentinel with no id behind it.
///
/// The pair is a session-scoped key, not a proven stable GUID: keep the
/// connection epoch alongside it (guide §3.2, §9.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ref {
    pub lo: u64,
    pub id: u32,
}

/// What went wrong reading one message. The decoder never guesses forward:
/// every error means the walk stopped at a known offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The bytes ended inside a field.
    Truncated,
    /// A varint never ended within its ten-byte bound.
    UnterminatedVarint,
    /// A varint needed more than 64 bits.
    VarintOverflow,
    /// A one-byte boolean was neither 0 nor 1.
    InvalidBool(u8),
    /// The layout is understood but this decoder does not implement it; the
    /// walk must stop, not skip (the guide's fail-closed rule).
    Unsupported(String),
    /// The bytes contradict the layout: a bad marker, an index out of range,
    /// a length that cannot be, a non-UTF-8 name.
    Malformed(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Truncated => write!(f, "truncated input"),
            ParseError::UnterminatedVarint => write!(f, "unterminated varint"),
            ParseError::VarintOverflow => write!(f, "varint overflows 64 bits"),
            ParseError::InvalidBool(byte) => {
                write!(f, "boolean is neither 0 nor 1: {byte:#04x}")
            }
            ParseError::Unsupported(what) => write!(f, "unsupported: {what}"),
            ParseError::Malformed(what) => write!(f, "malformed: {what}"),
        }
    }
}

impl std::error::Error for ParseError {}

pub type ParseResult<T> = Result<T, ParseError>;

/// A bounds-checked reader over one buffer. Every read is extent-checked;
/// nothing allocates from a length without the bytes being present.
pub struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Cursor { data, pos: 0 }
    }

    /// How many bytes have been consumed.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Whether the whole buffer has been consumed.
    pub fn done(&self) -> bool {
        self.pos == self.data.len()
    }

    /// The bytes left unconsumed.
    pub fn rest(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }

    /// Consume exactly `len` bytes, or stop at the buffer's end.
    pub fn take(&mut self, len: usize) -> ParseResult<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(ParseError::Malformed("length overflows the cursor".into()))?;
        let bytes = self.data.get(self.pos..end).ok_or(ParseError::Truncated)?;
        self.pos = end;
        Ok(bytes)
    }

    pub fn byte(&mut self) -> ParseResult<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn u16_le(&mut self) -> ParseResult<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32_le(&mut self) -> ParseResult<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn u64_le(&mut self) -> ParseResult<u64> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    /// Unsigned LEB128: seven value bits per byte, continuation in the high
    /// bit. `300` is `ac 02` (guide §3.1). Ten bytes is the 64-bit bound:
    /// a tenth continuation byte is unterminated, a tenth value byte with
    /// more than one bit overflows.
    pub fn varint(&mut self) -> ParseResult<u64> {
        let mut value = 0u64;
        let mut i = 0;
        loop {
            let byte = self.byte()?;
            let bits = u64::from(byte & 0x7f);
            if i == 9 {
                if byte & 0x80 != 0 {
                    return Err(ParseError::UnterminatedVarint);
                }
                if bits > 1 {
                    return Err(ParseError::VarintOverflow);
                }
                return Ok(value | (bits << 63));
            }
            value |= bits << (7 * i);
            i += 1;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
    }

    /// Zigzag-encoded varint: encoded `1` is `-1` (guide §3.1).
    pub fn zigzag(&mut self) -> ParseResult<i64> {
        let unsigned = self.varint()?;
        Ok(((unsigned >> 1) as i64) ^ -((unsigned & 1) as i64))
    }

    /// A schema-style string: a varint byte length and that many bytes.
    pub fn string(&mut self) -> ParseResult<Vec<u8>> {
        let len = self.length()?;
        Ok(self.take(len)?.to_vec())
    }

    /// A length field: a varint that must fit both the cursor and `usize`.
    pub fn length(&mut self) -> ParseResult<usize> {
        let len = self.varint()?;
        let len = usize::try_from(len).map_err(|_| ParseError::VarintOverflow)?;
        if len > self.data.len() - self.pos {
            return Err(ParseError::Truncated);
        }
        Ok(len)
    }

    /// An instance reference **[GAME]**: varint `lo`, then a little-endian
    /// `u32` id only when `lo != 0`.
    pub fn reference(&mut self) -> ParseResult<Option<Ref>> {
        let lo = self.varint()?;
        if lo == 0 {
            return Ok(None);
        }
        Ok(Some(Ref {
            lo,
            id: self.u32_le()?,
        }))
    }

    /// A reference that must not be the null sentinel.
    pub fn instance(&mut self) -> ParseResult<Ref> {
        self.reference()?
            .ok_or_else(|| ParseError::Malformed("null instance reference".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_300_is_ac_02() {
        let mut c = Cursor::new(&[0xac, 0x02]);
        assert_eq!(c.varint(), Ok(300));
        assert!(c.done());
    }

    #[test]
    fn varint_rejects_overflow_and_runon() {
        let ten = [0xff; 10];
        assert_eq!(
            Cursor::new(&ten).varint(),
            Err(ParseError::UnterminatedVarint)
        );
        let eleven = [0xff_u8; 11];
        assert_eq!(
            Cursor::new(&eleven).varint(),
            Err(ParseError::UnterminatedVarint)
        );
        let overflow = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f];
        assert_eq!(
            Cursor::new(&overflow).varint(),
            Err(ParseError::VarintOverflow)
        );
    }

    #[test]
    fn zigzag_one_is_minus_one() {
        let mut c = Cursor::new(&[0x01]);
        assert_eq!(c.zigzag(), Ok(-1));
    }

    #[test]
    fn reference_sentinel_has_no_id() {
        let mut c = Cursor::new(&[0x00]);
        assert_eq!(c.reference(), Ok(None));
        assert!(c.done());

        // lo=1, id=101: the worked `Block` reference of guide §7.3.
        let mut c = Cursor::new(&[0x01, 0x65, 0x00, 0x00, 0x00]);
        assert_eq!(c.reference(), Ok(Some(Ref { lo: 1, id: 101 })));
        assert!(c.done());
    }

    #[test]
    fn string_and_truncation() {
        let mut c = Cursor::new(&[0x03, b'a', b'b', b'c']);
        assert_eq!(c.string(), Ok(b"abc".to_vec()));
        let mut c = Cursor::new(&[0x05, b'a']);
        assert_eq!(c.string(), Err(ParseError::Truncated));
    }
}
