// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Scene definition language: a small Clojure-subset Lisp.
//!
//! Phase 1 — language core only. No host bindings to the renderer yet;
//! that's Phase 2. See `CLAUDE.md` for the full implementation plan.
//!
//! Public entry points:
//!
//! - [`read_and_eval`] — parse a source string and evaluate it in a
//!   fresh interpreter, returning the value of the final form.
//! - [`default_env`] — a fresh environment populated with the Phase 1
//!   built-in functions and special-form bindings the evaluator needs.
//!
//! Errors are surfaced via panic with a source position, matching the
//! rest of this codebase. The test harness in `tests/sdl_suite.rs`
//! catches these via `std::panic::catch_unwind`.

pub mod ast;
pub mod builtins;
pub mod env;
pub mod error;
pub mod eval;
pub mod reader;
pub mod value;

pub use env::{EnvRef, Environment};
pub use error::{Position, SdlError};
pub use value::Value;

/// Build a fresh environment populated with all Phase 1 built-ins.
pub fn default_env() -> EnvRef {
    let env = Environment::new_root();
    builtins::install(&env);
    env
}

/// Read a source string and evaluate every top-level form in order,
/// returning the value of the last form (or `Value::Nil` if there were
/// no forms). `filename` is used only for error messages.
pub fn read_and_eval(source: &str, filename: &str) -> Value {
    let env = default_env();
    eval_source(source, filename, &env)
}

/// Like [`read_and_eval`] but takes a caller-supplied environment so
/// the test harness can populate extra bindings before evaluation.
pub fn eval_source(source: &str, filename: &str, env: &EnvRef) -> Value {
    let forms = reader::read_all(source, filename);
    let mut last = Value::Nil;
    for form in &forms {
        last = eval::eval(form, env);
    }
    last
}
