// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Source positions and error reporting.
//!
//! Every AST node carries a [`Position`] tagging where it came from.
//! Runtime errors panic with an [`SdlError`] formatted to include the
//! position so failures are debuggable. The test harness catches the
//! panic and reports the failing file.

use std::fmt;
use std::rc::Rc;

/// A 1-indexed line/column position within a named source.
///
/// `file` is shared via `Rc` so that AST nodes (and any error spawned
/// from them) can clone positions cheaply.
#[derive(Debug, Clone)]
pub struct Position {
    pub file: Rc<String>,
    pub line: usize,
    pub col: usize,
}

impl Position {
    pub fn new(file: Rc<String>, line: usize, col: usize) -> Self {
        Position { file, line, col }
    }

    /// A synthesized position used for values that didn't come from
    /// any specific source location (e.g. results of computation).
    /// Only ever appears in error messages when a value is misused
    /// far from its construction site; the message will say
    /// `<synthetic>:0:0` which is acceptable noise.
    pub fn synthetic() -> Self {
        Position {
            file: Rc::new("<synthetic>".to_string()),
            line: 0,
            col: 0,
        }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.col)
    }
}

/// All SDL errors share the same shape: a message, a position, and an
/// optional "phase" tag (read / eval) for downstream tooling.
#[derive(Debug, Clone)]
pub struct SdlError {
    pub message: String,
    pub pos: Position,
    pub phase: ErrorPhase,
}

#[derive(Debug, Clone, Copy)]
pub enum ErrorPhase {
    Read,
    Eval,
}

impl SdlError {
    pub fn read(pos: Position, message: impl Into<String>) -> Self {
        SdlError {
            message: message.into(),
            pos,
            phase: ErrorPhase::Read,
        }
    }

    pub fn eval(pos: Position, message: impl Into<String>) -> Self {
        SdlError {
            message: message.into(),
            pos,
            phase: ErrorPhase::Eval,
        }
    }
}

impl fmt::Display for SdlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let phase = match self.phase {
            ErrorPhase::Read => "read error",
            ErrorPhase::Eval => "eval error",
        };
        write!(f, "{} at {}: {}", phase, self.pos, self.message)
    }
}

/// Convenience for "panic with a formatted SDL error" — the error
/// `Display` impl produces the user-facing message.
///
/// Callers should prefer [`SdlError::eval`] / [`SdlError::read`]
/// followed by `panic!("{}", err)` so the panic payload is a
/// formatted string the test harness can match against.
#[macro_export]
macro_rules! sdl_panic {
    ($pos:expr, $($arg:tt)*) => {
        panic!("{}", $crate::sdl::error::SdlError::eval($pos.clone(), format!($($arg)*)))
    };
}

/// Same as `sdl_panic!` but tagged as a read-phase error.
#[macro_export]
macro_rules! sdl_read_panic {
    ($pos:expr, $($arg:tt)*) => {
        panic!("{}", $crate::sdl::error::SdlError::read($pos.clone(), format!($($arg)*)))
    };
}
