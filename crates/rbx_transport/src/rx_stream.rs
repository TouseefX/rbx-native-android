//! The receive side of the RakNet data layer, ported from Boblox's
//! `rx_stream.cpp` **[GAME]**: one decrypted datagram in, complete
//! application messages out.
//!
//! ```text
//! datagram: [flags][dgram id u24 LE][message]*
//! message : [reliability][u16 BE bit length]
//!           [if RELIABLE: msg number u24 LE]
//!           [if ORDERED/SEQUENCED: ordering index u24 LE][channel u8]
//!           [if SPLIT: count u32 BE][split id u16 BE][index u32 BE]
//!           [data]
//! flags   : bit7 valid, bit6 ACK, bit5 continuous send
//! ```
//!
//! ACK datagrams are counted and dropped, split messages are reassembled
//! with a bounded buffer, and ordered messages are delivered in
//! reliability order per channel — held until the gap before them fills.
//! The bytes handed back are the *application* payload (id + body): the
//! same shape the replication decoder consumes, so the transport adds
//! framing and nothing else.

use std::collections::{BTreeMap, HashMap};

/// No application message is larger.
const MAX_PAYLOAD: usize = 1 << 20;
/// Fragments per split message.
const MAX_SPLIT_COUNT: u32 = 1024;
/// Concurrent split ids — a hostile peer must not grow the buffer
/// unboundedly.
const MAX_PENDING_SPLITS: usize = 64;

/// One complete application message. `payload` includes the id byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppMessage {
    pub id: u8,
    /// The raw RakNet reliability byte.
    pub reliability: u8,
    /// RELIABLE_ORDERED (kind 3).
    pub ordered: bool,
    pub channel: u8,
    pub ordering: u32,
    pub payload: Vec<u8>,
}

/// Reassembles datagrams. No I/O: hand it the bytes the data AEAD
/// produced; it returns every message that became complete, in order.
#[derive(Default)]
pub struct RxStream {
    splits: HashMap<u16, SplitBuffer>,
    ord_buf: HashMap<u8, BTreeMap<u32, AppMessage>>,
    ord_next: HashMap<u8, u32>,
    /// Datagrams seen (including ACKs).
    pub datagrams: u64,
    /// ACK datagrams dropped.
    pub ack_datagrams: u64,
    /// Complete messages delivered.
    pub messages: u64,
    /// Of those, reassembled from splits.
    pub split_messages: u64,
    /// Retransmitted fragments ignored.
    pub duplicate_splits: u64,
    /// Retransmitted ordered messages we already delivered.
    pub duplicate_ordering: u64,
    /// Datagrams that stopped parsing.
    pub malformed: u64,
}

struct SplitBuffer {
    count: u32,
    have: u32,
    parts: Vec<Vec<u8>>,
    reliability: u8,
    channel: u8,
    ordering: u32,
    ordered: bool,
}

impl RxStream {
    /// Split ids still being reassembled.
    pub fn pending_splits(&self) -> usize {
        self.splits.len()
    }

    /// Ordered messages still held for a gap to fill.
    pub fn pending_ordered(&self) -> usize {
        self.ord_buf.values().map(BTreeMap::len).sum()
    }

    /// Walk one datagram body as it comes out of the data AEAD (flags
    /// byte first), returning the messages that became complete.
    pub fn on_datagram(&mut self, d: &[u8]) -> Vec<AppMessage> {
        let mut delivered = Vec::new();
        if d.len() < 4 {
            self.malformed += 1;
            return delivered;
        }
        self.datagrams += 1;
        // ACK datagrams (bit7 + bit6) carry only transport state.
        if (d[0] & 0xc0) == 0xc0 {
            self.ack_datagrams += 1;
            return delivered;
        }
        let mut off = 4; // flags + datagram id (u24 LE)
        while off + 3 <= d.len() {
            let rel = d[off];
            let bits = u16::from_be_bytes([d[off + 1], d[off + 2]]);
            let dlen = usize::from(bits).div_ceil(8);
            let kind = rel >> 5;
            off += 3;

            // RELIABLE, and every ordered/sequenced variant: a message
            // number, which the sender's counters already account for.
            if kind >= 2 {
                if off + 3 > d.len() {
                    self.malformed += 1;
                    return delivered;
                }
                off += 3;
            }
            let mut ordered = false;
            let mut channel = 0u8;
            let mut ordering = 0u32;
            if kind == 1 || kind == 3 || kind == 4 {
                if off + 4 > d.len() {
                    self.malformed += 1;
                    return delivered;
                }
                ordering = read_u24_le(&d[off..]);
                channel = d[off + 3];
                ordered = kind == 3;
                off += 4;
            }

            let mut split_count = 0u32;
            let mut split_index = 0u32;
            let mut split_id = 0u16;
            if (rel & 0x10) != 0 {
                if off + 10 > d.len() {
                    self.malformed += 1;
                    return delivered;
                }
                split_count = u32::from_be_bytes(d[off..off + 4].try_into().unwrap());
                split_id = u16::from_be_bytes([d[off + 4], d[off + 5]]);
                split_index = u32::from_be_bytes(d[off + 6..off + 10].try_into().unwrap());
                off += 10;
            }
            if off + dlen > d.len() || dlen > MAX_PAYLOAD {
                self.malformed += 1;
                return delivered;
            }
            let data = &d[off..off + dlen];
            off += dlen;

            if dlen == 0 {
                continue;
            }

            if (rel & 0x10) == 0 {
                if ordered {
                    let msg = AppMessage {
                        id: data[0],
                        reliability: rel,
                        ordered: true,
                        channel,
                        ordering,
                        payload: data.to_vec(),
                    };
                    self.deliver_ordered(msg, &mut delivered);
                } else {
                    self.deliver(data, rel, false, channel, ordering, &mut delivered);
                }
                continue;
            }

            let fragment = self.take_split(
                split_id,
                split_count,
                split_index,
                rel,
                channel,
                ordering,
                ordered,
                data,
            );
            if let Some(msg) = fragment {
                if msg.ordered {
                    self.deliver_ordered(msg, &mut delivered);
                } else {
                    self.deliver(
                        &msg.payload,
                        msg.reliability,
                        false,
                        msg.channel,
                        msg.ordering,
                        &mut delivered,
                    );
                }
            }
        }
        delivered
    }

    /// File one fragment, and assemble the message when the last one
    /// lands. All bookkeeping — duplicates, malformed shapes, bounded
    /// buffers — happens in here so the walk stays linear.
    #[allow(clippy::too_many_arguments)]
    fn take_split(
        &mut self,
        split_id: u16,
        split_count: u32,
        split_index: u32,
        reliability: u8,
        channel: u8,
        ordering: u32,
        ordered: bool,
        data: &[u8],
    ) -> Option<AppMessage> {
        if split_count == 0 || split_count > MAX_SPLIT_COUNT || split_index >= split_count {
            self.malformed += 1;
            return None;
        }
        if !self.splits.contains_key(&split_id) {
            if self.splits.len() >= MAX_PENDING_SPLITS {
                self.malformed += 1;
                return None;
            }
            let buffer = SplitBuffer {
                count: split_count,
                have: 0,
                parts: vec![Vec::new(); split_count as usize],
                reliability,
                channel,
                ordering,
                ordered,
            };
            self.splits.insert(split_id, buffer);
        }
        if self.splits[&split_id].count != split_count {
            self.malformed += 1; // split id reused with a different shape
            self.splits.remove(&split_id);
            return None;
        }
        let duplicate = {
            let buffer = self.splits.get_mut(&split_id).unwrap();
            if buffer.parts[split_index as usize].is_empty() {
                buffer.parts[split_index as usize] = data.to_vec();
                buffer.have += 1;
                false
            } else {
                true
            }
        };
        if duplicate {
            self.duplicate_splits += 1; // retransmitted fragment
            return None;
        }
        if self.splits[&split_id].have != self.splits[&split_id].count {
            return None;
        }
        let assembled = {
            let buffer = &self.splits[&split_id];
            let total: usize = buffer.parts.iter().map(Vec::len).sum();
            if total == 0 || total > MAX_PAYLOAD {
                None
            } else {
                let mut payload = Vec::with_capacity(total);
                for part in &buffer.parts {
                    payload.extend_from_slice(part);
                }
                Some(AppMessage {
                    id: payload[0],
                    reliability: buffer.reliability,
                    ordered: buffer.ordered,
                    channel: buffer.channel,
                    ordering: buffer.ordering,
                    payload,
                })
            }
        };
        self.splits.remove(&split_id);
        match assembled {
            Some(msg) => {
                self.split_messages += 1;
                Some(msg)
            }
            None => {
                self.malformed += 1;
                None
            }
        }
    }

    fn deliver(
        &mut self,
        payload: &[u8],
        reliability: u8,
        ordered: bool,
        channel: u8,
        ordering: u32,
        out: &mut Vec<AppMessage>,
    ) {
        if payload.is_empty() {
            return;
        }
        self.messages += 1;
        out.push(AppMessage {
            id: payload[0],
            reliability,
            ordered,
            channel,
            ordering,
            payload: payload.to_vec(),
        });
    }

    /// Delivery in reliability order: the first message a channel sees
    /// seeds the sequence, anything below `next` is a retransmit we
    /// already delivered, and anything ahead waits in the buffer until
    /// the gap fills.
    fn deliver_ordered(&mut self, msg: AppMessage, out: &mut Vec<AppMessage>) {
        let channel = msg.channel;
        // A channel's sequence starts at 0 (the native's streams do);
        // an arrival above the expected index waits for the gap, even
        // when it is the first thing the channel ever carried.
        let next = *self.ord_next.entry(channel).or_insert(0);
        if msg.ordering < next {
            self.duplicate_ordering += 1;
            return;
        }
        if msg.ordering != next {
            // Out of order: hold it until the gap fills. A duplicate
            // replaces nothing.
            self.ord_buf
                .entry(channel)
                .or_default()
                .entry(msg.ordering)
                .or_insert(msg);
            return;
        }
        self.messages += 1;
        out.push(msg);
        let mut n = next + 1;
        {
            let buffer = self.ord_buf.entry(channel).or_default();
            while let Some(held) = buffer.remove(&n) {
                self.messages += 1;
                out.push(held);
                n += 1;
            }
        }
        self.ord_next.insert(channel, n);
    }
}

fn read_u24_le(b: &[u8]) -> u32 {
    u32::from(b[0]) | (u32::from(b[1]) << 8) | (u32::from(b[2]) << 16)
}
