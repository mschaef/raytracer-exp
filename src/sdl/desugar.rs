// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Desugaring pass: rewrites syntactic sugar into core special forms
//! before evaluation.
//!
//! Runs between the reader and the evaluator, once per top-level form
//! (see [`crate::sdl::eval_source`]). The pass walks the whole form
//! tree and rewrites any list whose head symbol names a sugar form,
//! then re-walks the rewritten result so sugar that expands into more
//! sugar (or that contains sugar in its body) is fully expanded. The
//! evaluator only ever sees core forms.
//!
//! Current sugar:
//!
//! - `(defn name docstring? [params] body...)` →
//!   `(def name (fn name [params] body...))`
//! - `(when test body...)` → `(if test (do body...) nil)`
//! - `(when-not test body...)` → `(if test nil (do body...))`
//! - `(cond t1 e1 t2 e2 ...)` → `(if t1 e1 (cond t2 e2 ...))`, with
//!   `(cond)` → `nil`
//! - `(-> x f (g a))` → `(g (f x) a)` (thread first)
//! - `(->> x f (g a))` → `(g a (f x))` (thread last)
//!
//! This is deliberately the same shape as macroexpansion: walk the
//! tree, look up the head symbol in a table of transformers, rewrite,
//! repeat. Today the table is the fixed `match` in [`desugar_list`];
//! a future `defmacro` would make it user-extensible. Expanding one
//! top-level form at a time (rather than a whole file up front) is
//! what a macro system needs too — a macro defined by one top-level
//! form must be visible when expanding the next.
//!
//! Because these are source rewrites, they behave like Clojure's
//! macros rather than like the special forms they replaced. The
//! visible consequences are all in threading: a step may itself be a
//! special form or sugar (`(-> x (if :yes :no))` is `(if x :yes :no)`),
//! and evaluation order follows the rewritten call — in
//! `(-> x (g a))` the head `g` is evaluated before `x`. Only code with
//! side effects in threaded forms can tell the difference.
//!
//! Quoted data is left untouched: `'(defn f [x] x)` is a list of
//! symbols, not a definition.
//!
//! Like the special forms in `eval.rs`, sugar names are matched by
//! symbol name in head position, so a local binding named `defn`
//! (or `when`, `cond`, ...) can't shadow the sugar.

use crate::sdl::ast::{Form, FormKind};
use crate::sdl::error::Position;
use crate::sdl_panic;

/// Fully desugar `form`, returning a new form containing only core
/// special forms. Source positions are preserved from the original
/// forms; synthesized forms (`def`, `fn`, `if`, `do`, `nil`) take the
/// position of the sugar form or its head symbol.
pub fn desugar(form: &Form) -> Form {
    match &form.kind {
        FormKind::List(items) => desugar_list(items, form),
        FormKind::Vector(items) => Form::new(
            FormKind::Vector(items.iter().map(desugar).collect()),
            form.pos.clone(),
        ),
        FormKind::Map(pairs) => Form::new(
            FormKind::Map(
                pairs
                    .iter()
                    .map(|(k, v)| (desugar(k), desugar(v)))
                    .collect(),
            ),
            form.pos.clone(),
        ),
        _ => form.clone(),
    }
}

fn desugar_list(items: &[Form], form: &Form) -> Form {
    if let Some(Form {
        kind: FormKind::Symbol(head),
        ..
    }) = items.first()
    {
        // Each expansion is re-walked so nested sugar is expanded too.
        let expanded = match head.as_str() {
            "quote" => return form.clone(),
            "defn" => expand_defn(items, &form.pos),
            "when" => expand_when(items, &form.pos, false),
            "when-not" => expand_when(items, &form.pos, true),
            "cond" => expand_cond(items, &form.pos),
            "->" => expand_thread(items, &form.pos, true),
            "->>" => expand_thread(items, &form.pos, false),
            _ => {
                return Form::new(
                    FormKind::List(items.iter().map(desugar).collect()),
                    form.pos.clone(),
                )
            }
        };
        return desugar(&expanded);
    }
    Form::new(
        FormKind::List(items.iter().map(desugar).collect()),
        form.pos.clone(),
    )
}

// ---------------------------------------------------------------------------
// Form-building helpers
// ---------------------------------------------------------------------------

fn sym(name: &str, pos: &Position) -> Form {
    Form::new(FormKind::Symbol(name.to_string()), pos.clone())
}

fn list(items: Vec<Form>, pos: &Position) -> Form {
    Form::new(FormKind::List(items), pos.clone())
}

fn nil(pos: &Position) -> Form {
    Form::new(FormKind::Nil, pos.clone())
}

// ---------------------------------------------------------------------------
// Expanders. Each takes the sugar form's items (head symbol included
// at index 0) and the form's position, and returns one rewrite step.
// ---------------------------------------------------------------------------

/// `(defn name docstring? [params] body...)` →
/// `(def name (fn name [params] body...))`.
///
/// The optional docstring is accepted for Clojure familiarity and
/// discarded; nothing in the SDL reads documentation yet. A string
/// *after* the parameter vector is an ordinary body form, as in
/// Clojure. Multi-arity definitions (`(defn f ([x] ...) ([x y] ...))`)
/// are rejected because `fn` doesn't support them.
///
/// The name is passed through to `fn` as well as `def`, so the
/// function value is named (`#<fn name>`) — the same result `def`'s
/// auto-naming produces for the long-hand form.
fn expand_defn(items: &[Form], pos: &Position) -> Form {
    let head_pos = &items[0].pos;

    let name = match items.get(1) {
        None => sdl_panic!(pos, "defn requires a name and a parameter vector"),
        Some(f) => match &f.kind {
            FormKind::Symbol(_) => f,
            other => sdl_panic!(
                f.pos,
                "defn expected a symbol name, got {}",
                other.type_name()
            ),
        },
    };

    let mut rest = &items[2..];
    if let Some(Form {
        kind: FormKind::String(_),
        ..
    }) = rest.first()
    {
        rest = &rest[1..];
    }

    match rest.first() {
        None => sdl_panic!(pos, "defn requires a parameter vector"),
        Some(f) => match &f.kind {
            FormKind::Vector(_) => {}
            FormKind::List(_) => sdl_panic!(
                f.pos,
                "defn: multi-arity definitions are not supported"
            ),
            other => sdl_panic!(
                f.pos,
                "defn expected a parameter vector, got {}",
                other.type_name()
            ),
        },
    }

    let mut fn_items = Vec::with_capacity(rest.len() + 2);
    fn_items.push(sym("fn", head_pos));
    fn_items.push(name.clone());
    fn_items.extend(rest.iter().cloned());

    list(
        vec![sym("def", head_pos), name.clone(), list(fn_items, pos)],
        pos,
    )
}

/// `(when test body...)` → `(if test (do body...) nil)`, and
/// `(when-not test body...)` → `(if test nil (do body...))`.
///
/// Laziness comes from `if`: the body is only evaluated when the gate
/// is open. An empty body is `(do)`, which evaluates to `nil`.
fn expand_when(items: &[Form], pos: &Position, negate: bool) -> Form {
    let name = if negate { "when-not" } else { "when" };
    let head_pos = &items[0].pos;
    let test = match items.get(1) {
        Some(t) => t.clone(),
        None => sdl_panic!(pos, "{} requires a test expression", name),
    };

    let mut do_items = Vec::with_capacity(items.len() - 1);
    do_items.push(sym("do", head_pos));
    do_items.extend(items[2..].iter().cloned());
    let body = list(do_items, pos);

    let (then_form, else_form) = if negate {
        (nil(head_pos), body)
    } else {
        (body, nil(head_pos))
    };
    list(vec![sym("if", head_pos), test, then_form, else_form], pos)
}

/// `(cond t1 e1 t2 e2 ...)` → `(if t1 e1 (cond t2 e2 ...))`, and
/// `(cond)` → `nil`.
///
/// Each step peels off one test/expr pair; the re-walk in
/// [`desugar_list`] expands the remaining `(cond ...)`, so the result
/// is a chain of nested `if`s ending in `nil` when no test matches
/// (matches Clojure). The `:else expr` convention needs no support:
/// `:else` is a truthy keyword.
fn expand_cond(items: &[Form], pos: &Position) -> Form {
    let clauses = &items[1..];
    if clauses.len() % 2 != 0 {
        sdl_panic!(
            pos,
            "cond requires an even number of forms (test/expr pairs), got {}",
            clauses.len()
        );
    }
    if clauses.is_empty() {
        return nil(pos);
    }
    let head_pos = &items[0].pos;

    let mut rest = Vec::with_capacity(clauses.len() - 1);
    rest.push(items[0].clone());
    rest.extend(clauses[2..].iter().cloned());

    list(
        vec![
            sym("if", head_pos),
            clauses[0].clone(),
            clauses[1].clone(),
            list(rest, pos),
        ],
        pos,
    )
}

/// `(-> x step...)` / `(->> x step...)` — thread `x` through each
/// step. A list step `(f a b)` becomes `(f x a b)` for `->` or
/// `(f a b x)` for `->>`; any other step `f` becomes `(f x)`. With no
/// steps the result is just `x`.
///
/// The whole chain is rewritten in one expansion step, innermost
/// first, so `(-> x f g)` becomes `(g (f x))`.
fn expand_thread(items: &[Form], pos: &Position, first: bool) -> Form {
    let name = if first { "->" } else { "->>" };
    let mut acc = match items.get(1) {
        Some(x) => x.clone(),
        None => sdl_panic!(pos, "{} requires at least 1 argument", name),
    };

    for step in &items[2..] {
        acc = match &step.kind {
            FormKind::List(parts) => {
                if parts.is_empty() {
                    sdl_panic!(step.pos, "{}: thread step cannot be an empty list", name);
                }
                let mut call = Vec::with_capacity(parts.len() + 1);
                call.push(parts[0].clone());
                if first {
                    call.push(acc);
                    call.extend(parts[1..].iter().cloned());
                } else {
                    call.extend(parts[1..].iter().cloned());
                    call.push(acc);
                }
                list(call, &step.pos)
            }
            _ => list(vec![step.clone(), acc], &step.pos),
        };
    }
    acc
}
