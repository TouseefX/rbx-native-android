//! Expired credentials during an editor session.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::h_flex;
use gpui_kit::*;

use super::Shell;
use crate::command_bar::Feedback;
use crate::launcher::setup::SetupPlan;
use crate::launcher::ui::{self, Weight};
use crate::tokens;

/// How often the client's refusal flags are read; a local read only.
const POLL: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(super) struct CredentialWatch {
    notice: Option<SetupPlan>,
    poll: Option<Task<()>>,
}

impl Shell {
    pub(super) fn start_credential_watch(&mut self, cx: &mut Context<Self>) {
        let fixture = std::env::var("RBX_STUDIO_CREDENTIAL_NOTICE").ok();
        let reports = match fixture.as_deref() {
            Some("session") => Some(rbx_cloud::expiry::Reports {
                key: false,
                session: true,
            }),
            Some("key") => Some(rbx_cloud::expiry::Reports {
                key: true,
                session: false,
            }),
            Some("both") => Some(rbx_cloud::expiry::Reports {
                key: true,
                session: true,
            }),
            _ => None,
        };
        if let Some(reports) = reports {
            self.credential_watch.notice = Some(merged(None, reports));
        }
        self.credential_watch.poll = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(POLL).await;
            let reports = rbx_cloud::expiry::take_reports();
            if !(reports.key || reports.session) {
                continue;
            }
            if this
                .update(cx, |shell, cx| shell.credentials_expired(reports, cx))
                .is_err()
            {
                return;
            }
        }));
    }

    fn credentials_expired(&mut self, reports: rbx_cloud::expiry::Reports, cx: &mut Context<Self>) {
        if reports.session {
            self.stop_studio_presence();
            self.studio_presence.session = None;
            self.studio_presence.user_id = None;
        }
        cx.spawn(async move |this, cx| {
            let mut errors = Vec::new();
            if reports.session {
                if let Err(error) = crate::key_store::forget_session(cx).await {
                    errors.push(format!("Could not remove the Roblox sign-in: {error}"));
                }
            }
            if reports.key {
                if let Err(error) = crate::key_store::forget(cx).await {
                    errors.push(format!("Could not remove the API key: {error}"));
                }
            }
            if errors.is_empty() {
                return;
            }
            let _ = this.update(cx, |shell, cx| {
                for message in errors {
                    shell.output.push("Roblox", Feedback::Error(message));
                }
                cx.notify();
            });
        })
        .detach();
        self.credential_watch.notice = Some(merged(self.credential_watch.notice, reports));
        cx.notify();
    }

    pub(super) fn credential_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let plan = self.credential_watch.notice?;
        let label = match (plan.sign_in, plan.key) {
            (true, false) => "Sign in again",
            (false, true) => "Set up key",
            _ => "Set up again",
        };
        let text = plan.reasons().join(" ");
        let weak = cx.entity().downgrade();
        let setup = ui::button("credential-setup", label, Weight::Primary, true).on_click(
            cx.listener(move |_, _, _, cx| {
                let weak = weak.clone();
                cx.defer(move |cx| {
                    let on_finish: crate::launcher::Finish = Rc::new(move |cx: &mut App| {
                        let _ = weak.update(cx, |shell, cx| {
                            shell.credential_watch.notice = None;
                            shell.restore_roblox_session(cx);
                            cx.notify();
                        });
                    });
                    crate::launcher::open_wizard(
                        Rc::new(RefCell::new(None)),
                        plan,
                        Some(on_finish),
                        cx,
                    );
                });
            }),
        );
        let dismiss = ui::button("credential-dismiss", "Dismiss", Weight::Ghost, true).on_click(
            cx.listener(|shell, _, _, cx| {
                shell.credential_watch.notice = None;
                cx.notify();
            }),
        );
        Some(
            h_flex()
                .w_full()
                .h(px(32.))
                .flex_none()
                .px(px(12.))
                .gap(px(10.))
                .items_center()
                .bg(ui::panel())
                .border_b_1()
                .border_color(tokens::border())
                .child(ui::icon("triangle-alert", 14.).text_color(ui::red()))
                .child(ui::text(12., 16.).child(text))
                .child(div().flex_1())
                .child(setup)
                .child(dismiss)
                .into_any_element(),
        )
    }
}

fn merged(current: Option<SetupPlan>, reports: rbx_cloud::expiry::Reports) -> SetupPlan {
    let mut plan = current.unwrap_or_default();
    if reports.session {
        plan.sign_in = true;
        plan.session_expired = true;
    }
    if reports.key {
        plan.key = true;
        plan.key_expired = true;
    }
    plan
}
