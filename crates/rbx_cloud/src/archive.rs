//! Archiving and restoring an experience, Home's Recent menu entries, and
//! which experiences are archived.

use std::collections::HashMap;

use crate::error::CloudError;
use crate::heartbeats::SessionCookie;
use crate::web::{self, Method, DEVELOP};

/// Ids `multiget` takes at once; 101 is refused.
const MULTIGET: usize = 100;

/// Archives (`archived`) or restores an experience the way Studio's start
/// page does: `PATCH develop.roblox.com/v1/universes/{id}/configuration
/// {"isArchived":…}`. Open Cloud has no universe archive, so this takes the
/// web sign-in's cookie (see `web`).
pub fn set_experience_archived(
    cookie: &SessionCookie,
    universe_id: u64,
    archived: bool,
) -> Result<(), CloudError> {
    archive_at(DEVELOP, cookie, universe_id, archived)
}

/// Whether each of `universe_ids` is archived, from the public `GET
/// develop.roblox.com/v1/universes/multiget`; no sign-in needed. Ids
/// Roblox doesn't know are left out.
pub fn archived_experiences(universe_ids: &[u64]) -> Result<HashMap<u64, bool>, CloudError> {
    archived_at(DEVELOP, universe_ids)
}

fn archived_at(root: &str, universe_ids: &[u64]) -> Result<HashMap<u64, bool>, CloudError> {
    #[derive(serde::Deserialize)]
    struct Universe {
        id: u64,
        #[serde(rename = "isArchived")]
        archived: bool,
    }
    #[derive(serde::Deserialize)]
    struct Page {
        data: Vec<Universe>,
    }
    let agent = web::agent();
    let mut archived = HashMap::new();
    for chunk in universe_ids.chunks(MULTIGET) {
        let ids: Vec<String> = chunk.iter().map(|id| format!("ids={id}")).collect();
        let url = format!("{root}/v1/universes/multiget?{}", ids.join("&"));
        let mut response = agent
            .get(&url)
            .call()
            .map_err(|err| CloudError::Transport(err.to_string()))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(CloudError::Refused {
                status,
                message: format!("HTTP {status} from {url}"),
            });
        }
        let page: Page = response
            .body_mut()
            .read_json()
            .map_err(|err| CloudError::Transport(err.to_string()))?;
        archived.extend(page.data.into_iter().map(|u| (u.id, u.archived)));
    }
    Ok(archived)
}

fn archive_at(
    root: &str,
    cookie: &SessionCookie,
    universe_id: u64,
    archived: bool,
) -> Result<(), CloudError> {
    let url = format!("{root}/v1/universes/{universe_id}/configuration");
    let body = serde_json::json!({ "isArchived": archived });
    web::send(cookie, Method::Patch(&body), &url, &[]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::serve::serve;

    fn cookie() -> SessionCookie {
        SessionCookie::new("test-cookie")
    }

    #[test]
    fn archiving_patches_the_universe_with_the_cookie_after_the_csrf_retry() {
        let (root, served) = serve(vec![
            (
                403,
                "x-csrf-token: fresh\r\n",
                r#"{"errors":[{"code":0,"message":"Token Validation Failed"}]}"#,
            ),
            (200, "", "{}"),
        ]);
        archive_at(&root, &cookie(), 42, true).unwrap();
        let requests = served.join().unwrap();
        for request in &requests {
            assert!(
                request.starts_with("PATCH /v1/universes/42/configuration "),
                "{request}"
            );
            let lower = request.to_lowercase();
            assert!(
                lower.contains("cookie: .roblosecurity=test-cookie"),
                "{request}"
            );
            assert!(!lower.contains("x-api-key"), "{request}");
            let body = request.split_once("\r\n\r\n").unwrap().1;
            let body: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(body, serde_json::json!({ "isArchived": true }));
        }
        assert!(!requests[0].to_lowercase().contains("x-csrf-token"));
        assert!(requests[1].to_lowercase().contains("x-csrf-token: fresh"));
    }

    #[test]
    fn a_refused_archive_says_why() {
        let (root, served) = serve(vec![(
            403,
            "",
            r#"{"errors":[{"code":1,"message":"You are not authorized to configure this universe."}]}"#,
        )]);
        let err = archive_at(&root, &cookie(), 42, true).unwrap_err();
        assert_eq!(served.join().unwrap().len(), 1);
        assert!(
            err.to_string().contains("not authorized to configure"),
            "{err}"
        );
    }

    #[test]
    fn restoring_sends_is_archived_false() {
        let (root, served) = serve(vec![(200, "", "{}")]);
        archive_at(&root, &cookie(), 42, false).unwrap();
        let request = &served.join().unwrap()[0];
        let body = request.split_once("\r\n\r\n").unwrap().1;
        let body: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(body, serde_json::json!({ "isArchived": false }));
    }

    #[test]
    fn archived_states_come_from_one_multiget_per_hundred_ids_without_a_cookie() {
        let (root, served) = serve(vec![
            (
                200,
                "",
                r#"{"data":[{"id":1,"name":"a","isArchived":true},{"id":2,"name":"b","isArchived":false}]}"#,
            ),
            (200, "", r#"{"data":[]}"#),
        ]);
        let ids: Vec<u64> = (1..=101).collect();
        let archived = archived_at(&root, &ids).unwrap();
        assert_eq!(archived, HashMap::from([(1, true), (2, false)]));
        let requests = served.join().unwrap();
        assert!(requests[0].starts_with("GET /v1/universes/multiget?ids=1&ids=2&"));
        assert!(requests[1].starts_with("GET /v1/universes/multiget?ids=101 "));
        assert!(!requests[0].to_lowercase().contains("cookie"));
    }
}
