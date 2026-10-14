//! Which setup steps a launch needs, from the state of the two credentials:
//! the Roblox sign-in (session cookie) and the Open Cloud API key.

/// One credential as a launch finds it. A check that could not reach
/// Roblox counts as `Present`: only a definite refusal is `Expired`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cred {
    Missing,
    Present,
    Expired,
}

/// The setup steps to show, and why they are shown again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SetupPlan {
    /// Show the "Sign in to Roblox" step.
    pub(crate) sign_in: bool,
    /// Show the API key steps (Welcome, Create, Paste, Done).
    pub(crate) key: bool,
    pub(crate) session_expired: bool,
    pub(crate) key_expired: bool,
}

impl SetupPlan {
    /// Only the key steps: Home's "set up a key".
    pub(crate) const KEY_ONLY: SetupPlan = SetupPlan {
        sign_in: false,
        key: true,
        session_expired: false,
        key_expired: false,
    };

    /// The short lines saying why setup opened again, in display order:
    /// "Your Roblox sign-in expired." then "Your API key expired or was
    /// revoked." Empty when nothing expired.
    pub(crate) fn reasons(&self) -> Vec<&'static str> {
        let mut lines = Vec::new();
        if self.session_expired {
            lines.push("Your Roblox sign-in expired.");
        }
        if self.key_expired {
            lines.push("Your API key expired or was revoked.");
        }
        lines
    }
}

/// The setup a launch without a place path needs, or `None` for Home.
/// `forced` is `--setup`. Rules: a missing or expired key, or `forced`,
/// shows the key steps, with the sign-in step too unless the session is
/// `Present`; a present key with an expired session shows the sign-in step
/// alone; a present key with a present or missing session needs nothing
/// (a never-signed-in user is not nagged). The `*_expired` flags mirror
/// `Cred::Expired`.
pub(crate) fn plan(key: Cred, session: Cred, forced: bool) -> Option<SetupPlan> {
    let session_expired = session == Cred::Expired;
    let key_expired = key == Cred::Expired;
    let key_steps = key != Cred::Present || forced;
    let sign_in = if key_steps {
        session != Cred::Present
    } else {
        session_expired
    };
    (key_steps || sign_in).then_some(SetupPlan {
        sign_in,
        key: key_steps,
        session_expired,
        key_expired,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use Cred::{Expired, Missing, Present};

    #[test]
    fn a_present_key_needs_nothing_unless_the_session_expired() {
        assert_eq!(plan(Present, Present, false), None);
        assert_eq!(plan(Present, Missing, false), None);
        let only_sign_in = plan(Present, Expired, false).unwrap();
        assert!(only_sign_in.sign_in && !only_sign_in.key);
        assert!(only_sign_in.session_expired && !only_sign_in.key_expired);
    }

    #[test]
    fn a_bad_key_shows_the_key_steps_and_sign_in_unless_present() {
        for key in [Missing, Expired] {
            let with_session = plan(key, Present, false).unwrap();
            assert!(with_session.key && !with_session.sign_in);
            assert_eq!(with_session.key_expired, key == Expired);
            for session in [Missing, Expired] {
                let both = plan(key, session, false).unwrap();
                assert!(both.key && both.sign_in);
                assert_eq!(both.session_expired, session == Expired);
            }
        }
    }

    #[test]
    fn forcing_shows_the_key_steps_for_a_present_key() {
        let present = plan(Present, Present, true).unwrap();
        assert!(present.key && !present.sign_in);
        let missing = plan(Present, Missing, true).unwrap();
        assert!(missing.key && missing.sign_in);
        let expired = plan(Present, Expired, true).unwrap();
        assert!(expired.key && expired.sign_in && expired.session_expired);
    }

    #[test]
    fn reasons_follow_display_order() {
        assert!(SetupPlan::KEY_ONLY.reasons().is_empty());
        let both = SetupPlan {
            session_expired: true,
            key_expired: true,
            ..SetupPlan::default()
        };
        assert_eq!(
            both.reasons(),
            [
                "Your Roblox sign-in expired.",
                "Your API key expired or was revoked."
            ]
        );
        let key = SetupPlan {
            key_expired: true,
            ..SetupPlan::default()
        };
        assert_eq!(key.reasons(), ["Your API key expired or was revoked."]);
    }
}
