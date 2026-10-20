//! Recursive-descent parser from tokens to the [`Ast`] the evaluator walks.

use std::ops::Range;

use super::lex::{lex, Kind, Op, Token};
use super::QueryError;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Selector {
    /// `None` is `*`.
    pub(super) class: Option<String>,
    pub(super) span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Under {
    pub(super) path: String,
    pub(super) span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Value {
    Num(f64),
    Str(String),
    Bool(bool),
    /// A bare word: a string against a string property, an enum item name
    /// against an enum one.
    Word(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Target {
    Property(Vec<String>),
    Attribute(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Expr {
    Or(Vec<Expr>),
    And(Vec<Expr>),
    Not(Box<Expr>),
    HasTag(String),
    HasAttribute(String),
    Test {
        target: Target,
        /// The operator, the value and the value's span.
        compare: Option<(Op, Value, Range<usize>)>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Ast {
    pub(super) selector: Option<Selector>,
    pub(super) filter: Option<Expr>,
    pub(super) under: Option<Under>,
}

const KEYWORDS: [&str; 10] = [
    "where", "under", "and", "or", "not", "has", "tag", "attr", "contains", "true",
];

fn is_keyword(word: &str) -> bool {
    word.eq_ignore_ascii_case("false") || KEYWORDS.iter().any(|k| word.eq_ignore_ascii_case(k))
}

fn error(message: impl Into<String>, span: Range<usize>) -> QueryError {
    QueryError {
        message: message.into(),
        span,
    }
}

pub(super) fn parse_query(source: &str) -> Result<Ast, QueryError> {
    let tokens = lex(source)?;
    if tokens.is_empty() {
        return Err(error("type a query", 0..0));
    }
    let mut parser = Parser {
        source,
        tokens,
        pos: 0,
    };
    parser.query()
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser<'_> {
    fn end(&self) -> Range<usize> {
        self.source.len()..self.source.len()
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn word_at(&self, offset: usize, word: &str) -> bool {
        matches!(
            self.tokens.get(self.pos + offset),
            Some(Token { kind: Kind::Ident(w), .. }) if w.eq_ignore_ascii_case(word)
        )
    }

    fn at_word(&self, word: &str) -> bool {
        self.word_at(0, word)
    }

    fn query(&mut self) -> Result<Ast, QueryError> {
        let mut selector = None;
        let mut filter = None;
        if self.at_word("where") {
            self.pos += 1;
            filter = Some(self.expr()?);
        } else if let Some(found) = self.selector() {
            selector = Some(found);
            if self.at_word("where") {
                self.pos += 1;
                filter = Some(self.expr()?);
            }
        } else if !self.at_word("under") {
            filter = Some(self.expr()?);
        }
        let under = if self.at_word("under") {
            Some(self.under()?)
        } else {
            None
        };
        if let Some(extra) = self.peek() {
            let shown = &self.source[extra.span.clone()];
            return Err(error(
                format!("unexpected \"{shown}\" after the query"),
                extra.span.clone(),
            ));
        }
        Ok(Ast {
            selector,
            filter,
            under,
        })
    }

    /// A leading word is a class only when `where`, `under` or the end follows
    /// it; otherwise it starts a condition.
    fn selector(&mut self) -> Option<Selector> {
        let token = self.peek()?;
        let class = match &token.kind {
            Kind::Star => None,
            Kind::Ident(name) if !is_keyword(name) => Some(name.clone()),
            _ => return None,
        };
        let span = token.span.clone();
        if self.pos + 1 < self.tokens.len()
            && !self.word_at(1, "where")
            && !self.word_at(1, "under")
        {
            return None;
        }
        self.pos += 1;
        Some(Selector { class, span })
    }

    fn under(&mut self) -> Result<Under, QueryError> {
        self.pos += 1;
        let Some(Token {
            kind: Kind::Ident(first),
            span,
        }) = self.peek().cloned()
        else {
            let span = self.peek().map_or(self.end(), |t| t.span.clone());
            return Err(error("expected a path after \"under\"", span));
        };
        self.pos += 1;
        let (segments, end) = self.dotted(first, span.end)?;
        Ok(Under {
            path: segments.join("."),
            span: span.start..end,
        })
    }

    /// The rest of a dotted name whose first segment is already consumed, and
    /// the byte offset where it ends.
    fn dotted(
        &mut self,
        first: String,
        mut end: usize,
    ) -> Result<(Vec<String>, usize), QueryError> {
        let mut segments = vec![first];
        while let Some(Token {
            kind: Kind::Dot,
            span: dot,
        }) = self.peek().cloned()
        {
            self.pos += 1;
            match self.peek().cloned() {
                Some(Token {
                    kind: Kind::Ident(name),
                    span,
                }) => {
                    self.pos += 1;
                    end = span.end;
                    segments.push(name);
                }
                _ => return Err(error("expected a name after \".\"", dot)),
            }
        }
        Ok((segments, end))
    }

    fn expr(&mut self) -> Result<Expr, QueryError> {
        let mut parts = vec![self.and()?];
        while self.at_word("or") {
            self.pos += 1;
            parts.push(self.and()?);
        }
        Ok(if parts.len() == 1 {
            parts.swap_remove(0)
        } else {
            Expr::Or(parts)
        })
    }

    fn and(&mut self) -> Result<Expr, QueryError> {
        let mut parts = vec![self.unary()?];
        while self.at_word("and") {
            self.pos += 1;
            parts.push(self.unary()?);
        }
        Ok(if parts.len() == 1 {
            parts.swap_remove(0)
        } else {
            Expr::And(parts)
        })
    }

    fn unary(&mut self) -> Result<Expr, QueryError> {
        if self.at_word("not") {
            self.pos += 1;
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Expr, QueryError> {
        let Some(token) = self.peek().cloned() else {
            return Err(error("expected a condition", self.end()));
        };
        let Kind::Ident(word) = &token.kind else {
            if token.kind == Kind::Open {
                self.pos += 1;
                let inner = self.expr()?;
                return if matches!(
                    self.peek(),
                    Some(Token {
                        kind: Kind::Close,
                        ..
                    })
                ) {
                    self.pos += 1;
                    Ok(inner)
                } else {
                    Err(error("expected \")\" to close this \"(\"", token.span))
                };
            }
            return Err(error("expected a condition", token.span));
        };
        if word.eq_ignore_ascii_case("has") {
            self.pos += 1;
            let tag = self.at_word("tag");
            if !tag && !self.at_word("attr") {
                return Err(error(
                    "\"has\" must be followed by \"tag\" or \"attr\"",
                    token.span,
                ));
            }
            self.pos += 1;
            let name = self.name(if tag { "tag" } else { "attribute" })?;
            return Ok(if tag {
                Expr::HasTag(name)
            } else {
                Expr::HasAttribute(name)
            });
        }
        if word.eq_ignore_ascii_case("attr") {
            self.pos += 1;
            let name = self.name("attribute")?;
            let compare = self.compare()?;
            return Ok(Expr::Test {
                target: Target::Attribute(name),
                compare,
            });
        }
        if is_keyword(word) {
            return Err(error("expected a condition", token.span));
        }
        self.pos += 1;
        let (path, _) = self.dotted(word.clone(), token.span.end)?;
        let compare = self.compare()?;
        Ok(Expr::Test {
            target: Target::Property(path),
            compare,
        })
    }

    fn name(&mut self, what: &str) -> Result<String, QueryError> {
        match self.peek().cloned() {
            Some(Token {
                kind: Kind::Str(name) | Kind::Ident(name),
                ..
            }) => {
                self.pos += 1;
                Ok(name)
            }
            other => Err(error(
                format!("expected a {what} name"),
                other.map_or(self.end(), |t| t.span),
            )),
        }
    }

    fn compare(&mut self) -> Result<Option<(Op, Value, Range<usize>)>, QueryError> {
        let Some(token) = self.peek().cloned() else {
            return Ok(None);
        };
        let op = match &token.kind {
            Kind::Op(op) => *op,
            Kind::Ident(word) if word.eq_ignore_ascii_case("contains") => Op::Contains,
            _ => return Ok(None),
        };
        self.pos += 1;
        let start = self.peek().map_or(self.end(), |t| t.span.clone()).start;
        match self.value() {
            Some(value) => Ok(Some((op, value, start..self.tokens[self.pos - 1].span.end))),
            None => {
                let shown = &self.source[token.span.clone()];
                Err(error(
                    format!("expected a value after \"{shown}\""),
                    token.span,
                ))
            }
        }
    }

    fn value(&mut self) -> Option<Value> {
        let token = self.peek()?.clone();
        match token.kind {
            Kind::Number(n) => {
                self.pos += 1;
                Some(Value::Num(n))
            }
            Kind::Minus => match self.tokens.get(self.pos + 1) {
                Some(Token {
                    kind: Kind::Number(n),
                    ..
                }) => {
                    let n = *n;
                    self.pos += 2;
                    Some(Value::Num(-n))
                }
                _ => None,
            },
            Kind::Str(text) => {
                self.pos += 1;
                Some(Value::Str(text))
            }
            Kind::Ident(word) if word.eq_ignore_ascii_case("true") => {
                self.pos += 1;
                Some(Value::Bool(true))
            }
            Kind::Ident(word) if word.eq_ignore_ascii_case("false") => {
                self.pos += 1;
                Some(Value::Bool(false))
            }
            Kind::Ident(word) if !is_keyword(&word) => {
                self.pos += 1;
                let mut last = word;
                while matches!(
                    self.peek(),
                    Some(Token {
                        kind: Kind::Dot,
                        ..
                    })
                ) {
                    match self.tokens.get(self.pos + 1) {
                        Some(Token {
                            kind: Kind::Ident(next),
                            ..
                        }) => {
                            last = next.clone();
                            self.pos += 2;
                        }
                        _ => break,
                    }
                }
                Some(Value::Word(last))
            }
            _ => None,
        }
    }
}
