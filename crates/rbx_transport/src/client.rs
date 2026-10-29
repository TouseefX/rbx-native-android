//! The client state machine: Boblox's `raknet.cpp` **[GAME]** with the
//! sockets taken out. Every method that would have called `sendto` pushes
//! the wire bytes into an outgoing queue instead ([`Client::take_sends`]),
//! and the receive path is [`Client::on_packet`], fed whatever the socket
//! layer read — so the same code runs on any transport, and the tests
//! drive it against a mock server built from this crate's own crypto.
//!
//! What the state machine owns, exactly like the reference:
//!
//! - the offline handshake's sequencing (Req1, Reply1, Req2, Reply2) and
//!   the key installation that turns a verified Reply2 into `data_ready`;
//! - the datagram counters — `dgram_no`, `rel_msg`, `ord_idx`, the split
//!   id sequence, and the AEAD's transmit counter — with the ACK
//!   datagrams deliberately *not* consuming a datagram id;
//! - the RUPP subtype/token rule: the join token under subtype 1 for the
//!   whole session by default (the subtype-2 switch is
//!   `Config::token_sub_2` territory — it lost the session live on
//!   2026-10-28), echoing the server's token only as an experiment;
//! - receive bookkeeping: datagram dedup, ACKs flushed from inside the
//!   receive path (the RCC's retransmit timer is faster than any outer
//!   loop), and the server's 0xd0 ACKs counted as the only receipts;
//! - the keepalive mirror: the 9/25-byte pings answered in kind, the
//!   1160-byte MTU pad answered with one 0x03, two replies per id —
//!   mirroring every pad builds an echo loop that drowns the join.
//!
//! Not here, on purpose: the 2 ms pacing between split fragments (the
//! socket layer's business — the reference sleeps inline, which a pure
//! state machine must not), the `RBX_*` environment knobs (their defaults
//! are `Config` fields instead), and logging beyond a handful of lines
//! through an optional sink.

use std::collections::HashSet;
use std::time::Instant;

use crate::crypto::{self, SessionKeys, DATA_COUNTER_BASE};
use crate::frames;
use crate::offline;
use crate::rupp;
use crate::rx_stream::{AppMessage, RxStream};

/// Everything the join script contributes, in the shape the reference's
/// `RaknetConfig` carried it.
#[derive(Clone, Debug)]
pub struct Config {
    /// The RakNet endpoint (`UdmuxEndpoints[0]`), resolved — the 0x13's
    /// second address and Req2's inverted IP.
    pub peer_ip: [u8; 4],
    pub peer_port: u16,
    /// The RCC pair every datagram's TLV2 carries (the join script's
    /// NetStack address, *not* the UDP endpoint — substituting the UDP
    /// port makes the server stop answering Req1).
    pub rcc_ip: [u8; 4],
    pub rcc_port: u16,
    /// RUPP TLV1's token while we ride subtype 1: NetStackTokenValue.
    pub join_token: [u8; 16],
    /// Req1/Req2's protocol field (native: 5).
    pub protocol_version: u8,
    /// The offline probe size (native: 1129 bytes of payload).
    pub probe_size: usize,
    /// `joinScript.EphemeralEarlyPubKey` — the per-session server key
    /// the handshake's first DH runs against.
    pub peer_pub: [u8; 32],
    /// ClientTicket part 2 (native: 33 bytes).
    pub ticket_pre: Vec<u8>,
    /// ClientTicket part 3 (native: 66 bytes).
    pub ticket_auth: Vec<u8>,
    /// The capability bitmask Req2's plaintext leads with — build-static.
    pub capability_word: [u8; 8],
    /// The per-session connection id. Zero means derive one from the
    /// session keypair (the native draws fresh random bytes).
    pub session_id: [u8; 8],
    /// Req2's `[18]` (native: 2).
    pub encryption_mode: u8,
    /// The RUPP application id (native: 4).
    pub application_id: u8,
    /// Ride subtype 2 with our own token from the 0x09 onward, like the
    /// native. Off by default: the live 2026-10-28 result had the RCC
    /// stop ACKing the moment subtype 2 started.
    pub token_sub_2: bool,
    /// Echo the server's announced token on subtype-2 datagrams (an A/B
    /// knob; echoing Reply2's token gets everything dropped).
    pub echo_server_token: bool,
    /// Send ACKs under subtype 1 with the join token even after a switch.
    pub ack_sub_1: bool,
    /// 1 = ACK with the join token under subtype 2, 2 = with the server's
    /// token. Zero (default) = the ordinary token rule.
    pub ack_token_mode: u8,
    /// Deterministic session keys, for tests and replays.
    pub key_seed: Option<[u8; 32]>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            peer_ip: [0; 4],
            peer_port: 0,
            rcc_ip: [0; 4],
            rcc_port: 0,
            join_token: [0; 16],
            protocol_version: 5,
            probe_size: 1129,
            peer_pub: [0; 32],
            ticket_pre: Vec::new(),
            ticket_auth: Vec::new(),
            capability_word: [0x00, 0x2d, 0xd3, 0xc7, 0x50, 0xb7, 0x08, 0x92],
            session_id: [0; 8],
            encryption_mode: 2,
            application_id: 4,
            token_sub_2: false,
            echo_server_token: false,
            ack_sub_1: false,
            ack_token_mode: 0,
            key_seed: None,
        }
    }
}

/// The session, ready to run. Send nothing before [`Client::req1`].
pub struct Client {
    cfg: Config,
    keys: SessionKeys,
    log: Option<Box<dyn FnMut(String)>>,

    stage: u8,
    mtu: u16,
    guid: u64,
    reply2_plain: Vec<u8>,
    data_ready: bool,

    session_token: [u8; 16],
    own_token: [u8; 16],
    own_token_live: bool,
    have_session: bool,
    session_guid: [u8; 8],

    tx_ctr: u64,
    rx_ctr: u64,
    dgram_no: u32,
    rel_msg: u32,
    ord_idx: u32,
    split_seq: u16,

    seen_dgrams: HashSet<u32>,
    pending_acks: Vec<u32>,
    rx: RxStream,

    saw_net_settings: bool,
    kal_per_id: [u32; 256],
    pad_replied: bool,
    t0: Instant,

    out: Vec<Vec<u8>>,
    messages: Vec<Vec<u8>>,

    pkts_rx: u64,
    payloads_rx: u64,
    bytes_rx: u64,
    non_rupp: u64,
    dgram_tx: u64,
    split_tx: u64,
    acks_sent: u64,
    dups_dropped: u64,
    server_acks: u64,
    data_rx_ok: u64,
    data_rx_frames: u64,
    app_bytes: u64,
    kal_replies: u64,
}

impl Client {
    /// A fresh session. The keypair is generated (or taken from
    /// `Config::key_seed`) and the handshake keys are derived against
    /// `Config::peer_pub` immediately — the reference does it just
    /// before Req2, but nothing before then reads them.
    pub fn new(cfg: Config) -> Client {
        let mut keys = match cfg.key_seed {
            Some(seed) => SessionKeys::from_seed(seed),
            None => SessionKeys::generate(),
        };
        keys.install(cfg.peer_pub);
        // An all-zero session id means "draw one"; any 8 bytes work, so
        // take them from the keypair we are actually presenting.
        let mut session_guid = cfg.session_id;
        if session_guid.iter().all(|b| *b == 0) {
            session_guid.copy_from_slice(&keys.public()[..8]);
        }
        Client {
            cfg,
            keys,
            log: None,
            stage: 0,
            mtu: 0,
            guid: 0,
            reply2_plain: Vec::new(),
            data_ready: false,
            session_token: [0; 16],
            own_token: [0; 16],
            own_token_live: false,
            have_session: false,
            session_guid,
            tx_ctr: 0,
            rx_ctr: 0,
            dgram_no: 0,
            rel_msg: 0,
            ord_idx: 0,
            split_seq: 0,
            seen_dgrams: HashSet::new(),
            pending_acks: Vec::new(),
            rx: RxStream::default(),
            saw_net_settings: false,
            kal_per_id: [0; 256],
            pad_replied: false,
            t0: Instant::now(),
            out: Vec::new(),
            messages: Vec::new(),
            pkts_rx: 0,
            payloads_rx: 0,
            bytes_rx: 0,
            non_rupp: 0,
            dgram_tx: 0,
            split_tx: 0,
            acks_sent: 0,
            dups_dropped: 0,
            server_acks: 0,
            data_rx_ok: 0,
            data_rx_frames: 0,
            app_bytes: 0,
            kal_replies: 0,
        }
    }

    /// Optional log sink — a handful of session-level lines, nothing per
    /// datagram.
    pub fn set_logger(&mut self, log: Box<dyn FnMut(String)>) {
        self.log = Some(log);
    }

    fn say(&mut self, line: &str) {
        if let Some(log) = self.log.as_mut() {
            log(line.to_string());
        }
    }

    // --- state -----------------------------------------------------------

    /// 0 = offline, 1 = Reply1 seen, 2 = Reply2 seen.
    pub fn stage(&self) -> u8 {
        self.stage
    }

    pub fn data_ready(&self) -> bool {
        self.data_ready
    }

    pub fn mtu(&self) -> u16 {
        self.mtu
    }

    pub fn guid(&self) -> u64 {
        self.guid
    }

    /// Our X25519 public key, for whoever assembles the mock side.
    pub fn public(&self) -> [u8; 32] {
        self.keys.public()
    }

    /// True once the server pushed its 0x93 network-settings list — the
    /// native sends its join only after that lands.
    pub fn saw_net_settings(&self) -> bool {
        self.saw_net_settings
    }

    pub fn datagrams_tx(&self) -> u64 {
        self.dgram_tx
    }

    pub fn split_messages_tx(&self) -> u64 {
        self.split_tx
    }

    pub fn acks_sent(&self) -> u64 {
        self.acks_sent
    }

    pub fn server_acks(&self) -> u64 {
        self.server_acks
    }

    pub fn dups_dropped(&self) -> u64 {
        self.dups_dropped
    }

    pub fn data_datagrams_rx(&self) -> u64 {
        self.data_rx_ok
    }

    pub fn data_frames_rx(&self) -> u64 {
        self.data_rx_frames
    }

    pub fn app_messages(&self) -> u64 {
        self.rx.messages
    }

    pub fn app_bytes(&self) -> u64 {
        self.app_bytes
    }

    pub fn non_rupp_dropped(&self) -> u64 {
        self.non_rupp
    }

    pub fn packets_rx(&self) -> u64 {
        self.pkts_rx
    }

    pub fn payloads_rx(&self) -> u64 {
        self.payloads_rx
    }

    pub fn bytes_rx(&self) -> u64 {
        self.bytes_rx
    }

    pub fn keepalive_replies(&self) -> u64 {
        self.kal_replies
    }

    /// Reply2's decrypted tail, when the tag verified.
    pub fn reply2_plaintext(&self) -> &[u8] {
        &self.reply2_plain
    }

    /// Drain the datagrams to transmit, oldest first.
    pub fn take_sends(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.out)
    }

    /// Drain the delivered application payloads, oldest first.
    pub fn take_messages(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.messages)
    }

    // --- the offline handshake ------------------------------------------

    /// Req1: the padded probe, RUPP-wrapped under the join token.
    pub fn req1(&mut self) -> bool {
        let Some(probe) = offline::build_req1(self.cfg.protocol_version, self.cfg.probe_size)
        else {
            return false;
        };
        self.wrap_send(1, self.cfg.join_token, &probe)
    }

    /// Req2: the sealed join material. Needs Reply1's MTU, so stage 1.
    pub fn req2(&mut self) -> bool {
        if self.stage < 1 {
            return false;
        }
        let public = self.keys.public();
        let inputs = offline::Req2Inputs {
            public_key: &public,
            ticket_pre: &self.cfg.ticket_pre,
            ticket_auth: &self.cfg.ticket_auth,
            capability_word: self.cfg.capability_word,
            session_id: self.session_guid,
            mtu: self.mtu,
            protocol_version: self.cfg.protocol_version,
            encryption_mode: self.cfg.encryption_mode,
            application_id: self.cfg.application_id,
            peer_ip: self.cfg.peer_ip,
            peer_port: self.cfg.peer_port,
            probe_size: self.cfg.probe_size,
        };
        let Some(packet) = offline::build_req2(self.keys.tx(), &inputs) else {
            return false;
        };
        self.wrap_send(1, self.cfg.join_token, &packet)
    }

    // --- the RakNet connection ------------------------------------------

    /// ID_CONNECTION_REQUEST (`0x09`): our session GUID and the 15-byte
    /// build constant, as one reliable unordered datagram.
    pub fn connection_request(&mut self) -> bool {
        if !self.data_ready {
            return false;
        }
        let d = frames::connection_request(&self.session_guid);
        let ok = self.send_reliable(&d, false);
        // The native's first three datagrams ride the join token under
        // subtype 1; only the datagram after the 0x09 switches, and the
        // switch still loses the live session — so it stays opt-in.
        self.own_token_live = ok && self.cfg.token_sub_2;
        self.say("connection request sent");
        ok
    }

    /// ID_NEW_INCOMING_CONNECTION (`0x13`): our socket address, the
    /// peer's, twelve dummies — reliable ordered.
    pub fn new_incoming(&mut self, local: ([u8; 4], u16)) -> bool {
        if !self.data_ready {
            return false;
        }
        let peer_port = if self.cfg.rcc_port != 0 {
            self.cfg.rcc_port
        } else {
            self.cfg.peer_port
        };
        let d = frames::new_incoming(local, (self.cfg.peer_ip, peer_port));
        self.send_reliable(&d, true)
    }

    /// ID_SUBMIT_TICKET (`0x8a`): the 5-part ClientTicket, reliable
    /// ordered, once — right after the connection is accepted.
    pub fn send_ticket(&mut self, ticket: &str) -> bool {
        if !self.data_ready || ticket.is_empty() || ticket.len() > 1100 {
            return false;
        }
        let mut d = Vec::with_capacity(ticket.len() + 1);
        d.push(0x8a);
        d.extend_from_slice(ticket.as_bytes());
        self.send_reliable(&d, true)
    }

    // --- the data layer ---------------------------------------------------

    /// One reliable application message, optionally reliable-ordered.
    pub fn send_reliable(&mut self, data: &[u8], ordered: bool) -> bool {
        if !self.data_ready || data.is_empty() || data.len() > frames::MAX_MESSAGE {
            return false;
        }
        let frame = frames::reliable(self.dgram_no, self.rel_msg, ordered, self.ord_idx, data);
        let Some(frame) = frame else {
            return false;
        };
        if ordered {
            self.ord_idx += 1;
        }
        self.rel_msg += 1;
        self.seal_and_send(&frame, true)
    }

    /// One unreliable application message (kind 0) — input and pings.
    pub fn send_unreliable(&mut self, data: &[u8]) -> bool {
        if !self.data_ready || data.is_empty() || data.len() > frames::MAX_MESSAGE {
            return false;
        }
        let Some(frame) = frames::unreliable(self.dgram_no, data) else {
            return false;
        };
        self.seal_and_send(&frame, true)
    }

    /// One application message, split into 1061-byte fragments when it
    /// does not fit a datagram. The caller paces the fragments — the
    /// reference sleeps 2 ms between them, and a pure state machine does
    /// not get to sleep.
    pub fn send_join_message(&mut self, msg: &[u8], ordered: bool) -> usize {
        if !self.data_ready || msg.is_empty() {
            return 0;
        }
        if msg.len() <= frames::SPLIT_SIZE {
            return usize::from(self.send_reliable(msg, ordered));
        }
        let count = msg.len().div_ceil(frames::SPLIT_SIZE) as u32;
        let split_id = self.split_seq;
        self.split_seq = self.split_seq.wrapping_add(1);
        let ord = if ordered {
            let ord = self.ord_idx;
            self.ord_idx += 1;
            ord
        } else {
            0
        };
        let base = self.rel_msg;
        for i in 0..count {
            let off = i as usize * frames::SPLIT_SIZE;
            let n = (msg.len() - off).min(frames::SPLIT_SIZE);
            let frame = frames::split_fragment(
                self.dgram_no,
                base + i,
                count,
                split_id,
                i,
                ord,
                ordered,
                &msg[off..off + n],
            );
            let Some(frame) = frame else {
                return 0;
            };
            if !self.seal_and_send(&frame, true) {
                return 0;
            }
            self.split_tx += 1;
        }
        self.rel_msg += count;
        count as usize
    }

    /// Flush the pending ACKs as one coalesced 0xc0 datagram.
    pub fn flush_acks(&mut self) -> bool {
        if !self.data_ready || self.pending_acks.is_empty() {
            return false;
        }
        let ranges = frames::coalesce_acks(&self.pending_acks);
        let d = frames::ack(&ranges);
        self.pending_acks.clear();
        if !self.seal_and_send(&d, false) {
            return false;
        }
        self.acks_sent += 1;
        true
    }

    // --- the receive path --------------------------------------------------

    /// Feed one datagram exactly as the socket read it. Collects the
    /// replies (ACKs, keepalive mirrors) in the outgoing queue and the
    /// delivered payloads in the message queue.
    pub fn on_packet(&mut self, pkt: &[u8]) {
        self.pkts_rx += 1;
        self.bytes_rx += pkt.len() as u64;
        let view = rupp::view(pkt);
        let (payload, token) = match view {
            Some(v) => {
                if let Some((token, 2)) = v.token {
                    if !self.have_session {
                        self.session_token = token;
                        self.have_session = true;
                    }
                }
                self.payloads_rx += 1;
                (v.payload, v.token)
            }
            None => {
                // Offline handshake replies can arrive bare: the server's
                // Reply1 is the id, magic and fields on their own.
                let bare = pkt.len() >= 17
                    && matches!(
                        pkt[0],
                        offline::REQ1 | offline::REPLY1 | offline::REQ2 | offline::REPLY2
                    )
                    && pkt[1..17] == offline::MAGIC;
                if !bare {
                    self.non_rupp += 1;
                    return;
                }
                self.payloads_rx += 1;
                (pkt, None)
            }
        };
        if payload.is_empty() {
            return;
        }
        // Post-handshake frames are one AEAD datagram under the session
        // token: [ct][counter16][tag16].
        if self.data_ready && matches!(token, Some((_, 2))) && payload.len() >= 18 {
            self.data_rx_frames += 1;
            let opened = crypto::datagram_open(self.keys.data_rx(), payload, self.rx_ctr);
            let Some((plain, used)) = opened else {
                self.say("data frame failed to open");
                return;
            };
            self.rx_ctr = used + 1;
            self.data_rx_ok += 1;
            if plain.len() >= 4 && plain[0] & 0xc0 == 0xc0 {
                // The server's ACK form is 0xd0: transport only, and the
                // only receipt the 0x09/0x13/0x90/0x8a ever get.
                self.server_acks += 1;
                return;
            }
            if plain.len() >= 4 && plain[0] & 0x80 != 0 {
                let num =
                    u32::from(plain[1]) | (u32::from(plain[2]) << 8) | (u32::from(plain[3]) << 16);
                // ACK the highest reliable message number, not the
                // datagram id: the RCC resends the same pad under fresh
                // datagram ids forever if ids are what gets acked.
                let ack_num = num.max(max_message_number(&plain));
                if !self.seen_dgrams.insert(num) {
                    self.dups_dropped += 1;
                    return;
                }
                self.pending_acks.push(ack_num);
                // Immediately: the RCC's retransmit timer (~250 ms) is
                // faster than any once-a-second outer flush.
                self.flush_acks();
                self.on_repl_datagram(&plain);
            }
            return;
        }
        let id = payload[0];
        if id == offline::REPLY1 && payload.len() >= 28 {
            if let Some(reply) = offline::parse_reply1(payload) {
                self.guid = reply.guid;
                self.mtu = reply.mtu;
                self.stage = 1;
                self.say("reply1 accepted");
            }
        } else if id == offline::REPLY2 {
            self.handle_reply2(payload);
        }
    }

    fn handle_reply2(&mut self, payload: &[u8]) {
        self.stage = 2;
        let Some(reply) = offline::parse_reply2(self.keys.rx(), payload) else {
            self.say("reply2 tag rejected (rx key mismatch)");
            return;
        };
        self.reply2_plain = reply.plaintext;
        if let Some(peer) = reply.peer_data_public {
            self.keys.install_data(peer);
            self.data_ready = true;
            self.tx_ctr = DATA_COUNTER_BASE;
            self.rx_ctr = DATA_COUNTER_BASE;
            self.say("reply2 verified, datagram keys installed");
        }
    }

    fn on_repl_datagram(&mut self, d: &[u8]) {
        for m in self.rx.on_datagram(d) {
            self.app_bytes += m.payload.len() as u64;
            if m.id == 0x93 {
                self.saw_net_settings = true;
            }
            if self.data_ready {
                self.mirror_keepalive(&m);
            }
            self.messages.push(m.payload);
        }
    }

    /// Mirror one server ping. At most twice per id, and the 1160-byte
    /// MTU pad gets exactly one 0x03 answer — mirroring every pad builds
    /// the echo loop that drowns the join.
    fn mirror_keepalive(&mut self, m: &AppMessage) {
        let out_id = if m.payload.len() == 9 && matches!(m.id, 0x00 | 0x51..=0x53) {
            m.id
        } else if m.payload.len() == 25 && matches!(m.id, 0x03 | 0x54..=0x56) {
            m.id
        } else if m.payload.len() > 64 && m.id == 0x00 {
            if self.pad_replied {
                return;
            }
            self.pad_replied = true;
            0x03
        } else {
            return;
        };
        if self.kal_per_id[usize::from(out_id)] >= 2 {
            return;
        }
        self.kal_per_id[usize::from(out_id)] += 1;
        let mut r = m.payload.clone();
        r[0] = out_id;
        // Refresh the clock fields with our own monotonic ms, big-endian
        // like the native writes them.
        let ms = self.now_ms().to_be_bytes();
        if r.len() == 9 {
            r[5..9].copy_from_slice(&ms);
        } else if r.len() == 25 {
            r[21..25].copy_from_slice(&ms);
        }
        self.send_join_message(&r, false);
        self.kal_replies += 1;
    }

    fn now_ms(&self) -> u32 {
        self.t0.elapsed().as_millis() as u32
    }

    // --- the send plumbing --------------------------------------------------

    fn token(&self) -> ([u8; 16], u8) {
        if self.own_token_live {
            (
                if self.cfg.echo_server_token {
                    self.session_token
                } else {
                    self.own_token
                },
                2,
            )
        } else {
            (self.cfg.join_token, 1)
        }
    }

    /// Seal one plaintext datagram and hand it to the RUPP wrapper.
    /// `data` datagrams consume a datagram id; ACKs do not — the native's
    /// 0x13 is datagram 1 straight after the 0x09, and a hole stalls the
    /// RCC's whole sequence.
    fn seal_and_send(&mut self, plain: &[u8], data: bool) -> bool {
        let frame = crypto::datagram_seal(self.keys.data_tx(), self.tx_ctr, plain);
        self.tx_ctr += 1;
        if data {
            self.dgram_no += 1;
        }
        self.dgram_tx += 1;
        if !data {
            if self.cfg.ack_sub_1 {
                return self.wrap_send(1, self.cfg.join_token, &frame);
            }
            match self.cfg.ack_token_mode {
                1 => return self.wrap_send(2, self.cfg.join_token, &frame),
                2 => return self.wrap_send(2, self.session_token, &frame),
                _ => {}
            }
        }
        let (token, sub) = self.token();
        self.wrap_send(sub, token, &frame)
    }

    fn wrap_send(&mut self, sub: u8, token: [u8; 16], payload: &[u8]) -> bool {
        let header = rupp::Header {
            token,
            subtype: sub,
            rcc: Some((self.cfg.rcc_ip, self.cfg.rcc_port)),
        };
        self.out.push(rupp::wrap(&header, payload));
        true
    }
}

/// The highest reliable message number in a decrypted datagram — what the
/// ACK path quotes when the RCC resends one message under fresh datagram
/// ids. Ported verbatim, the 7-byte split skip included: the reference's
/// ACK heuristic works as measured, quirk and all.
fn max_message_number(d: &[u8]) -> u32 {
    let mut best = 0u32;
    let mut off = 4usize;
    while off + 3 <= d.len() {
        let rel = d[off];
        let bits = (usize::from(d[off + 1]) << 8) | usize::from(d[off + 2]);
        off += 3;
        let kind = rel >> 5;
        if kind > 4 || (rel == 0 && bits == 0) {
            break;
        }
        let dlen = bits.div_ceil(8);
        if kind >= 2 {
            if off + 3 > d.len() {
                break;
            }
            let mn = u32::from(d[off])
                | (u32::from(d[off + 1]) << 8)
                | (u32::from(d[off + 2]) << 16);
            best = best.max(mn);
            off += 3;
        }
        if matches!(kind, 1 | 3 | 4) {
            off += 4;
        }
        if rel & 0x10 != 0 {
            off += 7;
        }
        off += dlen;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use blake2::{Blake2b512, Digest};
    use x25519_dalek::{PublicKey, StaticSecret};

    const CLIENT_SEED: [u8; 32] = [1; 32];
    const SERVER_SEED: [u8; 32] = [2; 32];

    fn config() -> Config {
        Config {
            peer_ip: [10, 190, 0, 5],
            peer_port: 50105,
            rcc_ip: [10, 17, 9, 213],
            rcc_port: 51751,
            join_token: [0x42; 16],
            peer_pub: SessionKeys::from_seed(SERVER_SEED).public(),
            ticket_pre: vec![7; 33],
            ticket_auth: vec![9; 66],
            session_id: [1, 2, 3, 4, 5, 6, 7, 8],
            key_seed: Some(CLIENT_SEED),
            ..Config::default()
        }
    }

    /// The mock server: the client's own key math from the other side of
    /// the wire. The KDF's public-key order is canonical — client first,
    /// server second, whichever side runs it — so the mock derives the
    /// client-ordered digest and swaps the halves. This mock presents the
    /// same key for both the handshake and the data layer, which the
    /// client's double install against one peer key mirrors exactly.
    struct Server {
        /// Opens what the client seals (the client's tx half).
        open: [u8; 32],
        /// Seals what the client opens (the client's rx half).
        seal: [u8; 32],
        server_pub: [u8; 32],
        rx_ctr: u64,
        rx: RxStream,
    }

    impl Server {
        fn new(client_pub: [u8; 32]) -> Server {
            let secret = StaticSecret::from(SERVER_SEED);
            let server_pub = *PublicKey::from(&secret).as_bytes();
            let shared = secret.diffie_hellman(&PublicKey::from(client_pub));
            let mut hasher = Blake2b512::new();
            hasher.update(shared.as_bytes());
            hasher.update(&client_pub);
            hasher.update(&server_pub);
            let digest = hasher.finalize();
            let mut seal = [0u8; 32];
            let mut open = [0u8; 32];
            seal.copy_from_slice(&digest[..32]);
            open.copy_from_slice(&digest[32..]);
            Server {
                open,
                seal,
                server_pub,
                rx_ctr: DATA_COUNTER_BASE,
                rx: RxStream::default(),
            }
        }

        /// Wrap a payload the way the server does: a short RUPP header
        /// riding subtype 2.
        fn wrap(&self, payload: &[u8]) -> Vec<u8> {
            let header = rupp::Header {
                token: [0x99; 16],
                subtype: 2,
                rcc: None,
            };
            rupp::wrap(&header, payload)
        }

        /// Seal one server datagram with the data key.
        fn data(&mut self, plain: &[u8]) -> Vec<u8> {
            let frame = crypto::datagram_seal(&self.seal, self.rx_ctr, plain);
            self.rx_ctr += 1;
            self.wrap(&frame)
        }

        /// Open one client datagram.
        fn open(&mut self, wire: &[u8]) -> Option<Vec<u8>> {
            let v = rupp::view(wire)?;
            let (plain, used) = crypto::datagram_open(&self.open, v.payload, self.rx_ctr)?;
            self.rx_ctr = used + 1;
            Some(plain)
        }

        /// The server's bare Reply1.
        fn reply1() -> Vec<u8> {
            let mut p = vec![0u8; 28];
            p[0] = offline::REPLY1;
            p[1..17].copy_from_slice(&offline::MAGIC);
            p[17..25].copy_from_slice(&0x0123_4567_89ab_cdefu64.to_le_bytes());
            p[26..28].copy_from_slice(&1400u16.to_be_bytes());
            p
        }

        /// The server's Reply2: its data public key split across the AAD
        /// tail and the plaintext head, exactly where the client's parser
        /// reconstructs it.
        fn reply2(&self) -> Vec<u8> {
            let mut p = Vec::new();
            p.push(offline::REPLY2);
            p.extend_from_slice(&offline::MAGIC);
            p.push(0x01);
            p.push(44); // aad_len
            p.extend_from_slice(&[0xAA, 0xBB]);
            p.extend_from_slice(&self.server_pub[..23]); // payload[21..44]
            p.extend_from_slice(&self.server_pub[23..]); // plaintext head
            p.extend_from_slice(&[0x33; 55]);
            p.extend_from_slice(&crypto::HANDSHAKE_NONCE);
            p.extend_from_slice(&[0u8; 16]); // tag slot
            let nonce_off = p.len() - 28;
            assert!(crypto::aead_seal(&self.seal, &mut p, nonce_off, 44));
            p
        }
    }

    fn handshake(client: &mut Client, server: &mut Server) {
        assert!(client.req1());
        client.take_sends();
        client.on_packet(&Server::reply1());
        assert_eq!(client.stage(), 1);
        assert!(client.req2());
        client.take_sends();
        let reply2 = server.reply2();
        client.on_packet(&server.wrap(&reply2));
        assert!(client.data_ready());
    }

    #[test]
    fn the_full_dance_against_a_mock_server() {
        let mut client = Client::new(config());
        let mut server = Server::new(client.public());

        // Req1 goes out RUPP-wrapped under the join token, subtype 1.
        assert!(client.req1());
        let sends = client.take_sends();
        assert_eq!(sends.len(), 1);
        assert_eq!(sends[0].len(), rupp::HEADER_LEN as usize + 1129);
        assert_eq!(sends[0][4..7], [0x01, 0x11, 0x01]);
        assert_eq!(sends[0][31], offline::REQ1);

        // Reply1 arrives bare and advances the stage.
        client.on_packet(&Server::reply1());
        assert_eq!(client.stage(), 1);
        assert_eq!(client.mtu(), 1400);
        assert_eq!(client.guid(), 0x0123_4567_89ab_cdef);

        // Req2, then the wrapped Reply2 turns the data layer on.
        assert!(client.req2());
        let sends = client.take_sends();
        assert_eq!(sends[0][31], offline::REQ2);
        let reply2 = server.reply2();
        client.on_packet(&server.wrap(&reply2));
        assert_eq!(client.stage(), 2);
        assert!(client.data_ready());
        assert_eq!(client.reply2_plaintext().len(), 9 + 55);

        // The connection request: datagram 0, reliable unordered.
        assert!(client.connection_request());
        let sends = client.take_sends();
        assert_eq!(sends.len(), 1);
        let plain = server.open(&sends[0]).expect("the 0x09 opens");
        assert_eq!(plain[0], 0x09);
        assert_eq!(plain[1..9], [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(&plain[9..], &frames::CONNECTION_REQUEST_TAIL[..]);

        // The 0x13: datagram 1, ordered.
        assert!(client.new_incoming(([127, 0, 0, 1], 0)));
        let sends = client.take_sends();
        let plain = server.open(&sends[0]).expect("the 0x13 opens");
        assert_eq!(plain[0], 0x13);
        assert_eq!(plain.len(), 1 + 14 * 7);
        assert_eq!(plain[1..8], [0x04, 127, 0, 0, 1, 0x00, 0x00]);
        let mut got = server.rx.on_datagram(&plain);
        assert_eq!(got.len(), 1);

        // A 3000-byte join message: three fragments, one split id.
        let mut body = vec![0x90];
        body.extend((0..3000u32).map(|i| (i % 251) as u8));
        assert_eq!(client.send_join_message(&body, true), 3);
        assert_eq!(client.split_messages_tx(), 3);
        for wire in client.take_sends() {
            let plain = server.open(&wire).expect("fragment opens");
            got.extend(server.rx.on_datagram(&plain));
        }
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].payload, body);
        assert_eq!(got[1].ordering, 1); // the 0x13 took ordering 0

        // Server -> client: two ordered messages, each ACKed immediately.
        let d1 = frames::reliable(0, 0, true, 0, b"\x93settings").unwrap();
        client.on_packet(&server.data(&d1));
        let d2 = frames::reliable(1, 1, true, 1, b"\x90first join data").unwrap();
        client.on_packet(&server.data(&d2));
        assert!(client.saw_net_settings());
        let messages = client.take_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], b"\x93settings");
        assert_eq!(messages[1], b"\x90first join data");

        let sends = client.take_sends();
        assert_eq!(sends.len(), 2);
        let ack1 = server.open(&sends[0]).expect("ack opens");
        assert_eq!(ack1[0], 0xc0);
        assert_eq!(ack1[1..7], [0x00, 0x01, 0x01, 0x00, 0x00, 0x00]);
        let ack2 = server.open(&sends[1]).expect("ack opens");
        assert_eq!(ack2[1..7], [0x00, 0x01, 0x01, 0x01, 0x00, 0x00]);
        assert_eq!(client.acks_sent(), 2);

        // A retransmit of the first datagram: dropped before the message
        // layer, no new ACK.
        client.on_packet(&server.data(&d1));
        assert!(client.take_messages().is_empty());
        assert!(client.take_sends().is_empty());
        assert_eq!(client.dups_dropped(), 1);

        // The server's 0xd0 ACK: counted, nothing delivered, no reply.
        client.on_packet(&server.data(&[0xd0, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00]));
        assert_eq!(client.server_acks(), 1);
        assert!(client.take_messages().is_empty());
        assert!(client.take_sends().is_empty());
    }

    #[test]
    fn keepalives_mirror_once_per_pad() {
        let mut client = Client::new(config());
        let mut server = Server::new(client.public());
        handshake(&mut client, &mut server);

        // A 9-byte ping: mirrored with the same id and a fresh clock, the
        // ACK first.
        let mut ping = vec![0x00u8, 1, 0, 0, 0, 0, 0, 0, 0];
        ping[5..9].copy_from_slice(&1234u32.to_be_bytes());
        client.on_packet(&server.data(&frames::reliable(0, 0, false, 0, &ping).unwrap()));
        let sends = client.take_sends();
        assert_eq!(sends.len(), 2);
        let mirror = server.open(&sends[1]).expect("mirror opens");
        assert_eq!(mirror[0], 0x81);
        assert_eq!(mirror[4], 0x40);
        assert_eq!(mirror[10], 0x00);
        assert_ne!(mirror[15..19], 1234u32.to_be_bytes());

        // Same id twice more: one more mirror, then the per-id cap.
        client.on_packet(&server.data(&frames::reliable(1, 1, false, 0, &ping).unwrap()));
        client.on_packet(&server.data(&frames::reliable(2, 2, false, 0, &ping).unwrap()));
        assert_eq!(client.keepalive_replies(), 2);

        // The 1160-byte MTU pad: answered with one 0x03, exactly once.
        let pad = vec![0x00u8; 1160];
        client.on_packet(&server.data(&frames::reliable(3, 3, true, 1, &pad).unwrap()));
        client.on_packet(&server.data(&frames::reliable(4, 4, true, 1, &pad).unwrap()));
        assert_eq!(client.keepalive_replies(), 3);
        // The pad reply itself split (1160 > 1061).
        assert_eq!(client.split_messages_tx(), 2);
    }

    #[test]
    fn max_message_number_walks_the_datagram() {
        let d = frames::reliable(3, 7, true, 2, b"\x8fdata").unwrap();
        assert_eq!(max_message_number(&d), 7);
        // Two messages in one datagram: the highest number wins.
        let mut two = frames::reliable(4, 8, false, 0, b"\x00a").unwrap();
        two.extend(&frames::reliable(4, 9, false, 0, b"\x00b").unwrap()[4..]);
        assert_eq!(max_message_number(&two), 9);
    }
}
