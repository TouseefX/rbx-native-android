//! The join burst: the ordered message run that asks the RCC to let
//! the session into the place. Ported from the reference's
//! `sendJoinFlow`, whose measured shape this keeps verbatim —
//! `0x13`@0, `0x90`@1, `0x92`@2, `0x8a`@3, `0x8f`@4, leaving index 5
//! for the challenge answer.
//!
//! The whole run is bracketed with
//! [`begin_join_flow`](crate::client::Client::begin_join_flow)/
//! [`end_join_flow`](crate::client::Client::end_join_flow), so a
//! challenge that lands mid-burst holds its answer until the burst's
//! messages have taken the indexes the native leaves them at.

use crate::client::Client;
use crate::join_msg;
use serde_json::Value;

/// One burst step, as it left (or did not leave) the wire.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// The step's message went out: what it was, how many datagrams it
    /// took.
    Sent {
        what: &'static str,
        datagrams: usize,
    },
    /// The step did not send, and why. A skipped `0x90` ends the burst
    /// (the join body is the one message the RCC cannot do without);
    /// the others only leave their slot empty.
    Skipped { what: &'static str, why: String },
}

impl Step {
    /// The step's name, for on-screen diagnostics.
    pub fn what(&self) -> &'static str {
        match self {
            Step::Sent { what, .. } | Step::Skipped { what, .. } => what,
        }
    }

    /// Whether the step's message went out.
    pub fn sent(&self) -> bool {
        matches!(self, Step::Sent { .. })
    }
}

/// The burst's A/B knobs, at their measured defaults.
#[derive(Debug)]
pub struct FlowOptions {
    /// The `0x92` and `0x8f` slots. On (default): the full burst, which
    /// is what makes the RCC issue the 0x9B challenge for our body —
    /// 6/6 measured runs challenged with it, 0/6 without. Off: the
    /// native's shape, empty slots 2 and 4.
    pub join_extra: bool,
    /// The 1160-byte `0x00` MTU pad the native puts between its 0x13
    /// and its 0x90. Off (empty, default): with the pad in front of
    /// our body the RCC answers 0x93 and never challenges (measured).
    /// Given, and at least 1160 bytes, it is sent sequenced — not
    /// ordered, so it takes no ordering slot.
    pub pre_pad: Vec<u8>,
    /// The `0xA7` script-hash queue `[a7][count u16le][count x 16B]`.
    /// On RbxTransport a non-empty queue is what draws the challenge;
    /// the native's RakNet stream carries none. Empty (default) = off.
    pub a7: Vec<u8>,
}

impl Default for FlowOptions {
    fn default() -> Self {
        FlowOptions {
            join_extra: true,
            pre_pad: Vec::new(),
            a7: Vec::new(),
        }
    }
}

/// The flow's per-session inputs: everything the burst builds from.
pub struct FlowInputs<'a> {
    /// Our socket address, for the `0x13`.
    pub local: ([u8; 4], u16),
    /// The join-game script (`joinScript` out of the join-game reply).
    pub js: &'a Value,
    /// The join-game HTTP reply, whose fields the 0x90 builder reads.
    pub reply: &'a Value,
    /// The place id; 0 = take `js`'s `PlaceId`.
    pub place: u32,
    /// The captured 0x90 template (>=64B, or the join is held).
    pub tmpl_90: &'a [u8],
    /// The captured 0x8a session template.
    pub tmpl_8a: &'a [u8],
    /// The caller's random draw for the `0x92` (the reference mixes
    /// the clock).
    pub rng32: u32,
}

/// Run the whole join burst — `0x13` through the last slot — with the
/// challenge bracketed so its answer can never land inside the run.
/// The handshake must be done (`data_ready`); the burst sends
/// `0x13`@0, `0x90`@1, `0x92`@2, `0x8a`@3, `0x8f`@4 and leaves index
/// 5 for the challenge answer.
pub fn send_join_flow(client: &mut Client, inputs: &FlowInputs, opts: &FlowOptions) -> Vec<Step> {
    let mut steps = Vec::new();
    if !client.data_ready() {
        steps.push(Step::Skipped {
            what: "join burst",
            why: "the data layer is not up".to_string(),
        });
        return steps;
    }
    client.begin_join_flow();

    // The 0x13: datagram 1 straight after the 0x09, ordered index 0 —
    // the one receipt the whole sequence hangs from.
    if client.new_incoming(inputs.local) {
        steps.push(Step::Sent {
            what: "0x13",
            datagrams: 1,
        });
    } else {
        steps.push(Step::Skipped {
            what: "0x13",
            why: "did not send".to_string(),
        });
        client.end_join_flow();
        return steps;
    }

    // The optional MTU pad: sequenced, not ordered, so it takes no
    // ordering slot.
    if opts.pre_pad.len() >= 1160 {
        let n = client.send_join_message(&opts.pre_pad, false);
        steps.push(Step::Sent {
            what: "MTU pad 0x00",
            datagrams: n,
        });
    }

    // The optional 0xA7 script-hash queue: `[a7][count u16le][count x
    // 16B]` or nothing.
    if !opts.a7.is_empty() {
        if opts.a7.len() >= 3 && opts.a7[0] == 0xA7 {
            let count = u16::from_le_bytes([opts.a7[1], opts.a7[2]]) as usize;
            if opts.a7.len() != 3 + count * 16 {
                steps.push(Step::Skipped {
                    what: "0xA7",
                    why: format!("{}B is not 3+{}B — not sent", opts.a7.len(), count * 16),
                });
            } else if count == 0 {
                steps.push(Step::Skipped {
                    what: "0xA7",
                    why: "count=0 — the server will not challenge".to_string(),
                });
            } else {
                let n = client.send_join_message(&opts.a7, true);
                steps.push(Step::Sent {
                    what: "0xA7",
                    datagrams: n,
                });
            }
        } else {
            steps.push(Step::Skipped {
                what: "0xA7",
                why: "malformed — not sent".to_string(),
            });
        }
    }

    // The 0x90 join body: built from the template, sealed with the
    // build-constant envelope key. A missing template (<64B) holds the
    // whole join — a server that never sees a join never challenges.
    let tmpl_90 = inputs.tmpl_90;
    if tmpl_90.len() < 64 {
        steps.push(Step::Skipped {
            what: "0x90",
            why: "no template (need >=64B) — join body held back".to_string(),
        });
        client.end_join_flow();
        return steps;
    }
    let (body90, _json) = match join_msg::build_90(tmpl_90, inputs.js, inputs.reply) {
        Ok(v) => v,
        Err(e) => {
            steps.push(Step::Skipped {
                what: "0x90",
                why: format!("build_90: {e}"),
            });
            client.end_join_flow();
            return steps;
        }
    };
    let msg = join_msg::sealed_message(0x90, &body90, &join_msg::KEY_90, join_msg::STATE_90);
    steps.push(sent(client, "0x90", &msg));

    // The 0x92's v9 is a build constant living in the template's tail.
    let v9 = join_msg::v9_from_90_template(tmpl_90).unwrap_or(0);
    if opts.join_extra {
        let msg = join_msg::build_92(inputs.rng32, v9);
        steps.push(sent(client, "0x92", &msg));
    } else {
        // The native leaves ordering index 2 empty and puts its ticket
        // at 3.
        client.reserve_ordering();
        steps.push(Step::Skipped {
            what: "0x92",
            why: "slot left empty (native shape)".to_string(),
        });
    }

    // The 0x8a ticket: skipped without a UserId/ClientTicket — the
    // slot is not reserved, so whatever follows lands one index early.
    let uid = inputs.js.get("UserId").and_then(Value::as_u64).unwrap_or(0) as u32;
    let ticket = inputs
        .js
        .get("ClientTicket")
        .and_then(Value::as_str)
        .unwrap_or("");
    if uid == 0 || ticket.is_empty() {
        steps.push(Step::Skipped {
            what: "0x8a",
            why: "join script has no UserId/ClientTicket".to_string(),
        });
    } else {
        match join_msg::build_8a(uid, ticket, inputs.tmpl_8a, inputs.js) {
            Ok(body8a) => {
                let place_id = if inputs.place != 0 {
                    inputs.place
                } else {
                    inputs
                        .js
                        .get("PlaceId")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as u32
                };
                let t2 = join_msg::tail2_from_90_template(tmpl_90).unwrap_or(0);
                // The key is the string's first 16 bytes, verbatim —
                // the native uses the ASCII digits as the AES key.
                let key_str = join_msg::key_8a(place_id, t2);
                let mut key = [0u8; 16];
                let kb = key_str.as_bytes();
                if kb.len() < 16 {
                    steps.push(Step::Skipped {
                        what: "0x8a",
                        why: format!("key \"{key_str}\" is under 16 bytes"),
                    });
                } else {
                    key.copy_from_slice(&kb[..16]);
                    let msg = join_msg::sealed_message(0x8A, &body8a, &key, join_msg::STATE_8A);
                    steps.push(sent(client, "0x8a", &msg));
                }
            }
            Err(e) => {
                steps.push(Step::Skipped {
                    what: "0x8a",
                    why: format!("build_8a: {e}"),
                });
                // The reference holds the rest of the flow here too.
                client.end_join_flow();
                return steps;
            }
        }
    }

    // The native sends this burst back-to-back with nothing in between;
    // the ACKs owed so far ride ahead of the tail.
    client.flush_acks();

    if opts.join_extra {
        let msg = join_msg::build_8f("");
        steps.push(sent(client, "0x8f", &msg));
    } else {
        // The native leaves ordering index 4 empty; its next ordered
        // message is the challenge answer at 5.
        client.reserve_ordering();
        steps.push(Step::Skipped {
            what: "0x8f",
            why: "slot left empty (native shape)".to_string(),
        });
    }

    client.end_join_flow();
    steps
}

/// Send one ordered join message and fold the result into a step.
fn sent(client: &mut Client, what: &'static str, msg: &[u8]) -> Step {
    let datagrams = client.send_join_message(msg, true);
    Step::Sent { what, datagrams }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::tests::{config, handshake, Server};
    use crate::frames;
    use serde_json::json;

    /// A 0x90 template: 3 prefix bytes, LEB length, JSON, 24-byte tail
    /// with the build constants where the reference reads them.
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

    fn join_script() -> Value {
        json!({"UserId": 5, "ClientTicket": "ticket-part-1|ticket-part-2", "PlaceId": 1818})
    }

    fn inputs<'a>(js: &'a Value, tmpl_90: &'a [u8], reply: &'a Value) -> FlowInputs<'a> {
        FlowInputs {
            local: ([192, 168, 1, 50], 50000),
            js,
            reply,
            place: 1818,
            tmpl_90,
            tmpl_8a: b"",
            rng32: 0x1234_5678,
        }
    }

    /// Run the flow against a fresh mock server and hand back the
    /// steps plus everything the server delivered, as (id, ordering).
    fn run(js: &Value, tmpl_90: &[u8], opts: &FlowOptions) -> (Vec<Step>, Vec<(u8, u32)>) {
        let mut client = Client::new(config());
        let mut server = Server::new(client.public());
        handshake(&mut client, &mut server);
        let reply = Value::Null;
        let steps = send_join_flow(&mut client, &inputs(js, tmpl_90, &reply), opts);
        let mut delivered = Vec::new();
        for wire in client.take_sends() {
            if let Some(plain) = server.open(&wire) {
                for m in server.rx.on_datagram(&plain) {
                    delivered.push((m.id, m.ordering));
                }
            }
        }
        (steps, delivered)
    }

    #[test]
    fn the_burst_lands_each_message_at_its_native_index() {
        let (steps, delivered) = run(&join_script(), &template_90(), &FlowOptions::default());
        assert!(steps.iter().all(Step::sent), "nothing may skip: {steps:?}");
        assert_eq!(
            steps.iter().map(Step::what).collect::<Vec<_>>(),
            ["0x13", "0x90", "0x92", "0x8a", "0x8f"]
        );
        assert_eq!(
            delivered,
            vec![(0x13, 0), (0x90, 1), (0x92, 2), (0x8a, 3), (0x8f, 4)]
        );
    }

    #[test]
    fn the_92_carries_the_template_tail_constants() {
        // v9 = tail[3] ^ (u16(tail[2]) | 0x22640000) = 0x33469889 here.
        let mut client = Client::new(config());
        let mut server = Server::new(client.public());
        handshake(&mut client, &mut server);
        let js = join_script();
        let reply = Value::Null;
        let steps = send_join_flow(
            &mut client,
            &inputs(&js, &template_90(), &reply),
            &FlowOptions::default(),
        );
        assert!(steps.iter().all(Step::sent));
        let mut got = Vec::new();
        for wire in client.take_sends() {
            if let Some(plain) = server.open(&wire) {
                got.extend(server.rx.on_datagram(&plain));
            }
        }
        let m92 = got.iter().find(|m| m.id == 0x92).expect("the 0x92 arrives");
        assert_eq!(m92.payload, join_msg::build_92(0x1234_5678, 0x3346_9889));
    }

    #[test]
    fn a_missing_90_template_holds_the_rest_of_the_join() {
        let (steps, delivered) = run(&join_script(), b"", &FlowOptions::default());
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].what(), "0x13");
        assert!(steps[0].sent());
        assert_eq!(steps[1].what(), "0x90");
        assert!(!steps[1].sent());
        // The 0x13 went out before the hold; nothing after it may.
        assert_eq!(delivered, vec![(0x13, 0)]);
    }

    #[test]
    fn native_shape_leaves_slots_2_and_4_empty_and_the_answer_lands_at_5() {
        let opts = FlowOptions {
            join_extra: false,
            ..FlowOptions::default()
        };
        let mut client = Client::new(config());
        let mut server = Server::new(client.public());
        handshake(&mut client, &mut server);
        let js = join_script();
        let reply = Value::Null;
        let steps = send_join_flow(&mut client, &inputs(&js, &template_90(), &reply), &opts);
        assert_eq!(
            steps.iter().map(Step::what).collect::<Vec<_>>(),
            ["0x13", "0x90", "0x92", "0x8a", "0x8f"]
        );
        let mut delivered = Vec::new();
        for wire in client.take_sends() {
            if let Some(plain) = server.open(&wire) {
                for m in server.rx.on_datagram(&plain) {
                    delivered.push((m.id, m.ordering));
                }
            }
        }
        // The 0x8a carries index 3 but sits behind the deliberately
        // empty slot 2; a filler at 2 is what flushes it out.
        assert_eq!(delivered, vec![(0x13, 0), (0x90, 1)]);
        let filler = frames::reliable(100, 500, true, 2, b"\x00filler2").unwrap();
        for m in server.rx.on_datagram(&filler) {
            delivered.push((m.id, m.ordering));
        }
        assert_eq!(delivered, vec![(0x13, 0), (0x90, 1), (0x8A, 3)]);
        // Slot 4 was reserved too, so the challenge answer's slot is 5.
        let answer = [0x9Bu8, 1, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(client.send_join_message(&answer, true), 1);
        let mut got = Vec::new();
        for wire in client.take_sends() {
            if let Some(plain) = server.open(&wire) {
                got.extend(server.rx.on_datagram(&plain));
            }
        }
        assert!(got.is_empty(), "the answer waits behind empty slot 4");
        let filler = frames::reliable(101, 501, true, 4, b"\x00filler4").unwrap();
        got.extend(server.rx.on_datagram(&filler));
        let m = got
            .iter()
            .find(|m| m.id == 0x9B)
            .expect("the answer arrives");
        assert_eq!(m.ordering, 5);
    }

    #[test]
    fn an_a7_queue_rides_ahead_of_the_90_when_given() {
        let mut a7 = vec![0xA7, 0x01, 0x00];
        a7.extend([0x11u8; 16]);
        let opts = FlowOptions {
            a7,
            ..FlowOptions::default()
        };
        let (steps, delivered) = run(&join_script(), &template_90(), &opts);
        assert!(steps.iter().all(Step::sent), "nothing may skip: {steps:?}");
        assert_eq!(delivered[1], (0xA7, 1));
        assert_eq!(delivered[2], (0x90, 2));
    }

    #[test]
    fn a_malformed_a7_queue_is_skipped_not_sent() {
        let mut a7 = vec![0xA7, 0x05, 0x00]; // count says 5, body has one
        a7.extend([0x11u8; 16]);
        let opts = FlowOptions {
            a7,
            ..FlowOptions::default()
        };
        let (steps, delivered) = run(&join_script(), &template_90(), &opts);
        assert_eq!(steps[1].what(), "0xA7");
        assert!(!steps[1].sent());
        assert_eq!(
            delivered,
            vec![(0x13, 0), (0x90, 1), (0x92, 2), (0x8a, 3), (0x8f, 4)]
        );
    }

    #[test]
    fn no_8a_without_a_ticket_and_the_next_message_takes_its_slot() {
        let js = json!({"UserId": 5, "PlaceId": 1818}); // no ClientTicket
        let (steps, delivered) = run(&js, &template_90(), &FlowOptions::default());
        assert!(!steps.iter().all(Step::sent));
        assert_eq!(
            steps.iter().map(Step::what).collect::<Vec<_>>(),
            ["0x13", "0x90", "0x92", "0x8a", "0x8f"]
        );
        // The 0x8a slot is not reserved, so the 0x8f lands at 3.
        assert_eq!(delivered, vec![(0x13, 0), (0x90, 1), (0x92, 2), (0x8f, 3)]);
    }

    #[test]
    fn nothing_leaves_before_the_data_layer_is_up() {
        let mut client = Client::new(config());
        let js = join_script();
        let reply = Value::Null;
        let steps = send_join_flow(
            &mut client,
            &inputs(&js, &template_90(), &reply),
            &FlowOptions::default(),
        );
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].what(), "join burst");
        assert!(!steps[0].sent());
        assert!(client.take_sends().is_empty());
    }
}
