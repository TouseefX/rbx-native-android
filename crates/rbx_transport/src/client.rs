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
//! - the RUPP subtype/token rule: the join token under subtype 1 through
//!   the 0x09, then the server-issued token under subtype 2 from the
//!   0x13 on — the native's schedule. The subtype-2 TLV2 port is the
//!   session's real port (`rcc_data_port`), not NetStackPort;
//! - receive bookkeeping: datagram dedup with *re-ACKed* retransmissions
//!   (the lost packet may have been our ACK), ACKs flushed from inside
//!   the receive path, and the server's 0xd0 ACKs counted as the only
//!   receipts;
//! - the keepalive mirror: the 9/25-byte pings answered unreliable with
//!   our own clock, the big MTU pads *generated* per pad id with their
//!   ping trio — echoing the server's own bytes back wedges its
//!   MTU/time handshake forever.
//!
//! Not here, on purpose: the 2 ms pacing between split fragments (the
//! socket layer's business — the reference sleeps inline, which a pure
//! state machine must not), the `RBX_*` environment knobs (their defaults
//! are `Config` fields instead), and logging beyond a handful of lines
//! through an optional sink.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::challenge;
use crate::crypto::{self, SessionKeys, DATA_COUNTER_BASE};
use crate::frames;
use crate::offline;
use crate::rupp;
use crate::rx_stream::{AppMessage, RxStream};

/// The 0x9B solver: the challenge message and the JobId in, the answer
/// out. `None` means unsolvable -- nothing is sent, the join stays
/// gated instead of getting kicked for a wrong answer.
pub type Solver = Box<dyn Fn(&[u8], &str) -> Option<u32>>;

/// Everything the join script contributes, in the shape the reference's
/// `RaknetConfig` carried it.
#[derive(Clone, Debug)]
pub struct Config {
    /// The RakNet endpoint (`UdmuxEndpoints[0]`), resolved — the 0x13's
    /// second address and Req2's inverted IP.
    pub peer_ip: [u8; 4],
    pub peer_port: u16,
    /// The RCC pair every subtype-1 datagram's TLV2 carries (the join
    /// script's NetStack address, *not* the UDP endpoint — substituting
    /// the UDP port makes the server stop answering Req1).
    pub rcc_ip: [u8; 4],
    pub rcc_port: u16,
    /// TLV2 port for subtype-2 datagrams — the established session's
    /// real port, `ServerConnections[0].Port` (the port we dial). The
    /// RCC routes the session path by TLV2, so advertising NetStackPort
    /// there black-holes every subtype-2 datagram: the 0x09 is ACKed and
    /// nothing after it is read. Zero = use `peer_port`.
    pub rcc_data_port: u16,
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
    /// Ride subtype 2 with the session token from the 0x09 onward, ahead
    /// of the native's schedule. Off by default: the 0x13 itself is the
    /// native's first subtype-2 datagram, and that switch is what
    /// `Client::new_incoming` performs.
    pub token_sub_2: bool,
    /// Pin the session to subtype 1 for the whole session, overriding the
    /// 0x13's switch (an A/B knob; the established session the RCC
    /// answers rides subtype 2).
    pub token_sub_1: bool,
    /// Echo the token the server issued in Reply2's RUPP header on every
    /// subtype-2 datagram — the native's behaviour and the default since
    /// the subtype-2 TLV2 port fix (`01 00 00 17 01 11 02 <16B>` is
    /// reused verbatim). Off = present our own token instead.
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
            rcc_data_port: 0,
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
            token_sub_1: false,
            echo_server_token: true,
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

    // Reliable-datagram retransmission for the join. A single lost 0x90
    // fragment leaves a hole in the RCC's ordered stream: it stops
    // ACKing, the 0x8a behind the hole is never processed, and the 0x9b
    // challenge never comes. Each unacknowledged reliable datagram is
    // re-sent under a *fresh* datagram id (the reliable message number
    // and split id stay put, so the receiver reassembles the same
    // message; replaying the original id is discarded by the receiver's
    // datagram-level dedupe before the message layer runs).
    rtx: Vec<PendingTx>,
    rtx_budget: i32,

    saw_net_settings: bool,
    kal_per_id: [u32; 256],
    pad_replies: [u32; 256],
    last_rx: Option<Instant>,
    t0: Instant,

    solver: Option<Solver>,
    job_id: String,
    join_flow_open: bool,
    deferred_challenge: Option<Vec<u8>>,
    saw_challenge: bool,
    challenge_answers: u64,
    marker_acks: u64,

    out: Vec<Vec<u8>>,
    messages: Vec<Vec<u8>>,

    pkts_rx: u64,
    payloads_rx: u64,
    bytes_rx: u64,
    non_rupp: u64,
    dgram_tx: u64,
    split_tx: u64,
    rtx_tx: u64,
    acks_sent: u64,
    dups_dropped: u64,
    server_acks: u64,
    data_rx_ok: u64,
    data_rx_frames: u64,
    app_bytes: u64,
    kal_replies: u64,
    data_pongs: u64,
}

/// One queued reliable retransmit: the sealed datagram's plaintext, the
/// time it becomes due again, and how many times it has been re-sent.
struct PendingTx {
    plain: Vec<u8>,
    due: Instant,
    tries: u32,
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
            rtx: Vec::new(),
            rtx_budget: 64,
            saw_net_settings: false,
            kal_per_id: [0; 256],
            pad_replies: [0; 256],
            last_rx: None,
            t0: Instant::now(),
            solver: None,
            job_id: String::new(),
            join_flow_open: false,
            deferred_challenge: None,
            saw_challenge: false,
            challenge_answers: 0,
            marker_acks: 0,
            out: Vec::new(),
            messages: Vec::new(),
            pkts_rx: 0,
            payloads_rx: 0,
            bytes_rx: 0,
            non_rupp: 0,
            dgram_tx: 0,
            split_tx: 0,
            rtx_tx: 0,
            acks_sent: 0,
            dups_dropped: 0,
            server_acks: 0,
            data_rx_ok: 0,
            data_rx_frames: 0,
            app_bytes: 0,
            kal_replies: 0,
            data_pongs: 0,
        }
    }

    /// Optional log sink — a handful of session-level lines, nothing per
    /// datagram.
    pub fn set_logger(&mut self, log: Box<dyn FnMut(String)>) {
        self.log = Some(log);
    }

    /// Install the 0x9B solver: message + JobId -> answer. Without one
    /// the client keeps running when challenged — no answer is sent, the
    /// join simply stays gated (a wrong answer gets the session kicked,
    /// a missing one does not).
    pub fn set_solver(&mut self, solver: Solver, job_id: &str) {
        self.solver = Some(solver);
        self.job_id = job_id.to_string();
    }

    /// Leave ordering index `n` unused, the way the native's stream does
    /// when a join-burst message is suppressed (its 0x92 and 0x8f slots).
    pub fn reserve_ordering(&mut self) {
        self.ord_idx += 1;
    }

    /// Mark the join burst open: a challenge that lands now is held until
    /// [`end_join_flow`](Self::end_join_flow), so its ordered answer
    /// cannot push the burst's later messages past the indexes the
    /// native leaves them at.
    pub fn begin_join_flow(&mut self) {
        self.join_flow_open = true;
    }

    /// Close the join burst and answer any challenge it deferred.
    pub fn end_join_flow(&mut self) {
        self.join_flow_open = false;
        if let Some(p) = self.deferred_challenge.take() {
            self.answer_challenge_now(&p);
        }
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

    /// True once the RCC sent a challenge (whether or not it was
    /// answered). The join never waits on this — Team Create servers
    /// often never send one.
    pub fn saw_challenge(&self) -> bool {
        self.saw_challenge
    }

    pub fn challenge_answers(&self) -> u64 {
        self.challenge_answers
    }

    pub fn marker_acks(&self) -> u64 {
        self.marker_acks
    }

    pub fn datagrams_tx(&self) -> u64 {
        self.dgram_tx
    }

    pub fn split_messages_tx(&self) -> u64 {
        self.split_tx
    }

    /// Reliable datagrams re-sent under a fresh id (diagnostics).
    pub fn retransmits_tx(&self) -> u64 {
        self.rtx_tx
    }

    /// Milliseconds since the last authenticated server datagram, `None`
    /// before the first one. The RCC ends a dead session without any
    /// disconnect message, so silence is the signal.
    pub fn ms_since_rx(&self) -> Option<u64> {
        self.last_rx.map(|t| t.elapsed().as_millis() as u64)
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

    /// 0x83/05 data pings answered with a 0x83/06 pong.
    pub fn data_pongs(&self) -> u64 {
        self.data_pongs
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
        // The 0x09 stays on subtype 1 (native order) — it is the native's
        // *last* subtype-1 datagram. The switch to subtype 2 happens in
        // `new_incoming`; `token_sub_2` only pulls it earlier, and even
        // then not before the server has issued a session token.
        self.own_token_live = ok && self.cfg.token_sub_2 && self.have_session;
        self.say("connection request sent");
        ok
    }

    /// ID_NEW_INCOMING_CONNECTION (`0x13`): the native's 114-byte shape —
    /// a synthetic 127.139.202.222 address with a random port,
    /// 128.255.254.254:0, nine 255.255.255.255:0 entries and the 36-byte
    /// extension. Reliable ordered, and the datagram that flips the
    /// session onto subtype 2 with the server-issued token.
    pub fn new_incoming(&mut self) -> bool {
        if !self.data_ready {
            return false;
        }
        // The native's 0x13 is the first datagram of the established
        // session and the first under subtype 2; the earlier drops were
        // the subtype-2 TLV2 port, not this payload. `token_sub_1` pins
        // subtype 1 for A/B runs.
        if self.have_session && !self.cfg.token_sub_1 {
            self.own_token_live = true;
        }
        // The address/tail randoms are rolled per session; any fresh
        // bytes work, so derive them from the keypair we present.
        let mut rand = [0u8; 32];
        rand.copy_from_slice(&self.keys.public());
        let d = frames::new_incoming(&rand);
        let ok = self.send_reliable(&d, true);
        self.say(if ok {
            "ID_NEW_INCOMING_CONNECTION sent 114B ok"
        } else {
            "ID_NEW_INCOMING_CONNECTION send failed"
        });
        ok
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
    /// Queued for retransmission — the receiver's ACK is the only
    /// receipt, and a lost fragment otherwise wedges the RCC's ordered
    /// stream behind a hole.
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
        self.queue_rtx(&frame);
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
            self.queue_rtx(&frame);
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
            self.last_rx = Some(Instant::now());
            if plain.len() >= 4 && plain[0] & 0xc0 == 0xc0 {
                // The server's ACK form is 0xd0: transport only, and the
                // only receipt the 0x09/0x13/0x90/0x8a ever get.
                self.server_acks += 1;
                return;
            }
            if plain.len() >= 4 && plain[0] & 0x80 != 0 {
                let num =
                    u32::from(plain[1]) | (u32::from(plain[2]) << 8) | (u32::from(plain[3]) << 16);
                // ACK the datagram id — that is what RakNet's ACK
                // datagram carries. Acking a *message number* instead
                // leaves the server's reliable window jammed: it resends
                // the same 0x97/replication fragments forever, and the
                // session dies right after the join snapshot.
                let ack_num = num;
                // A retransmission must be ACKed again: our previous ACK
                // may have been the packet that got lost. Skip delivery,
                // never the receipt.
                if !self.seen_dgrams.insert(num) {
                    self.dups_dropped += 1;
                    self.pending_acks.push(ack_num);
                    self.flush_acks();
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
            self.rtx.clear();
            self.rtx_budget = 64;
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
            if challenge::is_challenge(&m.payload) {
                self.handle_challenge(&m.payload);
                continue;
            }
            if m.payload.len() == 5 && m.id == 0x84 {
                self.ack_marker(&m.payload);
                continue;
            }
            // The server's ~1 Hz data ping: the body echoed back with
            // the sub-id flipped to 0x06. Left unanswered the RCC goes
            // data-silent ~20 s into the session.
            if m.payload.len() >= 2 && m.id == 0x83 && m.payload[1] == 0x05 {
                let mut pong = m.payload.clone();
                pong[1] = 0x06;
                self.send_join_message(&pong, m.ordered);
                self.data_pongs += 1;
                if self.data_pongs == 1 {
                    self.say("0x83/05 data ping answered (0x83/06)");
                }
                continue;
            }
            self.messages.push(m.payload);
        }
    }

    /// The 0x9B exchange. Answered the moment one lands — unless the
    /// join burst is mid-flight, in which case the answer waits for its
    /// end (`begin_join_flow`/`end_join_flow`).
    fn handle_challenge(&mut self, p: &[u8]) {
        self.saw_challenge = true;
        if self.join_flow_open {
            self.deferred_challenge = Some(p.to_vec());
            self.say("0x9B challenge during the join burst - answering after it");
            return;
        }
        self.answer_challenge_now(p);
    }

    fn answer_challenge_now(&mut self, p: &[u8]) {
        let u2 = u32::from_le_bytes([p[5], p[6], p[7], p[8]]);
        let Some(solver) = self.solver.as_ref() else {
            self.say("0x9B challenge with no solver - continuing without an answer");
            return;
        };
        let Some(answer) = solver(p, &self.job_id) else {
            self.say("0x9B solve failed - nothing sent (a wrong answer gets the session kicked)");
            return;
        };
        let resp = challenge::build_response(u2, answer);
        if self.send_join_message(&resp, true) > 0 {
            self.challenge_answers += 1;
        }
    }

    /// The 0x84 snapshot-done marker: acked with `[04][same u32]`,
    /// ordered. The RCC's marker processing gates join progress on this
    /// ack — without it the server keeps our Player out of the
    /// replicated world.
    fn ack_marker(&mut self, p: &[u8]) {
        let mut ack = [0u8; 5];
        ack[0] = 0x04;
        ack[1..5].copy_from_slice(&p[1..5]);
        if self.send_join_message(&ack, true) > 0 {
            self.marker_acks += 1;
        }
    }

    /// Mirror one server ping or MTU pad. The pads are *generated* from
    /// our own clocks — echoing the server's own bytes back gives the RCC
    /// its own timestamp, so its MTU/time handshake never completes: it
    /// keeps probing with 1160-byte pads forever and never sends the
    /// 0x9B challenge. A big pad is answered with the pad (same id, up
    /// to six times) *and* its ping trio, all unreliable, or the RCC
    /// never closes the exchange.
    fn mirror_keepalive(&mut self, m: &AppMessage) {
        let ping9 = m.payload.len() == 9 && matches!(m.id, 0x00 | 0x51..=0x53);
        let ping25 = m.payload.len() == 25 && matches!(m.id, 0x03 | 0x54..=0x56);
        let big_pad = m.payload.len() > 64 && matches!(m.id, 0x00 | 0x03);
        let out_id = if ping9 || ping25 {
            m.id
        } else if big_pad {
            // Echo the same pad id, not 0x03: the RCC moves on to the
            // next pad id only when it sees its own id back.
            if self.pad_replies[usize::from(m.id)] >= 6 {
                return;
            }
            self.pad_replies[usize::from(m.id)] += 1;
            m.id
        } else {
            return;
        };
        // The per-id cap of two applies to the pings only; a pad exchange
        // runs as many rounds as the RCC asks for (capped above).
        if !big_pad {
            if self.kal_per_id[usize::from(out_id)] >= 2 {
                return;
            }
            self.kal_per_id[usize::from(out_id)] += 1;
        }
        let ms = self.now_ms();
        let clk24 = (ms & 0xff_ff_ff) as u32;
        let mut r;
        if big_pad {
            // Native pad shape: `00 <00*5> <u24 clock> 00*5 ...`, and the
            // 0x03 form adds a u32 clock, the 06 5f marker and five
            // random bytes — zeros to the length the RCC probed with.
            let n = m.payload.len();
            r = vec![0u8; n];
            r[0] = out_id;
            r[6] = (clk24 >> 16) as u8;
            r[7] = (clk24 >> 8) as u8;
            r[8] = clk24 as u8;
            if out_id == 0x03 && n >= 25 {
                r[14..18].copy_from_slice(&ms.to_be_bytes());
                r[18] = 0x06;
                r[19] = 0x5f;
                // Five session-random bytes (any fresh values work).
                r[20..25].copy_from_slice(&self.keys.public()[20..25]);
            }
            // Pads ride the reliable sequenced path (no ordering slot);
            // the ping trio below rides unreliable.
            self.send_join_message(&r, false);
            if r.len() >= 9 {
                let (a, b, c) = if out_id == 0x03 {
                    (0x56u8, 0x55u8, 0x54u8)
                } else {
                    (0x53u8, 0x52u8, 0x51u8)
                };
                for pid in [a, b, c] {
                    let p = [
                        pid,
                        0,
                        0,
                        0,
                        0,
                        0,
                        (clk24 >> 16) as u8,
                        (clk24 >> 8) as u8,
                        clk24 as u8,
                    ];
                    self.send_unreliable(&p);
                }
            }
            self.kal_replies += 1;
            return;
        }
        r = m.payload.clone();
        r[0] = out_id;
        if r.len() == 9 {
            // Our own 24-bit clock's low half, like the native's 9-byte
            // answer — never the server's value it arrived with.
            r[5] = 0;
            r[6] = 0;
            r[7] = (clk24 >> 8) as u8;
            r[8] = clk24 as u8;
        } else if r.len() == 25 {
            // The peer's clock stays at [6:9]; ours goes at [14:18].
            r[14..18].copy_from_slice(&ms.to_be_bytes());
        }
        // 9/25-byte pings are unreliable on the wire — giving them
        // message numbers the RCC's ping path never sees wedges the
        // exchange.
        self.send_unreliable(&r);
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
        // TLV2's port is subtype-dependent, and this is the difference
        // that made every post-handshake session die after 2-5
        // datagrams: subtype 1 (Req1/Req2/0x09) carries the join
        // script's NetStackPort, subtype 2 (the established session)
        // carries the port the session really sends to —
        // ServerConnections[0].Port, the port we dial. The RCC routes
        // the session path by TLV2, so advertising NetStackPort there
        // black-holes every subtype-2 datagram: the 0x09 is ACKed and
        // nothing after it is read.
        let port = if sub == 2 {
            if self.cfg.rcc_data_port != 0 {
                self.cfg.rcc_data_port
            } else {
                self.cfg.peer_port
            }
        } else {
            self.cfg.rcc_port
        };
        let header = rupp::Header {
            token,
            subtype: sub,
            rcc: Some((self.cfg.rcc_ip, port)),
        };
        self.out.push(rupp::wrap(&header, payload));
        true
    }

    /// Queue one reliable datagram for retransmission. Budgeted so the
    /// post-join control ticks are not amplified, and capped so a stuck
    /// session cannot grow the queue without bound.
    fn queue_rtx(&mut self, plain: &[u8]) {
        if self.rtx_budget <= 0 || plain.is_empty() || plain.len() > 1200 {
            return;
        }
        self.rtx_budget -= 1;
        if self.rtx.len() > 160 {
            self.rtx.remove(0);
        }
        self.rtx.push(PendingTx {
            plain: plain.to_vec(),
            due: Instant::now() + Duration::from_millis(180),
            tries: 0,
        });
    }

    /// Re-send every due unacknowledged reliable datagram under a *new*
    /// datagram number (the reliable message number and split id stay
    /// identical, so the receiver reassembles the same message). Call
    /// from the session loop — the reference pumps this in its receive
    /// loop.
    pub fn pump_rtx(&mut self) {
        if self.rtx.is_empty() {
            return;
        }
        let now = Instant::now();
        let mut i = 0;
        while i < self.rtx.len() {
            if now < self.rtx[i].due {
                i += 1;
                continue;
            }
            let n = self.rtx[i].plain.len();
            if !(4..=1200).contains(&n) {
                self.rtx.remove(i);
                continue;
            }
            let mut d = self.rtx[i].plain.clone();
            d[1..4].copy_from_slice(&self.dgram_no.to_le_bytes()[..3]);
            let ok = self.seal_and_send(&d, true);
            if ok {
                self.rtx_tx += 1;
            }
            self.rtx[i].tries += 1;
            if self.rtx[i].tries >= 5 {
                self.rtx.remove(i);
            } else {
                self.rtx[i].due = now + Duration::from_millis(250);
                i += 1;
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use blake2::{Blake2b512, Digest};
    use x25519_dalek::{PublicKey, StaticSecret};

    const CLIENT_SEED: [u8; 32] = [1; 32];
    const SERVER_SEED: [u8; 32] = [2; 32];

    pub(crate) fn config() -> Config {
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

    /// The public key the mock server's `SERVER_SEED` presents, on its
    /// own: a join script built outside `config()` (see `drive::tests`)
    /// must carry exactly this as its `EphemeralEarlyPubKey` for the
    /// handshake's DH to come out the same on both sides.
    pub(crate) fn peer_pub() -> [u8; 32] {
        SessionKeys::from_seed(SERVER_SEED).public()
    }

    /// The mock server: the client's own key math from the other side of
    /// the wire. The KDF's public-key order is canonical — client first,
    /// server second, whichever side runs it — so the mock derives the
    /// client-ordered digest and swaps the halves. This mock presents the
    /// same key for both the handshake and the data layer, which the
    /// client's double install against one peer key mirrors exactly.
    pub(crate) struct Server {
        /// Opens what the client seals (the client's tx half).
        open: [u8; 32],
        /// Seals what the client opens (the client's rx half).
        seal: [u8; 32],
        pub(crate) server_pub: [u8; 32],
        rx_ctr: u64,
        pub(crate) rx: RxStream,
    }

    impl Server {
        pub(crate) fn new(client_pub: [u8; 32]) -> Server {
            let secret = StaticSecret::from(SERVER_SEED);
            let server_pub = *PublicKey::from(&secret).as_bytes();
            let shared = secret.diffie_hellman(&PublicKey::from(client_pub));
            let mut hasher = Blake2b512::new();
            hasher.update(shared.as_bytes());
            hasher.update(client_pub);
            hasher.update(server_pub);
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
        pub(crate) fn wrap(&self, payload: &[u8]) -> Vec<u8> {
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
        pub(crate) fn open(&mut self, wire: &[u8]) -> Option<Vec<u8>> {
            let v = rupp::view(wire)?;
            let (plain, used) = crypto::datagram_open(&self.open, v.payload, self.rx_ctr)?;
            self.rx_ctr = used + 1;
            Some(plain)
        }

        /// The server's bare Reply1.
        pub(crate) fn reply1() -> Vec<u8> {
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
        pub(crate) fn reply2(&self) -> Vec<u8> {
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

    pub(crate) fn handshake(client: &mut Client, server: &mut Server) {
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
        // The 0x09 rides a reliable frame like everything else, so the
        // assertions land on the delivered message, not the raw
        // plaintext (which starts with the frame's flags byte).
        let mut got = server.rx.on_datagram(&plain);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, 0x09);
        assert_eq!(got[0].payload[1..9], [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(&got[0].payload[9..], &frames::CONNECTION_REQUEST_TAIL[..]);
        assert!(!got[0].ordered);

        // The 0x13: datagram 1, ordered — and the datagram that flips
        // the session onto subtype 2 (the mock's Reply2 already carried
        // the server token, so the switch is live from here on).
        assert!(client.new_incoming());
        let sends = client.take_sends();
        assert_eq!(sends[0][4..7], [0x01, 0x11, 0x02]);
        let plain = server.open(&sends[0]).expect("the 0x13 opens");
        got.extend(server.rx.on_datagram(&plain));
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].id, 0x13);
        assert_eq!(got[1].payload.len(), 114);
        assert_eq!(got[1].payload[1..6], [0x04, 127, 139, 202, 222]);
        assert_eq!(got[1].payload[8..15], [0x04, 128, 255, 254, 254, 0x00, 0x00]);
        assert!(got[1].ordered);
        assert_eq!(got[1].ordering, 0);

        // A 3000-byte join message: three fragments, one split id.
        let mut body = vec![0x90];
        body.extend((0..3000u32).map(|i| (i % 251) as u8));
        assert_eq!(client.send_join_message(&body, true), 3);
        assert_eq!(client.split_messages_tx(), 3);
        for wire in client.take_sends() {
            let plain = server.open(&wire).expect("fragment opens");
            got.extend(server.rx.on_datagram(&plain));
        }
        assert_eq!(got.len(), 3);
        assert_eq!(got[2].payload, body);
        assert_eq!(got[2].ordering, 1); // the 0x13 took ordering 0

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
        // layer, but ACKed *again* — the lost packet may have been our
        // ACK, and deduping without re-ACKing jams the RCC's reliable
        // window.
        client.on_packet(&server.data(&d1));
        assert!(client.take_messages().is_empty());
        assert_eq!(client.dups_dropped(), 1);
        let sends = client.take_sends();
        assert_eq!(sends.len(), 1);
        let reack = server.open(&sends[0]).expect("the re-ACK opens");
        assert_eq!(reack[0], 0xc0);
        assert_eq!(reack[1..7], [0x00, 0x01, 0x01, 0x00, 0x00, 0x00]);

        // The server's 0xd0 ACK: counted, nothing delivered, no reply.
        client.on_packet(&server.data(&[0xd0, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00]));
        assert_eq!(client.server_acks(), 1);
        assert!(client.take_messages().is_empty());
        assert!(client.take_sends().is_empty());
    }

    #[test]
    fn keepalives_mirror_pings_and_generate_pads() {
        let mut client = Client::new(config());
        let mut server = Server::new(client.public());
        handshake(&mut client, &mut server);

        // A 9-byte ping: mirrored with the same id and a fresh clock —
        // *unreliable*, so no reliable frame and no message number. The
        // datagram's ACK goes out first.
        let mut ping = vec![0x00u8, 1, 0, 0, 0, 0, 0, 0, 0];
        ping[5..9].copy_from_slice(&1234u32.to_be_bytes());
        client.on_packet(&server.data(&frames::reliable(0, 0, false, 0, &ping).unwrap()));
        let sends = client.take_sends();
        assert_eq!(sends.len(), 2);
        let mirror = server.open(&sends[1]).expect("mirror opens");
        assert_eq!(mirror[0], 0x81);
        assert_eq!(mirror[4], 0x00); // kind 0: unreliable
        assert_eq!(mirror[7], 0x00); // same id echoed
        // Our own clock, not the server's 00 00 04 d2.
        assert_ne!(&mirror[12..16], &[0x00, 0x00, 0x04, 0xd2]);

        // Same id twice more: one more mirror, then the per-id cap.
        client.on_packet(&server.data(&frames::reliable(1, 1, false, 0, &ping).unwrap()));
        client.on_packet(&server.data(&frames::reliable(2, 2, false, 0, &ping).unwrap()));
        assert_eq!(client.keepalive_replies(), 2);

        // The 1160-byte MTU pad: generated (own clock), same pad id, plus
        // its ping trio — unreliable, so the exchange closes.
        let pad = vec![0x00u8; 1160];
        client.on_packet(&server.data(&frames::reliable(3, 3, true, 0, &pad).unwrap()));
        let sends = client.take_sends();
        // ACK, two pad fragments (1160 > 1061 splits), then 53/52/51.
        assert_eq!(sends.len(), 6);
        let p0 = server.open(&sends[1]).expect("pad fragment opens");
        assert_eq!(p0[0], 0x81);
        let trio = [
            server.open(&sends[3]).expect("ping opens"),
            server.open(&sends[4]).expect("ping opens"),
            server.open(&sends[5]).expect("ping opens"),
        ];
        assert_eq!(trio.iter().filter(|d| d[7] == 0x53).count(), 1);
        assert_eq!(trio.iter().filter(|d| d[7] == 0x52).count(), 1);
        assert_eq!(trio.iter().filter(|d| d[7] == 0x51).count(), 1);
        assert!(trio.iter().all(|d| d[4] == 0x00));
        assert_eq!(client.keepalive_replies(), 3);

        // A second pad with the same id: echoed again (the cap is six
        // rounds per pad id, not one).
        client.on_packet(&server.data(&frames::reliable(4, 4, true, 0, &pad).unwrap()));
        assert_eq!(client.keepalive_replies(), 4);
        assert_eq!(client.split_messages_tx(), 4);
    }

    #[test]
    fn challenges_are_answered_but_never_waited_for() {
        let mut client = Client::new(config());
        let mut server = Server::new(client.public());
        handshake(&mut client, &mut server);
        client.set_solver(Box::new(|_p, _job| Some(0x0BAD_F00D)), "job-1");

        // The server's form: 13-byte header promising an 8-byte blob.
        let mut ch = vec![0x9Bu8, 1, 0, 0, 0, 2, 0, 0, 0, 8, 0, 0, 0];
        ch.extend([0xAB; 8]);
        let d = frames::reliable(0, 0, true, 0, &ch).unwrap();
        client.on_packet(&server.data(&d));

        // Answered at once, and the challenge is not handed to the app.
        assert!(client.saw_challenge());
        assert!(client.take_messages().is_empty());
        let sends = client.take_sends();
        assert_eq!(sends.len(), 2); // the ACK, then the answer
        let ans = server.open(&sends[1]).expect("answer opens");
        assert_eq!(ans[0], 0x81);
        assert_eq!(ans[4], 0x60); // reliable ordered
        assert_eq!(ans[14..19], [0x9B, 2, 0, 0, 0]); // [9b][u2]
        assert_eq!(ans[19..23], [0x0D, 0xF0, 0xAD, 0x0B]); // answer LE
        assert_eq!(client.challenge_answers(), 1);

        // Mid-burst: held, then answered when the burst closes.
        let mut ch2 = ch.clone();
        ch2[5] = 3; // a different u2
        let d = frames::reliable(1, 1, true, 1, &ch2).unwrap();
        client.begin_join_flow();
        client.on_packet(&server.data(&d));
        assert_eq!(client.challenge_answers(), 1);
        client.end_join_flow();
        assert_eq!(client.challenge_answers(), 2);
        let sends = client.take_sends();
        let ans = server.open(&sends[1]).expect("deferred answer opens");
        assert_eq!(ans[14..19], [0x9B, 3, 0, 0, 0]);

        // An unsolvable challenge: nothing sent, the session continues.
        let mut ch3 = ch;
        ch3[5] = 4;
        client.set_solver(Box::new(|_p, _job| None), "job-1");
        let d = frames::reliable(2, 2, true, 2, &ch3).unwrap();
        client.on_packet(&server.data(&d));
        assert_eq!(client.challenge_answers(), 2);
        assert!(client.data_ready());
        // Only the datagram's ACK came back.
        assert_eq!(client.take_sends().len(), 1);
    }

    #[test]
    fn a_client_without_a_solver_just_continues() {
        let mut client = Client::new(config());
        let mut server = Server::new(client.public());
        handshake(&mut client, &mut server);

        let mut ch = vec![0x9Bu8, 1, 0, 0, 0, 2, 0, 0, 0, 8, 0, 0, 0];
        ch.extend([0xAB; 8]);
        let d = frames::reliable(0, 0, true, 0, &ch).unwrap();
        client.on_packet(&server.data(&d));

        assert!(client.saw_challenge());
        assert!(client.take_messages().is_empty());
        assert_eq!(client.challenge_answers(), 0);
        assert_eq!(client.take_sends().len(), 1); // just the ACK
        assert!(client.data_ready());
    }

    #[test]
    fn snapshot_markers_are_acked() {
        let mut client = Client::new(config());
        let mut server = Server::new(client.public());
        handshake(&mut client, &mut server);

        let marker = [0x84u8, 0x01, 0x02, 0x03, 0x04];
        let d = frames::reliable(0, 0, true, 0, &marker).unwrap();
        client.on_packet(&server.data(&d));

        assert!(client.take_messages().is_empty());
        let sends = client.take_sends();
        assert_eq!(sends.len(), 2);
        let ack = server.open(&sends[1]).expect("marker ack opens");
        assert_eq!(ack[14..19], [0x04, 0x01, 0x02, 0x03, 0x04]);
        assert_eq!(client.marker_acks(), 1);
    }

    #[test]
    fn data_pings_are_ponged_with_the_sub_id_flipped() {
        let mut client = Client::new(config());
        let mut server = Server::new(client.public());
        handshake(&mut client, &mut server);

        // The server's ~1 Hz 0x83/05 ping: the body echoed, the sub-id
        // flipped to 0x06, and nothing handed to the app layer.
        let ping = [0x83u8, 0x05, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
        let d = frames::reliable(0, 0, true, 0, &ping).unwrap();
        client.on_packet(&server.data(&d));
        assert!(client.take_messages().is_empty());
        assert_eq!(client.data_pongs(), 1);
        let sends = client.take_sends();
        assert_eq!(sends.len(), 2); // the ACK, then the pong
        let pong = server.open(&sends[1]).expect("pong opens");
        assert_eq!(pong[0], 0x81);
        assert_eq!(pong[4], 0x60); // reliable ordered, like the ping
        // [14..] is the delivered payload: id, sub-id, body verbatim.
        assert_eq!(&pong[14..21], &[0x83, 0x06, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE]);

        // Any other 0x83 subtype is app data, not a ping.
        let chain = [0x83u8, 0x16, 0x00, 0x01, 0x02];
        let d = frames::reliable(1, 1, true, 1, &chain).unwrap();
        client.on_packet(&server.data(&d));
        assert_eq!(client.data_pongs(), 1);
        assert_eq!(client.take_messages(), vec![chain.to_vec()]);
    }
}
