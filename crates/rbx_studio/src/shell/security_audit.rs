//! The Security Audit dock: `rbx_audit::audit` run over the whole place,
//! its findings grouped under the script or remote they concern. Clicking a
//! line finding opens the script at that line; a place-level finding selects
//! the remote in the Explorer.
//!
//! Studio has no equivalent. The audit is cached against the DOM's mutation
//! count, so it re-runs only after an edit (or Re-run), not on every frame.
//!
//! `RBX_STUDIO_SECURITY_AUDIT=1` opens the dock at startup: a screenshot aid
//! like the other `RBX_STUDIO_*` variables.

use crate::shell::chrome::ScrollbarY as _;
use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Icon, Sizable};
use gpui_kit::*;
use rbx_audit::{Finding, Severity};

use crate::tokens;

use super::chrome;
use super::layout::Panel;
use super::menu::{self, MenuId};
use super::Shell;

/// Read once at startup; documented in this module's doc comment.
const SECURITY_AUDIT_VARIABLE: &str = "RBX_STUDIO_SECURITY_AUDIT";

/// The last audit and the DOM state it was taken at.
#[derive(Default)]
pub(super) struct AuditState {
    findings: Vec<Finding>,
    at: Option<u64>,
    scroll: ScrollHandle,
}

impl Shell {
    /// `RBX_STUDIO_SECURITY_AUDIT=1`: see this module's doc comment.
    pub(super) fn apply_debug_security_audit(&mut self, cx: &mut Context<Self>) {
        if std::env::var_os(SECURITY_AUDIT_VARIABLE).is_some() {
            self.set_panel_open(Panel::SecurityAudit, true, cx);
        }
    }

    pub(super) fn security_audit_dock(
        &mut self,
        cx: &mut Context<Self>,
    ) -> (Option<AnyElement>, Option<AnyElement>) {
        let stamp = self.edit_dom().mutations();
        if self.audit.at != Some(stamp) {
            self.audit.findings = rbx_audit::audit(self.edit_dom());
            // One group per path below; a stable sort keeps the worst
            // finding first within each.
            self.audit.findings.sort_by(|a, b| a.path.cmp(&b.path));
            self.audit.at = Some(stamp);
        }

        let overflow = menu::dropdown(
            self,
            MenuId::SecurityAuditOverflow,
            chrome::dock_options_trigger(
                "security-audit-overflow",
                IconName::Ellipsis,
                16.,
                "Security Audit dock options",
            ),
            self.move_items(Panel::SecurityAudit),
            cx,
        );

        let count = |severity| {
            self.audit
                .findings
                .iter()
                .filter(|finding| finding.severity == severity)
                .count()
        };
        let header = h_flex()
            .w_full()
            .flex_none()
            .gap_2()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(tokens::border())
            .child(
                div()
                    .flex_1()
                    .text_size(tokens::text_xs())
                    .text_color(tokens::text2())
                    .child(summary(
                        count(Severity::High),
                        count(Severity::Medium),
                        count(Severity::Low),
                    )),
            )
            .child(
                div()
                    .id("security-audit-rerun")
                    .px_2()
                    .rounded_sm()
                    .text_size(tokens::text_xs())
                    .text_color(tokens::text())
                    .hover(|this| tokens::hover_fx(this).bg(tokens::hover()))
                    .on_click(cx.listener(|shell, _, _, cx| {
                        shell.audit.at = None;
                        cx.notify();
                    }))
                    .child("Re-run"),
            );

        let mut list = v_flex().w_full();
        let mut index = 0;
        while index < self.audit.findings.len() {
            let path = &self.audit.findings[index].path;
            let run = self.audit.findings[index..]
                .iter()
                .take_while(|finding| &finding.path == path)
                .count();
            list = list.child(path_row(path, run));
            for offset in index..index + run {
                list = list.child(finding_row(offset, &self.audit.findings[offset], cx));
            }
            index += run;
        }

        let body = v_flex().size_full().child(header).child(
            div()
                .id("security-audit-list")
                .flex_1()
                .overflow_y_scroll()
                .track_scroll(&self.audit.scroll)
                .pr(super::chrome::SCROLLBAR_GUTTER)
                .child(list)
                .scrollbar_y(&self.audit.scroll),
        );
        (
            Some(overflow.into_any_element()),
            Some(body.into_any_element()),
        )
    }
}

fn path_row(path: &str, count: usize) -> impl IntoElement {
    h_flex()
        .w_full()
        .gap_2()
        .px_2()
        .pt_1()
        .text_size(tokens::text_xs())
        .child(
            div()
                .text_color(tokens::text_strong())
                .child(path.to_owned()),
        )
        .child(div().text_color(tokens::text3()).child(count.to_string()))
}

fn finding_row(index: usize, finding: &Finding, cx: &mut Context<Shell>) -> impl IntoElement {
    let (icon, color) = match finding.severity {
        Severity::High => (IconName::CircleX, cx.theme().danger),
        Severity::Medium => (IconName::TriangleAlert, cx.theme().warning),
        Severity::Low => (IconName::Info, cx.theme().muted_foreground),
    };
    let (instance, line) = (finding.instance, finding.line);
    h_flex()
        .id(("security-audit-finding", index))
        .w_full()
        .gap_2()
        // Indented under its path's row.
        .pl_6()
        .pr_2()
        .py_0p5()
        .text_size(tokens::text_xs())
        .hover(|this| tokens::hover_fx(this).bg(tokens::hover()))
        .on_click(cx.listener(move |shell, _, window, cx| match line {
            Some(line) => {
                let pos = lsp_types::Position::new(line as u32 - 1, 0);
                shell.reveal(instance, lsp_types::Range::new(pos, pos), window, cx);
            }
            None => shell.select(instance, cx),
        }))
        .child(Icon::new(icon).small().text_color(color))
        .children(line.map(|line| {
            div()
                .flex_none()
                .text_color(tokens::text3())
                .child(format!("Ln {line}"))
        }))
        .child(
            div()
                .flex_none()
                .text_color(tokens::text_strong())
                .child(finding.check.title()),
        )
        .child(
            div()
                .flex_1()
                .text_color(tokens::text())
                .child(finding.message.clone()),
        )
}

/// What the header says: the counts that are not zero, or that all is well.
fn summary(high: usize, medium: usize, low: usize) -> String {
    let parts: Vec<String> = [(high, "high"), (medium, "medium"), (low, "low")]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, label)| format!("{n} {label}"))
        .collect();
    if parts.is_empty() {
        return "No problems found".into();
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::summary;

    #[test]
    fn the_summary_names_only_the_severities_there_are() {
        assert_eq!(summary(0, 0, 0), "No problems found");
        assert_eq!(summary(2, 0, 1), "2 high, 1 low");
        assert_eq!(summary(1, 3, 4), "1 high, 3 medium, 4 low");
    }
}
