//! The join sequencer: the whole run from a join script to a live
//! session, the reference `client/apps/raknet_main.cpp`'s shape, with
//! the sockets behind an [`Io`] so a test drives it without one.
//!
//! The sequence, every step of it measured against live servers:
//!
//! 1. Req1 up to three times (a 2s answer window each) until Reply1;
//! 2. Req2 up to three times until Reply2 verifies (its tag proves the
//!    rx key);
//! 3. the first encrypted datagram, ID_CONNECTION_REQUEST (0x09), a
//!    short answer window, then the ACKs it owed — the native sends its
//!    0x13 and 0x90 back-to-back with the 0x09;
//! 4. the join burst ([`join_flow::send_join_flow`]): `0x13`@0, the
//!    pre-join MTU ladder (unordered), `0x90`@1, `0x92`@2, `0x8a`@3,
//!    `0x8f`@4, the challenge's answer at 5 — never gated on the `0x93`
//!    (the RCC answers the `0x90` with its `0x93` and the `0x9B` within
//!    two datagrams, and the native has its `0x90` in flight before the
//!    `0x93` lands);
//! 5. the session, at the native's measured cadence: pump every 30ms,
//!    `0xA6` route frames every ~75ms (`01` once, then the tagged `02`
//!    through `04`, `stepId` +4 per message from `0x14c`), one `0xA1`
//!    camera frame per ~730ms, `0x83/23` input clocks every ~170ms, the
//!    `0x83/05` physics-ack pair every ~2.5/5s, the `0x83/07` opener
//!    once, ACKs every ~250ms, and reliable retransmits pumped every
//!    loop.
//!
//! The challenge is answered reactively inside the client, never waited
//! for; a failed solve sends nothing (a wrong answer gets the session
//! kicked, a missing one does not).

use crate::client::{Client, Solver};
use crate::join_flow::{self, FlowInputs, FlowOptions, Step};
use crate::join_target;
use serde_json::Value;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;

/// What the driver needs from the network: datagrams out to the peer a
/// socket is connected to, one back within `ms` (or `None`).
pub trait Io {
    fn send(&mut self, datagram: &[u8]) -> bool;
    fn recv_timeout(&mut self, ms: u32) -> Option<Vec<u8>>;
}

/// The real [`Io`]: one UDP socket, bound anywhere, connected to the
/// RakNet endpoint (the join script's `UdmuxEndpoints[0]`, not the RCC —
/// every datagram's TLV2 carries the RCC pair instead).
pub struct UdpIo {
    socket: UdpSocket,
}

impl UdpIo {
    pub fn connect(peer: ([u8; 4], u16)) -> Result<UdpIo, String> {
        let addr = SocketAddr::from((IpAddr::V4(Ipv4Addr::from(peer.0)), peer.1));
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(|err| format!("socket: {err}"))?;
        socket
            .connect(addr)
            .map_err(|err| format!("connect: {err}"))?;
        Ok(UdpIo { socket })
    }
}

impl Io for UdpIo {
    fn send(&mut self, datagram: &[u8]) -> bool {
        self.socket.send(datagram).is_ok()
    }

    fn recv_timeout(&mut self, ms: u32) -> Option<Vec<u8>> {
        let _ = self
            .socket
            .set_read_timeout(Some(Duration::from_millis(u64::from(ms))));
        let mut buf = [0u8; 2048];
        match self.socket.recv(&mut buf) {
            Ok(n) => Some(buf[..n].to_vec()),
            Err(_) => None,
        }
    }
}

/// Everything one run needs. `js` is the join script with the hydration
/// fields already merged in (see [`crate::join_game`]); the templates
/// are the captured `0x90` and `0x8a` bodies a native session sent, the
/// same ones the reference replays; `key_seed` pins the session
/// keypair, for tests and replays.
pub struct DriveInputs<'a> {
    pub js: &'a Value,
    pub reply: &'a Value,
    pub place: u64,
    pub tmpl_90: &'a [u8],
    pub tmpl_8a: &'a [u8],
    pub solver: Solver,
    pub duration_ms: u64,
    pub key_seed: Option<[u8; 32]>,
}

/// How a run ended: what the burst sent, and what the session saw.
#[derive(Debug)]
pub struct Report {
    pub steps: Vec<Step>,
    pub datagrams_tx: u64,
    pub app_messages: u64,
    pub app_bytes: u64,
    pub challenge_answers: u64,
    pub saw_challenge: bool,
    pub saw_net_settings: bool,
    /// What the replication stream carried; see `replication`.
    pub replication: crate::replication::Replication,
}

/// The RakNet endpoint never answered Req1.
pub const NO_REPLY1: &str = "no OpenConnectionReply1 from the RakNet endpoint";
/// Req2's reply never verified (or never came).
pub const NO_REPLY2: &str = "no verified OpenConnectionReply2 (rx key mismatch?)";

/// True for the errors a fresh join script and a fresh socket are worth:
/// the offline/encrypted handshake never completed. The reference
/// measured roughly one session in four wedging this way — the RCC
/// stops ACKing partway through, and re-sending inside the *same*
/// session cannot help, everything queues behind the same hole. A
/// fresh session with a fresh script takes a ~75% per-attempt rate to
/// ~99% over three attempts.
pub fn is_handshake_miss(err: &str) -> bool {
    err == NO_REPLY1 || err == NO_REPLY2
}

/// Runs the whole sequence on `io`, until `duration_ms` of session have
/// passed by `now_ms`'s clock. Every step is logged through `log`, a
/// handful of lines, for on-screen diagnostics.
pub fn drive(
    inputs: DriveInputs,
    io: &mut dyn Io,
    now_ms: &dyn Fn() -> u64,
    log: impl FnMut(String) + 'static,
) -> Result<Report, String> {
    let mut cfg = join_target::config_from_join_script(inputs.js)?;
    cfg.key_seed = inputs.key_seed;
    let job_id = inputs
        .js
        .get("JobId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut client = Client::new(cfg);
    let log = std::rc::Rc::new(std::cell::RefCell::new(log));
    {
        let sink = log.clone();
        client.set_logger(Box::new(move |line| (*sink.borrow_mut())(line)));
    }
    client.set_solver(inputs.solver, &job_id);
    let mut say = |line: String| (*log.borrow_mut())(line);

    // 1. Req1, up to three times, two seconds per answer window.
    for attempt in 1..=3u8 {
        if !client.req1() {
            return Err("Req1 build/send failed".to_string());
        }
        flush_sends(&mut client, io);
        pump(&mut client, io, 2000);
        if client.stage() >= 1 {
            break;
        }
        say(format!("no Reply1 yet, retrying Req1 ({attempt}/3)"));
    }
    if client.stage() < 1 {
        return Err(NO_REPLY1.to_string());
    }
    say(format!(
        "Reply1 guid={} mtu={} (offline handshake accepted)",
        client.guid(),
        client.mtu()
    ));

    // 2. Req2, up to three times, until the tag verifies.
    for attempt in 1..=3u8 {
        if !client.req2() {
            return Err("Req2 build/send failed (ticket blobs too large?)".to_string());
        }
        flush_sends(&mut client, io);
        pump(&mut client, io, 2000);
        if client.data_ready() {
            break;
        }
        say(format!(
            "no verified Reply2 yet, retrying Req2 ({attempt}/3)"
        ));
    }
    if !client.data_ready() {
        return Err(NO_REPLY2.to_string());
    }
    say("encrypted handshake complete (Reply2 verified)".to_string());

    // 3. The first encrypted datagram: ID_CONNECTION_REQUEST, a short
    //    answer window, then the ACKs it owed. The native sends its 0x13
    //    and 0x90 back-to-back with the 0x09 — a 600 ms wait for the
    //    0x93 settings push is not part of its join.
    client.connection_request();
    flush_sends(&mut client, io);
    pump(&mut client, io, 30);
    client.flush_acks();
    flush_sends(&mut client, io);
    say(if client.saw_net_settings() {
        "network settings (0x93) received before the join".to_string()
    } else {
        "0x93 not seen, joining anyway (the native does not wait)".to_string()
    });

    // 4. The join burst. The 0x92's random draw mixes the clock, as the
    //    reference does.
    let rng32 = (now_ms() as u32) ^ 0x9E37_79B9;
    let flow = FlowInputs {
        js: inputs.js,
        reply: inputs.reply,
        place: inputs.place,
        tmpl_90: inputs.tmpl_90,
        tmpl_8a: inputs.tmpl_8a,
        rng32,
    };
    let steps = join_flow::send_join_flow(&mut client, &flow, &FlowOptions::default());
    for step in &steps {
        match step {
            Step::Sent { what, datagrams } => {
                say(format!("join: {what} sent ({datagrams} datagrams)"))
            }
            Step::Skipped { what, why } => say(format!("join: {what} skipped: {why}")),
        }
    }
    flush_sends(&mut client, io);

    // 5. The session. The cadence is the native's, measured from its
    // decrypted TX stream: 0xA6 route frames at ~13/s, one 0xA1 camera
    // frame per ~730 ms, 0x83/23 input clocks at ~6/s (the clock starts
    // at 0x270 and advances +8 per tick), a 0x83/05 physics ack pair
    // (unreliable 45B every ~2.5 s, reliable 50B every ~5 s) and the
    // 19-byte 0x83/07 control opener once.
    let start = now_ms();
    let mut next_input = start;
    let mut next_ack = start + 250;
    let mut next_camera = start;
    let mut next_clock_tick = start;
    let mut next_ack83 = start + 2500;
    let mut next_ack83_pair = start + 5000;
    let mut a6_step: u32 = 0x14c;
    let mut a6_stage = 0u32;
    let mut clock_step: u32 = 0x270;
    let mut hb_started = false;
    // The quiet-line state: see the liveness check in the loop below.
    let mut quiet_announced: u64 = 0;
    let mut quiet_acks: u64 = 0;
    let mut quiet_alive: Option<bool> = None;
    let mut app_messages = 0u64;
    let mut app_bytes = 0u64;
    let mut first_message = true;
    let mut replication = crate::replication::Replication::default();
    while now_ms().saturating_sub(start) < inputs.duration_ms {
        pump(&mut client, io, 30);
        let now = now_ms();
        if now >= next_input {
            next_input += 75;
            // A clock that jumped (or a stall) does not fire a burst of
            // catch-up frames; the reference snaps the schedule.
            if now > next_input + 500 {
                next_input = now + 75;
            }
            let frame = a6_frame(a6_index(a6_stage), a6_step);
            if client.send_unreliable(&frame) && a6_stage == 0 {
                say("0xA6 input stream started (native route frames)".to_string());
            }
            // The 0xA1 camera frame: `a1 <u32le clock>` + the 39-byte
            // body the native's capture carries verbatim, one per
            // ~730 ms.
            if now >= next_camera {
                next_camera = now + 730;
                let mut a1 = vec![0xA1];
                a1.extend_from_slice(&a6_step.to_le_bytes());
                a1.extend_from_slice(&A1_TAIL);
                client.send_unreliable(&a1);
            }
            a6_step = a6_step.wrapping_add(4);
            a6_stage += 1;
        }
        // The post-join control stream starts with the first app
        // message — the reference keys it off the world existing.
        if !hb_started && app_messages > 0 {
            hb_started = true;
            let opener = hex_bytes("830701b612050013100000e04023b401000000");
            client.send_join_message(&opener, true);
        }
        if hb_started {
            if now >= next_clock_tick {
                next_clock_tick = now + 170;
                clock_step = clock_step.wrapping_add(8);
                let hb = [
                    0x83u8,
                    0x23,
                    (clock_step & 0xff) as u8,
                    ((clock_step >> 8) & 0xff) as u8,
                    ((clock_step >> 16) & 0xff) as u8,
                    0,
                    0,
                ];
                client.send_join_message(&hb, true);
            }
            if now >= next_ack83 {
                let pair = now >= next_ack83_pair;
                next_ack83 = now + 2500;
                if pair {
                    next_ack83_pair = now + 5000;
                }
                let unrel = hex_bytes(
                    "830516dc10000000000000dc10000000000000b4f3314298612842000000000000000001db100000a03a834400",
                );
                client.send_unreliable(&unrel);
                if pair {
                    let rel = hex_bytes(
                        "830516dc10000000000000dc10000000000000b4f3314298612842000000000000000000db100000a03a8344230c02000000",
                    );
                    client.send_join_message(&rel, true);
                }
            }
        }
        // The RCC never sends a disconnect: silence alone cannot tell a
        // reaped session from an idle world, but its ACKs can.
        if let Some(quiet) = client.ms_since_rx() {
            if quiet > 6000 {
                let acks = client.server_acks();
                let alive = acks > quiet_acks;
                let flip = quiet_alive != Some(alive);
                if flip || quiet.saturating_sub(quiet_announced) > 60_000 {
                    quiet_announced = quiet;
                    quiet_acks = acks;
                    quiet_alive = Some(alive);
                    say(if alive {
                        format!(
                            "no server data for {}s — idle world (the RCC still ACKs our datagrams)",
                            quiet / 1000
                        )
                    } else {
                        format!(
                            "no server data for {}s and no ACKs either — reaped session",
                            quiet / 1000
                        )
                    });
                }
            } else {
                // Data flowed again: the next silence starts from zero.
                quiet_announced = 0;
                quiet_acks = client.server_acks();
                quiet_alive = None;
            }
        }
        if now >= next_ack {
            next_ack += 250;
            client.flush_acks();
        }
        flush_sends(&mut client, io);
        for message in client.take_messages() {
            app_messages += 1;
            app_bytes += message.len() as u64;
            // What the session carries, framed and classified (see
            // `replication`): join data, instance batches, terrain,
            // peers — the census the world builder will feed on.
            replication.feed(&message, &mut say);
            if first_message {
                first_message = false;
                let id = message.first().copied().unwrap_or(0);
                say(format!(
                    "first app message: 0x{id:02x} ({}B)",
                    message.len()
                ));
            }
        }
    }
    let mut line = format!(
        "session stopped after {}ms: {} messages ({}B), {} datagrams out, {} server ACKs, {} retransmits, {} challenges answered",
        now_ms().saturating_sub(start),
        app_messages,
        app_bytes,
        client.datagrams_tx(),
        client.server_acks(),
        client.retransmits_tx(),
        client.challenge_answers()
    );
    if client.rtx_untracked() > 0 {
        // The retry budget was spent when these went out: the sends
        // still left, but nothing is retransmitting them. Counted
        // rather than silent — this is the backpressure signal.
        line.push_str(&format!(", {} untracked", client.rtx_untracked()));
    }
    say(line);
    if let Some(summary) = replication.summary() {
        say(format!("replication: {summary}"));
    }
    Ok(Report {
        steps,
        datagrams_tx: client.datagrams_tx(),
        app_messages,
        app_bytes,
        challenge_answers: client.challenge_answers(),
        saw_challenge: client.saw_challenge(),
        saw_net_settings: client.saw_net_settings(),
        replication,
    })
}

/// Everything the client queued, out.
fn flush_sends(client: &mut Client, io: &mut dyn Io) {
    for datagram in client.take_sends() {
        io.send(&datagram);
    }
}

/// Everything that arrives within `ms`, into the client — and everything
/// that makes it queue, back out. The window is per quiet spell: the
/// first receive that times out ends it, which for a join client's
/// traffic is the same wall clock as the reference's deadline. Due
/// reliable retransmits go out here too, so a lost join fragment is
/// re-sent on time instead of after a long receive block.
fn pump(client: &mut Client, io: &mut dyn Io, ms: u32) {
    while let Some(datagram) = io.recv_timeout(ms) {
        client.on_packet(&datagram);
    }
    client.pump_rtx();
    flush_sends(client, io);
}

/// The native's `0xA6` route frames (2026-10-30): `01` is the short
/// count=1 form, `02`-`04` carry the 11-byte tag
/// `10 03 00 64 d0 b7 e6 d7 ff 03 e0`, and the step starts at `0x14c`,
/// +4 per message. The step is patched into the two 4-byte LE slots
/// each frame carries.
const A6: [(&str, usize); 4] = [
    ("a6010100014c0100004c0100000000", 5),
    ("a602010010030064d0b7e6d7ff03e050010000500100000000040400", 15),
    (
        "a603010010030064d0b7e6d7ff03e054010000540100000000040400040400",
        15,
    ),
    (
        "a604010010030064d0b7e6d7ff03e058010000580100000000040400040400040400",
        15,
    ),
];

/// The native `0xA1` camera frame's 39-byte body, carried verbatim by
/// every frame in the capture (only the clock changes).
const A1_TAIL: [u8; 39] = [
    0x01, 0x00, 0x00, 0x00, 0x43, 0x62, 0xf9, 0xff, 0x01, 0x00, //
    0x00, 0x00, 0x01, 0xb6, 0x12, 0x05, 0x00, 0x7e, 0x01, 0x09, //
    0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, //
    0x00, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
];

fn a6_index(stage: u32) -> usize {
    if stage < 3 {
        stage as usize
    } else {
        3
    }
}

fn a6_frame(idx: usize, step: u32) -> Vec<u8> {
    let mut frame = hex_bytes(A6[idx].0);
    let at = A6[idx].1;
    if frame.len() >= at + 8 {
        for k in 0..4 {
            frame[at + k] = (step >> (8 * k)) as u8;
            frame[at + 4 + k] = (step >> (8 * k)) as u8;
        }
    }
    frame
}

fn hex_bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks(2)
        .map(|pair| match pair {
            [a, b] => {
                let hi = (*a as char).to_digit(16).unwrap_or(0);
                let lo = (*b as char).to_digit(16).unwrap_or(0);
                (hi * 16 + lo) as u8
            }
            _ => 0,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::tests::Server;
    use serde_json::json;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::rc::Rc;

    /// The join script the target reader accepts, with the same shape
    /// `join_target`'s own tests use.
    fn join_script() -> Value {
        json!({
            "UserId": 5,
            "JobId": "0f0f0f0f-1111-2222-3333-444444444444",
            "PlaceId": 1818,
            "ClientTicket": format!(
                "a;b;{};{};e",
                b64(&[0xAAu8; 33]),
                b64(&[0x55u8; 66])
            ),
            "NetStackPort": 51751,
            "UdmuxEndpoints": [{"Address": "10.190.0.5", "Port": 50105}],
            "ServerConnections": [{"Address": "10.17.9.213", "Port": 9999}],
            "NetStackTokenValue": b64(&[0x42u8; 16]),
            // The mock server presents SERVER_SEED's key, so the script's
            // EphemeralEarlyPubKey must be exactly that for Reply2 to
            // verify (a different one is an rx key mismatch).
            "EphemeralEarlyPubKey": b64(&crate::client::tests::peer_pub()),
        })
    }

    /// A 0x90 template, the same shape `join_flow`'s own tests use: 3
    /// prefix bytes, LEB length, JSON, 24-byte tail with the build
    /// constants where the reference reads them.
    fn template_90() -> Vec<u8> {
        let json = br#"{"UserId":1,"A":1,"B":"x","C":{"D":true,"E":2}}"#;
        let mut t = vec![0x01, 0x02, 0x03];
        let mut leb = Vec::new();
        let mut n = json.len() as u64;
        loop {
            let mut b = (n & 0x7F) as u8;
            n >>= 7;
            if n != 0 {
                b |= 0x80;
            }
            leb.push(b);
            if n == 0 {
                break;
            }
        }
        t.extend_from_slice(&leb);
        t.extend_from_slice(json);
        let mut tail = [0u8; 24];
        tail[8..12].copy_from_slice(&0x0000_ABCDu32.to_le_bytes());
        tail[12..16].copy_from_slice(&0x1122_3344u32.to_le_bytes());
        t.extend_from_slice(&tail);
        t
    }

    fn b64(data: &[u8]) -> String {
        const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            out.push(A[(n >> 18) as usize & 63] as char);
            out.push(A[(n >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 {
                A[(n >> 6) as usize & 63] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                A[n as usize & 63] as char
            } else {
                '='
            });
        }
        out
    }

    /// The script's side of the network: a queue of datagrams the
    /// client will receive, and a count of what it sent.
    struct Scripted {
        inbox: VecDeque<Vec<u8>>,
        sent: usize,
    }

    impl Io for Scripted {
        fn send(&mut self, _datagram: &[u8]) -> bool {
            self.sent += 1;
            true
        }

        fn recv_timeout(&mut self, _ms: u32) -> Option<Vec<u8>> {
            self.inbox.pop_front()
        }
    }

    #[test]
    fn the_a6_frames_patch_the_step_into_both_slots() {
        assert_eq!(a6_index(0), 0);
        assert_eq!(a6_index(2), 2);
        assert_eq!(a6_index(3), 3);
        assert_eq!(a6_index(9), 3);
        // The steps the native's frames carry: 0x14c +4 per message.
        let f = a6_frame(0, 0x14c);
        assert_eq!(f.len(), 15);
        assert_eq!(f[0], 0xa6);
        assert_eq!(&f[5..9], &0x14cu32.to_le_bytes());
        assert_eq!(&f[9..13], &0x14cu32.to_le_bytes());
        let f = a6_frame(1, 0x150);
        assert_eq!(f.len(), 28);
        assert_eq!(&f[15..19], &0x150u32.to_le_bytes());
        let f = a6_frame(2, 0x154);
        assert_eq!(f.len(), 31);
        assert_eq!(&f[15..19], &0x154u32.to_le_bytes());
        let f = a6_frame(3, 0x158);
        assert_eq!(f.len(), 34);
        assert_eq!(&f[15..19], &0x158u32.to_le_bytes());
        assert_eq!(&f[19..23], &0x158u32.to_le_bytes());
    }

    #[test]
    fn the_control_stream_constants_match_the_capture() {
        // The 0x83/07 opener is 19 bytes; the 0x83/05 acks are the
        // captured 45-byte unreliable and 50-byte reliable pair.
        assert_eq!(
            hex_bytes("830701b612050013100000e04023b401000000").len(),
            19
        );
        let unrel = hex_bytes(
            "830516dc10000000000000dc10000000000000b4f331429861284200000000000000001db100000a03a834400",
        );
        let rel = hex_bytes(
            "830516dc10000000000000dc10000000000000b4f3314298612842000000000000000000db100000a03a8344230c02000000",
        );
        assert_eq!(unrel.len(), 45);
        assert_eq!(rel.len(), 50);
        assert_eq!(A1_TAIL.len(), 39);
        assert_eq!(A1_TAIL[0], 0x01);
        assert_eq!(A1_TAIL[38], 0x3f);
    }

    #[test]
    fn hex_bytes_decodes_pairs() {
        assert_eq!(hex_bytes("00ff10"), vec![0x00, 0xff, 0x10]);
        assert_eq!(hex_bytes(""), Vec::<u8>::new());
        assert_eq!(hex_bytes("0"), vec![0]);
    }

    #[test]
    fn the_whole_run_handshake_burst_and_session() {
        let js = join_script();
        // The join reply the real v1 endpoint sends: status plus the
        // joinTicket build_90 splices into the 0x90 template's
        // `__joinTicket` (an object reply without one is an error).
        let reply = json!({
            "status": 2,
            "joinTicket": {
                "SerializedClientFields": "scf",
                "EncryptedServerFields": "esf"
            }
        });
        // The session keypair is pinned, so a mock server can be built
        // for the public key the driver's own client will draw.
        let mut cfg = join_target::config_from_join_script(&js).unwrap();
        cfg.key_seed = Some([1u8; 32]);
        let probe = Client::new(cfg);
        let server = Server::new(probe.public());
        let mut inbox = VecDeque::new();
        inbox.push_back(Server::reply1());
        inbox.push_back(server.wrap(&server.reply2()));
        // After Reply2 the server goes quiet; the session runs on the
        // injected clock alone.
        let mut io = Scripted { inbox, sent: 0 };

        let seen = Rc::new(RefCell::new(Vec::<String>::new()));
        let sink = seen.clone();
        let log = move |line: String| sink.borrow_mut().push(line);
        let tick = Rc::new(Cell::new(0u64));
        let clock = tick.clone();
        let now = move || {
            clock.set(clock.get() + 30);
            clock.get()
        };

        let report = drive(
            DriveInputs {
                js: &js,
                reply: &reply,
                place: 1818,
                tmpl_90: &template_90(),
                tmpl_8a: b"",
                solver: Box::new(|_msg, _job| Some(0x0BAD_F00D)),
                duration_ms: 300,
                key_seed: Some([1u8; 32]),
            },
            &mut io,
            &now,
            log,
        )
        .unwrap();

        // The handshake completed, the burst sent its receipts, and the
        // session pumped input without anything arriving.
        let lines = seen.borrow().join("\n");
        assert!(lines.contains("encrypted handshake complete"), "{lines}");
        assert!(
            report.steps.iter().any(|s| s.what() == "0x13" && s.sent()),
            "{lines}"
        );
        assert!(
            report.steps.iter().any(|s| s.what() == "0x90" && s.sent()),
            "{lines}"
        );
        assert!(lines.contains("0xA6 input stream started"), "{lines}");
        assert!(io.sent > 0);
        assert!(report.datagrams_tx > 0);
        assert_eq!(report.app_messages, 0);
        assert!(!report.saw_net_settings);
        assert!(lines.contains("session stopped after"), "{lines}");
    }

    #[test]
    fn a_silent_endpoint_runs_out_of_tries() {
        let js = join_script();
        let mut io = Scripted {
            inbox: VecDeque::new(),
            sent: 0,
        };
        let seen = Rc::new(RefCell::new(Vec::<String>::new()));
        let sink = seen.clone();
        let log = move |line: String| sink.borrow_mut().push(line);
        let err = drive(
            DriveInputs {
                js: &js,
                reply: &Value::Null,
                place: 1818,
                tmpl_90: &template_90(),
                tmpl_8a: b"",
                solver: Box::new(|_msg, _job| None),
                duration_ms: 0,
                key_seed: None,
            },
            &mut io,
            &|| 0,
            log,
        )
        .unwrap_err();
        assert_eq!(err, "no OpenConnectionReply1 from the RakNet endpoint");
        // All three Req1 tries went out first.
        assert_eq!(io.sent, 3);
    }

    #[test]
    fn only_the_handshake_misses_are_worth_a_fresh_session() {
        assert!(is_handshake_miss(NO_REPLY1));
        assert!(is_handshake_miss(NO_REPLY2));
        // Local failures and parse errors are not the RCC wedging.
        assert!(!is_handshake_miss("Req1 build/send failed"));
        assert!(!is_handshake_miss("Req2 build/send failed (ticket blobs too large?)"));
        assert!(!is_handshake_miss("joinScript lacks NetStackPort"));
        assert!(!is_handshake_miss(""));
    }
}
