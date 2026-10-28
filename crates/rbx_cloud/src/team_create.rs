//! Team Create's switch for an experience and who is in a place's session
//! right now, from develop.roblox.com with the web sign-in (see `web`).

use std::collections::HashMap;

use serde::Deserialize;

use crate::error::CloudError;
use crate::heartbeats::SessionCookie;
use crate::web::{self, Method, DEVELOP};

/// Someone in a place's live Team Create session.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SessionMember {
    pub id: u64,
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[derive(Deserialize)]
struct Page<T> {
    data: Vec<T>,
    #[serde(rename = "nextPageCursor", default)]
    next_page_cursor: Option<String>,
}

#[derive(Deserialize)]
struct Switch {
    #[serde(rename = "isEnabled")]
    enabled: bool,
}

#[derive(Deserialize)]
struct UniverseSwitch {
    id: u64,
    #[serde(rename = "isEnabled")]
    enabled: bool,
}

/// Whether Team Create is on for a universe
/// (`GET /v1/universes/{id}/teamcreate`).
pub fn team_create_enabled(cookie: &SessionCookie, universe_id: u64) -> Result<bool, CloudError> {
    enabled_at(DEVELOP, cookie, universe_id)
}

/// Turns Team Create on or off for a universe the signed-in user can edit
/// (`PATCH /v1/universes/{id}/teamcreate {"isEnabled":…}`).
pub fn set_team_create_enabled(
    cookie: &SessionCookie,
    universe_id: u64,
    on: bool,
) -> Result<(), CloudError> {
    set_enabled_at(DEVELOP, cookie, universe_id, on)
}

/// Team Create on or off for several universes in one call
/// (`GET /v1/universes/multiget/teamcreate`).
pub fn team_create_enabled_many(
    cookie: &SessionCookie,
    universe_ids: &[u64],
) -> Result<HashMap<u64, bool>, CloudError> {
    enabled_many_at(DEVELOP, cookie, universe_ids)
}

/// Everyone in the place's live Team Create session, every page of them
/// (`GET /v1/places/{id}/teamcreate/active_session/members`).
pub fn team_create_members(
    cookie: &SessionCookie,
    place_id: u64,
) -> Result<Vec<SessionMember>, CloudError> {
    members_at(DEVELOP, cookie, place_id)
}

fn enabled_at(root: &str, cookie: &SessionCookie, universe_id: u64) -> Result<bool, CloudError> {
    let url = format!("{root}/v1/universes/{universe_id}/teamcreate");
    let body = web::send(cookie, Method::Get, &url, &[])?;
    Ok(serde_json::from_slice::<Switch>(&body)?.enabled)
}

fn set_enabled_at(
    root: &str,
    cookie: &SessionCookie,
    universe_id: u64,
    on: bool,
) -> Result<(), CloudError> {
    let url = format!("{root}/v1/universes/{universe_id}/teamcreate");
    let body = serde_json::json!({ "isEnabled": on });
    web::send(cookie, Method::Patch(&body), &url, &[]).map(|_| ())
}

fn enabled_many_at(
    root: &str,
    cookie: &SessionCookie,
    universe_ids: &[u64],
) -> Result<HashMap<u64, bool>, CloudError> {
    if universe_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let ids: Vec<String> = universe_ids.iter().map(u64::to_string).collect();
    let query: Vec<(&str, &str)> = ids.iter().map(|id| ("ids", id.as_str())).collect();
    let url = format!("{root}/v1/universes/multiget/teamcreate");
    let body = web::send(cookie, Method::Get, &url, &query)?;
    let page: Page<UniverseSwitch> = serde_json::from_slice(&body)?;
    Ok(page.data.into_iter().map(|u| (u.id, u.enabled)).collect())
}

fn members_at(
    root: &str,
    cookie: &SessionCookie,
    place_id: u64,
) -> Result<Vec<SessionMember>, CloudError> {
    let url = format!("{root}/v1/places/{place_id}/teamcreate/active_session/members");
    let mut members = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut query = vec![("limit", "100")];
        if let Some(cursor) = &cursor {
            query.push(("cursor", cursor.as_str()));
        }
        let body = web::send(cookie, Method::Get, &url, &query)?;
        let page: Page<SessionMember> = serde_json::from_slice(&body)?;
        members.extend(page.data);
        match page.next_page_cursor.filter(|c| !c.is_empty()) {
            Some(next) => cursor = Some(next),
            None => return Ok(members),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::serve::serve;

    fn cookie() -> SessionCookie {
        SessionCookie::new("test-cookie")
    }

    #[test]
    fn the_switch_is_read_with_the_cookie() {
        let (root, served) = serve(vec![(200, "", r#"{"isEnabled":true}"#)]);
        assert!(enabled_at(&root, &cookie(), 42).unwrap());
        let request = &served.join().unwrap()[0];
        assert!(
            request.starts_with("GET /v1/universes/42/teamcreate "),
            "{request}"
        );
        assert!(request
            .to_lowercase()
            .contains("cookie: .roblosecurity=test-cookie"));
    }

    #[test]
    fn turning_it_on_patches_after_the_csrf_retry() {
        let (root, served) = serve(vec![
            (
                403,
                "x-csrf-token: fresh\r\n",
                r#"{"errors":[{"code":0,"message":"Token Validation Failed"}]}"#,
            ),
            (200, "", "{}"),
        ]);
        set_enabled_at(&root, &cookie(), 42, true).unwrap();
        let requests = served.join().unwrap();
        for request in &requests {
            assert!(
                request.starts_with("PATCH /v1/universes/42/teamcreate "),
                "{request}"
            );
            let body: serde_json::Value =
                serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert_eq!(body, serde_json::json!({ "isEnabled": true }));
        }
        assert!(!requests[0].to_lowercase().contains("x-csrf-token"));
        assert!(requests[1].to_lowercase().contains("x-csrf-token: fresh"));
    }

    #[test]
    fn a_refusal_carries_roblox_s_own_message() {
        let (root, served) = serve(vec![(
            401,
            "",
            r#"{"errors":[{"code":0,"message":"Authorization has been denied for this request."}]}"#,
        )]);
        let err = enabled_at(&root, &cookie(), 42).unwrap_err();
        assert_eq!(served.join().unwrap().len(), 1);
        assert!(
            matches!(&err, CloudError::Refused { status: 401, message } if message == "Authorization has been denied for this request."),
            "{err:?}"
        );
    }

    #[test]
    fn the_batch_call_sends_every_id_once() {
        let (root, served) = serve(vec![(
            200,
            "",
            r#"{"data":[{"id":1,"isEnabled":true},{"id":2,"isEnabled":false}]}"#,
        )]);
        let on = enabled_many_at(&root, &cookie(), &[1, 2]).unwrap();
        assert_eq!(on, HashMap::from([(1, true), (2, false)]));
        let request = &served.join().unwrap()[0];
        assert!(
            request.starts_with("GET /v1/universes/multiget/teamcreate?ids=1&ids=2 "),
            "{request}"
        );
        assert!(enabled_many_at(&root, &cookie(), &[]).unwrap().is_empty());
    }

    #[test]
    fn members_follow_the_cursor_to_the_last_page() {
        let (root, served) = serve(vec![
            (
                200,
                "",
                r#"{"data":[{"id":1,"name":"builderman","displayName":"Builder"}],"nextPageCursor":"a+b/c="}"#,
            ),
            (
                200,
                "",
                r#"{"data":[{"id":2,"name":"mara","displayName":"Mara"}],"nextPageCursor":null}"#,
            ),
        ]);
        let members = members_at(&root, &cookie(), 7).unwrap();
        assert_eq!(
            members.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            ["builderman", "mara"]
        );
        let requests = served.join().unwrap();
        let path = "GET /v1/places/7/teamcreate/active_session/members?limit=100";
        assert!(
            requests[0].starts_with(&format!("{path} ")),
            "{}",
            requests[0]
        );
        assert!(
            requests[1].starts_with(&format!("{path}&cursor=a%2Bb%2Fc%3D ")),
            "{}",
            requests[1]
        );
    }

    /// Roblox's real refusals for a sign-in it doesn't know: `401` on the
    /// reads, and the write still passes the CSRF retry before refusing.
    #[test]
    #[ignore]
    fn live_refusals_for_a_stale_sign_in() {
        let stale = SessionCookie::new("stale");
        let (universe, place) = (10771044762, 104688317714596);
        let reads = [
            team_create_enabled(&stale, universe).map(|_| ()),
            team_create_enabled_many(&stale, &[universe]).map(|_| ()),
            team_create_members(&stale, place).map(|_| ()),
            set_team_create_enabled(&stale, universe, true),
        ];
        for read in reads {
            let err = read.unwrap_err();
            assert!(
                matches!(&err, CloudError::Refused { status: 401, message } if message == "User is not authenticated"),
                "{err:?}"
            );
        }
    }
}
