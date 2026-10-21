//! The query language behind query-based selection: a small, pure language
//! over the `WeakDom` (`Part where Anchored = false under Workspace.Map`).
//!
//! It is split in three so each stage can be tested on its own: [`lex`]
//! turns text into spanned tokens, `parse` builds the syntax tree, `eval`
//! walks the DOM. Every error carries the byte span of the text it is about,
//! so the shell can underline it in the query box.
//!
//! Evaluation never mutates anything and never fails on an instance that
//! lacks a property: such a comparison is simply false.

mod check;
mod eval;
mod lex;
mod parse;

use std::ops::Range;

use rbx_dom::{Ref, WeakDom};
use rbx_reflection::ReflectionDatabase;

/// A mistake in a query, with the byte span of the text it is about.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct QueryError {
    pub(crate) message: String,
    pub(crate) span: Range<usize>,
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (column {})", self.message, self.span.start + 1)
    }
}

/// A parsed query, ready to run against any number of places.
#[derive(Debug, Clone)]
pub(crate) struct Query {
    ast: parse::Ast,
}

pub(crate) fn parse(source: &str) -> Result<Query, QueryError> {
    parse::parse_query(source).map(|ast| Query { ast })
}

impl Query {
    /// Every instance the query matches, depth-first in file order.
    pub(crate) fn select(
        &self,
        dom: &WeakDom,
        db: &ReflectionDatabase,
    ) -> Result<Vec<Ref>, QueryError> {
        eval::select(&self.ast, dom, db)
    }
}

#[cfg(test)]
#[path = "query/tests.rs"]
mod tests;
