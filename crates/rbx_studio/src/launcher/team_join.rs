//! The live Team Create session, from Home's Join button: the
//! team-create endpoint (asked until its all-null warming answer turns
//! into a config with an endpoint) with the saved cookie, then the
//! RakNet session on a UDP socket, on one background thread that lives
//! as long as the app does. The `0x90`/`0x8a` bodies are the captured
//! native templates (`assets/team-create`, the reference's own
//! captures), spliced with this session's values by `rbx_transport`;
//! the `0x9B` challenge is solved in-process by `rbx_challenge`.
//!
//! The place itself opens as a new local copy through Home's ordinary
//! download flow — the session rides beside the editor, its handful of
//! log lines on the status card (see `shell::team_create`) and stderr.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rbx_cloud::heartbeats::SessionCookie;
use rbx_transport::drive::{self, DriveInputs, UdpIo};
use serde_json::Value;

/// The captured native `0x90` join body — the 2026-10-29 capture, with
/// its 20-byte tail — spliced per session by `join_msg::build_90`.
pub(crate) const TMPL_90: &[u8] =
    include_bytes!("../../../../assets/team-create/native_90_fresh.bin");
/// The captured native `0x8a` ticket body.
pub(crate) const TMPL_8A: &[u8] = include_bytes!("../../../../assets/team-create/native_8a.bin");

/// One day: a session longer than that outlives the app anyway, and the
/// thread is a daemon the process takes with it.
const SESSION_MS: u64 = 24 * 60 * 60 * 1000;

/// How many times to re-ask the team-create endpoint for its config: the
/// first POST warms the session's server and answers an all-null
/// document, and the real one comes once the server is up — usually on
/// the second call, cold starts slower.
const NEGOTIATION_TRIES: u32 = 30;

/// How many fresh sessions a wedged RCC gets: roughly one in four never
/// completes the offline/encrypted handshake — it stops ACKing partway
/// through, and re-sending inside the same session cannot help,
/// everything queues behind the same hole (measured in the reference).
/// A fresh session with a fresh join script takes a ~75% per-attempt
/// rate to ~99%.
const SESSION_ATTEMPTS: u32 = 3;

/// The reply the join flow takes: the negotiation document when it
/// carries a `joinTicket`, `Null` otherwise — `build_90` keeps the
/// template's ticket for a non-object reply.
fn reply_for_flow(reply: &Value) -> &Value {
    static NULL: Value = Value::Null;
    if reply.get("joinTicket").is_some() {
        reply
    } else {
        &NULL
    }
}

/// How many of the session's last lines the status keeps.
const LOG_TAIL: usize = 6;

/// The session's on-screen status: the thread writes, the editor
/// window's card (see `shell::team_create`) reads a snapshot, and
/// `version` ticks on every write so the card's poller only repaints
/// when something actually changed. Without it the join is
/// logcat-only, which a phone never shows.
#[derive(Clone)]
pub(crate) struct Session {
    inner: Arc<SessionInner>,
}

struct SessionInner {
    state: Mutex<SessionState>,
    finished: AtomicBool,
}

struct SessionState {
    phase: Phase,
    log: VecDeque<String>,
    version: u64,
}

/// Where the join stands, for the card's headline and its dot's color.
#[derive(Clone)]
pub(crate) enum Phase {
    /// Asking the team-create endpoint, which answers an all-null
    /// document while the session's server warms up.
    Negotiating { attempt: u32 },
    /// The config arrived; the RakNet connect and the join burst follow.
    Joined,
    /// The RCC wedged the handshake; a fresh script and socket redo it.
    Rejoining { attempt: u32 },
    /// The session ran its course — `app_messages` in is the
    /// replication verdict.
    Ended {
        datagrams_tx: u64,
        app_messages: u64,
        app_bytes: u64,
    },
    /// The join never got there.
    Failed(String),
}

impl Session {
    fn new() -> Self {
        Session {
            inner: Arc::new(SessionInner {
                state: Mutex::new(SessionState {
                    phase: Phase::Negotiating { attempt: 1 },
                    log: VecDeque::new(),
                    version: 0,
                }),
                finished: AtomicBool::new(false),
            }),
        }
    }

    /// One log line, newest last, only the last `LOG_TAIL` kept.
    fn line(&self, text: &str) {
        self.edit(|state| {
            state.log.push_back(text.to_string());
            while state.log.len() > LOG_TAIL {
                state.log.pop_front();
            }
        });
    }

    fn set_phase(&self, phase: Phase) {
        self.edit(|state| state.phase = phase);
    }

    fn edit(&self, change: impl FnOnce(&mut SessionState)) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        change(&mut state);
        state.version += 1;
    }

    /// The thread is done: the poller stops once this and the last
    /// write are both seen.
    fn finished(&self) {
        self.inner.finished.store(true, Ordering::Release);
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.inner.finished.load(Ordering::Acquire)
    }

    pub(crate) fn version(&self) -> u64 {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .version
    }

    /// What the card paints this frame.
    pub(crate) fn snapshot(&self) -> (Phase, Vec<String>) {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (state.phase.clone(), state.log.iter().cloned().collect())
    }
}

/// Starts the session on its own thread and returns at once, with its
/// status handle: the editor window this join opens paints it as its
/// card, and stderr still gets every line.
pub(crate) fn start(place: u64, cookie: SessionCookie) -> Session {
    let session = Session::new();
    let status = session.clone();
    let _ = std::thread::Builder::new()
        .name("team-create-session".to_string())
        .spawn(move || {
            match run(place, &cookie, &status) {
                Ok(report) => status.set_phase(Phase::Ended {
                    datagrams_tx: report.datagrams_tx,
                    app_messages: report.app_messages,
                    app_bytes: report.app_bytes,
                }),
                Err(err) => {
                    status.line(&format!("join failed: {err}"));
                    status.set_phase(Phase::Failed(err));
                }
            }
            status.finished();
        });
    session
}

/// The join and the session: the whole `drive` sequence on one socket,
/// logged a handful of lines — and a second and third socket when the
/// RCC wedges the handshake (see `SESSION_ATTEMPTS`).
fn run(place: u64, cookie: &SessionCookie, status: &Session) -> Result<drive::Report, String> {
    let say = |line: String| {
        eprintln!("team-create: {line}");
        status.line(&line);
    };
    let mut reply = negotiate(place, cookie, status, &say)?;
    for attempt in 1..=SESSION_ATTEMPTS {
        if attempt > 1 {
            status.set_phase(Phase::Rejoining { attempt });
            // Reply2 is bound to the join ticket: replaying the previous
            // script gets Reply1 and then silence (measured in the
            // reference). A fresh one; the endpoint does not change.
            say(format!(
                "rejoining with a fresh session (attempt {attempt} of {SESSION_ATTEMPTS})"
            ));
            reply = negotiate(place, cookie, status, &say)?;
        }
        status.set_phase(Phase::Joined);
        let mut js = match rbx_transport::join_game::script_of(&reply) {
            Ok(js) => js,
            // A fleet version that serializes its `joinScript` as a JSON
            // string: the whole reply stands in, and the merge below decodes
            // the string's keys into it.
            Err(_) => reply.clone(),
        };
        carry_wrapper_fields(&mut js, &reply);
        hydrate(&mut js, cookie);
        let peer = peer_of(&reply)?;
        ensure_udmux(&mut js, peer);
        say(format!("joining {}:{}", std::net::Ipv4Addr::from(peer.0), peer.1));
        let mut io = UdpIo::connect(peer)?;
        let began = std::time::Instant::now();
        let now = || began.elapsed().as_millis() as u64;
        let live = status.clone();
        let outcome = drive::drive(
            DriveInputs {
                js: &js,
                reply: reply_for_flow(&reply),
                place,
                tmpl_90: TMPL_90,
                tmpl_8a: TMPL_8A,
                solver: rbx_challenge::solver(),
                duration_ms: SESSION_MS,
                key_seed: None,
            },
            &mut io,
            &now,
            move |line| {
                eprintln!("team-create: {line}");
                live.line(&line);
            },
        );
        match outcome {
            Ok(report) => return Ok(report),
            // A wedged RCC: re-sending inside the same session cannot
            // help — everything queues behind the same hole, measured.
            // Drop the connection and redo the handshake with a fresh
            // script.
            Err(err) if attempt < SESSION_ATTEMPTS && drive::is_handshake_miss(&err) => {
                say(format!("{err} — dropping the connection for a fresh session"));
                continue;
            }
            Err(err) => return Err(err),
        }
    }
    Err("the Team Create session never came up".to_string())
}

/// Asks the team-create endpoint until its answer carries an endpoint:
/// for up to ~10s after the first POST the answer is JSON whose keys
/// are null (a stray scalar can ride along) while the session's server
/// comes up, and the real config arrives only then — so keep asking
/// until the answer carries an actual endpoint, a fresh attempt id on
/// every call.
fn negotiate(
    place: u64,
    cookie: &SessionCookie,
    status: &Session,
    say: &impl Fn(String),
) -> Result<Value, String> {
    for attempt in 1..=NEGOTIATION_TRIES {
        status.set_phase(Phase::Negotiating { attempt });
        let answered = rbx_transport::join_game::team_create_join(
            place,
            cookie.expose_secret(),
            false,
        )?;
        if !rbx_transport::join_game::team_create_endpoints(&answered).is_empty() {
            return Ok(answered);
        }
        if rbx_transport::join_game::all_null(&answered) {
            say(format!("session warming up (attempt {attempt}), asking again"));
        } else {
            say(format!("no endpoint in the answer yet (attempt {attempt}), asking again"));
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    Err("the Team Create session never became ready".to_string())
}

/// The account's hydration fields, merged into the join script the way
/// the reference does it. A refused or unreachable fetch joins without
/// them: the template keeps its own values (the 0x90 merge only touches
/// what the script carries).
fn hydrate(js: &mut Value, cookie: &SessionCookie) {
    let fields = rbx_cloud::heartbeats::authenticated_user_id(cookie)
        .ok()
        .and_then(|id| {
            rbx_transport::join_game::fetch_player_hydration(id, cookie.expose_secret()).ok()
        })
        .map(|doc| rbx_transport::join_game::hydration_fields(&doc));
    if let Some(fields) = fields {
        if let (Some(script), Some(add)) = (js.as_object_mut(), fields.as_object()) {
            for (key, value) in add {
                script.insert(key.clone(), value.clone());
            }
        }
    }
}

/// The RakNet endpoint: the reply's endpoints (see
/// `team_create_endpoints` — UDMUX first, public preferred, every
/// wrapper and shape) — the first one that is IPv4, which is what the
/// socket dials.
fn peer_of(reply: &Value) -> Result<([u8; 4], u16), String> {
    for endpoint in rbx_transport::join_game::team_create_endpoints(reply) {
        if let Ok(ip) = endpoint.address.parse::<std::net::Ipv4Addr>() {
            return Ok((ip.octets(), endpoint.port));
        }
    }
    Err("the Team Create answer has no IPv4 endpoint".to_string())
}

/// Different gamejoin fleet versions put the config under `joinScript`,
/// `settings`, or a `joinTicket` serialized as a JSON string;
/// `script_of` takes the `joinScript` branch when there is one, and the
/// flow reads flat fields (`NetStackPort`, `ServerConnections`,
/// `ClientTicket`, …). This merges every key the reply's other levels
/// carry into `js` when `js` lacks it, so a field is found wherever the
/// fleet put it. Existing keys are never overwritten.
fn carry_wrapper_fields(js: &mut Value, reply: &Value) {
    let Some(target) = js.as_object_mut() else {
        return;
    };
    merge_level(reply, target, 0);
}

fn merge_level(v: &Value, target: &mut serde_json::Map<String, Value>, depth: usize) {
    if depth > 12 {
        return;
    }
    match v {
        Value::Object(map) => {
            for (key, value) in map {
                let missing = matches!(target.get(key), None | Some(Value::Null));
                if missing && !value.is_null() {
                    target.insert(key.clone(), value.clone());
                }
            }
            for value in map.values() {
                merge_level(value, target, depth + 1);
            }
        }
        Value::Array(items) => {
            for item in items {
                merge_level(item, target, depth + 1);
            }
        }
        Value::String(text) => {
            let text = text.trim();
            if (text.starts_with('{') || text.starts_with('[')) && text.len() <= 2_000_000 {
                if let Ok(decoded) = serde_json::from_str::<Value>(text) {
                    merge_level(&decoded, target, depth + 1);
                }
            }
        }
        _ => {}
    }
}

/// The flow's config reader takes `UdmuxEndpoints[0]`; a fleet version
/// that answered `Address`+`Port` (or `MachineAddress`+`ServerPort`)
/// instead gets the chosen endpoint written there.
fn ensure_udmux(js: &mut Value, peer: ([u8; 4], u16)) {
    let has = js
        .get("UdmuxEndpoints")
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty());
    if has {
        return;
    }
    let address = std::net::Ipv4Addr::from(peer.0).to_string();
    if let Some(map) = js.as_object_mut() {
        map.insert(
            "UdmuxEndpoints".to_string(),
            serde_json::json!([{"Address": address, "Port": peer.1}]),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_gets_the_story() {
        let session = Session::new();
        let (phase, log) = session.snapshot();
        assert!(matches!(phase, Phase::Negotiating { attempt: 1 }));
        assert!(log.is_empty());
        session.line("Reply1 guid=17 mtu=1492 (offline handshake accepted)");
        session.set_phase(Phase::Joined);
        let (phase, log) = session.snapshot();
        assert!(matches!(phase, Phase::Joined));
        assert_eq!(log.len(), 1);
        // A wedged RCC: the card says which rejoin is running.
        session.set_phase(Phase::Rejoining { attempt: 2 });
        let (phase, _) = session.snapshot();
        assert!(matches!(phase, Phase::Rejoining { attempt: 2 }));
        // A failure reaches both the log tail and the headline, and the
        // finished flag only follows the last write.
        session.line("join failed: endpoint answered 403");
        session.set_phase(Phase::Failed("endpoint answered 403".to_string()));
        assert!(!session.is_finished());
        session.finished();
        let (phase, log) = session.snapshot();
        assert!(matches!(phase, Phase::Failed(ref err) if err == "endpoint answered 403"));
        assert_eq!(log.last().map(String::as_str), Some("join failed: endpoint answered 403"));
        assert!(session.is_finished());
    }

    #[test]
    fn the_log_keeps_only_its_tail() {
        let session = Session::new();
        for n in 0..LOG_TAIL + 4 {
            session.line(&format!("line {n}"));
        }
        let (_, log) = session.snapshot();
        assert_eq!(log.len(), LOG_TAIL);
        assert_eq!(log.first().map(String::as_str), Some("line 4"));
    }

    #[test]
    fn the_flow_takes_the_reply_only_when_it_carries_a_ticket() {
        let with = serde_json::json!({
            "status": 2,
            "joinTicket": {"SerializedClientFields": "a", "EncryptedServerFields": "b"}
        });
        assert!(reply_for_flow(&with).get("joinTicket").is_some());
        // Without one, the flow takes Null: build_90 keeps the
        // template's ticket for a non-object reply.
        let without = serde_json::json!({"status": 2, "UdmuxEndpoints": []});
        assert!(reply_for_flow(&without).is_null());
    }

    #[test]
    fn the_peer_comes_from_the_first_endpoint() {
        let js: Value = serde_json::from_str(
            r#"{"UdmuxEndpoints":[{"Address":"10.190.0.5","Port":50105}]}"#,
        )
        .unwrap();
        assert_eq!(peer_of(&js), Ok(([10, 190, 0, 5], 50105)));
        // The port as a string, as scripts have been seen carrying it.
        let js: Value = serde_json::from_str(
            r#"{"UdmuxEndpoints":[{"Address":"10.190.0.5","Port":"50105"}]}"#,
        )
        .unwrap();
        assert_eq!(peer_of(&js), Ok(([10, 190, 0, 5], 50105)));
        assert!(peer_of(&serde_json::json!({})).is_err());
        assert!(peer_of(&serde_json::json!({"UdmuxEndpoints":[]})).is_err());
    }

    #[test]
    fn a_warming_answer_with_a_stray_scalar_still_has_no_endpoint() {
        // The ~10s warming answer is null keys — but not always only
        // nulls: a status or message scalar rides along, which is why
        // the gate is "carries an endpoint", not "not all-null".
        let warming = serde_json::json!({
            "status": 2,
            "message": "starting",
            "joinScript": null,
            "joinTicket": null
        });
        assert!(!rbx_transport::join_game::all_null(&warming));
        assert!(rbx_transport::join_game::team_create_endpoints(&warming).is_empty());
        assert!(peer_of(&warming).is_err());
    }

    #[test]
    fn the_flat_and_machine_shapes_join_too() {
        // A fleet version answers Address+Port with no UDMUX list…
        let flat = serde_json::json!({"Address": "52.84.150.39", "Port": 50105});
        assert_eq!(peer_of(&flat), Ok(([52, 84, 150, 39], 50105)));
        // …and one pairs MachineAddress with ServerPort, sometimes under
        // a joinScript wrapper, keys not always cased the same.
        let machine = serde_json::json!({
            "joinScript": {"machineaddress": "52.84.150.39", "serverport": "49153"}
        });
        assert_eq!(peer_of(&machine), Ok(([52, 84, 150, 39], 49153)));

        // The chosen endpoint is written where the flow's config reader
        // looks for it.
        let mut js = serde_json::json!({"Address": "52.84.150.39", "Port": 50105});
        ensure_udmux(&mut js, ([52, 84, 150, 39], 50105));
        assert_eq!(js["UdmuxEndpoints"][0]["Address"], "52.84.150.39");
        assert_eq!(js["UdmuxEndpoints"][0]["Port"], 50105);
        // An existing list is left alone.
        let mut js = serde_json::json!({
            "UdmuxEndpoints": [{"Address": "10.190.0.5", "Port": 50105}]
        });
        ensure_udmux(&mut js, ([52, 84, 150, 39], 50105));
        assert_eq!(js["UdmuxEndpoints"][0]["Address"], "10.190.0.5");
    }

    #[test]
    fn public_endpoints_win_over_the_rcc_private_network() {
        let both = serde_json::json!({
            "UdmuxEndpoints": [
                {"Address": "10.190.0.5", "Port": 50105},
                {"Address": "52.84.150.39", "Port": 50105}
            ]
        });
        assert_eq!(peer_of(&both), Ok(([52, 84, 150, 39], 50105)));
    }

    #[test]
    fn wrapper_fields_reach_the_flow_wherever_the_fleet_put_them() {
        // The v1 shape: status and the ticket at the reply level, the
        // dial fields under joinScript — the flow reads them all flat
        // off the script.
        let reply = serde_json::json!({
            "status": 2,
            "ClientTicket": "a;b;pre;auth",
            "joinScript": {
                "UdmuxEndpoints": [{"Address": "10.190.0.5", "Port": 50105}],
                "NetStackPort": 49153
            }
        });
        let mut js = rbx_transport::join_game::script_of(&reply).unwrap();
        carry_wrapper_fields(&mut js, &reply);
        assert_eq!(js["ClientTicket"], "a;b;pre;auth");
        assert_eq!(js["NetStackPort"], 49153);
        // A joinTicket serialized as a JSON string carries its settings
        // in — decoded, and without overwriting what js already has.
        let reply = serde_json::json!({
            "joinTicket": "{\"settings\":{\"Address\":\"52.84.150.39\",\"ServerPort\":5000}}",
            "joinScript": {"NetStackPort": 49153}
        });
        let mut js = rbx_transport::join_game::script_of(&reply).unwrap();
        carry_wrapper_fields(&mut js, &reply);
        assert_eq!(js["Address"], "52.84.150.39");
        assert_eq!(js["ServerPort"], 5000);
        assert_eq!(js["NetStackPort"], 49153);
    }

    #[test]
    fn hydration_merges_only_what_the_service_sent() {
        let mut js: Value =
            serde_json::from_str(r#"{"UserId":5,"AgeBracket":"old"}"#).unwrap();
        let doc: Value = serde_json::from_str(
            r#"{"playerInfo":{"ageBracket":"13-to-17","gender":2,"platform":"Android"}}"#,
        )
        .unwrap();
        let fields = rbx_transport::join_game::hydration_fields(&doc);
        if let (Some(script), Some(add)) = (js.as_object_mut(), fields.as_object()) {
            for (key, value) in add {
                script.insert(key.clone(), value.clone());
            }
        }
        assert_eq!(js.get("AgeBracket"), Some(&serde_json::json!("13-to-17")));
        assert_eq!(js.get("Gender"), Some(&serde_json::json!(2)));
        assert_eq!(js.get("Platform"), Some(&serde_json::json!("Android")));
        // What the service did not send stays as it was.
        assert_eq!(js.get("UserId"), Some(&serde_json::json!(5)));
    }

    /// The vendored captures are the real ones the transport was built
    /// against: the 0x90 splices around its `{"UserId"` JSON with this
    /// session's values, and the 0x8a is the ticket shape `build_8a`
    /// parses. A capture that stopped matching would fail here, in CI,
    /// before any device ever tried a join.
    #[test]
    fn the_captured_templates_still_build() {
        assert_eq!(TMPL_90[0], 0x90);
        assert_eq!(TMPL_8A[0], 0x8a);
        let js: Value =
            serde_json::from_str(r#"{"UserId":5,"CharacterAppearanceId":5}"#).unwrap();
        let reply: Value = serde_json::from_str(
            r#"{"joinTicket":{"SerializedClientFields":"a","EncryptedServerFields":"b"}}"#,
        )
        .unwrap();
        let (body, json) =
            rbx_transport::join_msg::build_90(TMPL_90, &js, &reply).expect("the capture splices");
        assert!(json.contains(r#""UserId":5"#), "{json}");
        assert!(json.contains(r#""CharacterAppearanceId":5"#), "{json}");
        // The prefix up to the LEB and the 20-byte tail are spliced
        // verbatim from the capture, never re-encoded.
        assert_eq!(&body[..3], &TMPL_90[..3]);
        assert_eq!(&body[body.len() - 20..], &TMPL_90[TMPL_90.len() - 20..]);
    }
}
