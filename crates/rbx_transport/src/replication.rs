//! The RbxTransport replication stream: the session's app messages,
//! framed and classified. Ported from the reference's own pure parsers
//! (`client/src/solve/replicate.h` / `repl_snapshot.cpp`, every layout
//! live-capture-verified there) — this layer reads the envelopes and
//! keeps the census; building the world out of the 0x97 schema and the
//! 0x16 instance batches on top of it is the next step.
//!
//! Wire: each app message off the RakNet session is one compact-varint
//! length plus its body (the body's first byte is the message id), and
//! a body can arrive split across messages — so `feed` buffers and
//! nothing is parsed until a whole frame is in.
//!
//! - `0x97` join data: `[97][2B codec magic][00 00][u32 inner size
//!   @5:9][zstd @9:]` running to the end of the frame. The codec magic
//!   rolls with every server build; the known pairs are listed below
//!   and an unknown one fails loudly, naming the pair that arrived.
//! - `0x83` chain: `[83][subtype][body]`. Subtype `0x16` is an instance
//!   batch: `[16][flags][u32 comp @3:7][u32 decomp @7:11][zstd
//!   @11:11+comp][trailer]` (comp counts the frame only). Flags `01`
//!   carries shared strings rather than instances.
//! - `0x81` peer init: `[81][7 flag bytes][LEB128 peer id][tail]`.
//! - `0x8D` terrain region: `[8d][01 ec 1a 09 00][u32 comp][u32
//!   decomp][zstd]`, frame-exact.
//! - `0x84` the snapshot-done marker.
//!
//! Everything else is counted and left alone: join replies, 0x9B
//! challenges and ACKs are the transport's, and unmapped ids are the
//! census's business.

use std::io::Read;

/// The 0x97 codec-version magics seen across server builds (the
/// reference's own capture list — same `[00 00][u32][zstd]` layout
/// after every one of them).
const JOIN_MAGICS: [(u8, u8); 7] = [
    (0xA3, 0xCF),
    (0x31, 0x8B),
    (0x93, 0xD2),
    (0xF7, 0x8C),
    (0x50, 0xD3),
    (0xBA, 0xD3),
    (0xC1, 0x8D),
];

const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];
/// The 0x8D region prefix, fixed across every live sighting.
const REGION_PREFIX: [u8; 5] = [0x01, 0xEC, 0x1A, 0x09, 0x00];

/// What the session's replication stream carried. Failures are counted
/// and logged, never fatal: the session's own liveness does not depend
/// on a parseable world.
#[derive(Debug)]
pub struct Replication {
    /// Bytes still waiting for their frame to complete.
    pending: Vec<u8>,
    /// One slot per first body byte — the census.
    pub types: [u64; 256],
    /// One slot per 0x83 chain subtype.
    pub chain_types: [u64; 256],
    /// Bodies the framing layer handed through.
    pub messages: u64,
    /// The 0x97 join data's decompressed size, once it lands.
    pub join_inner: Option<u64>,
    /// 0x97s that would not parse.
    pub join_failures: u64,
    /// 0x83/0x16 instance batches, and their inner bytes total.
    pub batches: u64,
    /// Batches whose flags said shared strings rather than instances.
    pub shared_batches: u64,
    pub batch_bytes: u64,
    pub batch_failures: u64,
    /// 0x8D terrain region chunks, and their inner bytes total.
    pub regions: u64,
    pub region_bytes: u64,
    /// 0x81 peer ids, in arrival order.
    pub peers: Vec<u32>,
    /// The 0x84 snapshot-done marker.
    pub snapshot_done: bool,
}

// Manual because `Default` is not implemented for `[u64; 256]` — std
// only derives it for arrays up to 32 elements.
impl Default for Replication {
    fn default() -> Self {
        Replication {
            pending: Vec::new(),
            types: [0; 256],
            chain_types: [0; 256],
            messages: 0,
            join_inner: None,
            join_failures: 0,
            batches: 0,
            shared_batches: 0,
            batch_bytes: 0,
            batch_failures: 0,
            regions: 0,
            region_bytes: 0,
            peers: Vec::new(),
            snapshot_done: false,
        }
    }
}

impl Replication {
    /// One app message off the session, framed the way the native
    /// frames them and appended — the parse only runs over whole
    /// frames.
    pub fn feed(&mut self, msg: &[u8], log: &mut dyn FnMut(String)) {
        write_varint(&mut self.pending, msg.len() as u64);
        self.pending.extend_from_slice(msg);
        let mut off = 0usize;
        while let Some((len, k)) = read_varint(&self.pending[off..]) {
            let len = len as usize;
            if off + k + len > self.pending.len() {
                break; // incomplete body: wait for the rest of it
            }
            if len > 0 {
                // Owned: `handle` takes &mut self, which the borrow of
                // `self.pending` above would conflict with.
                let body = self.pending[off + k..off + k + len].to_vec();
                self.handle(&body, &mut *log);
            }
            off += k + len;
        }
        if off > 0 {
            self.pending.drain(..off);
        }
    }

    fn handle(&mut self, body: &[u8], log: &mut dyn FnMut(String)) {
        self.messages += 1;
        self.types[body[0] as usize] += 1;
        match body[0] {
            0x97 => self.on_join_data(body, log),
            0x83 => self.on_chain(body, log),
            0x81 => self.on_peer(body, log),
            0x8D => self.on_region(body, log),
            0x84 if !self.snapshot_done => {
                self.snapshot_done = true;
                log("replication: snapshot done (0x84 marker)".to_string());
            }
            _ => {}
        }
    }

    fn on_join_data(&mut self, body: &[u8], log: &mut dyn FnMut(String)) {
        match parse_join_outer(body) {
            Ok(inner) => {
                if self.join_inner.is_none() {
                    log(format!(
                        "replication: join data {}B inner (magic {:02x} {:02x})",
                        inner.len(),
                        body[1],
                        body[2]
                    ));
                }
                self.join_inner = Some(inner.len() as u64);
            }
            Err(err) => {
                self.join_failures += 1;
                log(format!("replication: join data failed: {err}"));
            }
        }
    }

    fn on_chain(&mut self, body: &[u8], log: &mut dyn FnMut(String)) {
        if body.len() < 2 {
            return;
        }
        self.chain_types[body[1] as usize] += 1;
        match body[1] {
            0x16 => self.on_batch(body, log),
            sub if self.chain_types[sub as usize] <= 5 => log(format!(
                "replication: 0x83 chain sub=0x{sub:02x} len={}",
                body.len()
            )),
            _ => {}
        }
    }

    fn on_batch(&mut self, body: &[u8], log: &mut dyn FnMut(String)) {
        match parse_delta_batch(body) {
            Ok(inner) => {
                self.batches += 1;
                self.batch_bytes += inner.len() as u64;
                let shared = body[2] == 1;
                if shared {
                    self.shared_batches += 1;
                }
                if self.batches <= 5 || self.batches.is_multiple_of(100) {
                    let kind = if shared { "shared-string" } else { "instance" };
                    log(format!(
                        "replication: {kind} batch #{} ({}B inner)",
                        self.batches,
                        inner.len()
                    ));
                }
            }
            Err(err) => {
                self.batch_failures += 1;
                log(format!("replication: batch failed: {err}"));
            }
        }
    }

    fn on_peer(&mut self, body: &[u8], log: &mut dyn FnMut(String)) {
        match parse_peer_init(body) {
            Ok(peer) => {
                self.peers.push(peer);
                log(format!("replication: peer {peer} assigned"));
            }
            Err(err) => log(format!("replication: peer parse failed: {err}")),
        }
    }

    fn on_region(&mut self, body: &[u8], log: &mut dyn FnMut(String)) {
        match parse_region_chunk(body) {
            Ok(inner) => {
                self.regions += 1;
                self.region_bytes += inner.len() as u64;
                if self.regions <= 5 || self.regions.is_multiple_of(100) {
                    log(format!(
                        "replication: terrain chunk #{} ({}B)",
                        self.regions,
                        inner.len()
                    ));
                }
            }
            Err(err) => log(format!("replication: terrain chunk failed: {err}")),
        }
    }

    /// One line for the session's end, when anything replicated.
    pub fn summary(&self) -> Option<String> {
        if self.messages == 0 {
            return None;
        }
        let mut parts = vec![format!("{} framed messages", self.messages)];
        if let Some(inner) = self.join_inner {
            parts.push(format!("join data {inner}B"));
        }
        if self.batches > 0 {
            parts.push(format!(
                "{} batches ({}B){}",
                self.batches,
                self.batch_bytes,
                if self.shared_batches > 0 {
                    format!(", {} shared", self.shared_batches)
                } else {
                    String::new()
                }
            ));
        }
        if self.regions > 0 {
            parts.push(format!("{} terrain chunks ({}B)", self.regions, self.region_bytes));
        }
        if !self.peers.is_empty() {
            parts.push(format!("{} peers", self.peers.len()));
        }
        if self.snapshot_done {
            parts.push("snapshot done".to_string());
        }
        let failures = self.join_failures + self.batch_failures;
        if failures > 0 {
            parts.push(format!("{failures} parse failures"));
        }
        Some(parts.join(", "))
    }
}

/// The 0x97 join data outer envelope, decompressed.
fn parse_join_outer(body: &[u8]) -> Result<Vec<u8>, String> {
    if body.len() < 13 || body[0] != 0x97 {
        return Err("not a 0x97 frame".to_string());
    }
    if !JOIN_MAGICS.contains(&(body[1], body[2])) || body[3] != 0x00 || body[4] != 0x00 {
        // Name the bytes: the codec magic rolls with every server
        // build, so a rejection has to say which pair arrived.
        return Err(format!("bad 0x97 magic {:02x} {:02x}", body[1], body[2]));
    }
    let size = u32::from_le_bytes(body[5..9].try_into().unwrap()) as usize;
    if body[9..13] != ZSTD_MAGIC {
        return Err("zstd magic missing".to_string());
    }
    // Unlike the 0x16 batches (whose comp length counts the frame
    // only), the 0x97 frame runs to its end: no trailer.
    inflate(&body[9..], size, "join data")
}

/// The 0x81 peer init's peer id.
fn parse_peer_init(body: &[u8]) -> Result<u32, String> {
    // Proven prefix: 7 flag bytes + LEB128 peer, then a tail this
    // layer does not need.
    if body.len() < 9 || body[0] != 0x81 {
        return Err("not a 0x81 frame".to_string());
    }
    let (peer, k) = read_leb128(&body[8..]).ok_or("bad peer varint")?;
    if peer > u64::from(u32::MAX) || 8 + k > body.len() {
        return Err("bad peer varint".to_string());
    }
    Ok(peer as u32)
}

/// The 0x83/0x16 batch, decompressed.
fn parse_delta_batch(body: &[u8]) -> Result<Vec<u8>, String> {
    if body.len() < 15 || body[0] != 0x83 || body[1] != 0x16 {
        return Err("not a 0x83/0x16 batch".to_string());
    }
    if body[11..15] != ZSTD_MAGIC {
        return Err("zstd magic missing".to_string());
    }
    let comp = u32::from_le_bytes(body[3..7].try_into().unwrap()) as usize;
    let decomp = u32::from_le_bytes(body[7..11].try_into().unwrap()) as usize;
    // comp counts the zstd frame only; the trailer after it is the
    // flags' business, not ours.
    if comp == 0 || 11 + comp > body.len() {
        return Err("comp out of range".to_string());
    }
    inflate(&body[11..11 + comp], decomp, "batch")
}

/// The 0x8D terrain region chunk, decompressed.
fn parse_region_chunk(body: &[u8]) -> Result<Vec<u8>, String> {
    if body.len() < 18 || body[0] != 0x8D {
        return Err("not a 0x8D region chunk".to_string());
    }
    if body[1..6] != REGION_PREFIX {
        return Err("prefix mismatch".to_string());
    }
    if body[14..18] != ZSTD_MAGIC {
        return Err("zstd magic missing".to_string());
    }
    let comp = u32::from_le_bytes(body[6..10].try_into().unwrap()) as usize;
    let decomp = u32::from_le_bytes(body[10..14].try_into().unwrap()) as usize;
    // Frame-exact: no trailer, comp == the rest of the body.
    if comp == 0 || 14 + comp != body.len() {
        return Err("comp out of range".to_string());
    }
    inflate(&body[14..], decomp, "terrain chunk")
}

/// Decompresses a zstd frame, requiring exactly `decomp` bytes out —
/// both overruns and short frames are mismatches, the reference
/// rejects either.
fn inflate(frame: &[u8], decomp: usize, what: &str) -> Result<Vec<u8>, String> {
    let decoder = ruzstd::decoding::StreamingDecoder::new(frame)
        .map_err(|err| format!("{what}: {err}"))?;
    // One byte past the declared size is enough to tell an overrun.
    let mut limited = decoder.take(decomp as u64 + 1);
    let mut out = Vec::new();
    limited
        .read_to_end(&mut out)
        .map_err(|err| format!("{what}: {err}"))?;
    if out.len() != decomp {
        return Err(format!("{what}: decoded {}B, expected {decomp}B", out.len()));
    }
    Ok(out)
}

/// Roblox's compact varint — the stream's framing, not LEB128: the
/// top two bits give the width (<0x40 one byte, <0x4000 two, <0x4000
/// 0000 four, else eight).
fn read_varint(bytes: &[u8]) -> Option<(u64, usize)> {
    let first = *bytes.first()?;
    let n = 1usize << (first >> 6);
    if bytes.len() < n {
        return None;
    }
    let mut v = u64::from(first & 0x3f);
    for b in &bytes[1..n] {
        v = (v << 8) | u64::from(*b);
    }
    Some((v, n))
}

fn write_varint(out: &mut Vec<u8>, v: u64) {
    if v < 0x40 {
        out.push(v as u8);
    } else if v < 0x4000 {
        out.push(0x40 | (v >> 8) as u8);
        out.push((v & 0xff) as u8);
    } else if v < 0x4000_0000 {
        out.push(0x80 | (v >> 24) as u8);
        out.push(((v >> 16) & 0xff) as u8);
        out.push(((v >> 8) & 0xff) as u8);
        out.push((v & 0xff) as u8);
    } else {
        out.push(0xC0 | (v >> 56) as u8);
        out.push(((v >> 48) & 0xff) as u8);
        out.push(((v >> 40) & 0xff) as u8);
        out.push(((v >> 32) & 0xff) as u8);
        out.push(((v >> 24) & 0xff) as u8);
        out.push(((v >> 16) & 0xff) as u8);
        out.push(((v >> 8) & 0xff) as u8);
        out.push((v & 0xff) as u8);
    }
}

/// Unsigned LEB128 — the inner-field varint (the 0x81's peer id).
fn read_leb128(bytes: &[u8]) -> Option<(u64, usize)> {
    let mut v = 0u64;
    for (k, b) in bytes.iter().enumerate() {
        let b = *b;
        if k >= 10 {
            return None;
        }
        v |= u64::from(b & 0x7f) << (7 * k);
        if b & 0x80 == 0 {
            return Some((v, k + 1));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn zstd(data: &[u8]) -> Vec<u8> {
        ruzstd::encoding::compress_to_vec(
            data,
            ruzstd::encoding::CompressionLevel::Fastest,
        )
    }

    #[test]
    fn the_compact_varint_round_trips_at_every_width() {
        for v in [
            0u64,
            0x3f,
            0x40,
            0x1234,
            0x3fff,
            0x4000,
            0x0010_0000,
            0x3fff_ffff,
            0x4000_0000,
            0x0010_0000_0000,
            // The 8-byte form's ceiling: the width marker owns the
            // first byte's top two bits, so only 62 bits are value.
            0x3fff_ffff_ffff_ffff,
        ] {
            let mut out = Vec::new();
            write_varint(&mut out, v);
            assert_eq!(read_varint(&out), Some((v, out.len())), "value {v}");
        }
        // A truncation at every width is a wait, not an error.
        assert_eq!(read_varint(&[]), None);
        assert_eq!(read_varint(&[0xC0]), None);
        // Values past the ceiling lose their top bits — the reference's
        // format does the same, and no frame length gets anywhere near
        // 2^62.
        let mut out = Vec::new();
        write_varint(&mut out, u64::MAX);
        assert_eq!(read_varint(&out), Some((0x3fff_ffff_ffff_ffff, 8)));
    }

    #[test]
    fn every_message_is_one_frame_and_empty_ones_are_skipped() {
        let mut repl = Replication::default();
        let lines = RefCell::new(Vec::new());
        let mut log = |l: String| lines.borrow_mut().push(l);
        repl.feed(&[0x84, 0x00, 0x00, 0x00, 0x00], &mut log);
        assert_eq!(repl.messages, 1);
        assert!(repl.snapshot_done);
        // A zero-length body: the frame is consumed, no handler runs.
        repl.feed(&[], &mut log);
        assert_eq!(repl.messages, 1);
        assert!(repl.pending.is_empty());
        assert!(lines.borrow().iter().any(|l| l.contains("0x84 marker")));
    }

    #[test]
    fn the_census_counts_what_it_does_not_know() {
        let mut repl = Replication::default();
        let lines = RefCell::new(Vec::new());
        let mut log = |l: String| lines.borrow_mut().push(l);
        // 0x18: the first app message a live Team Create session was
        // seen carrying — counted, no parse, no failure.
        repl.feed(&[0x18, 1, 2, 3], &mut log);
        repl.feed(&[0x00], &mut log);
        assert_eq!(repl.types[0x18], 1);
        assert_eq!(repl.types[0x00], 1);
        assert_eq!(repl.messages, 2);
        assert_eq!(repl.summary().unwrap(), "2 framed messages");
    }

    #[test]
    fn join_data_decompresses_and_names_unknown_magics() {
        let inner = b"the class schema, two string tables, records".to_vec();
        let frame = zstd(&inner);
        let mut body = vec![0x97, 0xA3, 0xCF, 0x00, 0x00];
        body.extend_from_slice(&(inner.len() as u32).to_le_bytes());
        body.extend_from_slice(&frame);
        let mut repl = Replication::default();
        let lines = RefCell::new(Vec::new());
        let mut log = |l: String| lines.borrow_mut().push(l);
        repl.feed(&body, &mut log);
        assert_eq!(repl.join_inner, Some(inner.len() as u64));
        assert_eq!(repl.join_failures, 0);
        assert!(lines
            .borrow()
            .iter()
            .any(|l| l.contains(&format!("join data {}B inner", inner.len()))));

        // The magic rolls with server builds: an unknown pair is
        // rejected by naming it, never guessed.
        let mut future = body.clone();
        future[1] = 0xEE;
        future[2] = 0x11;
        repl.feed(&future, &mut log);
        assert_eq!(repl.join_failures, 1);
        assert!(lines
            .borrow()
            .iter()
            .any(|l| l.contains("bad 0x97 magic ee 11")));
        // A short frame cannot be examined at all.
        assert_eq!(
            parse_join_outer(&[0x97, 0xA3, 0xCF]),
            Err("not a 0x97 frame".to_string())
        );
    }

    #[test]
    fn batches_decompress_and_shared_ones_are_told_apart() {
        let inner = vec![7u8; 300];
        let frame = zstd(&inner);
        let mut body = vec![0x83, 0x16, 0x00];
        body.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        body.extend_from_slice(&(inner.len() as u32).to_le_bytes());
        body.extend_from_slice(&frame);
        body.push(0x00); // flags=0 trailer: the 00 end
        let mut repl = Replication::default();
        let lines = RefCell::new(Vec::new());
        let mut log = |l: String| lines.borrow_mut().push(l);
        repl.feed(&body, &mut log);
        assert_eq!(repl.batches, 1);
        assert_eq!(repl.shared_batches, 0);
        assert_eq!(repl.batch_bytes, 300);
        // flags=01: shared strings, not instances.
        let mut shared = body.clone();
        shared[2] = 0x01;
        repl.feed(&shared, &mut log);
        assert_eq!(repl.batches, 2);
        assert_eq!(repl.shared_batches, 1);
        // A comp length past the body is a failure, not a crash.
        let mut bad = body.clone();
        bad[3..7].copy_from_slice(&0x1000_0000u32.to_le_bytes());
        repl.feed(&bad, &mut log);
        assert_eq!(repl.batch_failures, 1);
    }

    #[test]
    fn peers_regions_and_the_marker_all_land() {
        let mut repl = Replication::default();
        let lines = RefCell::new(Vec::new());
        let mut log = |l: String| lines.borrow_mut().push(l);
        // [81][7 flag bytes][LEB128 peer 31]
        repl.feed(&[0x81, 1, 0, 1, 1, 0, 1, 0, 31], &mut log);
        assert_eq!(repl.peers, vec![31]);
        // A region chunk: fixed prefix, sizes, zstd.
        let inner = vec![9u8; 64];
        let frame = zstd(&inner);
        let mut region = vec![0x8D];
        region.extend_from_slice(&REGION_PREFIX);
        region.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        region.extend_from_slice(&(inner.len() as u32).to_le_bytes());
        region.extend_from_slice(&frame);
        repl.feed(&region, &mut log);
        assert_eq!(repl.regions, 1);
        assert_eq!(repl.region_bytes, 64);
        // And the summary names everything.
        repl.feed(&[0x84, 0x00, 0x00, 0x00, 0x00], &mut log);
        let summary = repl.summary().unwrap();
        assert!(summary.contains("3 framed messages"));
        assert!(summary.contains("1 terrain chunks (64B)"));
        assert!(summary.contains("1 peers"));
        assert!(summary.contains("snapshot done"));
    }
}
