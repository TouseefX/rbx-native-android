//! The join-game HTTP layer, ported from the reference's
//! `client/src/join/join.cpp` (Boblox): the POST that trades a
//! `.ROBLOSECURITY` cookie for a join script, the universe lookup that
//! names the game in that POST, and the player-hydration fetch whose
//! fields the 0x90 body must carry (a captured template holds another
//! account's).
//!
//! Wire facts, all **[GAME]** — the reference's own live measurements:
//!
//! * `v2/join-game` was retired server-side (HTTP 404 even with a valid
//!   cookie, verified live 2026-10-26/27); `v1` is the only path;
//! * the server answers `status == 2` with either a `joinScript` object
//!   or a `joinScriptUrl` to GET, and anything else is a refusal;
//! * the player-hydration service answers 415 unless the client asks
//!   for JSON explicitly.

use rand_core::{OsRng, RngCore};
use serde_json::{json, Map, Value};
use std::time::Duration;

/// One join-game answer: the whole reply (whose fields the 0x90 builder
/// reads) and its `joinScript`.
pub struct JoinReply {
    pub reply: Value,
    pub join_script: Value,
}

/// The cookie as a `Cookie:` header value: a bare cookie gets the
/// `.ROBLOSECURITY=` name, a string that already has one keeps whatever
/// it is.
pub fn cookie_header(cookie: &str) -> String {
    if cookie.is_empty() {
        return String::new();
    }
    let first = cookie.split(';').next().unwrap_or(cookie);
    if first.contains('=') {
        cookie.to_string()
    } else {
        format!(".ROBLOSECURITY={cookie}")
    }
}

/// A v4 UUID, the three random draws a join POST needs.
pub fn uuid4() -> String {
    let mut b = [0u8; 16];
    OsRng.fill_bytes(&mut b);
    b[6] = (b[6] & 0x0F) | 0x40;
    b[8] = (b[8] & 0x3F) | 0x80;
    let hex: String = b.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// The join-game POST body, at the field order the reference keeps
/// (`nlohmann::ordered_json`); the optional ids only go in when there is
/// one.
pub fn join_body(
    place: u64,
    job: Option<&str>,
    follow: Option<u64>,
    universe: Option<u64>,
    attempt: &str,
    session: &str,
    event: &str,
) -> Value {
    let mut body = Map::new();
    body.insert("placeId".into(), json!(place));
    body.insert("isTeleport".into(), json!(false));
    body.insert("gameJoinAttemptId".into(), json!(attempt));
    body.insert("browserTrackerId".into(), json!(0));
    body.insert("playSessionId".into(), json!(session));
    body.insert("eventId".into(), json!(event));
    body.insert("launchData".into(), json!(""));
    body.insert("joinAttemptOrigin".into(), json!("PlayButton"));
    if let Some(job) = job {
        if !job.is_empty() {
            body.insert("jobId".into(), json!(job));
        }
    }
    if let Some(follow) = follow {
        if follow > 0 {
            body.insert("followUserId".into(), json!(follow));
        }
    }
    if let Some(universe) = universe {
        if universe > 0 {
            body.insert("gameId".into(), json!(universe));
        }
    }
    Value::Object(body)
}

/// What a join-game reply says, before any second request.
pub enum Outcome {
    /// `status == 2` with a `joinScript` object: the answer itself.
    Script(Value),
    /// `status == 2` with a `joinScriptUrl`: one more GET.
    Url(String),
    /// Anything else, with the status the server saw.
    Failed(String),
}

/// The reply's classification, the reference's own: a `joinScript` that
/// is not an object is not an answer.
pub fn classify(reply: &Value) -> Outcome {
    let status = reply.get("status").and_then(Value::as_i64).unwrap_or(-1);
    if status == 2 {
        if let Some(script) = reply.get("joinScript") {
            if script.is_object() {
                return Outcome::Script(script.clone());
            }
        }
        if let Some(url) = reply.get("joinScriptUrl").and_then(Value::as_str) {
            return Outcome::Url(url.to_string());
        }
    }
    let st = reply
        .get("status")
        .map(|v| v.to_string())
        .unwrap_or_else(|| "?".to_string());
    let message = reply
        .get("message")
        .map(|v| v.to_string())
        .unwrap_or_else(|| "?".to_string());
    Outcome::Failed(format!("join failed: status={st} message={message}"))
}

/// The `joinScriptUrl` fallback's half that needs no network: the
/// document is the script itself, or carries it under `joinScript`.
pub fn script_of(doc: &Value) -> Result<Value, String> {
    let script = match doc.get("joinScript") {
        Some(script) => script.clone(),
        None => doc.clone(),
    };
    if script.is_object() {
        Ok(script)
    } else {
        Err("joinScriptUrl: script is not an object".to_string())
    }
}

/// The reference's transport: 20s timeouts, `Roblox/WinInet`, and HTTP
/// statuses reported rather than thrown (a refusal is data).
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent("Roblox/WinInet")
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into()
}

/// Team Create's own negotiation, not `join-game`: a `POST` to
/// `gamejoin.roblox.com/v1/team-create` (or `/team-create-preemptive`,
/// which warms the session's server without joining) with just the place
/// and a fresh attempt id, the cookie, and the CSRF retry a gamejoin
/// POST needs. The answer is the raw document — **all nulls while the
/// session's server is still coming up** (see [`all_null`]); re-asking
/// with another fresh attempt id is the caller's half of the contract.
pub fn team_create_join(place: u64, cookie: &str, preemptive: bool) -> Result<Value, String> {
    let url = if preemptive {
        "https://gamejoin.roblox.com/v1/team-create-preemptive"
    } else {
        "https://gamejoin.roblox.com/v1/team-create"
    };
    let body = json!({
        "placeId": place,
        "gameJoinAttemptId": uuid4(),
    });
    let bytes = serde_json::to_vec(&body).map_err(|err| format!("team-create: {err}"))?;
    let cookie = cookie_header(cookie);
    let send = |token: Option<&str>| -> Result<(u16, Option<String>, String), String> {
        let mut request = agent()
            .post(url)
            .content_type("application/json")
            .header("Accept", "application/json");
        if !cookie.is_empty() {
            request = request.header("Cookie", cookie.as_str());
        }
        if let Some(token) = token {
            request = request.header("X-CSRF-TOKEN", token);
        }
        let mut response = request.send(&bytes).map_err(|err| format!("team-create: {err}"))?;
        let token = response
            .headers()
            .get("x-csrf-token")
            .and_then(|t| t.to_str().ok())
            .map(str::to_string);
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().unwrap_or_default();
        Ok((status, token, text))
    };
    // A gamejoin POST with a cookie answers 403 carrying the CSRF token
    // to use; one retry with it is the handshake.
    let mut answered = send(None)?;
    if (400..600).contains(&answered.0) {
        if let Some(token) = answered.1.clone() {
            answered = send(Some(&token))?;
        }
    }
    let (status, _, text) = answered;
    if !(200..300).contains(&status) {
        let snippet: String = text.chars().take(140).collect();
        return Err(format!("team-create: HTTP {status}: {snippet}"));
    }
    serde_json::from_str(&text).map_err(|_| "team-create: bad json".to_string())
}

/// True when the document holds nothing but nulls and empty containers:
/// the team-create endpoint's "not ready yet" answer, which a 200
/// carries while the session's server is still coming up.
pub fn all_null(doc: &Value) -> bool {
    !has_value(doc)
}

fn has_value(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Array(items) => items.iter().any(has_value),
        Value::Object(map) => map.values().any(has_value),
        Value::String(s) => !s.trim().is_empty(),
        Value::Bool(_) | Value::Number(_) => true,
    }
}

/// POSTs the join-game request and follows the `joinScriptUrl` fallback,
/// the reference's `joinGame` verbatim.
pub fn join_game(
    place: u64,
    cookie: &str,
    job: Option<&str>,
    follow: Option<u64>,
    universe: Option<u64>,
) -> Result<JoinReply, String> {    let agent = agent();
    let body = join_body(place, job, follow, universe, &uuid4(), &uuid4(), &uuid4());
    let bytes = serde_json::to_vec(&body).map_err(|err| format!("v1: {err}"))?;
    let cookie = cookie_header(cookie);
    let mut request = agent
        .post("https://gamejoin.roblox.com/v1/join-game")
        .content_type("application/json; charset=utf-8");
    if !cookie.is_empty() {
        request = request.header("Cookie", cookie.as_str());
    }
    let mut response = request.send(&bytes).map_err(|err| format!("v1: {err}"))?;
    let text = response
        .body_mut()
        .read_to_vec()
        .map_err(|err| format!("v1: {err}"))?;
    let reply: Value = serde_json::from_slice(&text).map_err(|_| "v1: bad json".to_string())?;
    match classify(&reply) {
        Outcome::Script(join_script) => Ok(JoinReply { reply, join_script }),
        Outcome::Url(url) => {
            let mut request = agent.get(url.as_str());
            if !cookie.is_empty() {
                request = request.header("Cookie", cookie.as_str());
            }
            let mut got = request
                .call()
                .map_err(|err| format!("joinScriptUrl: {err}"))?;
            let text = got
                .body_mut()
                .read_to_vec()
                .map_err(|err| format!("joinScriptUrl: {err}"))?;
            let doc: Value =
                serde_json::from_slice(&text).map_err(|_| "joinScriptUrl: bad json".to_string())?;
            Ok(JoinReply {
                reply,
                join_script: script_of(&doc)?,
            })
        }
        Outcome::Failed(message) => Err(message),
    }
}

/// The universe (game) a place belongs to, for the POST's `gameId`.
pub fn resolve_universe(place: u64) -> Result<u64, String> {
    let url = format!("https://apis.roblox.com/universes/v1/places/{place}/universe");
    let mut response = agent()
        .get(url.as_str())
        .call()
        .map_err(|err| format!("universe lookup: {err}"))?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_vec()
        .map_err(|err| format!("universe lookup: {err}"))?;
    let bad = || {
        format!(
            "universe lookup: HTTP {status} {}",
            String::from_utf8_lossy(&text)
        )
    };
    let doc: Value = serde_json::from_slice(&text).map_err(|_| bad())?;
    doc.get("universeId")
        .and_then(Value::as_u64)
        .ok_or_else(bad)
}

/// The hydration fields the 0x90 body carries, as the join script's own
/// keys (the reference merges them into it): only the ones the service
/// actually sent, so a missing one keeps the template's value rather
/// than blanking it. The timestamp arrives as a string of milliseconds
/// and is carried as a number.
pub fn hydration_fields(doc: &Value) -> Value {
    let player = doc.get("playerInfo").unwrap_or(&Value::Null);
    const KMAP: [(&str, &str); 5] = [
        ("ageBracket", "AgeBracket"),
        ("gender", "Gender"),
        ("platform", "Platform"),
        ("os", "Os"),
        ("isOriginalUser", "IsOriginalUser"),
    ];
    let mut out = Map::new();
    for (from, to) in KMAP {
        if let Some(v) = player.get(from) {
            out.insert(to.to_string(), v.clone());
        }
    }
    if let Some(v) = player.get("originalAccountCreationTimestampMs") {
        let as_number = v
            .as_u64()
            .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()));
        if let Some(n) = as_number {
            out.insert("OriginalAccountCreationTimestampMs".to_string(), json!(n));
        }
    }
    Value::Object(out)
}

/// Fetches the account's hydration fields (age bracket, gender,
/// platform, os, original-user flags), the reference's
/// `fetchPlayerHydration` verbatim: this service answers 415 unless the
/// client asks for JSON explicitly.
pub fn fetch_player_hydration(user_id: u64, cookie: &str) -> Result<Value, String> {
    let url = format!(
        "https://apis.roblox.com/player-hydration-service/v1/players/signed?userId={user_id}"
    );
    let cookie = cookie_header(cookie);
    let mut request = agent()
        .get(url.as_str())
        .header("Accept", "application/json");
    if !cookie.is_empty() {
        request = request.header("Cookie", cookie.as_str());
    }
    let mut response = request
        .call()
        .map_err(|err| format!("player hydration: {err}"))?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_vec()
        .map_err(|err| format!("player hydration: {err}"))?;
    let bad = || hydration_error(status, &text);
    let doc: Value = serde_json::from_slice(&text).map_err(|_| bad())?;
    if !doc.get("playerInfo").is_some_and(Value::is_object) {
        return Err(bad());
    }
    Ok(hydration_fields(&doc))
}

fn hydration_error(status: u16, text: &[u8]) -> String {
    let body = String::from_utf8_lossy(text);
    let cut = body.char_indices().nth(200).map_or(body.len(), |(i, _)| i);
    format!("player hydration: HTTP {status} {}", &body[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_cookie_gets_the_name_a_header_keeps_its_own() {
        assert_eq!(cookie_header(""), "");
        assert_eq!(
            cookie_header("_|WARNING:-DO-NOT-SHARE"),
            ".ROBLOSECURITY=_|WARNING:-DO-NOT-SHARE"
        );
        assert_eq!(
            cookie_header(".ROBLOSECURITY=x; other=1"),
            ".ROBLOSECURITY=x; other=1"
        );
        assert_eq!(cookie_header("a=b"), "a=b");
    }

    #[test]
    fn all_null_spots_the_warming_answer() {
        // The team-create endpoint's "server still coming up": a 200
        // whose every key is null, an empty container, or blank text.
        assert!(all_null(&json!({})));
        assert!(all_null(&json!({"status": null, "joinScript": null})));
        assert!(all_null(&json!({"a": [], "b": {}, "c": "", "d": null})));
        assert!(all_null(&json!([null, {}, []])));
        // Any real value anywhere — nested, in a list, a bool — is ready.
        assert!(!all_null(&json!({"status": 2})));
        assert!(!all_null(&json!({"joinScript": {"isTeleport": false}})));
        assert!(!all_null(&json!({"a": [null, {"b": "10.0.0.1"}]})));
        assert!(!all_null(&json!("x")));
    }

    #[test]
    fn uuid4_is_version_4_variant_10_and_unique() {
        let a = uuid4();
        assert_eq!(a.len(), 36);
        let dashes: Vec<usize> = a.match_indices('-').map(|(i, _)| i).collect();
        assert_eq!(dashes, vec![8, 13, 18, 23]);
        assert_eq!(&a[14..15], "4");
        assert!(matches!(&a[19..20], "8" | "9" | "a" | "b"));
        assert_ne!(a, uuid4());
    }

    #[test]
    fn the_body_keeps_the_reference_fields_and_order() {
        let body = join_body(1818, Some("job-1"), None, Some(777), "a", "s", "e");
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            "{\"placeId\":1818,\"isTeleport\":false,\"gameJoinAttemptId\":\"a\",\
             \"browserTrackerId\":0,\"playSessionId\":\"s\",\"eventId\":\"e\",\
             \"launchData\":\"\",\"joinAttemptOrigin\":\"PlayButton\",\"jobId\":\"job-1\",\
             \"gameId\":777}"
        );
        let text =
            serde_json::to_string(&join_body(1818, None, None, None, "a", "s", "e")).unwrap();
        assert!(!text.contains("jobId"));
        assert!(!text.contains("followUserId"));
        assert!(!text.contains("gameId"));
    }

    #[test]
    fn the_reply_classification() {
        assert!(matches!(
            classify(&json!({"status": 2, "joinScript": {"a": 1}})),
            Outcome::Script(s) if s == json!({"a": 1})
        ));
        assert!(matches!(
            classify(&json!({"status": 2, "joinScriptUrl": "https://x"})),
            Outcome::Url(u) if u == "https://x"
        ));
        // A joinScript that is not an object is not an answer.
        assert!(matches!(
            classify(&json!({"status": 2, "joinScript": 3})),
            Outcome::Failed(_)
        ));
        assert!(matches!(
            classify(&json!({"status": 1, "message": "nope"})),
            Outcome::Failed(m) if m.contains("nope")
        ));
    }

    #[test]
    fn the_fallback_document_is_the_script_or_carries_it() {
        assert_eq!(script_of(&json!({"a": 1})).unwrap(), json!({"a": 1}));
        assert_eq!(
            script_of(&json!({"joinScript": {"a": 1}})).unwrap(),
            json!({"a": 1})
        );
        assert!(script_of(&json!([1])).is_err());
    }

    #[test]
    fn hydration_maps_the_service_fields() {
        let doc = json!({"playerInfo": {
            "ageBracket": 3, "gender": 2, "platform": 0, "os": "Windows",
            "isOriginalUser": true,
            "originalAccountCreationTimestampMs": "1234567890",
            "somethingElse": "ignored"
        }});
        assert_eq!(
            hydration_fields(&doc),
            json!({
                "AgeBracket": 3, "Gender": 2, "Platform": 0, "Os": "Windows",
                "IsOriginalUser": true,
                "OriginalAccountCreationTimestampMs": 1234567890
            })
        );
        // A missing field stays absent (the template's value survives
        // upstream); a number passes through; garbage is not a field.
        assert_eq!(
            hydration_fields(&json!({"playerInfo": {
                "gender": 1, "originalAccountCreationTimestampMs": 99
            }})),
            json!({"Gender": 1, "OriginalAccountCreationTimestampMs": 99})
        );
        assert_eq!(
            hydration_fields(&json!({"playerInfo": {
                "originalAccountCreationTimestampMs": "not a number"
            }})),
            json!({})
        );
        assert_eq!(hydration_fields(&json!({})), json!({}));
    }
}
