//! The Explorer search field's query mode: the field takes a
//! `crate::query` query instead of a name, the tree narrows to what it
//! matches, and Enter (or the status line's Select) selects every match,
//! so the viewport outlines them and the Properties panel edits them all at
//! once. The Command Bar reaches the same selection through
//! `:select <query>`, which never starts a Luau run.

use gpui_kit::assets::IconName;
use gpui_kit::component::h_flex;
use gpui_kit::component::tree::TreeItem;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rbx_dom::Ref;

use crate::command_bar::Feedback;
use crate::query::{self, QueryError};
use crate::tokens;

use super::{chrome, Shell};

/// The Command Bar prefix that selects by query rather than running Luau. A
/// `:` can never start a Luau statement, so no script is shadowed by it.
pub(super) const SELECT_COMMAND: &str = ":select ";

#[derive(Default)]
pub(super) struct QuerySearch {
    enabled: bool,
    /// What the field's query last matched, or why it could not run; `None`
    /// while the mode is off or the field is empty.
    result: Option<Result<Vec<Ref>, QueryError>>,
}

impl Shell {
    /// Re-runs the field's query against the DOM as it is now: called before
    /// every push of the tree's rows, so an edit that changes what matches
    /// shows up in the tree and the count without retyping.
    pub(super) fn refresh_query(&mut self) {
        self.query.result = (self.query.enabled && !self.explorer_query.is_empty())
            .then(|| self.run_query(&self.explorer_query));
    }

    fn run_query(&self, source: &str) -> Result<Vec<Ref>, QueryError> {
        query::parse(source)?.select(&self.dom, &self.database)
    }

    /// The Explorer's rows narrowed by the field: by name normally, by what
    /// the query matched in query mode. A query that does not parse leaves
    /// the tree whole, with the error under the field, rather than blanking
    /// it on every half-typed keystroke.
    pub(super) fn filter_rows(&self, items: &[TreeItem]) -> Vec<TreeItem> {
        match &self.query.result {
            Some(Ok(matches)) => crate::explorer::search_refs(items, matches),
            Some(Err(_)) => items.to_vec(),
            None if self.query.enabled => items.to_vec(),
            None => crate::explorer::search(items, &self.explorer_query),
        }
    }

    fn toggle_query_mode(&mut self, cx: &mut Context<Self>) {
        self.query.enabled = !self.query.enabled;
        self.push_root_rows(cx);
    }

    /// Enter in the search field, or the status line's Select: selects
    /// every match, the first as the Explorer's row. Nothing happens
    /// outside query mode, where Enter has never done anything.
    pub(super) fn select_query_matches(&mut self, cx: &mut Context<Self>) {
        if let Some(Ok(matches)) = &self.query.result {
            if !matches.is_empty() {
                self.reselect(matches.clone(), cx);
            }
        }
    }

    /// `:select <query>` in the Command Bar: selects every match, reporting
    /// how many or the query's error the way a run's result is reported.
    pub(super) fn run_select_command(&mut self, source: &str, query: &str, cx: &mut Context<Self>) {
        self.command_bar.remember(source);
        let feedback = match self.run_query(query) {
            Ok(matches) => {
                let message = format!("Selected {}", count(matches.len()));
                self.reselect(matches, cx);
                Feedback::Output(message)
            }
            Err(error) => Feedback::Error(error.to_string()),
        };
        self.output.push(source, feedback.clone());
        self.command_bar.set_feedback(feedback);
        cx.notify();
    }

    /// The toggle beside the search field, lit while query mode is on.
    pub(super) fn query_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let button = chrome::icon_button("explorer-query-mode", IconName::Code, "Query mode");
        match self.query.enabled {
            true => button
                .bg(tokens::accent_soft())
                .text_color(tokens::check_on()),
            false => button,
        }
        .on_click(cx.listener(|shell, _, _, cx| shell.toggle_query_mode(cx)))
    }

    /// The line under the field in query mode: the match count with a
    /// Select button, or the query's error and where it is.
    pub(super) fn query_status(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let line = h_flex()
            .gap(px(8.))
            .px(px(2.))
            .text_size(tokens::text_xs())
            .line_height(tokens::line_xs());
        let element = match self.query.result.as_ref()? {
            Ok(matches) => line
                .text_color(tokens::text_label())
                .child(div().flex_1().child(count(matches.len())))
                .when(!matches.is_empty(), |line| {
                    line.child(
                        chrome::button("explorer-query-select", "Select", false).on_click(
                            cx.listener(|shell, _, _, cx| shell.select_query_matches(cx)),
                        ),
                    )
                })
                .into_any_element(),
            Err(error) => line
                .text_color(tokens::text_error())
                .child(SharedString::from(error.to_string()))
                .into_any_element(),
        };
        Some(element)
    }
}

fn count(matches: usize) -> String {
    match matches {
        1 => "1 match".to_owned(),
        n => format!("{n} matches"),
    }
}
