//! The live Team Create session, from Home's Join button: the v1
//! join-game POST with the saved cookie, then the RakNet session on a
//! UDP socket, on one background thread that lives as long as the app
//! does. The `0x90`/`0x8a` bodies are the captured native templates
//! (`assets/team-create`, the reference's own captures), spliced with
//! this session's values by `rbx_transport`; the `0x9B` challenge is
//! solved in-process by `rbx_challenge`.
//!
//! The place itself opens as a new local copy through Home's ordinary
//! download flow — the session rides beside the editor, its handful of
//! log lines on stderr (logcat) for now.

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

/// Starts the session on its own thread and returns at once; the join,
/// and whatever it has to say, belongs to the thread.
pub(crate) fn start(place: u64, universe: Option<u64>, cookie: SessionCookie) {
    let _ = std::thread::Builder::new()
        .name("team-create-session".to_string())
        .spawn(move || {
            let say = |line: String| eprintln!("team-create: {line}");
            match run(place, universe, &cookie) {
                Ok(report) => say(format!(
                    "session ended: {} datagrams out, {} app messages in ({} bytes)",
                    report.datagrams_tx, report.app_messages, report.app_bytes
                )),
                Err(err) => say(format!("join failed: {err}")),
            }
        });
}

/// The join and the session: the whole `drive` sequence on one socket,
/// logged a handful of lines.
fn run(place: u64, universe: Option<u64>, cookie: &SessionCookie) -> Result<drive::Report, String> {
    let joined = rbx_transport::join_game::join_game(
        place,
        cookie.expose_secret(),
        None,
        None,
        universe,
    )?;
    let mut js = joined.join_script;
    hydrate(&mut js, cookie);
    let peer = peer_of(&js)?;
    let mut io = UdpIo::connect(peer)?;
    let began = std::time::Instant::now();
    let now = || began.elapsed().as_millis() as u64;
    drive::drive(
        DriveInputs {
            js: &js,
            reply: &joined.reply,
            place: u32::try_from(place).map_err(|_| "place id too large".to_string())?,
            tmpl_90: TMPL_90,
            tmpl_8a: TMPL_8A,
            solver: rbx_challenge::solver(),
            duration_ms: SESSION_MS,
            key_seed: None,
        },
        &mut io,
        &now,
        |line| eprintln!("team-create: {line}"),
    )
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

/// The RakNet endpoint: the join script's `UdmuxEndpoints[0]`, address
/// and port (the port arrives as a number or a string).
fn peer_of(js: &Value) -> Result<(([u8; 4], u16), String)> {
    let end = js
        .get("UdmuxEndpoints")
        .and_then(|v| v.get(0))
        .ok_or("join script has no UdmuxEndpoints[0]")?;
    let ip = end
        .get("Address")
        .and_then(Value::as_str)
        .and_then(|a| a.parse::<std::net::Ipv4Addr>().ok())
        .ok_or("UdmuxEndpoints[0] has no IPv4 Address")?;
    let port = end
        .get("Port")
        .and_then(|p| match p {
            Value::Number(n) => n.as_u64(),
            Value::String(s) => s.parse::<u64>().ok(),
            _ => None,
        })
        .and_then(|p| u16::try_from(p).ok())
        .ok_or("UdmuxEndpoints[0] has no Port")?;
    Ok((ip.octets(), port))
}

#[cfg(test)]
mod tests {
    use super::*;

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
