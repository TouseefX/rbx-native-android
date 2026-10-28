//! Basic Info's Team Create half: the experience's switch and who
//! is in the place's live session. Both run as the editor's Roblox sign-in
//! rather than the API key, so the switch keeps its own draft beside the
//! cloud one and joins the same Save and Cancel.
//!
//! Under `RBX_STUDIO_GAME_SETTINGS_MOCK` (see `fetch`) both answer as signed
//! in, with a canned session, and `=signed-out` answers as signed out.

use std::time::Duration;

use gpui_kit::*;
use rbx_cloud::heartbeats::SessionCookie;
use rbx_cloud::{CloudError, SessionMember};

use super::model::TeamCreateDraft;
use super::{fetch, GameSettingsWindow, Loading};
use crate::home::Link;

/// How often the session list is read again while the window is open.
const MEMBERS_POLL: Duration = Duration::from_secs(15);

/// Who the calls run as: the stored sign-in, or the capture mock's stand-in.
#[derive(Clone)]
pub(super) enum Web {
    Live(SessionCookie),
    Mock,
}

impl Web {
    fn of(session: Option<SessionCookie>) -> Option<Web> {
        match fetch::mock_mode() {
            Some(mode) => (mode.trim() != "signed-out").then_some(Web::Mock),
            None => session.map(Web::Live),
        }
    }
}

pub(super) struct TeamCreate {
    /// `None` signed out.
    pub(super) web: Option<Web>,
    pub(super) draft: TeamCreateDraft,
    pub(super) loading: Loading,
    /// Bumped by every read and save of the switch, so a reply that lost the
    /// race is dropped.
    pub(super) generation: u64,
    /// Who is in the place's live session, once read.
    pub(super) members: Option<Result<Vec<SessionMember>, String>>,
}

impl TeamCreate {
    pub(super) fn new(session: Option<SessionCookie>) -> Self {
        TeamCreate {
            web: Web::of(session),
            draft: TeamCreateDraft::default(),
            loading: Loading::Idle,
            generation: 0,
            members: None,
        }
    }

    /// What a save sends: who as, the universe and the new value.
    pub(super) fn pending(&self, link: Option<&Link>) -> Option<(Web, u64, bool)> {
        Some((self.web.clone()?, link?.universe_id, self.draft.pending()?))
    }
}

impl GameSettingsWindow {
    /// Reads the session list again every [`MEMBERS_POLL`] for as long as
    /// the window lives.
    pub(super) fn poll_members(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(MEMBERS_POLL).await;
            if this
                .update(cx, |this, cx| this.refresh_members(cx))
                .is_err()
            {
                break;
            }
        })
        .detach();
    }

    /// Catches up with the editor signing in or out meanwhile.
    pub(super) fn follow_sign_in(
        &mut self,
        session: Option<SessionCookie>,
        cx: &mut Context<Self>,
    ) {
        let web = Web::of(session);
        if web.is_some() != self.team_create.web.is_some() {
            self.team_create.web = web;
            self.load_team_create(cx);
        }
    }

    /// Reads the switch and the session list for the current link, dropping
    /// the switch's edit (nothing unlinked or signed out).
    pub(super) fn load_team_create(&mut self, cx: &mut Context<Self>) {
        let tc = &mut self.team_create;
        tc.generation += 1;
        let generation = tc.generation;
        tc.draft = TeamCreateDraft::default();
        tc.members = None;
        let (Some(link), Some(web)) = (self.link.clone(), tc.web.clone()) else {
            tc.loading = Loading::Idle;
            cx.notify();
            return;
        };
        tc.loading = Loading::Busy;
        self.refresh_members(cx);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let reply = cx
                .background_spawn(async move { read(&web, link.universe_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let tc = &mut this.team_create;
                if generation != tc.generation {
                    return;
                }
                match reply {
                    Ok(on) => {
                        tc.draft.reset(on);
                        tc.loading = Loading::Idle;
                    }
                    Err(message) => tc.loading = Loading::Failed(message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn refresh_members(&mut self, cx: &mut Context<Self>) {
        let (Some(link), Some(web)) = (self.link.clone(), self.team_create.web.clone()) else {
            return;
        };
        let place_id = link.place_id;
        cx.spawn(async move |this, cx| {
            let reply = cx
                .background_spawn(async move { members(&web, place_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                // Dropped if the link moved or the editor signed out meanwhile.
                if this.team_create.web.is_some()
                    && this.link.as_ref().map(|l| l.place_id) == Some(place_id)
                {
                    this.team_create.members = Some(reply);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn set_team_create(&mut self, on: bool, cx: &mut Context<Self>) {
        self.team_create.draft.set(on);
        self.notice = None;
        cx.notify();
    }
}

fn read(web: &Web, universe_id: u64) -> Result<bool, String> {
    match web {
        Web::Live(cookie) => {
            rbx_cloud::team_create_enabled(cookie, universe_id).map_err(|e| describe(&e))
        }
        Web::Mock => Ok(true),
    }
}

/// Turns Team Create on or off and returns what Roblox then holds (`on`
/// when the re-read fails: the switch itself was accepted).
pub(super) fn save(web: &Web, universe_id: u64, on: bool) -> Result<bool, String> {
    match web {
        Web::Live(cookie) => {
            rbx_cloud::set_team_create_enabled(cookie, universe_id, on)
                .map_err(|e| describe(&e))?;
            Ok(rbx_cloud::team_create_enabled(cookie, universe_id).unwrap_or(on))
        }
        Web::Mock => Ok(on),
    }
}

fn members(web: &Web, place_id: u64) -> Result<Vec<SessionMember>, String> {
    match web {
        Web::Live(cookie) => {
            rbx_cloud::team_create_members(cookie, place_id).map_err(|e| describe(&e))
        }
        Web::Mock => Ok(vec![
            SessionMember {
                id: 1,
                name: "builderman".to_string(),
                display_name: "Builder".to_string(),
            },
            SessionMember {
                id: 2,
                name: "obby_architect".to_string(),
                display_name: "Mara".to_string(),
            },
        ]),
    }
}

/// Roblox's own message, as is. Not `fetch::describe`: its API-key wording
/// is wrong for the sign-in. A 401 means the stored sign-in expired, which
/// Roblox's "Authorization has been denied" doesn't say how to fix.
fn describe(err: &CloudError) -> String {
    match err {
        CloudError::Refused { status: 401, .. } => format!(
            "{err}. Your Roblox sign-in has expired: sign in again in Settings \u{203a} Account \u{203a} Roblox sign-in."
        ),
        _ => err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{describe, CloudError, Link, TeamCreate, Web};

    #[test]
    fn an_expired_sign_in_says_how_to_fix_it_and_other_refusals_stay_roblox_s() {
        let expired = describe(&CloudError::Refused {
            status: 401,
            message: "Authorization has been denied for this request.".to_string(),
        });
        assert!(
            expired.contains("Authorization has been denied") && expired.contains("sign in again")
        );
        let refused = describe(&CloudError::Refused {
            status: 403,
            message: "You are not authorized to configure this universe.".to_string(),
        });
        assert!(refused.ends_with("You are not authorized to configure this universe."));
    }

    #[test]
    fn a_save_needs_a_sign_in_a_link_and_an_edit() {
        let link = Link {
            universe_id: 9,
            place_id: 3,
            name: None,
        };
        let mut tc = TeamCreate {
            web: Some(Web::Mock),
            ..TeamCreate::new(None)
        };
        tc.draft.reset(false);
        assert!(tc.pending(Some(&link)).is_none());
        tc.draft.set(true);
        assert!(matches!(
            tc.pending(Some(&link)),
            Some((Web::Mock, 9, true))
        ));
        assert!(tc.pending(None).is_none());
        tc.web = None;
        assert!(tc.pending(Some(&link)).is_none());
    }
}
