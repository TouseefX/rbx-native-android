//! The Team Create session's status card. Home's Join starts the live
//! session beside the editor window it opens, and this is how the phone
//! shows it: without it the join is logcat-only, which a user never
//! reads. The card floats over the workspace's top-right corner — its
//! headline the phase, its tail the session's own last lines — and the
//! verdict also lands in the Output dock when the session ends.

use std::time::Duration;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;

use super::Shell;
use crate::command_bar::Feedback;
use crate::launcher::team_join::{Phase, Session};
use crate::launcher::ui;
use crate::tokens;

/// How often the card reads the session: the lines are few and small, a
/// local read only.
const POLL: Duration = Duration::from_millis(600);

/// How many of the session's last lines the card shows.
const LINES: usize = 3;

/// Characters past which a headline or a log line is cut with an
/// ellipsis: the HTTP errors carry 140-char snippets, and the card
/// cannot stretch for them.
const WIDTH: usize = 90;

impl Shell {
    /// The card's poller: repaints when the session wrote something,
    /// records the verdict in the Output dock when it ends, and stops
    /// once that end is painted (or the window is gone).
    pub(in crate::shell) fn watch_team_create(
        &mut self,
        session: Session,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let mut seen = None;
            let mut recorded = false;
            loop {
                cx.background_executor().timer(POLL).await;
                let version = session.version();
                let done = session.is_finished();
                if seen == Some(version) && !done {
                    continue;
                }
                seen = Some(version);
                let verdict = if done && !recorded {
                    recorded = true;
                    match session.snapshot().0 {
                        Phase::Ended {
                            datagrams_tx,
                            app_messages,
                            app_bytes,
                        } => Some(Feedback::Info(format!(
                            "Team Create session ended: {datagrams_tx} datagrams out, {app_messages} app messages in ({app_bytes} bytes)"
                        ))),
                        Phase::Failed(err) => Some(Feedback::Error(format!(
                            "Team Create join failed: {err}"
                        ))),
                        _ => None,
                    }
                } else {
                    None
                };
                let alive = this
                    .update(cx, |shell, cx| {
                        if let Some(feedback) = verdict {
                            shell.output.push("Team Create", feedback);
                        }
                        cx.notify();
                    })
                    .is_ok();
                if done || !alive {
                    return;
                }
            }
        })
        .detach();
    }

    /// The card itself, over the workspace's top-right corner, from the
    /// first frame: the session starts before the download that opens
    /// this window does.
    pub(in crate::shell) fn team_create_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.team_create.as_ref()?;
        let (phase, log) = session.snapshot();
        let (headline, dot) = match phase {
            Phase::Negotiating { attempt } => (
                format!("Asking the session server ({attempt})\u{2026}"),
                ui::accent(),
            ),
            Phase::Joined => ("Joined \u{2014} session live".to_string(), ui::green()),
            Phase::Ended {
                datagrams_tx,
                app_messages,
                app_bytes,
            } => (
                format!("Ended: {datagrams_tx} out, {app_messages} in ({app_bytes}B)"),
                tokens::text3(),
            ),
            Phase::Failed(err) => (format!("Join failed: {err}"), ui::red()),
        };
        let skip = log.len().saturating_sub(LINES);
        let dismiss = h_flex()
            .id("team-create-dismiss")
            .size(px(20.))
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .cursor_pointer()
            .hover(|this| tokens::hover_fx(this).bg(tokens::hover_subtle()))
            .child(ui::icon("x", 11.).text_color(tokens::text3()))
            .on_click(cx.listener(|shell, _, _, cx| {
                // The card goes; the session and its Output verdict stay.
                shell.team_create = None;
                cx.notify();
            }));
        Some(
            v_flex()
                .id("team-create-card")
                .absolute()
                .top(px(46.))
                .right(px(8.))
                .w(px(250.))
                .flex_none()
                .gap(px(3.))
                .p(px(10.))
                .rounded(px(8.))
                .border_1()
                .border_color(tokens::border())
                .bg(ui::panel())
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(6.))
                        .child(div().size(px(7.)).flex_none().rounded_full().bg(dot))
                        .child(
                            ui::text(11.5, 16.)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(tokens::text())
                                .truncate()
                                .child(short(&headline)),
                        )
                        .child(div().flex_1())
                        .child(dismiss),
                )
                .children(log.into_iter().skip(skip).map(|line| {
                    ui::text(10.5, 14.)
                        .text_color(tokens::text3())
                        .truncate()
                        .child(short(&line))
                }))
                .into_any_element(),
        )
    }
}

/// `text` cut at `WIDTH` characters with an ellipsis.
fn short(text: &str) -> String {
    if text.chars().count() > WIDTH {
        format!("{}\u{2026}", text.chars().take(WIDTH).collect::<String>())
    } else {
        text.to_string()
    }
}
