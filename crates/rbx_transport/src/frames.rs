//! The send side of the RakNet data layer, ported from the
//! `sendRaknetDatagram`/`sendUnreliable`/`sendSplitFragment`/`flushAcks`
//! paths of Boblox's `raknet.cpp` **[GAME]**. These builders produce the
//! plaintext datagram body; sealing it with the data AEAD and wrapping it
//! in RUPP is the client's job, as is every counter — the datagram id,
//! the reliable-message number, the ordering index and the split id all
//! come in as parameters so the caller owns the sequence.
//!
//! ```text
//! reliable   : [81][dgram u24 LE][40|60][bits BE16][msgno u24 LE]
//!              [if ordered: ordering u24 LE][channel 00][data]
//! split frag : [83][dgram u24 LE][50|70][bits BE16][msgno u24 LE]
//!              [if ordered: ordering u24 LE][channel 00]
//!              [count u32 BE][split id u16 BE][index u32 BE][data]
//! unreliable : [81][dgram u24 LE][00][bits BE16][data]
//! ack        : [c0][count u16 BE]{ [01][id u24 LE] | [00][lo u24 LE][hi u24 LE] }
//! ```
//!
//! The ACK entries' tag byte is a type tag, not a "min equals max" flag:
//! `01` announces one id, `00` an inclusive range. A range written with
//! the `01` tag leaves the server parsing the trailing bytes as further
//! entries, dropping the whole ACK — the reference found that one the
//! hard way. ACKs carry no datagram id on the wire, so they must not
//! consume one either: the native's `0x13` is datagram 1 straight after
//! the `0x09`, and the RCC stalls on a hole.

/// The largest message one datagram carries (the negotiated MTU minus
/// the framing the AEAD and RUPP add).
pub const MAX_MESSAGE: usize = 1200;

/// The native's split size (measured across two live sessions).
pub const SPLIT_SIZE: usize = 1061;

/// The 15 bytes after the GUID in ID_CONNECTION_REQUEST — identical in
/// every captured native session (2026-10-27), so a build constant.
pub const CONNECTION_REQUEST_TAIL: [u8; 15] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0d, 0xac, 0x00, 0x37, 0x4f, 0x5e, 0x11, 0x6c, 0x45,
];

/// One reliable (optionally reliable-ordered) application message in a
/// single datagram. `0x40` reliable, `0x60` reliable-ordered.
pub fn reliable(
    dgram_no: u32,
    message_number: u32,
    ordered: bool,
    ordering_index: u32,
    data: &[u8],
) -> Option<Vec<u8>> {
    if data.is_empty() || data.len() > MAX_MESSAGE {
        return None;
    }
    let mut out = Vec::with_capacity(data.len() + 17);
    out.push(0x81);
    out.extend_from_slice(&dgram_no.to_le_bytes()[..3]);
    out.push(if ordered { 0x60 } else { 0x40 });
    out.extend_from_slice(&bits(data));
    out.extend_from_slice(&message_number.to_le_bytes()[..3]);
    if ordered {
        out.extend_from_slice(&ordering_index.to_le_bytes()[..3]);
        out.push(0x00); // ordering channel
    }
    out.extend_from_slice(data);
    Some(out)
}

/// One split fragment: `0x83` (valid + continuous send), `0x70`
/// reliable-ordered-with-split or `0x50` reliable-with-split, then the
/// split header the receiver reassembles by.
#[allow(clippy::too_many_arguments)]
pub fn split_fragment(
    dgram_no: u32,
    message_number: u32,
    count: u32,
    split_id: u16,
    index: u32,
    ordering_index: u32,
    ordered: bool,
    data: &[u8],
) -> Option<Vec<u8>> {
    if data.is_empty() || data.len() > MAX_MESSAGE {
        return None;
    }
    let mut out = Vec::with_capacity(data.len() + 27);
    out.push(0x83);
    out.extend_from_slice(&dgram_no.to_le_bytes()[..3]);
    out.push(if ordered { 0x70 } else { 0x50 });
    out.extend_from_slice(&bits(data));
    out.extend_from_slice(&message_number.to_le_bytes()[..3]);
    if ordered {
        out.extend_from_slice(&ordering_index.to_le_bytes()[..3]);
        out.push(0x00); // ordering channel
    }
    out.extend_from_slice(&count.to_be_bytes());
    out.extend_from_slice(&split_id.to_be_bytes());
    out.extend_from_slice(&index.to_be_bytes());
    out.extend_from_slice(data);
    Some(out)
}

/// One unreliable application message (RakNet kind 0): no message number
/// and no ordering header, which is how the native sends `0xA6` input
/// and the ping/pad mirrors.
pub fn unreliable(dgram_no: u32, data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() || data.len() > MAX_MESSAGE {
        return None;
    }
    let mut out = Vec::with_capacity(data.len() + 10);
    out.push(0x81);
    out.extend_from_slice(&dgram_no.to_le_bytes()[..3]);
    out.push(0x00);
    out.extend_from_slice(&bits(data));
    out.extend_from_slice(data);
    Some(out)
}

/// Sort, deduplicate and merge the datagram ids to ACK into inclusive
/// ranges — ascending, no overlap, the shape the native emits.
pub fn coalesce_acks(ids: &[u32]) -> Vec<(u32, u32)> {
    let mut sorted = ids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for id in sorted {
        match ranges.last_mut() {
            Some((_, hi)) if *hi + 1 == id => *hi = id,
            _ => ranges.push((id, id)),
        }
    }
    ranges
}

/// Build the ACK datagram body from coalesced ranges.
pub fn ack(ranges: &[(u32, u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(0xc0);
    out.extend_from_slice(&(ranges.len() as u16).to_be_bytes());
    for &(lo, hi) in ranges {
        if lo == hi {
            out.push(0x01);
            out.extend_from_slice(&lo.to_le_bytes()[..3]);
        } else {
            out.push(0x00);
            out.extend_from_slice(&lo.to_le_bytes()[..3]);
            out.extend_from_slice(&hi.to_le_bytes()[..3]);
        }
    }
    out
}

/// ID_CONNECTION_REQUEST (`0x09`): our session GUID — the eight bytes
/// Req2's plaintext carried — then the build-constant tail.
pub fn connection_request(session_guid: &[u8; 8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(24);
    out.push(0x09);
    out.extend_from_slice(session_guid);
    out.extend_from_slice(&CONNECTION_REQUEST_TAIL);
    out
}

/// ID_NEW_INCOMING_CONNECTION (`0x13`): our socket address, the peer's,
/// then twelve dummy entries. RakNet addresses are
/// `[ver][ip4][port BE]`.
pub fn new_incoming(local: ([u8; 4], u16), peer: ([u8; 4], u16)) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + 14 * 7);
    out.push(0x13);
    push_address(&mut out, &local.0, local.1);
    push_address(&mut out, &peer.0, peer.1);
    for _ in 0..12 {
        push_address(&mut out, &[0xff, 0xff, 0xff, 0xff], 0);
    }
    out
}

fn push_address(out: &mut Vec<u8>, ip: &[u8; 4], port: u16) {
    out.push(0x04);
    out.extend_from_slice(ip);
    out.extend_from_slice(&port.to_be_bytes());
}

fn bits(data: &[u8]) -> [u8; 2] {
    ((data.len() * 8) as u16).to_be_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rx_stream::RxStream;

    #[test]
    fn reliable_message_roundtrips() {
        let datagram = reliable(7, 3, true, 2, b"\x83a replication body").unwrap();
        assert_eq!(datagram[0], 0x81);
        assert_eq!(datagram[1..4], [7, 0, 0]); // datagram id, u24 LE
        assert_eq!(datagram[4], 0x60);
        assert_eq!(datagram[5..7], ((b"\x83a replication body".len() * 8) as u16).to_be_bytes());
        assert_eq!(datagram[7..10], [3, 0, 0]); // message number
        assert_eq!(datagram[10..13], [2, 0, 0]); // ordering index
        assert_eq!(datagram[13], 0); // channel

        let mut stream = RxStream::default();
        let messages = stream.on_datagram(&datagram);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, 0x83);
        assert_eq!(messages[0].payload, b"\x83a replication body");
        assert!(messages[0].ordered);
        assert_eq!(messages[0].ordering, 2);
        assert_eq!(messages[0].reliability, 0x60);
        assert_eq!(stream.messages, 1);
    }

    #[test]
    fn unreliable_message_roundtrips() {
        let datagram = unreliable(9, b"\xa6input").unwrap();
        assert_eq!(datagram[4], 0x00);
        let mut stream = RxStream::default();
        let messages = stream.on_datagram(&datagram);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, 0xa6);
        assert!(!messages[0].ordered);
        assert_eq!(messages[0].reliability, 0x00);
    }

    /// The join's big messages split at 1061 bytes, each fragment its own
    /// reliable message with a fresh datagram id and one ordering index
    /// shared by all — and the receiver does not care what order the
    /// fragments arrive in.
    #[test]
    fn split_message_reassembles_in_any_order() {
        let body: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();
        let mut message = vec![0x90u8];
        message.extend_from_slice(&body);
        let count = 3u32;

        let fragments = [
            split_fragment(1, 10, count, 0, 0, 4, true, &message[..1061]).unwrap(),
            split_fragment(2, 11, count, 0, 1, 4, true, &message[1061..2122]).unwrap(),
            split_fragment(3, 12, count, 0, 2, 4, true, &message[2122..]).unwrap(),
        ];

        let mut stream = RxStream::default();
        // Delivered out of order: middle twice (a retransmit), last, first.
        assert!(stream.on_datagram(&fragments[1]).is_empty());
        assert!(stream.on_datagram(&fragments[1]).is_empty());
        assert_eq!(stream.duplicate_splits, 1);
        assert!(stream.on_datagram(&fragments[2]).is_empty());
        assert_eq!(stream.pending_splits(), 1);
        let messages = stream.on_datagram(&fragments[0]);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, 0x90);
        assert_eq!(messages[0].payload, message);
        assert_eq!(messages[0].ordering, 4);
        assert_eq!(stream.split_messages, 1);
        assert_eq!(stream.pending_splits(), 0);
    }

    #[test]
    fn ordered_messages_wait_for_their_gap() {
        let second = reliable(1, 1, true, 1, b"\x10second").unwrap();
        let first = reliable(2, 2, true, 0, b"\x10first").unwrap();
        let mut stream = RxStream::default();
        assert!(stream.on_datagram(&second).is_empty());
        assert_eq!(stream.pending_ordered(), 1);
        let messages = stream.on_datagram(&first);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].payload, b"\x10first");
        assert_eq!(messages[1].payload, b"\x10second");
        // And a retransmit of what already went out is a duplicate.
        assert!(stream.on_datagram(&first).is_empty());
        assert_eq!(stream.duplicate_ordering, 1);
    }

    #[test]
    fn acks_coalesce_and_encode() {
        assert_eq!(coalesce_acks(&[5, 1, 3, 2, 2, 9]), [(1, 3), (5, 5), (9, 9)]);
        let wire = ack(&[(1, 3), (5, 5)]);
        assert_eq!(
            wire,
            [
                0xc0, 0x00, 0x02, // flag, count BE
                0x00, 0x01, 0x00, 0x00, 0x03, 0x00, 0x00, // range 1..=3
                0x01, 0x05, 0x00, 0x00, // single id 5
            ]
        );
        // An ACK datagram is counted and produces nothing.
        let mut stream = RxStream::default();
        assert!(stream.on_datagram(&wire).is_empty());
        assert_eq!(stream.ack_datagrams, 1);
        assert_eq!(stream.messages, 0);
    }

    #[test]
    fn connection_request_and_new_incoming() {
        let request = connection_request(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        assert_eq!(request.len(), 24);
        assert_eq!(request[0], 0x09);
        assert_eq!(&request[1..9], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(&request[9..], &CONNECTION_REQUEST_TAIL[..]);

        let incoming = new_incoming(([127, 0, 0, 1], 0), ([10, 190, 0, 5], 50105));
        assert_eq!(incoming.len(), 1 + 14 * 7);
        assert_eq!(incoming[0], 0x13);
        assert_eq!(incoming[1..8], [0x04, 127, 0, 0, 1, 0x00, 0x00]);
        assert_eq!(incoming[8..15], [0x04, 10, 190, 0, 5, 0xc3, 0xb9]);
        assert_eq!(incoming[15..22], [0x04, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00]);
        assert_eq!(incoming[92..99], [0x04, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00]);
    }

    #[test]
    fn bounds_and_malformed_input() {
        assert!(reliable(0, 0, false, 0, &[]).is_none());
        assert!(reliable(0, 0, false, 0, &[7u8; 1201]).is_none());
        let mut stream = RxStream::default();
        stream.on_datagram(&[0x81, 0x00]);
        assert_eq!(stream.malformed, 1);
        // A message whose length runs past the datagram stops the walk.
        stream.on_datagram(&[0x81, 0, 0, 0, 0x40, 0x00, 0x08, 1, 2, 3]);
        assert_eq!(stream.malformed, 2);
    }
}
