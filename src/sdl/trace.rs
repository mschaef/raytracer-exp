// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! The SDL call stack, for reporting where a script error happened.
//!
//! Every function call ([`crate::sdl::eval::apply`]) and every
//! `(load ...)` pushes a [`Frame`] for as long as it runs; a guard pops
//! it on return or unwind. When an SDL error panics inside
//! [`crate::sdl::catch_errors`], the panic hook copies the stack with
//! [`snapshot`] before unwinding pops it, and the error report prints
//! it innermost first.

use std::cell::RefCell;
use std::rc::Rc;

use crate::sdl::error::Position;
use crate::sdl::value::{Function, FunctionKind};

/// What a frame is running.
#[derive(Clone)]
enum FrameKind {
    Call(Rc<Function>),
    Load(Rc<String>),
}

/// One entry on the call stack: what's running and where it was
/// called (or loaded) from.
#[derive(Clone)]
pub struct Frame {
    kind: FrameKind,
    pos: Position,
}

thread_local! {
    static STACK: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

/// Pops its frame when dropped, on return or on unwind.
pub struct FrameGuard(());

impl Drop for FrameGuard {
    fn drop(&mut self) {
        STACK.with(|s| {
            s.borrow_mut().pop();
        });
    }
}

fn push(kind: FrameKind, pos: &Position) -> FrameGuard {
    STACK.with(|s| s.borrow_mut().push(Frame { kind, pos: pos.clone() }));
    FrameGuard(())
}

/// Record a call to `func` from `call_pos` until the guard drops.
pub fn enter_call(func: &Rc<Function>, call_pos: &Position) -> FrameGuard {
    push(FrameKind::Call(func.clone()), call_pos)
}

/// Record a `(load path)` at `pos` until the guard drops.
pub fn enter_load(path: &str, pos: &Position) -> FrameGuard {
    push(FrameKind::Load(Rc::new(path.to_string())), pos)
}

/// How many lines the report shows at most: the innermost and
/// outermost halves, with a count of the ones left out between.
const MAX_SHOWN: usize = 20;

/// The current stack as report lines, innermost first. Empty when
/// nothing is running (or the stack is busy, which can only happen if
/// a push or pop itself panicked).
///
/// - A native function at the top is left out: it's the one that
///   raised the error, at the position the message already gives.
/// - Runs of identical frames (plain recursion) become one line with a
///   count.
pub fn snapshot() -> Vec<String> {
    STACK.with(|s| match s.try_borrow() {
        Ok(stack) => report_lines(&stack),
        Err(_) => Vec::new(),
    })
}

fn is_native_call(frame: &Frame) -> bool {
    matches!(&frame.kind, FrameKind::Call(f) if matches!(f.kind, FunctionKind::Native { .. }))
}

fn describe(frame: &Frame) -> String {
    match &frame.kind {
        FrameKind::Call(func) => {
            let what = match &func.kind {
                FunctionKind::Native { name, .. } => name.to_string(),
                FunctionKind::Interpreted { name: Some(n), .. } => n.clone(),
                FunctionKind::Interpreted { name: None, pos, .. } => format!("fn defined at {}", pos),
            };
            format!("in {}, called at {}", what, frame.pos)
        }
        FrameKind::Load(path) => format!("in (load {:?}) at {}", path, frame.pos),
    }
}

fn report_lines(stack: &[Frame]) -> Vec<String> {
    let mut frames: Vec<&Frame> = stack.iter().rev().collect();
    if frames.first().map_or(false, |f| is_native_call(f)) {
        frames.remove(0);
    }

    // Collapse runs of identical lines.
    let mut lines: Vec<String> = Vec::new();
    let mut i = 0;
    while i < frames.len() {
        let line = describe(frames[i]);
        let mut run = 1;
        while i + run < frames.len() && describe(frames[i + run]) == line {
            run += 1;
        }
        lines.push(if run > 1 { format!("{} ({} times)", line, run) } else { line });
        i += run;
    }

    let n = lines.len();
    if n <= MAX_SHOWN {
        return lines;
    }
    let half = MAX_SHOWN / 2;
    let mut shown: Vec<String> = lines[..half].to_vec();
    shown.push(format!("... {} more ...", n - MAX_SHOWN));
    shown.extend_from_slice(&lines[n - half..]);
    shown
}
