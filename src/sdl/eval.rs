// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Tree-walking evaluator.
//!
//! Layout:
//! - [`eval`] is the single public entry point. It dispatches on the
//!   [`FormKind`].
//! - List forms are routed through [`eval_list`], which checks for a
//!   special-form symbol in head position before falling through to
//!   the function-call path.
//! - [`apply`] handles function application. For interpreted
//!   functions it runs a `recur` loop: each iteration binds args and
//!   re-evaluates the body until no `recur` signal is set.
//!
//! Pattern matching for destructuring lives in this module too —
//! [`compile_pattern`] turns a `Form` (e.g. `[a [b c] :as v & rest]`)
//! into a [`ParamPattern`], and [`bind_pattern`] applies one to a
//! value, populating an environment.

use std::collections::HashMap;
use std::rc::Rc;

use crate::sdl::ast::{Form, FormKind};
use crate::sdl::env::{EnvRef, Environment};
use crate::sdl::error::Position;
use crate::sdl::value::{
    Function, FunctionKind, ParamList, ParamPattern, RecurSignal, Value, RECUR,
};
use crate::{sdl_panic};

/// Evaluate a single form against the given environment.
pub fn eval(form: &Form, env: &EnvRef) -> Value {
    match &form.kind {
        FormKind::Nil => Value::Nil,
        FormKind::Bool(b) => Value::Bool(*b),
        FormKind::Int(i) => Value::Int(*i),
        FormKind::Float(f) => Value::Float(*f),
        FormKind::String(s) => Value::String(Rc::new(s.clone())),
        FormKind::Keyword(k) => Value::Keyword(Rc::new(k.clone())),
        FormKind::Symbol(s) => match env.borrow().lookup(s) {
            Some(v) => v,
            None => sdl_panic!(form.pos, "unbound symbol: {}", s),
        },
        FormKind::List(elements) => eval_list(elements, env, &form.pos),
        FormKind::Vector(elements) => {
            let items: Vec<Value> = elements.iter().map(|e| eval(e, env)).collect();
            Value::Vec(Rc::new(items))
        }
        FormKind::Map(pairs) => {
            let mut map = HashMap::with_capacity(pairs.len());
            for (kf, vf) in pairs {
                let kv = eval(kf, env);
                let key = match kv {
                    Value::Keyword(k) => (*k).clone(),
                    other => sdl_panic!(
                        kf.pos,
                        "map keys must be keywords (got {})",
                        other.type_name()
                    ),
                };
                let val = eval(vf, env);
                map.insert(key, val);
            }
            Value::Map(Rc::new(map))
        }
    }
}

/// Dispatch a list form: special form vs function call.
fn eval_list(elements: &[Form], env: &EnvRef, pos: &Position) -> Value {
    if elements.is_empty() {
        sdl_panic!(pos.clone(), "cannot evaluate empty list ()");
    }
    // Check for special-form keywords by name in head position.
    if let FormKind::Symbol(name) = &elements[0].kind {
        match name.as_str() {
            "def" => return eval_def(&elements[1..], env, pos),
            "let" => return eval_let(&elements[1..], env, pos),
            "fn" => return eval_fn(&elements[1..], env, pos),
            "if" => return eval_if(&elements[1..], env, pos),
            "do" => return eval_do(&elements[1..], env),
            "quote" => return eval_quote(&elements[1..], pos),
            "recur" => return eval_recur(&elements[1..], env, pos),
            "and" => return eval_and(&elements[1..], env),
            "or" => return eval_or(&elements[1..], env),
            _ => {}
        }
    }
    // Function call path.
    let head = eval(&elements[0], env);
    let args: Vec<Value> = elements[1..].iter().map(|e| eval(e, env)).collect();
    apply(&head, &args, &elements[0].pos)
}

/// Apply `head` to `args`. `pos` is the call-site of the head form,
/// used for error reporting if `head` isn't callable.
pub fn apply(head: &Value, args: &[Value], pos: &Position) -> Value {
    let func = match head {
        Value::Fn(f) => f.clone(),
        other => sdl_panic!(
            pos.clone(),
            "cannot call {} ({})",
            other,
            other.type_name()
        ),
    };
    apply_function(&func, args, pos)
}

fn apply_function(func: &Rc<Function>, args: &[Value], pos: &Position) -> Value {
    match &func.kind {
        FunctionKind::Native { func: f, .. } => (*f)(args, pos),
        FunctionKind::Interpreted {
            params, body, env, ..
        } => apply_interpreted(params, body, env, args, pos),
    }
}

/// Run an interpreted function body with `recur` looping support.
///
/// Each iteration:
/// 1. Make a fresh child env from the function's captured env.
/// 2. Bind args to params (with destructuring + rest).
/// 3. Evaluate body forms in order; the value of the last form is
///    the candidate result.
/// 4. Check `RECUR`. If set, reuse those args and re-loop. Otherwise
///    return the candidate result.
fn apply_interpreted(
    params: &ParamList,
    body: &Rc<Vec<Form>>,
    captured: &EnvRef,
    args: &[Value],
    pos: &Position,
) -> Value {
    let mut current_args: Vec<Value> = args.to_vec();
    loop {
        let frame = Environment::new_child(captured);
        bind_params(params, &current_args, &frame, pos);
        // Clear any stale recur signal from an outer scope before
        // evaluating the body, so we only catch recurs that this
        // body *itself* fires.
        RECUR.with(|cell| *cell.borrow_mut() = None);
        let mut last = Value::Nil;
        for form in body.iter() {
            last = eval(form, &frame);
            // If a recur signal fired during this form, skip the
            // remaining body forms — their values would be discarded
            // anyway.
            let pending = RECUR.with(|cell| cell.borrow().is_some());
            if pending {
                break;
            }
        }
        let recur = RECUR.with(|cell| cell.borrow_mut().take());
        match recur {
            Some(RecurSignal { args: new_args, pos: rpos }) => {
                // Arity check matches the fresh-call rules (rest
                // params accept >= n_fixed args).
                check_arity(params, new_args.len(), &rpos);
                current_args = new_args;
                continue;
            }
            None => return last,
        }
    }
}

/// Verify arg count is compatible with the parameter list. Panics
/// with a position-tagged message on mismatch.
fn check_arity(params: &ParamList, n_args: usize, pos: &Position) {
    let n_fixed = params.patterns.len();
    if params.rest.is_some() {
        if n_args < n_fixed {
            sdl_panic!(
                pos.clone(),
                "arity mismatch: expected at least {} args, got {}",
                n_fixed,
                n_args
            );
        }
    } else if n_args != n_fixed {
        sdl_panic!(
            pos.clone(),
            "arity mismatch: expected {} args, got {}",
            n_fixed,
            n_args
        );
    }
}

/// Bind a function call's positional + rest args to its parameter
/// patterns inside `env`. The `pos` is used for arity-mismatch
/// messages.
fn bind_params(params: &ParamList, args: &[Value], env: &EnvRef, pos: &Position) {
    check_arity(params, args.len(), pos);
    let n_fixed = params.patterns.len();
    for (i, pat) in params.patterns.iter().enumerate() {
        bind_pattern(pat, &args[i], env, pos);
    }
    if let Some(rest_pat) = &params.rest {
        let rest_vals: Vec<Value> = args[n_fixed..].to_vec();
        bind_pattern(rest_pat, &Value::Vec(Rc::new(rest_vals)), env, pos);
    }
}

/// Recursively bind a pattern against a value into `env`.
pub fn bind_pattern(pat: &ParamPattern, val: &Value, env: &EnvRef, pos: &Position) {
    match pat {
        ParamPattern::Wildcard => {}
        ParamPattern::Symbol(name) => {
            env.borrow_mut().define(name.clone(), val.clone());
        }
        ParamPattern::Vector {
            elements,
            rest,
            as_binding,
        } => {
            let items = match val {
                Value::Vec(v) => v.clone(),
                other => sdl_panic!(
                    pos.clone(),
                    "destructuring expected a vector, got {}",
                    other.type_name()
                ),
            };
            // :as binding sees the whole vec.
            if let Some(name) = as_binding {
                env.borrow_mut().define(name.clone(), val.clone());
            }
            // Positional sub-patterns. If the inner vec is shorter
            // than the pattern, bind missing slots to nil (matches
            // Clojure's lenient destructuring behaviour).
            for (i, sub) in elements.iter().enumerate() {
                let v = items.get(i).cloned().unwrap_or(Value::Nil);
                bind_pattern(sub, &v, env, pos);
            }
            // Rest pattern captures the remainder as a Vec.
            if let Some(rest_pat) = rest {
                let remainder: Vec<Value> = if items.len() > elements.len() {
                    items[elements.len()..].to_vec()
                } else {
                    Vec::new()
                };
                bind_pattern(rest_pat, &Value::Vec(Rc::new(remainder)), env, pos);
            }
        }
    }
}

/// Convert a binding `Form` into a `ParamPattern` tree.
///
/// - `Symbol("_")` → wildcard.
/// - Any other symbol → `Symbol`.
/// - Vector form → recurse, scanning for `& name` and `:as name`
///   markers. Markers may appear in any order at the tail; each is
///   followed by a single pattern (typically a symbol).
pub fn compile_pattern(form: &Form) -> ParamPattern {
    match &form.kind {
        FormKind::Symbol(s) if s == "_" => ParamPattern::Wildcard,
        FormKind::Symbol(s) => ParamPattern::Symbol(s.clone()),
        FormKind::Vector(elements) => compile_vector_pattern(elements, &form.pos),
        other => sdl_panic!(
            form.pos.clone(),
            "expected a binding pattern (symbol or vector), got {}",
            other.type_name()
        ),
    }
}

fn compile_vector_pattern(elements: &[Form], _pos: &Position) -> ParamPattern {
    let mut positional: Vec<ParamPattern> = Vec::new();
    let mut rest: Option<Box<ParamPattern>> = None;
    let mut as_binding: Option<String> = None;

    let mut i = 0;
    while i < elements.len() {
        let f = &elements[i];
        match &f.kind {
            FormKind::Symbol(s) if s == "&" => {
                if i + 1 >= elements.len() {
                    sdl_panic!(f.pos.clone(), "& must be followed by a pattern");
                }
                let next = &elements[i + 1];
                rest = Some(Box::new(compile_pattern(next)));
                i += 2;
                // After & PATTERN we may see :as NAME, but no further
                // positional patterns.
            }
            FormKind::Keyword(kw) if kw == "as" => {
                if i + 1 >= elements.len() {
                    sdl_panic!(f.pos.clone(), ":as must be followed by a name");
                }
                let next = &elements[i + 1];
                match &next.kind {
                    FormKind::Symbol(name) => {
                        as_binding = Some(name.clone());
                    }
                    other => sdl_panic!(
                        next.pos.clone(),
                        ":as must be followed by a symbol (got {})",
                        other.type_name()
                    ),
                }
                i += 2;
            }
            _ => {
                if rest.is_some() {
                    sdl_panic!(
                        f.pos.clone(),
                        "no positional patterns allowed after &"
                    );
                }
                positional.push(compile_pattern(f));
                i += 1;
            }
        }
    }

    ParamPattern::Vector {
        elements: positional,
        rest,
        as_binding,
    }
}

// ---------------------------------------------------------------------------
// Special forms
// ---------------------------------------------------------------------------

fn eval_def(args: &[Form], env: &EnvRef, pos: &Position) -> Value {
    if args.len() != 2 {
        sdl_panic!(
            pos.clone(),
            "def takes exactly 2 arguments (got {})",
            args.len()
        );
    }
    let name = match &args[0].kind {
        FormKind::Symbol(s) => s.clone(),
        other => sdl_panic!(
            args[0].pos.clone(),
            "def expected a symbol, got {}",
            other.type_name()
        ),
    };
    let value = eval(&args[1], env);
    // If the value is an interpreted function with no name, give it
    // the def-name for nicer printing.
    let value = name_function(value, &name);
    env.borrow_mut().define(name, value);
    Value::Nil
}

fn name_function(v: Value, def_name: &str) -> Value {
    if let Value::Fn(f) = &v {
        if let FunctionKind::Interpreted {
            name: None,
            params,
            body,
            env,
        } = &f.kind
        {
            let renamed = Function {
                kind: FunctionKind::Interpreted {
                    name: Some(def_name.to_string()),
                    params: params.clone(),
                    body: body.clone(),
                    env: env.clone(),
                },
            };
            return Value::Fn(Rc::new(renamed));
        }
    }
    v
}

fn eval_let(args: &[Form], env: &EnvRef, pos: &Position) -> Value {
    if args.is_empty() {
        sdl_panic!(pos.clone(), "let requires a binding vector");
    }
    let bindings = match &args[0].kind {
        FormKind::Vector(items) => items,
        other => sdl_panic!(
            args[0].pos.clone(),
            "let bindings must be a vector, got {}",
            other.type_name()
        ),
    };
    if bindings.len() % 2 != 0 {
        sdl_panic!(
            args[0].pos.clone(),
            "let binding vector must have an even number of forms"
        );
    }
    let frame = Environment::new_child(env);
    let mut i = 0;
    while i < bindings.len() {
        let pattern = compile_pattern(&bindings[i]);
        let value = eval(&bindings[i + 1], &frame);
        bind_pattern(&pattern, &value, &frame, &bindings[i].pos);
        i += 2;
    }
    // Body: implicit do over the remaining forms.
    let body = &args[1..];
    let mut last = Value::Nil;
    for form in body {
        last = eval(form, &frame);
    }
    last
}

fn eval_fn(args: &[Form], env: &EnvRef, pos: &Position) -> Value {
    // Two valid shapes:
    //   (fn [params] body...)
    //   (fn name [params] body...)
    if args.is_empty() {
        sdl_panic!(pos.clone(), "fn requires a parameter vector");
    }
    let (name, params_form, body_start) = match &args[0].kind {
        FormKind::Symbol(s) => {
            if args.len() < 2 {
                sdl_panic!(pos.clone(), "fn requires a parameter vector");
            }
            (Some(s.clone()), &args[1], 2)
        }
        FormKind::Vector(_) => (None, &args[0], 1),
        other => sdl_panic!(
            args[0].pos.clone(),
            "fn expected a name or parameter vector, got {}",
            other.type_name()
        ),
    };
    let params = compile_param_list(params_form);
    let body: Vec<Form> = args[body_start..].to_vec();
    let f = Function {
        kind: FunctionKind::Interpreted {
            name,
            params,
            body: Rc::new(body),
            env: env.clone(),
        },
    };
    Value::Fn(Rc::new(f))
}

fn compile_param_list(form: &Form) -> ParamList {
    let elements = match &form.kind {
        FormKind::Vector(v) => v,
        other => sdl_panic!(
            form.pos.clone(),
            "fn parameters must be a vector, got {}",
            other.type_name()
        ),
    };
    // Walk elements collecting positional patterns and an optional
    // & rest. :as is not meaningful at the top-level fn parameter
    // list, so we forbid it there.
    let mut positional: Vec<ParamPattern> = Vec::new();
    let mut rest: Option<ParamPattern> = None;
    let mut i = 0;
    while i < elements.len() {
        let f = &elements[i];
        match &f.kind {
            FormKind::Symbol(s) if s == "&" => {
                if i + 1 >= elements.len() {
                    sdl_panic!(f.pos.clone(), "& must be followed by a pattern");
                }
                rest = Some(compile_pattern(&elements[i + 1]));
                if i + 2 != elements.len() {
                    sdl_panic!(
                        elements[i + 2].pos.clone(),
                        "no parameters allowed after & rest"
                    );
                }
                break;
            }
            FormKind::Keyword(kw) if kw == "as" => {
                sdl_panic!(
                    f.pos.clone(),
                    ":as is not allowed in a top-level fn parameter list"
                );
            }
            _ => {
                positional.push(compile_pattern(f));
                i += 1;
            }
        }
    }
    ParamList {
        patterns: positional,
        rest,
    }
}

fn eval_if(args: &[Form], env: &EnvRef, pos: &Position) -> Value {
    if args.len() < 2 || args.len() > 3 {
        sdl_panic!(
            pos.clone(),
            "if takes 2 or 3 arguments (got {})",
            args.len()
        );
    }
    let cond = eval(&args[0], env);
    if cond.is_truthy() {
        eval(&args[1], env)
    } else if args.len() == 3 {
        eval(&args[2], env)
    } else {
        Value::Nil
    }
}

fn eval_do(args: &[Form], env: &EnvRef) -> Value {
    let mut last = Value::Nil;
    for form in args {
        last = eval(form, env);
    }
    last
}

fn eval_quote(args: &[Form], pos: &Position) -> Value {
    if args.len() != 1 {
        sdl_panic!(pos.clone(), "quote takes exactly 1 argument");
    }
    form_to_quoted_value(&args[0])
}

/// Turn a Form into a Value without evaluation. Symbols become
/// `Value::Symbol`; lists and vectors become `Value::Vec`; maps
/// become `Value::Map` (with the keyword-key restriction enforced).
fn form_to_quoted_value(form: &Form) -> Value {
    match &form.kind {
        FormKind::Nil => Value::Nil,
        FormKind::Bool(b) => Value::Bool(*b),
        FormKind::Int(i) => Value::Int(*i),
        FormKind::Float(f) => Value::Float(*f),
        FormKind::String(s) => Value::String(Rc::new(s.clone())),
        FormKind::Keyword(k) => Value::Keyword(Rc::new(k.clone())),
        FormKind::Symbol(s) => Value::Symbol(Rc::new(s.clone())),
        FormKind::List(items) | FormKind::Vector(items) => {
            let v: Vec<Value> = items.iter().map(form_to_quoted_value).collect();
            Value::Vec(Rc::new(v))
        }
        FormKind::Map(pairs) => {
            let mut map = HashMap::with_capacity(pairs.len());
            for (kf, vf) in pairs {
                let key = match &kf.kind {
                    FormKind::Keyword(k) => k.clone(),
                    other => sdl_panic!(
                        kf.pos.clone(),
                        "quoted map key must be a keyword (got {})",
                        other.type_name()
                    ),
                };
                map.insert(key, form_to_quoted_value(vf));
            }
            Value::Map(Rc::new(map))
        }
    }
}

fn eval_recur(args: &[Form], env: &EnvRef, pos: &Position) -> Value {
    let evaluated: Vec<Value> = args.iter().map(|a| eval(a, env)).collect();
    RECUR.with(|cell| {
        *cell.borrow_mut() = Some(RecurSignal {
            args: evaluated,
            pos: pos.clone(),
        });
    });
    // The function's apply loop will see the signal and re-bind. The
    // value we return is discarded by that loop. (If recur is used
    // outside a fn body the signal will sit until consumed by the
    // next interpreted-fn call — that's a known footgun but cheap to
    // catch in scripts via tests.)
    Value::Nil
}

fn eval_and(args: &[Form], env: &EnvRef) -> Value {
    if args.is_empty() {
        return Value::Bool(true);
    }
    let mut last = Value::Bool(true);
    for form in args {
        let v = eval(form, env);
        if !v.is_truthy() {
            return v;
        }
        last = v;
    }
    last
}

fn eval_or(args: &[Form], env: &EnvRef) -> Value {
    if args.is_empty() {
        return Value::Nil;
    }
    let mut last = Value::Nil;
    for form in args {
        let v = eval(form, env);
        if v.is_truthy() {
            return v;
        }
        last = v;
    }
    last
}
