//! The cookie card's logic: read the stored Roblox session, check it
//! against `users/authenticated`, paste and store a new one. Team Create
//! joins need the cookie; the card in the sidebar is where it lives.

use gpui_kit::*;

use super::{CookieState, Dialog, HomeWindow, COOKIE_VARIABLE};

impl HomeWindow {
    /// Reads the session from the keyring and checks it, so the card can
    /// say whose it is. A wizard or editor sign-in in the meantime is
    /// picked up the next time Home opens.
    pub(super) fn load_cookie(&mut self, cx: &mut Context<Self>) {
        match std::env::var(COOKIE_VARIABLE).as_deref() {
            Ok("ready") => {
                self.cookie = CookieState::Ready {
                    name: "Builderman".into(),
                    user_id: 156,
                };
                return;
            }
            Ok("expired") => {
                self.cookie = CookieState::Expired;
                return;
            }
            Ok("unchecked") => {
                self.cookie = CookieState::Unchecked;
                return;
            }
            _ => {}
        }
        self.cookie = CookieState::Checking;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let session = crate::key_store::read_session(cx).await.ok().flatten();
            let Some(session) = session else {
                let _ = this.update(cx, |this, cx| {
                    this.cookie = CookieState::Missing;
                    cx.notify();
                });
                return;
            };
            let who = cx
                .background_spawn(async move {
                    rbx_cloud::heartbeats::authenticated_user(&session)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.cookie = match who {
                    Ok(user) => CookieState::Ready {
                        name: user.display_name,
                        user_id: user.id,
                    },
                    Err(rbx_cloud::heartbeats::PulseError::Unauthorized) => CookieState::Expired,
                    Err(_) => CookieState::Unchecked,
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Opens the paste dialog with an empty field: a stored cookie is
    /// never echoed back into it.
    pub(super) fn open_cookie_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cookie_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.dialog = Some(Dialog::Cookie {
            busy: false,
            error: None,
        });
        cx.notify();
    }

    /// Checks the pasted `.ROBLOSECURITY` (bare or with the cookie name,
    /// `SessionCookie::new` takes either) and stores it only once Roblox
    /// names its owner — a dead cookie never reaches the keyring.
    pub(super) fn save_cookie(&mut self, cx: &mut Context<Self>) {
        let raw = self.cookie_input.read(cx).value().to_string();
        let cookie = rbx_cloud::heartbeats::SessionCookie::new(&raw);
        if cookie.expose_secret().is_empty() {
            return;
        }
        if let Some(Dialog::Cookie { busy, error }) = &mut self.dialog {
            *busy = true;
            *error = None;
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let probe = cookie.clone();
            let who = cx
                .background_spawn(async move {
                    rbx_cloud::heartbeats::authenticated_user(&probe)
                })
                .await;
            match who {
                Ok(user) => {
                    crate::key_store::save_session(&cookie, cx).await.ok();
                    let _ = this.update(cx, |this, cx| {
                        this.cookie = CookieState::Ready {
                            name: user.display_name,
                            user_id: user.id,
                        };
                        this.dialog = None;
                        cx.notify();
                    });
                }
                Err(rbx_cloud::heartbeats::PulseError::Unauthorized) => {
                    let _ = this.update(cx, |this, cx| {
                        cookie_error(
                            this,
                            "Roblox doesn\u{2019}t accept this cookie. Copy the .ROBLOSECURITY value fresh from a signed-in browser.",
                        );
                        cx.notify();
                    });
                }
                Err(_) => {
                    let _ = this.update(cx, |this, cx| {
                        cookie_error(
                            this,
                            "Couldn\u{2019}t reach Roblox. Check your connection and try again.",
                        );
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// Drops the stored session; the card goes back to offering a paste.
    pub(super) fn sign_out_cookie(&mut self, cx: &mut Context<Self>) {
        self.cookie = CookieState::Missing;
        self.dialog = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let _ = crate::key_store::forget_session(cx).await;
            let _ = this.update(cx, |this, cx| {
                this.session = None;
                cx.notify();
            });
        })
        .detach();
    }
}

/// The failed-check state both save errors share.
fn cookie_error(this: &mut HomeWindow, message: &str) {
    if let Some(Dialog::Cookie { busy, error }) = &mut this.dialog {
        *busy = false;
        *error = Some(message.to_string());
    }
}
