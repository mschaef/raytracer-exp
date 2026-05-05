// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Runtime values.
//!
//! Phase 1 had no host-type variants. Phase 2 adds them: the ray
//! tracer's `Surface`, `Light`, `Camera`, `Affine`, `AABB`, `Shape`,
//! and `Scene` types are now first-class SDL values, so scripts can
//! build scenes that the renderer can later consume. The shape of
//! [`Value`] is otherwise unchanged: a flat enum, with collection
//! variants holding `Rc` so that aliasing is free.
//!
//! Equality semantics:
//! - Numbers compare numerically; `Int(1) == Float(1.0)` is true.
//!   This matches Clojure and is convenient for tests.
//! - Strings compare by content.
//! - Keywords and symbols compare by name.
//! - Vecs compare elementwise; maps compare by key/value sets.
//! - Functions compare by `Rc` identity (pointer equality).
//! - Host-type values compare structurally (same field values),
//!   reusing the `PartialEq` derives on the host structs.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::render::{Camera, Light, Scene, Surface};
use crate::render::shapes::{AABB, Shape};
use crate::render::transform::Affine;
use crate::sdl::ast::Form;
use crate::sdl::env::EnvRef;
use crate::sdl::error::Position;

/// A runtime value.
#[derive(Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Rc<String>),
    Keyword(Rc<String>),
    Symbol(Rc<String>),
    Vec(Rc<Vec<Value>>),
    /// Map keys are stored as plain `String` (the keyword name without
    /// the leading `:`). This restricts keys to keywords — broader
    /// keys are deferred per the Phase 1 design decision in CLAUDE.md.
    Map(Rc<HashMap<String, Value>>),
    Fn(Rc<Function>),

    // ---------------- Phase 2 host types ----------------
    //
    // Inlined for the small Copy types so the Value enum stays cheap
    // to clone; Rc-wrapped for the larger ones (Light, Shape, Scene)
    // so that sharing is free and binding-side extraction can deep-clone
    // through the Rc when ownership is needed (host constructors take
    // owned values).
    Surface(Surface),
    Camera(Camera),
    Affine(Affine),
    Aabb(AABB),
    Light(Rc<Light>),
    Shape(Rc<Shape>),
    Scene(Rc<Scene>),
}

impl Value {
    /// Short human-readable type tag, used in error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::Keyword(_) => "keyword",
            Value::Symbol(_) => "symbol",
            Value::Vec(_) => "vector",
            Value::Map(_) => "map",
            Value::Fn(_) => "fn",
            Value::Surface(_) => "surface",
            Value::Camera(_) => "camera",
            Value::Affine(_) => "affine",
            Value::Aabb(_) => "aabb",
            Value::Light(_) => "light",
            Value::Shape(_) => "shape",
            Value::Scene(_) => "scene",
        }
    }

    /// Truthiness: only `nil` and `false` are falsy. Everything else
    /// (including `0`, `0.0`, empty vec, empty string) is truthy.
    /// Matches Clojure.
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }

    /// Coerce numeric values to `f64`. Returns `None` for non-numeric
    /// values. Used by arithmetic and comparison built-ins after
    /// promotion.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// True iff this value is `Int` or `Float`.
    pub fn is_number(&self) -> bool {
        matches!(self, Value::Int(_) | Value::Float(_))
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            // Cross-type numeric comparison via promotion.
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
            (Value::String(a), Value::String(b)) => **a == **b,
            (Value::Keyword(a), Value::Keyword(b)) => **a == **b,
            (Value::Symbol(a), Value::Symbol(b)) => **a == **b,
            (Value::Vec(a), Value::Vec(b)) => **a == **b,
            (Value::Map(a), Value::Map(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                for (k, v) in a.iter() {
                    match b.get(k) {
                        Some(bv) if bv == v => {}
                        _ => return false,
                    }
                }
                true
            }
            (Value::Fn(a), Value::Fn(b)) => Rc::ptr_eq(a, b),

            // Host-type structural equality. The Copy variants compare
            // by their fields (each host struct derives PartialEq);
            // the Rc-wrapped variants short-circuit on pointer equality
            // and otherwise fall through to derived field comparison.
            (Value::Surface(a), Value::Surface(b)) => a == b,
            (Value::Camera(a), Value::Camera(b)) => a == b,
            (Value::Affine(a), Value::Affine(b)) => a == b,
            (Value::Aabb(a), Value::Aabb(b)) => a == b,
            (Value::Light(a), Value::Light(b)) => Rc::ptr_eq(a, b) || **a == **b,
            (Value::Shape(a), Value::Shape(b)) => Rc::ptr_eq(a, b) || **a == **b,
            (Value::Scene(a), Value::Scene(b)) => Rc::ptr_eq(a, b) || **a == **b,
            _ => false,
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Debug and Display produce the same output: a Lisp-y reader
        // representation. Useful for assertions and test snapshots.
        write!(f, "{}", self)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil => f.write_str("nil"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(i) => write!(f, "{}", i),
            Value::Float(x) => {
                // Always show a decimal point so floats are
                // distinguishable from ints in printed output.
                if x.is_finite() && x.fract() == 0.0 {
                    write!(f, "{:.1}", x)
                } else {
                    write!(f, "{}", x)
                }
            }
            Value::String(s) => {
                f.write_str("\"")?;
                for c in s.chars() {
                    match c {
                        '"' => f.write_str("\\\"")?,
                        '\\' => f.write_str("\\\\")?,
                        '\n' => f.write_str("\\n")?,
                        '\t' => f.write_str("\\t")?,
                        '\r' => f.write_str("\\r")?,
                        _ => write!(f, "{}", c)?,
                    }
                }
                f.write_str("\"")
            }
            Value::Keyword(k) => write!(f, ":{}", k),
            Value::Symbol(s) => write!(f, "{}", s),
            Value::Vec(items) => {
                f.write_str("[")?;
                let mut first = true;
                for item in items.iter() {
                    if !first {
                        f.write_str(" ")?;
                    }
                    first = false;
                    write!(f, "{}", item)?;
                }
                f.write_str("]")
            }
            Value::Map(entries) => {
                f.write_str("{")?;
                let mut first = true;
                // Sort keys for stable output — useful for assertions.
                let mut keys: Vec<&String> = entries.keys().collect();
                keys.sort();
                for k in keys {
                    if !first {
                        f.write_str(", ")?;
                    }
                    first = false;
                    write!(f, ":{} {}", k, entries.get(k).unwrap())?;
                }
                f.write_str("}")
            }
            Value::Fn(fun) => match &fun.kind {
                FunctionKind::Native { name, .. } => write!(f, "#<native-fn {}>", name),
                FunctionKind::Interpreted { name, .. } => match name {
                    Some(n) => write!(f, "#<fn {}>", n),
                    None => f.write_str("#<fn>"),
                },
            },

            // Host-type summaries. These are opaque-ish for now —
            // detailed printing is deferred. Tests that need to compare
            // host values do so via `assert=` (structural equality) or
            // by introspection helpers added to the bindings module.
            Value::Surface(_) => f.write_str("#<surface>"),
            Value::Camera(_) => f.write_str("#<camera>"),
            Value::Affine(_) => f.write_str("#<affine>"),
            Value::Aabb(_) => f.write_str("#<aabb>"),
            Value::Light(_) => f.write_str("#<light>"),
            Value::Shape(s) => match &**s {
                Shape::Sphere(_)    => f.write_str("#<shape sphere>"),
                Shape::Plane(_)     => f.write_str("#<shape plane>"),
                Shape::Cuboid(_)    => f.write_str("#<shape cuboid>"),
                Shape::Triangle(_)  => f.write_str("#<shape triangle>"),
                Shape::Cylinder(_)  => f.write_str("#<shape cylinder>"),
                Shape::Group(_)     => f.write_str("#<shape group>"),
                Shape::Transform(_) => f.write_str("#<shape transform>"),
                Shape::Bounded(_)   => f.write_str("#<shape bounded>"),
            },
            Value::Scene(s) => write!(f, "#<scene {:?}>", s.name),
        }
    }
}

/// A callable. Wrapped in `Rc<Function>` inside `Value::Fn`.
pub struct Function {
    pub kind: FunctionKind,
}

pub enum FunctionKind {
    /// A function implemented in Rust. `name` is for debug output;
    /// `func` does its own arity / type checking and panics with the
    /// supplied call-site `Position` on misuse.
    Native {
        name: &'static str,
        func: NativeFn,
    },
    /// A user-defined function (`fn` form). Captures the enclosing
    /// environment by reference (downward closure).
    Interpreted {
        name: Option<String>,
        params: ParamList,
        body: Rc<Vec<Form>>,
        env: EnvRef,
    },
}

/// Function pointer for native built-ins. Takes the call-site
/// position so errors can point at the script.
pub type NativeFn = fn(args: &[Value], pos: &Position) -> Value;

/// A function's parameter list, with destructuring support already
/// resolved into a tree of [`ParamPattern`]s.
#[derive(Debug, Clone)]
pub struct ParamList {
    pub patterns: Vec<ParamPattern>,
    /// `& rest` capture, if any. Binds the remaining args as a vector.
    pub rest: Option<ParamPattern>,
}

/// One slot of a destructuring pattern.
///
/// Phase 1 supports vector destructuring only; map destructuring is
/// deferred. A pattern is either a plain symbol binding, an inner
/// vector pattern (recursive), or a wildcard.
#[derive(Debug, Clone)]
pub enum ParamPattern {
    /// Bind by name.
    Symbol(String),
    /// Recursive vector destructuring: pattern matches a value that
    /// must be a vector. Each inner pattern binds a positional slot;
    /// `as_binding` (from `:as name`) optionally binds the whole vec;
    /// `rest` (from `& name`) captures any remaining positional
    /// elements as a vec.
    Vector {
        elements: Vec<ParamPattern>,
        rest: Option<Box<ParamPattern>>,
        as_binding: Option<String>,
    },
    /// `_` — discards the value but consumes the slot.
    Wildcard,
}

/// A `recur` request: `apply` returns this from inside an interpreted
/// function body to signal the caller's loop to rebind and re-enter.
/// Lives inside `RefCell<Option<RecurSignal>>` on the eval thread.
pub struct RecurSignal {
    pub args: Vec<Value>,
    pub pos: Position,
}

thread_local! {
    /// The pending `recur` signal, if any. Set by the `recur` special
    /// form, consumed by the function-application loop.
    pub static RECUR: RefCell<Option<RecurSignal>> = RefCell::new(None);
}
