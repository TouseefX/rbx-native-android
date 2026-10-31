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

/// How many roster avatars the card paints before a `+n`.
const AVATARS: usize = 5;

/// The roster avatar's size, in pixels.
const AVATAR_PX: f32 = 16.;

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
                // The roster's new faces need headshots; the cache is
                // shared with the presence panel, so only ids it does
                // not have yet are fetched. Read before `update`: the
                // session is not the window.
                let (roster, _) = session.members();
                let member_ids: Vec<u64> = roster.into_iter().map(|m| m.id).collect();
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
                        let missing = shell.team_presence.take_unasked(member_ids);
                        shell.fetch_headshots(missing, cx);
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
        let (members, me) = session.members();
        let (headline, dot) = match phase {
            Phase::Negotiating { attempt } => (
                format!("Asking the session server ({attempt})\u{2026}"),
                ui::accent(),
            ),
            Phase::Joined if members.is_empty() => (
                "Joined \u{2014} session live".to_string(),
                ui::green(),
            ),
            Phase::Joined => (
                format!("Joined \u{2014} {} in server", members.len()),
                ui::green(),
            ),
            Phase::Rejoining { attempt } => (
                format!("Rejoining \u{2014} fresh session (attempt {attempt})\u{2026}"),
                ui::accent(),
            ),
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
                .children((!members.is_empty()).then(|| {
                    // Studio's team-create panel, on a card: the
                    // server's roster as round avatars (the same shared
                    // headshot cache the presence panel uses), a `+n`
                    // for the rest, and a green check when the server's
                    // list has us.
                    let more = members.len().saturating_sub(AVATARS);
                    h_flex()
                        .items_center()
                        .gap(px(4.))
                        .children(members.iter().take(AVATARS).map(|m| {
                            let color = super::team_presence::parse_color(
                                rbx_presence::schema::color_for(m.id),
                            )
                            .unwrap_or((255, 255, 255));
                            super::team_presence::avatar(
                                self.team_presence.avatars.get(&m.id).cloned(),
                                color,
                                member_name(m),
                                AVATAR_PX,
                            )
                        }))
                        .when(more > 0, |row| {
                            row.child(
                                ui::text(10., 14.)
                                    .text_color(tokens::text3())
                                    .child(format!("+{more}")),
                            )
                        })
                        .when(me, |row| {
                            let you = h_flex()
                                .items_center()
                                .gap(px(2.))
                                .child(ui::icon("check", 10.).text_color(ui::green()))
                                .child(ui::text(10., 12.).text_color(ui::green()).child("you"));
                            row.child(div().flex_1()).child(you)
                        })
                }))
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

/// The member's display name, falling back to the username — the same
/// pick the session's roster lines use.
fn member_name(member: &rbx_cloud::SessionMember) -> &str {
    if member.display_name.is_empty() {
        &member.name
    } else {
        &member.display_name
    }
}
