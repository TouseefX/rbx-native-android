//! Telling a dead credential from a working one.
//!
//! A Roblox sign-in (`.ROBLOSECURITY`) counts as expired when the
//! authenticated-user endpoint refuses it (401 or 403); an API key counts as expired when
//! introspection says it is expired or disabled, or when a keyed call is
//! refused with a body that says the key itself is bad. Only the
//! `Invalid API Key` wording was seen live; the `expired` wording is assumed
//! from Open Cloud's usual phrasing. Scope and permission refusals are not
//! expiry: the key still works, it just cannot do that.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::heartbeats::{authenticated_user_id, PulseError, SessionCookie};
use crate::{CloudError, KeyInfo};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    Valid,
    Expired,
    Unknown,
}

static KEY_REFUSED: AtomicBool = AtomicBool::new(false);
static SESSION_REFUSED: AtomicBool = AtomicBool::new(false);

/// The session's health from [`authenticated_user_id`]'s answer. Anything
/// but a refusal is `Unknown`: being offline must not sign the user out.
pub fn session_health(answer: &Result<u64, PulseError>) -> Health {
    match answer {
        Ok(_) => Health::Valid,
        Err(PulseError::Unauthorized) => Health::Expired,
        Err(_) => Health::Unknown,
    }
}

/// One `users/authenticated` request with the cookie.
pub fn check_session(cookie: &SessionCookie) -> Health {
    session_health(&authenticated_user_id(cookie))
}

/// Launch check of the key from `Client::introspect()`.
pub fn key_health(result: &Result<KeyInfo, CloudError>) -> Health {
    match result {
        Ok(info) if info.expired || !info.enabled => Health::Expired,
        Ok(_) => Health::Valid,
        Err(CloudError::Http {
            status: 401 | 403, ..
        }) => Health::Expired,
        Err(_) => Health::Unknown,
    }
}

/// A keyed response during use says the key itself is dead.
pub fn key_refused(status: u16, body: &[u8]) -> bool {
    if status != 401 && status != 403 {
        return false;
    }
    let text = String::from_utf8_lossy(body).to_lowercase();
    text.contains("invalid api key") || text.contains("expired")
}

pub fn report_key_refused() {
    KEY_REFUSED.store(true, Ordering::SeqCst);
}

pub fn report_session_refused() {
    SESSION_REFUSED.store(true, Ordering::SeqCst);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reports {
    pub key: bool,
    pub session: bool,
}

/// Reads and clears both flags.
pub fn take_reports() -> Reports {
    Reports {
        key: KEY_REFUSED.swap(false, Ordering::SeqCst),
        session: SESSION_REFUSED.swap(false, Ordering::SeqCst),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(enabled: bool, expired: bool) -> KeyInfo {
        KeyInfo {
            name: "k".into(),
            authorized_user_id: 1,
            scopes: Vec::new(),
            enabled,
            expired,
            expiration_time_utc: String::new(),
        }
    }

    fn http(status: u16) -> Result<KeyInfo, CloudError> {
        Err(CloudError::Http {
            status,
            url: "u".into(),
        })
    }

    #[test]
    fn session_statuses() {
        assert_eq!(session_health(&Ok(1)), Health::Valid);
        assert_eq!(
            session_health(&Err(PulseError::Unauthorized)),
            Health::Expired
        );
        assert_eq!(
            session_health(&Err(PulseError::Other("HTTP 500".into()))),
            Health::Unknown
        );
    }

    #[test]
    fn key_health_cases() {
        assert_eq!(key_health(&Ok(info(true, false))), Health::Valid);
        assert_eq!(key_health(&Ok(info(true, true))), Health::Expired);
        assert_eq!(key_health(&Ok(info(false, false))), Health::Expired);
        assert_eq!(key_health(&http(401)), Health::Expired);
        assert_eq!(key_health(&http(403)), Health::Expired);
        assert_eq!(key_health(&http(500)), Health::Unknown);
        assert_eq!(
            key_health(&Err(CloudError::Transport("down".into()))),
            Health::Unknown
        );
    }

    #[test]
    fn key_refusal_wording() {
        let live = br#"{"errors":[{"message":"Invalid API Key"}]}"#;
        assert!(key_refused(401, live));
        assert!(key_refused(403, b"API key has expired"));
        assert!(!key_refused(403, b"Insufficient scope"));
        assert!(!key_refused(401, b"not authorized"));
        assert!(!key_refused(200, live));
    }

    #[test]
    fn reports_are_taken_once() {
        take_reports();
        report_key_refused();
        report_session_refused();
        assert_eq!(
            take_reports(),
            Reports {
                key: true,
                session: true
            }
        );
        assert_eq!(take_reports(), Reports::default());
    }
}
