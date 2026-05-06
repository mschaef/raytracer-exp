// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Phase 1 built-in functions.
//!
//! Built-ins live in the language's namespace as plain `Value::Fn`
//! values; the evaluator's special-form dispatch runs first, so a
//! built-in named the same as a special form would be shadowed by
//! the special form. Names like `+`, `=`, `<`, etc. are not symbols
//! a user can confuse with anything else.
//!
//! Each function takes `(&[Value], &Position) -> Value` and panics
//! with the call-site position on bad arity or types.

use std::collections::HashMap;
use std::rc::Rc;

use crate::sdl::env::EnvRef;
use crate::sdl::error::Position;
use crate::sdl::eval;
use crate::sdl::value::{Function, FunctionKind, NativeFn, Value};
use crate::sdl_panic;

/// Install every Phase 1 built-in into `env`.
pub fn install(env: &EnvRef) {
    // Arithmetic.
    define_native(env, "+", builtin_add);
    define_native(env, "-", builtin_sub);
    define_native(env, "*", builtin_mul);
    define_native(env, "/", builtin_div);

    // Comparison.
    define_native(env, "=", builtin_eq);
    define_native(env, "not=", builtin_not_eq);
    define_native(env, "<", builtin_lt);
    define_native(env, ">", builtin_gt);
    define_native(env, "<=", builtin_le);
    define_native(env, ">=", builtin_ge);

    // Logic.
    define_native(env, "not", builtin_not);

    // Vector.
    define_native(env, "vector", builtin_vector);
    define_native(env, "vec", builtin_vector);
    define_native(env, "nth", builtin_nth);
    define_native(env, "count", builtin_count);
    define_native(env, "first", builtin_first);
    define_native(env, "rest", builtin_rest);
    define_native(env, "conj", builtin_conj);

    // Map.
    define_native(env, "hash-map", builtin_hash_map);
    define_native(env, "get", builtin_get);
    define_native(env, "assoc", builtin_assoc);
    define_native(env, "dissoc", builtin_dissoc);
    define_native(env, "keys", builtin_keys);
    define_native(env, "vals", builtin_vals);

    // Output / strings.
    define_native(env, "print", builtin_print);
    define_native(env, "println", builtin_println);
    define_native(env, "str", builtin_str);

    // Test assertions.
    define_native(env, "assert", builtin_assert);
    define_native(env, "assert=", builtin_assert_eq);

    // Type predicates.
    define_native(env, "nil?", builtin_nil_q);
    define_native(env, "boolean?", builtin_boolean_q);
    define_native(env, "int?", builtin_int_q);
    define_native(env, "float?", builtin_float_q);
    define_native(env, "number?", builtin_number_q);
    define_native(env, "string?", builtin_string_q);
    define_native(env, "keyword?", builtin_keyword_q);
    define_native(env, "symbol?", builtin_symbol_q);
    define_native(env, "vector?", builtin_vector_q);
    define_native(env, "map?", builtin_map_q);
    define_native(env, "fn?", builtin_fn_q);

    // Name/identity helpers (useful for testing).
    define_native(env, "name", builtin_name);

    // Phase 4 — higher-order functions.
    define_native(env, "map", builtin_map);
    define_native(env, "filter", builtin_filter);
    define_native(env, "reduce", builtin_reduce);
    define_native(env, "range", builtin_range);
    define_native(env, "repeat", builtin_repeat);
    define_native(env, "apply", builtin_apply);

    // Phase 4 — math helpers.
    define_native(env, "min", builtin_min);
    define_native(env, "max", builtin_max);
    define_native(env, "abs", builtin_abs);
    define_native(env, "sqrt", builtin_sqrt);
    define_native(env, "sin", builtin_sin);
    define_native(env, "cos", builtin_cos);
    define_native(env, "tan", builtin_tan);
}

fn define_native(env: &EnvRef, name: &'static str, func: NativeFn) {
    let f = Function {
        kind: FunctionKind::Native { name, func },
    };
    env.borrow_mut().define(name, Value::Fn(Rc::new(f)));
}

// ---------------------------------------------------------------------------
// Numeric helpers
// ---------------------------------------------------------------------------

fn require_number(v: &Value, fn_name: &str, pos: &Position) -> f64 {
    match v.as_f64() {
        Some(f) => f,
        None => sdl_panic!(
            pos.clone(),
            "{} expected a number, got {} ({})",
            fn_name,
            v,
            v.type_name()
        ),
    }
}

fn any_float(args: &[Value]) -> bool {
    args.iter().any(|v| matches!(v, Value::Float(_)))
}

fn require_int(v: &Value, fn_name: &str, pos: &Position) -> i64 {
    match v {
        Value::Int(i) => *i,
        other => sdl_panic!(
            pos.clone(),
            "{} expected an integer, got {} ({})",
            fn_name,
            other,
            other.type_name()
        ),
    }
}

// ---------------------------------------------------------------------------
// Arithmetic
// ---------------------------------------------------------------------------

fn builtin_add(args: &[Value], pos: &Position) -> Value {
    if args.is_empty() {
        return Value::Int(0);
    }
    if any_float(args) {
        let mut acc = 0.0;
        for a in args {
            acc += require_number(a, "+", pos);
        }
        Value::Float(acc)
    } else {
        let mut acc: i64 = 0;
        for a in args {
            acc = acc.wrapping_add(require_int(a, "+", pos));
        }
        Value::Int(acc)
    }
}

fn builtin_sub(args: &[Value], pos: &Position) -> Value {
    if args.is_empty() {
        sdl_panic!(pos.clone(), "- requires at least 1 argument");
    }
    if args.len() == 1 {
        return match &args[0] {
            Value::Int(i) => Value::Int(i.wrapping_neg()),
            Value::Float(f) => Value::Float(-*f),
            other => sdl_panic!(
                pos.clone(),
                "- expected a number, got {}",
                other.type_name()
            ),
        };
    }
    if any_float(args) {
        let mut acc = require_number(&args[0], "-", pos);
        for a in &args[1..] {
            acc -= require_number(a, "-", pos);
        }
        Value::Float(acc)
    } else {
        let mut acc = require_int(&args[0], "-", pos);
        for a in &args[1..] {
            acc = acc.wrapping_sub(require_int(a, "-", pos));
        }
        Value::Int(acc)
    }
}

fn builtin_mul(args: &[Value], pos: &Position) -> Value {
    if args.is_empty() {
        return Value::Int(1);
    }
    if any_float(args) {
        let mut acc = 1.0;
        for a in args {
            acc *= require_number(a, "*", pos);
        }
        Value::Float(acc)
    } else {
        let mut acc: i64 = 1;
        for a in args {
            acc = acc.wrapping_mul(require_int(a, "*", pos));
        }
        Value::Int(acc)
    }
}

fn builtin_div(args: &[Value], pos: &Position) -> Value {
    if args.is_empty() {
        sdl_panic!(pos.clone(), "/ requires at least 1 argument");
    }
    if args.len() == 1 {
        // Reciprocal — always float.
        let f = require_number(&args[0], "/", pos);
        if f == 0.0 {
            sdl_panic!(pos.clone(), "division by zero");
        }
        return Value::Float(1.0 / f);
    }
    // Multi-arg division: always promotes to float to avoid integer
    // truncation surprises in Phase 1. This matches Clojure's
    // behaviour of producing rationals (which we don't have) for
    // exact integer division — float is the closest practical
    // substitute.
    let mut acc = require_number(&args[0], "/", pos);
    for a in &args[1..] {
        let f = require_number(a, "/", pos);
        if f == 0.0 {
            sdl_panic!(pos.clone(), "division by zero");
        }
        acc /= f;
    }
    Value::Float(acc)
}

// ---------------------------------------------------------------------------
// Comparison
// ---------------------------------------------------------------------------

fn builtin_eq(args: &[Value], pos: &Position) -> Value {
    if args.is_empty() {
        sdl_panic!(pos.clone(), "= requires at least 1 argument");
    }
    let first = &args[0];
    for a in &args[1..] {
        if first != a {
            return Value::Bool(false);
        }
    }
    Value::Bool(true)
}

fn builtin_not_eq(args: &[Value], pos: &Position) -> Value {
    match builtin_eq(args, pos) {
        Value::Bool(b) => Value::Bool(!b),
        _ => unreachable!(),
    }
}

fn cmp_chain(args: &[Value], op_name: &str, pos: &Position, pred: fn(f64, f64) -> bool) -> Value {
    if args.is_empty() {
        sdl_panic!(pos.clone(), "{} requires at least 1 argument", op_name);
    }
    if args.len() == 1 {
        // Single-arg comparison is vacuously true (matches Clojure).
        let _ = require_number(&args[0], op_name, pos);
        return Value::Bool(true);
    }
    let mut prev = require_number(&args[0], op_name, pos);
    for a in &args[1..] {
        let curr = require_number(a, op_name, pos);
        if !pred(prev, curr) {
            return Value::Bool(false);
        }
        prev = curr;
    }
    Value::Bool(true)
}

fn builtin_lt(args: &[Value], pos: &Position) -> Value {
    cmp_chain(args, "<", pos, |a, b| a < b)
}
fn builtin_gt(args: &[Value], pos: &Position) -> Value {
    cmp_chain(args, ">", pos, |a, b| a > b)
}
fn builtin_le(args: &[Value], pos: &Position) -> Value {
    cmp_chain(args, "<=", pos, |a, b| a <= b)
}
fn builtin_ge(args: &[Value], pos: &Position) -> Value {
    cmp_chain(args, ">=", pos, |a, b| a >= b)
}

// ---------------------------------------------------------------------------
// Logic
// ---------------------------------------------------------------------------

fn builtin_not(args: &[Value], pos: &Position) -> Value {
    if args.len() != 1 {
        sdl_panic!(pos.clone(), "not takes exactly 1 argument");
    }
    Value::Bool(!args[0].is_truthy())
}

// ---------------------------------------------------------------------------
// Vectors
// ---------------------------------------------------------------------------

fn builtin_vector(args: &[Value], _pos: &Position) -> Value {
    Value::Vec(Rc::new(args.to_vec()))
}

fn builtin_nth(args: &[Value], pos: &Position) -> Value {
    if args.len() != 2 {
        sdl_panic!(pos.clone(), "nth takes 2 arguments");
    }
    let v = match &args[0] {
        Value::Vec(v) => v,
        other => sdl_panic!(
            pos.clone(),
            "nth expected a vector, got {}",
            other.type_name()
        ),
    };
    let i = require_int(&args[1], "nth", pos);
    if i < 0 || (i as usize) >= v.len() {
        sdl_panic!(pos.clone(), "nth index {} out of bounds (len {})", i, v.len());
    }
    v[i as usize].clone()
}

fn builtin_count(args: &[Value], pos: &Position) -> Value {
    if args.len() != 1 {
        sdl_panic!(pos.clone(), "count takes exactly 1 argument");
    }
    match &args[0] {
        Value::Vec(v) => Value::Int(v.len() as i64),
        Value::Map(m) => Value::Int(m.len() as i64),
        Value::String(s) => Value::Int(s.chars().count() as i64),
        Value::Nil => Value::Int(0),
        other => sdl_panic!(
            pos.clone(),
            "count expected a vector, map, string, or nil (got {})",
            other.type_name()
        ),
    }
}

fn builtin_first(args: &[Value], pos: &Position) -> Value {
    if args.len() != 1 {
        sdl_panic!(pos.clone(), "first takes exactly 1 argument");
    }
    match &args[0] {
        Value::Vec(v) => v.first().cloned().unwrap_or(Value::Nil),
        Value::Nil => Value::Nil,
        other => sdl_panic!(
            pos.clone(),
            "first expected a vector or nil (got {})",
            other.type_name()
        ),
    }
}

fn builtin_rest(args: &[Value], pos: &Position) -> Value {
    if args.len() != 1 {
        sdl_panic!(pos.clone(), "rest takes exactly 1 argument");
    }
    match &args[0] {
        Value::Vec(v) => {
            if v.is_empty() {
                Value::Vec(Rc::new(Vec::new()))
            } else {
                Value::Vec(Rc::new(v[1..].to_vec()))
            }
        }
        Value::Nil => Value::Vec(Rc::new(Vec::new())),
        other => sdl_panic!(
            pos.clone(),
            "rest expected a vector or nil (got {})",
            other.type_name()
        ),
    }
}

fn builtin_conj(args: &[Value], pos: &Position) -> Value {
    if args.is_empty() {
        sdl_panic!(pos.clone(), "conj requires at least 1 argument");
    }
    let mut new_vec: Vec<Value> = match &args[0] {
        Value::Vec(v) => (**v).clone(),
        Value::Nil => Vec::new(),
        other => sdl_panic!(
            pos.clone(),
            "conj expected a vector or nil (got {})",
            other.type_name()
        ),
    };
    for a in &args[1..] {
        new_vec.push(a.clone());
    }
    Value::Vec(Rc::new(new_vec))
}

// ---------------------------------------------------------------------------
// Maps
// ---------------------------------------------------------------------------

fn builtin_hash_map(args: &[Value], pos: &Position) -> Value {
    if args.len() % 2 != 0 {
        sdl_panic!(
            pos.clone(),
            "hash-map requires an even number of arguments (got {})",
            args.len()
        );
    }
    let mut map = HashMap::with_capacity(args.len() / 2);
    let mut i = 0;
    while i < args.len() {
        let key = match &args[i] {
            Value::Keyword(k) => (**k).clone(),
            other => sdl_panic!(
                pos.clone(),
                "hash-map key must be a keyword (got {})",
                other.type_name()
            ),
        };
        map.insert(key, args[i + 1].clone());
        i += 2;
    }
    Value::Map(Rc::new(map))
}

fn require_keyword(v: &Value, fn_name: &str, pos: &Position) -> String {
    match v {
        Value::Keyword(k) => (**k).clone(),
        other => sdl_panic!(
            pos.clone(),
            "{} expected a keyword (got {})",
            fn_name,
            other.type_name()
        ),
    }
}

fn builtin_get(args: &[Value], pos: &Position) -> Value {
    if args.len() != 2 && args.len() != 3 {
        sdl_panic!(pos.clone(), "get takes 2 or 3 arguments");
    }
    let default = args.get(2).cloned().unwrap_or(Value::Nil);
    let key = require_keyword(&args[1], "get", pos);
    match &args[0] {
        Value::Map(m) => m.get(&key).cloned().unwrap_or(default),
        Value::Nil => default,
        other => sdl_panic!(
            pos.clone(),
            "get expected a map or nil (got {})",
            other.type_name()
        ),
    }
}

fn builtin_assoc(args: &[Value], pos: &Position) -> Value {
    if args.len() < 3 || (args.len() - 1) % 2 != 0 {
        sdl_panic!(
            pos.clone(),
            "assoc takes a map and one or more key/value pairs"
        );
    }
    let mut map: HashMap<String, Value> = match &args[0] {
        Value::Map(m) => (**m).clone(),
        Value::Nil => HashMap::new(),
        other => sdl_panic!(
            pos.clone(),
            "assoc expected a map or nil (got {})",
            other.type_name()
        ),
    };
    let mut i = 1;
    while i < args.len() {
        let key = require_keyword(&args[i], "assoc", pos);
        map.insert(key, args[i + 1].clone());
        i += 2;
    }
    Value::Map(Rc::new(map))
}

fn builtin_dissoc(args: &[Value], pos: &Position) -> Value {
    if args.is_empty() {
        sdl_panic!(pos.clone(), "dissoc requires at least 1 argument");
    }
    let mut map: HashMap<String, Value> = match &args[0] {
        Value::Map(m) => (**m).clone(),
        Value::Nil => return Value::Nil,
        other => sdl_panic!(
            pos.clone(),
            "dissoc expected a map or nil (got {})",
            other.type_name()
        ),
    };
    for k in &args[1..] {
        let key = require_keyword(k, "dissoc", pos);
        map.remove(&key);
    }
    Value::Map(Rc::new(map))
}

fn builtin_keys(args: &[Value], pos: &Position) -> Value {
    if args.len() != 1 {
        sdl_panic!(pos.clone(), "keys takes exactly 1 argument");
    }
    let map = match &args[0] {
        Value::Map(m) => m,
        Value::Nil => return Value::Vec(Rc::new(Vec::new())),
        other => sdl_panic!(
            pos.clone(),
            "keys expected a map or nil (got {})",
            other.type_name()
        ),
    };
    let mut names: Vec<&String> = map.keys().collect();
    names.sort(); // stable order for tests
    let v: Vec<Value> = names
        .into_iter()
        .map(|n| Value::Keyword(Rc::new(n.clone())))
        .collect();
    Value::Vec(Rc::new(v))
}

fn builtin_vals(args: &[Value], pos: &Position) -> Value {
    if args.len() != 1 {
        sdl_panic!(pos.clone(), "vals takes exactly 1 argument");
    }
    let map = match &args[0] {
        Value::Map(m) => m,
        Value::Nil => return Value::Vec(Rc::new(Vec::new())),
        other => sdl_panic!(
            pos.clone(),
            "vals expected a map or nil (got {})",
            other.type_name()
        ),
    };
    // Sort by key for stable ordering paired with `keys`.
    let mut entries: Vec<(&String, &Value)> = map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let v: Vec<Value> = entries.into_iter().map(|(_, v)| v.clone()).collect();
    Value::Vec(Rc::new(v))
}

// ---------------------------------------------------------------------------
// Output / strings
// ---------------------------------------------------------------------------

fn builtin_print(args: &[Value], _pos: &Position) -> Value {
    let mut first = true;
    for a in args {
        if !first {
            print!(" ");
        }
        first = false;
        // Strings print without surrounding quotes for friendlier
        // user-facing output. Other values use Display.
        match a {
            Value::String(s) => print!("{}", s),
            other => print!("{}", other),
        }
    }
    Value::Nil
}

fn builtin_println(args: &[Value], pos: &Position) -> Value {
    builtin_print(args, pos);
    println!();
    Value::Nil
}

fn builtin_str(args: &[Value], _pos: &Position) -> Value {
    let mut s = String::new();
    for a in args {
        match a {
            Value::Nil => {} // (str nil) → "" matches Clojure
            Value::String(inner) => s.push_str(inner),
            other => s.push_str(&format!("{}", other)),
        }
    }
    Value::String(Rc::new(s))
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

fn builtin_assert(args: &[Value], pos: &Position) -> Value {
    if args.is_empty() || args.len() > 2 {
        sdl_panic!(pos.clone(), "assert takes 1 or 2 arguments");
    }
    if !args[0].is_truthy() {
        let msg = match args.get(1) {
            Some(Value::String(s)) => format!("assertion failed: {}", s),
            Some(other) => format!("assertion failed: {}", other),
            None => "assertion failed".to_string(),
        };
        sdl_panic!(pos.clone(), "{}", msg);
    }
    Value::Nil
}

fn builtin_assert_eq(args: &[Value], pos: &Position) -> Value {
    if args.len() < 2 || args.len() > 3 {
        sdl_panic!(pos.clone(), "assert= takes 2 or 3 arguments");
    }
    if args[0] != args[1] {
        let label = match args.get(2) {
            Some(Value::String(s)) => format!(" ({})", s),
            Some(other) => format!(" ({})", other),
            None => String::new(),
        };
        sdl_panic!(
            pos.clone(),
            "assert= failed{}: expected {}, got {}",
            label,
            args[1],
            args[0]
        );
    }
    Value::Nil
}

// ---------------------------------------------------------------------------
// Type predicates
// ---------------------------------------------------------------------------

fn unary_predicate(args: &[Value], name: &str, pos: &Position, pred: fn(&Value) -> bool) -> Value {
    if args.len() != 1 {
        sdl_panic!(pos.clone(), "{} takes exactly 1 argument", name);
    }
    Value::Bool(pred(&args[0]))
}

fn builtin_nil_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "nil?", pos, |v| matches!(v, Value::Nil))
}
fn builtin_boolean_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "boolean?", pos, |v| matches!(v, Value::Bool(_)))
}
fn builtin_int_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "int?", pos, |v| matches!(v, Value::Int(_)))
}
fn builtin_float_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "float?", pos, |v| matches!(v, Value::Float(_)))
}
fn builtin_number_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "number?", pos, Value::is_number)
}
fn builtin_string_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "string?", pos, |v| matches!(v, Value::String(_)))
}
fn builtin_keyword_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "keyword?", pos, |v| matches!(v, Value::Keyword(_)))
}
fn builtin_symbol_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "symbol?", pos, |v| matches!(v, Value::Symbol(_)))
}
fn builtin_vector_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "vector?", pos, |v| matches!(v, Value::Vec(_)))
}
fn builtin_map_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "map?", pos, |v| matches!(v, Value::Map(_)))
}
fn builtin_fn_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "fn?", pos, |v| matches!(v, Value::Fn(_)))
}

// ---------------------------------------------------------------------------
// Misc helpers
// ---------------------------------------------------------------------------

fn builtin_name(args: &[Value], pos: &Position) -> Value {
    if args.len() != 1 {
        sdl_panic!(pos.clone(), "name takes exactly 1 argument");
    }
    match &args[0] {
        Value::Keyword(k) => Value::String(k.clone()),
        Value::Symbol(s) => Value::String(s.clone()),
        Value::String(s) => Value::String(s.clone()),
        other => sdl_panic!(
            pos.clone(),
            "name expected a keyword, symbol, or string (got {})",
            other.type_name()
        ),
    }
}

// ---------------------------------------------------------------------------
// Phase 4 — higher-order functions
// ---------------------------------------------------------------------------
//
// These are eagerly-evaluating versions of Clojure's seq HOFs. They
// take a vector (the only collection type the SDL has so far) and
// return a vector. Lazy seqs and transducers are out of scope.
//
// Each one calls back into `eval::apply` for the user-supplied
// callable, so HOFs work uniformly over native and interpreted
// functions. Argument validation (the callable must be a Value::Fn)
// happens lazily inside `apply` — passing a non-fn produces a
// position-tagged error pointing at the HOF's call site.

fn require_fn<'a>(v: &'a Value, fn_name: &str, pos: &Position) -> &'a Value {
    if !matches!(v, Value::Fn(_)) {
        sdl_panic!(
            pos.clone(),
            "{} expected a function (got {})",
            fn_name,
            v.type_name()
        );
    }
    v
}

fn require_vec<'a>(v: &'a Value, fn_name: &str, pos: &Position) -> Rc<Vec<Value>> {
    match v {
        Value::Vec(items) => items.clone(),
        other => sdl_panic!(
            pos.clone(),
            "{} expected a vector (got {})",
            fn_name,
            other.type_name()
        ),
    }
}

/// `(map f coll)` — returns a vector of `(f x)` for each `x` in
/// `coll`. Single-arity only; multi-collection map is a Phase 6+
/// nicety.
fn builtin_map(args: &[Value], pos: &Position) -> Value {
    if args.len() != 2 {
        sdl_panic!(
            pos.clone(),
            "map takes 2 arguments (got {})",
            args.len()
        );
    }
    let f = require_fn(&args[0], "map", pos);
    let coll = require_vec(&args[1], "map", pos);
    let mut out = Vec::with_capacity(coll.len());
    for x in coll.iter() {
        out.push(eval::apply(f, std::slice::from_ref(x), pos));
    }
    Value::Vec(Rc::new(out))
}

/// `(filter pred coll)` — returns a vector of elements of `coll` for
/// which `(pred x)` is truthy.
fn builtin_filter(args: &[Value], pos: &Position) -> Value {
    if args.len() != 2 {
        sdl_panic!(
            pos.clone(),
            "filter takes 2 arguments (got {})",
            args.len()
        );
    }
    let pred = require_fn(&args[0], "filter", pos);
    let coll = require_vec(&args[1], "filter", pos);
    let mut out: Vec<Value> = Vec::new();
    for x in coll.iter() {
        if eval::apply(pred, std::slice::from_ref(x), pos).is_truthy() {
            out.push(x.clone());
        }
    }
    Value::Vec(Rc::new(out))
}

/// `(reduce f init coll)` or `(reduce f coll)` — left fold. The
/// init-less form requires a non-empty collection (matches Clojure);
/// the init-ful form returns `init` for an empty collection.
fn builtin_reduce(args: &[Value], pos: &Position) -> Value {
    let (f, mut acc, coll) = match args.len() {
        2 => {
            let f = require_fn(&args[0], "reduce", pos);
            let coll = require_vec(&args[1], "reduce", pos);
            if coll.is_empty() {
                sdl_panic!(
                    pos.clone(),
                    "reduce on an empty vector requires an init value"
                );
            }
            // Single-arity reduce on a 1-element vector returns the
            // single element without calling f, matching Clojure.
            if coll.len() == 1 {
                return coll[0].clone();
            }
            let init = coll[0].clone();
            let rest: Vec<Value> = coll[1..].to_vec();
            (f, init, rest)
        }
        3 => {
            let f = require_fn(&args[0], "reduce", pos);
            let init = args[1].clone();
            let coll = require_vec(&args[2], "reduce", pos);
            (f, init, coll.iter().cloned().collect::<Vec<_>>())
        }
        n => sdl_panic!(
            pos.clone(),
            "reduce takes 2 or 3 arguments (got {})",
            n
        ),
    };
    for x in coll {
        acc = eval::apply(f, &[acc, x], pos);
    }
    acc
}

/// `(range n)` → `[0 1 ... n-1]`.
/// `(range start end)` → `[start ... end-1]`.
/// `(range start end step)` → arithmetic progression with `step`. Step
/// must be non-zero. With a positive step, ranges are open at the high
/// end; with a negative step, open at the low end. Empty ranges are
/// returned as an empty vector rather than an error.
fn builtin_range(args: &[Value], pos: &Position) -> Value {
    let (start, end, step) = match args.len() {
        1 => (0, require_int(&args[0], "range", pos), 1),
        2 => (
            require_int(&args[0], "range", pos),
            require_int(&args[1], "range", pos),
            1,
        ),
        3 => (
            require_int(&args[0], "range", pos),
            require_int(&args[1], "range", pos),
            require_int(&args[2], "range", pos),
        ),
        n => sdl_panic!(
            pos.clone(),
            "range takes 1, 2, or 3 arguments (got {})",
            n
        ),
    };
    if step == 0 {
        sdl_panic!(pos.clone(), "range step must be non-zero");
    }
    let mut out: Vec<Value> = Vec::new();
    let mut i = start;
    if step > 0 {
        while i < end {
            out.push(Value::Int(i));
            i = i.wrapping_add(step);
        }
    } else {
        while i > end {
            out.push(Value::Int(i));
            i = i.wrapping_add(step);
        }
    }
    Value::Vec(Rc::new(out))
}

/// `(repeat n x)` — returns a vector of `n` copies of `x`. `n` must
/// be a non-negative integer. Returns an empty vector for n = 0.
fn builtin_repeat(args: &[Value], pos: &Position) -> Value {
    if args.len() != 2 {
        sdl_panic!(
            pos.clone(),
            "repeat takes 2 arguments (got {})",
            args.len()
        );
    }
    let n = require_int(&args[0], "repeat count", pos);
    if n < 0 {
        sdl_panic!(
            pos.clone(),
            "repeat count must be non-negative (got {})",
            n
        );
    }
    let mut out: Vec<Value> = Vec::with_capacity(n as usize);
    for _ in 0..n {
        out.push(args[1].clone());
    }
    Value::Vec(Rc::new(out))
}

/// `(apply f arg-vec)` or `(apply f a1 a2 ... arg-vec)` — calls `f`
/// with the elements of the trailing vector spread out as positional
/// args, optionally preceded by the leading explicit args. Matches
/// Clojure's variadic apply.
fn builtin_apply(args: &[Value], pos: &Position) -> Value {
    if args.len() < 2 {
        sdl_panic!(
            pos.clone(),
            "apply takes at least 2 arguments (got {})",
            args.len()
        );
    }
    let f = require_fn(&args[0], "apply", pos);
    let last = &args[args.len() - 1];
    let tail = require_vec(last, "apply (last argument)", pos);
    // Build the final argument list: leading positional args first,
    // then the tail vector spread out.
    let mut combined: Vec<Value> = args[1..args.len() - 1].to_vec();
    combined.extend(tail.iter().cloned());
    eval::apply(f, &combined, pos)
}

// ---------------------------------------------------------------------------
// Phase 4 — math helpers
// ---------------------------------------------------------------------------
//
// `min` and `max` are variadic and preserve int-ness when every
// argument is an int, mirroring `+`/`-`/`*`. The transcendental
// helpers (`sqrt`, `sin`, `cos`, `tan`) always return float, even
// when the input was an int — there's no point pretending sqrt(2)
// rounds to an integer.

fn builtin_min(args: &[Value], pos: &Position) -> Value {
    if args.is_empty() {
        sdl_panic!(pos.clone(), "min requires at least 1 argument");
    }
    if any_float(args) {
        let mut acc = require_number(&args[0], "min", pos);
        for a in &args[1..] {
            let v = require_number(a, "min", pos);
            if v < acc {
                acc = v;
            }
        }
        Value::Float(acc)
    } else {
        let mut acc = require_int(&args[0], "min", pos);
        for a in &args[1..] {
            let v = require_int(a, "min", pos);
            if v < acc {
                acc = v;
            }
        }
        Value::Int(acc)
    }
}

fn builtin_max(args: &[Value], pos: &Position) -> Value {
    if args.is_empty() {
        sdl_panic!(pos.clone(), "max requires at least 1 argument");
    }
    if any_float(args) {
        let mut acc = require_number(&args[0], "max", pos);
        for a in &args[1..] {
            let v = require_number(a, "max", pos);
            if v > acc {
                acc = v;
            }
        }
        Value::Float(acc)
    } else {
        let mut acc = require_int(&args[0], "max", pos);
        for a in &args[1..] {
            let v = require_int(a, "max", pos);
            if v > acc {
                acc = v;
            }
        }
        Value::Int(acc)
    }
}

/// `(abs n)` — preserves int-ness for int input, returns float for
/// float input. `i64::MIN.abs()` would overflow; we use
/// `wrapping_abs` so it saturates rather than panicking — matches
/// the existing arithmetic functions' wrapping convention.
fn builtin_abs(args: &[Value], pos: &Position) -> Value {
    if args.len() != 1 {
        sdl_panic!(
            pos.clone(),
            "abs takes 1 argument (got {})",
            args.len()
        );
    }
    match &args[0] {
        Value::Int(i) => Value::Int(i.wrapping_abs()),
        Value::Float(f) => Value::Float(f.abs()),
        other => sdl_panic!(
            pos.clone(),
            "abs expected a number (got {})",
            other.type_name()
        ),
    }
}

fn builtin_sqrt(args: &[Value], pos: &Position) -> Value {
    if args.len() != 1 {
        sdl_panic!(
            pos.clone(),
            "sqrt takes 1 argument (got {})",
            args.len()
        );
    }
    Value::Float(require_number(&args[0], "sqrt", pos).sqrt())
}

fn builtin_sin(args: &[Value], pos: &Position) -> Value {
    if args.len() != 1 {
        sdl_panic!(pos.clone(), "sin takes 1 argument (got {})", args.len());
    }
    Value::Float(require_number(&args[0], "sin", pos).sin())
}

fn builtin_cos(args: &[Value], pos: &Position) -> Value {
    if args.len() != 1 {
        sdl_panic!(pos.clone(), "cos takes 1 argument (got {})", args.len());
    }
    Value::Float(require_number(&args[0], "cos", pos).cos())
}

fn builtin_tan(args: &[Value], pos: &Position) -> Value {
    if args.len() != 1 {
        sdl_panic!(pos.clone(), "tan takes 1 argument (got {})", args.len());
    }
    Value::Float(require_number(&args[0], "tan", pos).tan())
}
