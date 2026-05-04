// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Reader output: AST nodes ([`Form`]) carrying source positions.
//!
//! The reader is the only thing that constructs `Form`s. The evaluator
//! consumes them but never builds new ones — at runtime everything
//! flows as [`Value`](crate::sdl::value::Value) instead.

use crate::sdl::error::Position;

/// A single AST node, paired with the source position it was read from.
#[derive(Debug, Clone)]
pub struct Form {
    pub kind: FormKind,
    pub pos: Position,
}

impl Form {
    pub fn new(kind: FormKind, pos: Position) -> Self {
        Form { kind, pos }
    }
}

/// The shape of an AST node.
///
/// `Map` carries pairs of forms rather than a `HashMap`; the evaluator
/// is responsible for evaluating each key/value pair and enforcing
/// that keys evaluate to keywords (the only allowed map-key type in
/// Phase 1). Carrying pairs in source order also keeps error messages
/// tied to the right key form.
#[derive(Debug, Clone)]
pub enum FormKind {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    /// Stored without the leading `:`. The reader strips it.
    Keyword(String),
    Symbol(String),
    /// `(a b c)` — function call or special form.
    List(Vec<Form>),
    /// `[a b c]` — vector literal.
    Vector(Vec<Form>),
    /// `{:a 1 :b 2}` — map literal. Stored as a vec of (key, value)
    /// pairs in source order.
    Map(Vec<(Form, Form)>),
}

impl FormKind {
    /// Short human-readable type tag, used in error messages.
    /// Mirrors the corresponding `Value::type_name`.
    pub fn type_name(&self) -> &'static str {
        match self {
            FormKind::Nil => "nil",
            FormKind::Bool(_) => "bool",
            FormKind::Int(_) => "int",
            FormKind::Float(_) => "float",
            FormKind::String(_) => "string",
            FormKind::Keyword(_) => "keyword",
            FormKind::Symbol(_) => "symbol",
            FormKind::List(_) => "list",
            FormKind::Vector(_) => "vector",
            FormKind::Map(_) => "map",
        }
    }
}
