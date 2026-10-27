//! The wizard's "Sign in to Roblox" step: the Quick Log In code flow, and
//! what a finished sign-in shows.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;

use super::ui;
use super::Wizard;
use crate::login_flow::{LoginEvent, LoginFlow};
use crate::tokens;

/// `RBX_STUDIO_LAUNCHER_SIGNIN=code|linked|failed|done`: the sign-in step's
/// state for a capture; set, nothing is fetched or saved.
pub(super) const SIGNIN_VARIABLE: &str = "RBX_STUDIO_LAUNCHER_SIGNIN";

/// The user id the `done` fixture shows as signed in.
pub(super) const FIXTURE_USER: u64 = 925308243;

impl Wizard {
    /// Creates the code flow the first time the step shows, so a launch that
    /// never reaches it never asks Roblox for a code.
    pub(super) fn ensure_login(&mut self, cx: &mut Context<Self>) {
        if self.login.is_some() || self.signed_in.is_some() {
            return;
        }
        let flow = cx.new(LoginFlow::new);
        self.login_subscriptions = vec![
            cx.observe(&flow, |_, _, cx| cx.notify()),
            cx.subscribe(&flow, |this, _, event: &LoginEvent, cx| {
                let LoginEvent::SignedIn(cookie, user_id) = event;
                this.signed_in(cookie.clone(), *user_id, cx);
            }),
        ];
        self.login = Some(flow);
    }

    fn signed_in(
        &mut self,
        cookie: rbx_cloud::heartbeats::SessionCookie,
        user_id: u64,
        cx: &mut Context<Self>,
    ) {
        self.signed_in = Some(user_id);
        self.session_error = None;
        cx.notify();
        if std::env::var(SIGNIN_VARIABLE).is_ok() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let result = crate::key_store::save_session(&cookie, cx).await;
            if let Err(err) = result {
                let _ = this.update(cx, |this, cx| {
                    this.session_error = Some(format!(
                        "Your system keychain couldn\u{2019}t be reached, so the sign-in wasn\u{2019}t kept; sign in again from Settings \u{203a} Account. ({err})"
                    ));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(super) fn login_failed(&self, cx: &App) -> bool {
        self.signed_in.is_none()
            && self
                .login
                .as_ref()
                .is_some_and(|flow| flow.read(cx).failed())
    }

    pub(super) fn retry_login(&mut self, cx: &mut Context<Self>) {
        if let Some(flow) = &self.login {
            flow.update(cx, |flow, cx| flow.restart(cx));
        }
    }

    /// The why-setup-opened-again row, when anything expired.
    pub(super) fn reasons_row(&self) -> Option<AnyElement> {
        let reasons = self.plan.reasons();
        if reasons.is_empty() {
            return None;
        }
        Some(
            h_flex()
                .items_center()
                .gap(px(10.))
                .py(px(10.))
                .px(px(14.))
                .rounded(px(8.))
                .border_1()
                .border_color(ui::red_line())
                .bg(ui::red_soft())
                .text_color(ui::red())
                .child(ui::icon("triangle-alert", 16.).flex_none())
                .child(ui::text(12.5, 18.).child(reasons.join(" ")))
                .into_any_element(),
        )
    }

    pub(super) fn sign_in(&self, cx: &App) -> AnyElement {
        let content: AnyElement = match self.signed_in {
            Some(user_id) => {
                let text = match &self.session_error {
                    Some(error) => error.clone(),
                    None => format!("Roblox user {user_id}. Stored in your system keychain."),
                };
                h_flex()
                    .items_start()
                    .gap(px(12.))
                    .py(px(14.))
                    .px(px(16.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(tokens::border())
                    .bg(ui::panel2())
                    .child(
                        h_flex()
                            .size(px(32.))
                            .flex_none()
                            .rounded(px(7.))
                            .bg(ui::green_soft())
                            .text_color(ui::green())
                            .items_center()
                            .justify_center()
                            .child(ui::icon("check", 16.)),
                    )
                    .child(
                        v_flex()
                            .gap(px(2.))
                            .child(
                                ui::text(12.5, 17.)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(tokens::text())
                                    .child("Signed in"),
                            )
                            .child(ui::text(12., 18.).text_color(tokens::text2()).child(text)),
                    )
                    .into_any_element()
            }
            None => v_flex()
                .gap(px(14.))
                .children(self.login.as_ref().map(|flow| {
                    div()
                        .p(px(20.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(tokens::border())
                        .bg(ui::panel2())
                        .child(flow.read(cx).body(cx))
                }))
                .child(ui::text(11.5, 17.).text_color(tokens::text3()).child(
                    "Skip it and the rest still works; Team Create, Play, In Studio presence and Home\u{2019}s Archive wait until you sign in (Settings \u{203a} Account).",
                ))
                .into_any_element(),
        };
        v_flex()
            .gap(px(18.))
            .child(Self::big_glyph("log-in", ui::accent(), tokens::accent_soft(), false))
            .child(Self::heading(
                "Sign in to Roblox",
                "Approve this code on a device where you're signed in to Roblox. RbxNative keeps the session in your system keychain.",
                None,
            ))
            .child(content)
            .into_any_element()
    }
}
