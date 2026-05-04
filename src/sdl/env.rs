// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Lexical environments.
//!
//! An [`Environment`] is a single frame: a map of bindings and an
//! optional parent. Lookups walk the parent chain until a name is
//! found. Defines insert into the *current* frame; they don't shadow
//! the parent silently — that's what `let` and `fn` parameter binding
//! do via [`Environment::new_child`].
//!
//! Closures capture their defining environment by `Rc<RefCell<...>>`
//! handle, so a function that escapes its enclosing scope still has
//! a live reference. Cycles are not possible in Phase 1: there's no
//! way for an interpreted function's environment to contain a
//! reference back to the function before the `def` is in place, and
//! we don't have mutation of bindings (no `set!`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::sdl::value::Value;

pub type EnvRef = Rc<RefCell<Environment>>;

pub struct Environment {
    parent: Option<EnvRef>,
    bindings: HashMap<String, Value>,
}

impl Environment {
    /// Create a fresh root environment with no parent and no bindings.
    pub fn new_root() -> EnvRef {
        Rc::new(RefCell::new(Environment {
            parent: None,
            bindings: HashMap::new(),
        }))
    }

    /// Create a child environment chained to `parent`.
    pub fn new_child(parent: &EnvRef) -> EnvRef {
        Rc::new(RefCell::new(Environment {
            parent: Some(parent.clone()),
            bindings: HashMap::new(),
        }))
    }

    /// Bind `name` to `value` in *this* frame, shadowing any
    /// parent-scope binding with the same name.
    pub fn define(&mut self, name: impl Into<String>, value: Value) {
        self.bindings.insert(name.into(), value);
    }

    /// Look up `name`, walking the parent chain. Returns `None` if
    /// unbound at every level.
    pub fn lookup(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.bindings.get(name) {
            return Some(v.clone());
        }
        match &self.parent {
            Some(p) => p.borrow().lookup(name),
            None => None,
        }
    }

    /// True iff this exact frame (not parents) contains `name`.
    /// Used by `let` to detect duplicate names within one binding form.
    pub fn has_local(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }
}
