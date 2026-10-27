//! The Roblox Quick Log In code flow as an embeddable entity, shared by the
//! Settings sign-in window and the start-up wizard's sign-in step.
//!
//! `LoginFlow` fetches a code (`rbx_cloud::quick_login`), polls its status
//! until the user approves it at `CONFIRM_PAGE` or in the Roblox app, and
//! emits [`LoginEvent::SignedIn`]. Codes live about two minutes: an
//! `Expired` status fetches a new code and QR silently. The host decides
//! what a finished sign-in does and draws its own buttons around
//! [`LoginFlow::body`].
//!
//! `RBX_STUDIO_LAUNCHER_SIGNIN=code|linked|failed` shows a fixed state and
//! never touches the network, for captures.

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use rbx_cloud::heartbeats::SessionCookie;
use rbx_cloud::quick_login::{LoginCode, LoginStatus, QuickLogin, CONFIRM_PAGE};

use crate::launcher::ui;
use crate::tokens;

const POLL: std::time::Duration = std::time::Duration::from_secs(3);

pub(crate) enum LoginEvent {
    SignedIn(SessionCookie, u64),
}

enum State {
    Fetching,
    Showing {
        code: String,
        qr: Option<Arc<RenderImage>>,
        linked: bool,
    },
    Failed(String),
}

pub(crate) struct LoginFlow {
    state: State,
    task: Task<()>,
}

impl EventEmitter<LoginEvent> for LoginFlow {}

fn decode_qr(png: &[u8]) -> Option<Arc<RenderImage>> {
    let rgba = image::load_from_memory(png).ok()?.to_rgba8();
    let (w, h) = rgba.dimensions();
    crate::render_image::to_render_image(rgba.into_raw(), w, h)
}

/// A new code with its QR image (a missing QR is not fatal: the code works).
fn fetch_code(login: &mut QuickLogin) -> Result<(LoginCode, Option<Vec<u8>>), String> {
    let code = login.create()?;
    let png = login.qr_png(&code).ok();
    Ok((code, png))
}

fn spaced(code: &str) -> String {
    code.chars().map(String::from).collect::<Vec<_>>().join(" ")
}

impl LoginFlow {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let mut this = LoginFlow {
            state: State::Fetching,
            task: Task::ready(()),
        };
        match std::env::var("RBX_STUDIO_LAUNCHER_SIGNIN").as_deref() {
            Ok("code" | "done") => this.state = Self::fixture(false),
            Ok("linked") => this.state = Self::fixture(true),
            Ok("failed") => {
                this.state = State::Failed(
                    "Roblox couldn't create a sign-in code. Check your connection and try again."
                        .to_owned(),
                )
            }
            _ => this.start(cx),
        }
        this
    }

    fn fixture(linked: bool) -> State {
        State::Showing {
            code: spaced("K7Q2MX"),
            qr: None,
            linked,
        }
    }

    pub(crate) fn restart(&mut self, cx: &mut Context<Self>) {
        self.start(cx);
    }

    pub(crate) fn failed(&self) -> bool {
        matches!(self.state, State::Failed(_))
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        self.state = State::Fetching;
        self.task = cx.spawn(async move |this, cx| {
            let executor = cx.background_executor().clone();
            let mut login = QuickLogin::new();
            let (mut login_back, first) = executor
                .spawn(async move {
                    let r = fetch_code(&mut login);
                    (login, r)
                })
                .await;
            let mut code = match first {
                Ok((code, png)) => {
                    Self::show(&this, cx, &code, png);
                    code
                }
                Err(e) => {
                    Self::fail(&this, cx, e);
                    return;
                }
            };
            loop {
                executor.timer(POLL).await;
                let poll_code = code.clone();
                let (back, status) = executor
                    .spawn(async move {
                        let mut login = login_back;
                        let r = login.status(&poll_code);
                        (login, r)
                    })
                    .await;
                login_back = back;
                match status {
                    Err(e) => {
                        Self::fail(&this, cx, e);
                        return;
                    }
                    Ok(LoginStatus::Created) => {}
                    Ok(LoginStatus::UserLinked) => {
                        let _ = this.update(cx, |this, cx| {
                            if let State::Showing { linked, .. } = &mut this.state {
                                *linked = true;
                            }
                            cx.notify();
                        });
                    }
                    Ok(LoginStatus::Cancelled) => {
                        Self::fail(&this, cx, "The sign-in was cancelled.".to_owned());
                        return;
                    }
                    Ok(LoginStatus::Expired) => {
                        let (back, fresh) = executor
                            .spawn(async move {
                                let mut login = login_back;
                                let r = fetch_code(&mut login);
                                (login, r)
                            })
                            .await;
                        login_back = back;
                        match fresh {
                            Ok((new_code, png)) => {
                                Self::show(&this, cx, &new_code, png);
                                code = new_code;
                            }
                            Err(e) => {
                                Self::fail(&this, cx, e);
                                return;
                            }
                        }
                    }
                    Ok(LoginStatus::Validated) => {
                        let done_code = code.clone();
                        let (_, finished) = executor
                            .spawn(async move {
                                let mut login = login_back;
                                let r = login.finish(&done_code);
                                (login, r)
                            })
                            .await;
                        match finished {
                            Ok((cookie, user_id)) => {
                                let _ = this.update(cx, |_, cx| {
                                    cx.emit(LoginEvent::SignedIn(cookie, user_id));
                                });
                            }
                            Err(e) => Self::fail(&this, cx, e),
                        }
                        return;
                    }
                }
            }
        });
        cx.notify();
    }

    fn show(this: &WeakEntity<Self>, cx: &mut AsyncApp, code: &LoginCode, png: Option<Vec<u8>>) {
        let text = spaced(&code.code);
        let _ = this.update(cx, |this, cx| {
            this.state = State::Showing {
                code: text,
                qr: png.as_deref().and_then(decode_qr),
                linked: false,
            };
            cx.notify();
        });
    }

    fn fail(this: &WeakEntity<Self>, cx: &mut AsyncApp, text: String) {
        let _ = this.update(cx, |this, cx| {
            this.state = State::Failed(text);
            cx.notify();
        });
    }

    pub(crate) fn body(&self, _cx: &App) -> AnyElement {
        let (main, status): (AnyElement, Option<(String, bool)>) = match &self.state {
            State::Fetching => (
                div().child("Getting a sign-in code…").into_any_element(),
                None,
            ),
            State::Failed(text) => (div().into_any_element(), Some((text.clone(), true))),
            State::Showing { code, qr, linked } => {
                let image: AnyElement = match qr {
                    Some(image) => img(image.clone()).size(px(180.)).into_any_element(),
                    None => div()
                        .size(px(180.))
                        .border_1()
                        .border_color(ui::wash())
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(ui::icon("qr-code", 48.))
                        .into_any_element(),
                };
                let message = if *linked {
                    "Approve it on your other device…"
                } else {
                    "Waiting for approval…"
                };
                let right = v_flex()
                    .flex_1()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(tokens::text())
                            .child("Enter this code at"),
                    )
                    .child(
                        div()
                            .id("confirm-link")
                            .text_size(px(13.))
                            .text_color(ui::accent())
                            .cursor_pointer()
                            .on_click(|_, _, cx| cx.open_url(CONFIRM_PAGE))
                            .child("roblox.com/crossdevicelogin/ConfirmCode"),
                    )
                    .child(
                        div()
                            .font_family("monospace")
                            .text_size(px(28.))
                            .text_color(tokens::text())
                            .child(code.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(tokens::text2())
                            .child("or scan it with the Roblox app"),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(tokens::text2())
                            .child(message),
                    );
                (
                    h_flex()
                        .gap(px(16.))
                        .child(image)
                        .child(right)
                        .into_any_element(),
                    None,
                )
            }
        };
        v_flex()
            .gap(px(12.))
            .text_color(tokens::text())
            .child(main)
            .children(status.map(|(text, error)| {
                div()
                    .text_size(px(13.))
                    .text_color(if error {
                        tokens::text_error()
                    } else {
                        tokens::text2()
                    })
                    .child(text)
            }))
            .into_any_element()
    }
}
