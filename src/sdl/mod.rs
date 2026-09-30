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
//! Phase 1 covers the language core (literals, special forms, vector
//! and map ops, recur, destructuring). Phase 2 adds host bindings for
//! scene construction: script-callable constructors for the ray
//! tracer's `Surface`, `Light`, `Camera`, `Shape`, and `Scene` types.
//! Phase 3 adds render dispatch — render-target constructors
//! (`png-target`, `offset-target`, `progress-target`), `(render ...)`,
//! and `(save-png ...)` — so a script can drive an end-to-end render
//! to disk without touching Rust. Phase 4 adds in-language ergonomics:
//! control-flow forms (`cond`, `when`, `when-not`, `->`, `->>`, now
//! implemented in the desugaring pass),
//! HOFs (`map`, `filter`, `reduce`, `range`, `repeat`, `apply`), and
//! a small lisp standard library (`stdlib.lisp`) with math constants,
//! angle conversions, and point helpers. See `CLAUDE.md` for the full
//! phased plan.
//!
//! Every top-level form passes through [`desugar::desugar`] before it
//! reaches the evaluator, which rewrites syntactic sugar (`defn`,
//! `when`, `when-not`, `cond`, `->`, `->>`) into core special forms.
//!
//! Public entry points:
//!
//! - [`read_and_eval`] — parse a source string and evaluate it in a
//!   fresh interpreter, returning the value of the final form.
//! - [`default_env`] — a fresh environment populated with the language
//!   built-ins (Phase 1) and host bindings (Phase 2).
//!
//! Errors are surfaced via panic with a source position, matching the
//! rest of this codebase. The test harness in `tests/sdl_suite.rs`
//! catches these via `std::panic::catch_unwind`; the binaries use
//! [`catch_errors`], which turns them into a one-line message.

pub mod ast;
pub mod bindings;
pub mod builtins;
pub mod desugar;
pub mod env;
pub mod error;
pub mod eval;
pub mod reader;
pub mod target;
pub mod value;

pub use env::{EnvRef, Environment};
pub use error::{Position, SdlError};
pub use value::Value;

use std::cell::{Cell, RefCell};
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::Once;

thread_local! {
    /// Directory of the file currently being evaluated. Set by
    /// [`eval_source`] via [`CurrentDirGuard`] for the duration of a
    /// source's evaluation, and consulted by the `(load ...)` special
    /// form to resolve relative paths against the loading file's
    /// directory rather than the process CWD. `None` while no source
    /// is being evaluated, or when the current source has no useful
    /// base directory (e.g. inline strings or bare filenames).
    pub(crate) static CURRENT_DIR: RefCell<Option<PathBuf>> =
        const { RefCell::new(None) };
}

/// RAII guard that points [`CURRENT_DIR`] at the parent directory of
/// `filename` for its lifetime, then restores the previous value on
/// drop. Drop runs on normal return *and* on panic-unwind, so a test
/// that triggers an SDL panic doesn't leak directory state across
/// thread reuse in cargo's parallel test runner.
///
/// The guard stacks naturally for nested `(load ...)`: each load
/// constructs a new guard before recursing into `eval_source`, and the
/// guard's `prev` field captures the outer file's directory so it gets
/// restored when the loaded file finishes.
pub(crate) struct CurrentDirGuard {
    prev: Option<PathBuf>,
}

impl CurrentDirGuard {
    /// Push `filename`'s parent directory onto the thread-local. An
    /// empty or missing parent (bare filename like "foo.lisp") leaves
    /// `CURRENT_DIR` as `None` rather than `""` — there's no useful
    /// anchor for relative loads in that case, and falling through to
    /// CWD is the least-surprising default.
    pub fn enter(filename: &str) -> Self {
        let path = Path::new(filename);
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_path_buf());
        let prev = CURRENT_DIR.with(|c| c.replace(dir));
        CurrentDirGuard { prev }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        CURRENT_DIR.with(|c| *c.borrow_mut() = self.prev.take());
    }
}

thread_local! {
    /// How many [`catch_errors`] calls are active on this thread.
    /// While it's non-zero the panic hook stays quiet about SDL errors,
    /// because the caller is about to report them itself.
    static CATCHING: Cell<usize> = const { Cell::new(0) };
}

/// Text of a panic payload, if it has any. `panic!("{}", ..)` gives a
/// `String`; `panic!("literal")` gives a `&'static str`.
fn payload_text(payload: &(dyn std::any::Any + Send)) -> Option<&str> {
    payload
        .downcast_ref::<String>()
        .map(|s| s.as_str())
        .or_else(|| payload.downcast_ref::<&'static str>().copied())
}

/// Run `f`, turning an SDL error (a mistake in a script) into
/// `Err(message)`, where the message is the position-tagged text
/// `sdl_panic!` built, e.g.
/// `eval error at scenes/foo.lisp:12:5: + expected an integer, ...`.
///
/// Only SDL errors are caught. Any other panic is a bug in the
/// renderer, so it keeps the default report (location and backtrace)
/// and carries on unwinding.
///
/// A plain `catch_unwind` isn't enough: the panic hook prints the
/// "thread 'main' panicked at ..." header and backtrace *before* the
/// unwind reaches the catch. So this installs, once per process, a
/// hook that skips SDL errors on threads inside `catch_errors` and
/// hands everything else to the hook that was there before.
pub fn catch_errors<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    static INSTALL_HOOK: Once = Once::new();
    INSTALL_HOOK.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let quiet = CATCHING.with(|c| c.get()) > 0
                && payload_text(info.payload()).map_or(false, error::is_sdl_error_message);
            if !quiet {
                previous(info);
            }
        }));
    });

    // Decrement on the way out, whether `f` returns or unwinds.
    struct Catching;
    impl Drop for Catching {
        fn drop(&mut self) {
            CATCHING.with(|c| c.set(c.get() - 1));
        }
    }
    CATCHING.with(|c| c.set(c.get() + 1));
    let result = {
        let _catching = Catching;
        panic::catch_unwind(panic::AssertUnwindSafe(f))
    };

    result.or_else(|payload| match payload_text(&*payload) {
        Some(text) if error::is_sdl_error_message(text) => Err(text.to_string()),
        _ => panic::resume_unwind(payload),
    })
}

/// In-language standard library. Compiled into the binary so every
/// fresh interpreter starts with the same set of conveniences. Loaded
/// after Rust built-ins and host bindings so the lisp definitions can
/// reference everything below them.
const STDLIB_SOURCE: &str = include_str!("stdlib.lisp");

/// Build a fresh environment populated with the language built-ins,
/// the host-type bindings, and the in-language standard library.
pub fn default_env() -> EnvRef {
    let env = Environment::new_root();
    builtins::install(&env);
    bindings::install(&env);
    eval_source(STDLIB_SOURCE, "stdlib.lisp", &env);
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
///
/// Sets [`CURRENT_DIR`] to the directory of `filename` for the
/// duration of evaluation via [`CurrentDirGuard`], so any `(load ...)`
/// calls inside the source resolve relative paths against the
/// loading file rather than the process CWD. Pass an absolute path as
/// `filename` when callers want `(load ...)` to work robustly; bare
/// filenames or relative paths fall back to CWD-relative resolution.
pub fn eval_source(source: &str, filename: &str, env: &EnvRef) -> Value {
    let _guard = CurrentDirGuard::enter(filename);
    let forms = reader::read_all(source, filename);
    let mut last = Value::Nil;
    for form in &forms {
        // Desugar one top-level form at a time, immediately before
        // evaluating it — the shape a future macro system needs, since
        // a macro defined by one form must be visible to the next.
        let expanded = desugar::desugar(form);
        last = eval::eval(&expanded, env);
    }
    last
}
