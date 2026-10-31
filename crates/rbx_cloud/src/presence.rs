//! Availability right now, from presence.roblox.com with the web
//! sign-in (see `web`): the same answers the website's friend list
//! shows. One POST covers a whole roster, but it only says that
//! someone is on Roblox — not that they are in this place's session,
//! which the team-create member list remains the authority on.

use serde::Deserialize;

use crate::error::CloudError;
use crate::heartbeats::SessionCookie;
use crate::web::{self, Method};

/// The presence kinds the API numbers, as constants rather than an
/// enum: the wire is a bare number and every reader here only asks
/// "does Roblox have them online?".
pub const OFFLINE: u8 = 0;
pub const ONLINE: u8 = 1;
pub const IN_GAME: u8 = 2;
pub const IN_STUDIO: u8 = 3;
/// Chosen invisibility: they are on Roblox but appear offline.
pub const INVISIBLE: u8 = 4;

#[derive(Deserialize)]
struct Presences {
    #[serde(rename = "userPresences")]
    list: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "userId")]
    id: u64,
    #[serde(rename = "userPresenceType")]
    kind: u8,
}

/// Each named user's availability right now (`POST /v1/presence/users`)
/// as (id, kind). The API takes at most fifty ids a call — the roster
/// this feeds is far smaller, so more are refused here rather than
/// half-answered by Roblox.
pub fn user_presence(
    cookie: &SessionCookie,
    user_ids: &[u64],
) -> Result<Vec<(u64, u8)>, CloudError> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    if user_ids.len() > 50 {
        return Err(CloudError::UnexpectedShape(format!(
            "{} user ids at once; the presence API takes 50",
            user_ids.len()
        )));
    }
    let body = serde_json::json!({ "userIds": user_ids });
    let url = "https://presence.roblox.com/v1/presence/users";
    let body = web::send(cookie, Method::Post(&body), url, &[])?;
    Ok(serde_json::from_slice::<Presences>(&body)?
        .list
        .into_iter()
        .map(|entry| (entry.id, entry.kind))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The API's own documented answer, one entry per user asked for:
    /// the presence kind first, everything else ignored.
    #[test]
    fn the_answer_parses_into_id_and_kind() {
        let body = br#"{"userPresences":[
            {"userPresenceType":3,"userId":925308243,"lastLocation":"Studio",
             "placeId":null,"rootPlaceId":null,"jobId":null,
             "lastOnline":"2026-10-31T20:00:00.000Z","universeId":null},
            {"userPresenceType":0,"userId":156,"lastLocation":"",
             "lastOnline":"2026-10-01T00:00:00.000Z"}
        ]}"#;
        let parsed = serde_json::from_slice::<Presences>(body).unwrap();
        let got: Vec<(u64, u8)> = parsed
            .list
            .into_iter()
            .map(|entry| (entry.id, entry.kind))
            .collect();
        assert_eq!(got, vec![(925_308_243, IN_STUDIO), (156, OFFLINE)]);
    }
}
