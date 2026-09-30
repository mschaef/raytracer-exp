// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Integration test harness for the SDL.
//!
//! Each test in this file evaluates one `tests/sdl/<name>.lisp`
//! script in a fresh interpreter. An SDL panic (e.g. from a failed
//! `assert`) bubbles out as a Rust test failure naming the offending
//! script.
//!
//! Run all scripts:
//!
//! ```sh
//! cargo test --test sdl_suite
//! ```
//!
//! Run a single script (Rust filters tests by name substring):
//!
//! ```sh
//! cargo test --test sdl_suite -- closures
//! ```
//!
//! ## Adding a new test script
//!
//! 1. Drop a `tests/sdl/<name>.lisp` file. Use `_` rather than `-`
//!    in the filename so it's a valid Rust identifier.
//! 2. Add a line `sdl_test!(<name>);` below.
//!
//! The macro line is one-time boilerplate per file; in exchange,
//! each script becomes its own `#[test]`, so cargo reports a real
//! test count and you can run scripts individually by name.
//!
//! The `all_scripts_have_a_test` test below is a guard: it walks the
//! directory and panics if any `.lisp` file is missing its
//! `sdl_test!` declaration, so a forgotten line shows up as a test
//! failure rather than silently skipping the script.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use raytracer::sdl;
use raytracer::sdl::value::Value;

/// The list of declared script tests. Kept manually in sync with
/// `tests/sdl/*.lisp`. The `all_scripts_have_a_test` test below
/// catches drift.
const DECLARED: &[&str] = &[
    "arithmetic",
    "bindings_bvh",
    "bindings_camera",
    "bindings_csg",
    "bindings_lights",
    "bindings_mesh",
    "bindings_scene",
    "bindings_shapes",
    "bindings_surface",
    "bindings_transforms",
    "closures",
    "comparison",
    "control_flow",
    "def_let",
    "defn",
    "destructuring",
    "fn_form",
    "for_comprehension",
    "hofs",
    "lights_in_objects",
    "literals",
    "load_form",
    "load_form_fixture",
    "logic",
    "map_ops",
    "math",
    "points",
    "predicates",
    "quote",
    "random",
    "recur",
    "render_dispatch",
    "clip_stats",
    "strings",
    "threading",
    "vec_ops",
    "with_surface",
];

fn run_script(name: &str) {
    let path = sdl_dir().join(format!("{}.lisp", name));
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("could not read {}: {}", path.display(), e));
    // Pass the absolute path (not just the filename) so the
    // CurrentDirGuard installed by eval_source picks up
    // tests/sdl/ as the base for any (load ...) calls inside
    // the script — the load_form.lisp test depends on this.
    let env = sdl::default_env();
    sdl::eval_source(&source, &path.to_string_lossy(), &env);
}

fn sdl_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("sdl")
}

macro_rules! sdl_test {
    ($name:ident) => {
        #[test]
        fn $name() {
            run_script(stringify!($name));
        }
    };
}

sdl_test!(arithmetic);
sdl_test!(bindings_bvh);
sdl_test!(bindings_camera);
sdl_test!(bindings_csg);
sdl_test!(bindings_lights);
sdl_test!(bindings_mesh);
sdl_test!(bindings_scene);
sdl_test!(bindings_shapes);
sdl_test!(bindings_surface);
sdl_test!(bindings_transforms);
sdl_test!(closures);
sdl_test!(comparison);
sdl_test!(control_flow);
sdl_test!(def_let);
sdl_test!(defn);
sdl_test!(destructuring);
sdl_test!(fn_form);
sdl_test!(for_comprehension);
sdl_test!(hofs);
sdl_test!(lights_in_objects);
sdl_test!(literals);
sdl_test!(load_form);
sdl_test!(load_form_fixture);
sdl_test!(logic);
sdl_test!(map_ops);
sdl_test!(math);
sdl_test!(points);
sdl_test!(predicates);
sdl_test!(quote);
sdl_test!(random);
sdl_test!(recur);
sdl_test!(render_dispatch);
sdl_test!(clip_stats);
sdl_test!(strings);
sdl_test!(threading);
sdl_test!(vec_ops);
sdl_test!(with_surface);

/// End-to-end render-dispatch test: build a small scene in script,
/// render it through the SDL bindings, save the result, and verify
/// the resulting file is a valid PNG of the expected dimensions.
///
/// Lives outside the `tests/sdl/*.lisp` discovery loop because it
/// needs the host to inject an `OUTPUT-PATH` binding and to do
/// post-render filesystem assertions — the script itself can't do
/// either. The script source is inline here for the same reason: it's
/// tied to the surrounding Rust harness, not standalone.
#[test]
fn render_dispatch_save() {
    let env = sdl::default_env();

    // Use a per-test-name path under the platform tempdir so the
    // file name is unique across the suite. `process::id` would also
    // disambiguate across concurrent test binaries if that ever
    // mattered.
    let path = std::env::temp_dir().join(format!(
        "sdl_phase3_render_dispatch_save_{}.png",
        std::process::id()
    ));
    // Clean up any leftover from a prior run before we render — a
    // failed previous run could have left a file behind, and we want
    // the existence check below to actually mean "this run wrote it."
    let _ = fs::remove_file(&path);

    let path_str = path.to_string_lossy().to_string();
    env.borrow_mut().define(
        "OUTPUT-PATH",
        Value::String(Rc::new(path_str.clone())),
    );

    let source = r#"
(def red (surface {:color [1.0 0.2 0.2] :ambient 0.4 :light 0.6}))
(def s (scene {:name "phase3-save"
               :camera (camera-looking-at [0 0 5] [0 0 0] [0 1 0] 1.0)
               :background [0 0 0]
               :objects [(light-white [10 10 10])
                         (sphere {:center [0 0 0] :r 1.0 :surface red})]
               :reflect-limit 0
               :min-samples 1
               :max-samples 1}))
(def t (png-target 16 16))
(render s t 16 16)
(save-png t OUTPUT-PATH)
"#;

    sdl::eval_source(source, "render_dispatch_save.lisp", &env);

    assert!(
        path.exists(),
        "save-png must produce a file at {}",
        path.display()
    );

    // Open the saved PNG and check its dimensions match what the
    // script asked for. The `image` crate is already a regular
    // dependency (it's what PngTarget writes through), so this is
    // free. Convert immediately to a concrete `RgbImage` so the
    // dimension/get_pixel calls below are inherent methods on the
    // buffer, not trait methods that would need `GenericImageView`
    // imported.
    let rgb = image::open(&path)
        .unwrap_or_else(|e| panic!("save-png file must be a readable PNG: {}", e))
        .to_rgb8();
    assert_eq!(rgb.width(), 16, "PNG width");
    assert_eq!(rgb.height(), 16, "PNG height");

    // Spot-check: a red sphere centered in the frame, lit by a white
    // light, should produce at least one non-black pixel near the
    // center. Don't pin exact values — anti-aliasing, lighting math,
    // and oversampling all interact and we just want to confirm the
    // renderer wrote *something* sensible. If this ever flakes, the
    // problem is upstream of the SDL bindings.
    let center = rgb.get_pixel(8, 8).0;
    assert!(
        center[0] > 0 || center[1] > 0 || center[2] > 0,
        "center pixel should be lit, got {:?}",
        center
    );

    fs::remove_file(&path).ok();
}

/// Phase-2 surface-decoupling validation. A `(scene ...)` containing
/// a leaf with no `:surface` and no enclosing `(with-surface ...)`
/// must `sdl-panic!` at scene-build time — `Shape::validate_surfaces`
/// in the binding catches it before the scene is ever constructed.
///
/// `sdl::catch_errors` is what `main` and `sdl_run` use to print a
/// script error as one line instead of a panic report. It must hand
/// back the position-tagged message for read and eval errors, and a
/// normal result when nothing goes wrong.
#[test]
fn catch_errors_returns_sdl_error_messages() {
    let eval = |source: &str| {
        let env = sdl::default_env();
        sdl::catch_errors(|| sdl::eval_source(source, "catch_errors_test.lisp", &env))
    };
    let cases: &[(&str, &str)] = &[
        ("(def x 1)\n(+ x \"two\")", "eval error at catch_errors_test.lisp:2:2: + expected"),
        ("(def x [1 2", "read error at catch_errors_test.lisp:1:8: unterminated vector"),
        ("(load-obj \"no_such_mesh.obj\")", "load-obj: Failed to load OBJ"),
        ("(camera-looking-at [0 5 0] [0 0 0] [0 1 0] 1.0)", "up-hint is parallel"),
        ("(camera-looking-at [1 2 3] [1 2 3] [0 1 0] 1.0)", "are the same point"),
    ];
    for &(source, expected) in cases.iter() {
        match eval(source) {
            Ok(v) => panic!("{:?} should fail, got {}", source, v),
            Err(failure) => assert!(
                failure.message.contains(expected),
                "{:?}: expected {:?} in {:?}",
                source,
                expected,
                failure.message
            ),
        }
    }
    assert!(matches!(eval("(+ 1 2)"), Ok(sdl::Value::Int(3))));
}

/// `SdlFailure::trace` is the SDL call stack at the error, innermost
/// first: named and anonymous functions, natives that call back into
/// the script (`map`), and `(load ...)`s still running. The native that
/// raised the error is left out, since the message already gives its
/// position.
#[test]
fn catch_errors_reports_the_sdl_call_stack() {
    let dir = std::env::temp_dir().join(format!("sdl_trace_test_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("helpers.lisp"),
        "(defn scale-it [x]\n  (* x \"two\"))\n(defn build [xs]\n  (map (fn [x] (scale-it x)) xs))\n(def early (build [1]))\n",
    )
    .unwrap();
    let main = dir.join("main.lisp");
    fs::write(&main, "(def x 1)\n(load \"helpers.lisp\")\n").unwrap();

    let env = sdl::default_env();
    let main_path = main.to_string_lossy().to_string();
    let failure = sdl::catch_errors(|| sdl::eval_source(&fs::read_to_string(&main).unwrap(), &main_path, &env))
        .expect_err("script must fail");
    let helpers = dir.join("helpers.lisp").to_string_lossy().to_string();
    assert_eq!(failure.message, format!("eval error at {}:2:4: * expected an integer, got \"two\" (string)", helpers));
    assert_eq!(
        failure.trace,
        vec![
            format!("in scale-it, called at {}:4:17", helpers),
            format!("in fn defined at {}:4:8, called at {}:4:4", helpers, helpers),
            format!("in map, called at {}:4:4", helpers),
            format!("in build, called at {}:5:13", helpers),
            format!("in (load {:?}) at {}:2:1", "helpers.lisp", main_path),
        ]
    );
    assert!(failure.to_string().contains(&format!("\n  in build, called at {}:5:13", helpers)));

    // The stack unwound with the error: the next one, at the top level,
    // has none.
    let top = sdl::catch_errors(|| sdl::eval_source("(+ 1 \"x\")", "top.lisp", &sdl::default_env()))
        .expect_err("must fail");
    assert!(top.trace.is_empty(), "stale frames: {:?}", top.trace);
    let _ = fs::remove_dir_all(&dir);
}

/// Recursion collapses into one line with a count; mutual recursion,
/// which doesn't repeat line by line, is cut to the innermost and
/// outermost 10 lines.
#[test]
fn catch_errors_shortens_recursive_call_stacks() {
    let eval = |source: &str| {
        sdl::catch_errors(|| sdl::eval_source(source, "rec.lisp", &sdl::default_env())).expect_err("must fail")
    };
    let direct = eval("(defn down [n] (if (= n 0) (+ 1 \"x\") (+ 1 (down (- n 1)))))\n(down 40)");
    assert_eq!(
        direct.trace,
        vec!["in down, called at rec.lisp:1:44 (40 times)".to_string(), "in down, called at rec.lisp:2:2".to_string()]
    );
    let mutual = eval("(defn ev? [n] (if (= n 0) (first 5) (od? (- n 1))))\n(defn od? [n] (ev? (- n 1)))\n(ev? 60)");
    assert_eq!(mutual.trace.len(), 21);
    assert_eq!(mutual.trace[10], "... 41 more ...");
    assert_eq!(mutual.trace[20], "in ev?, called at rec.lisp:3:2");
}

/// `SDL_RUST_BACKTRACE` adds the Rust backtrace to the report.
#[test]
fn catch_errors_adds_the_rust_backtrace_on_request() {
    std::env::set_var(sdl::RUST_BACKTRACE_VAR, "1");
    let failure = sdl::catch_errors(|| sdl::eval_source("(+ 1 \"x\")", "bt.lisp", &sdl::default_env()));
    std::env::remove_var(sdl::RUST_BACKTRACE_VAR);
    let failure = failure.expect_err("must fail");
    let bt = failure.rust_backtrace.as_deref().expect("backtrace requested");
    assert!(!bt.is_empty());
    assert!(failure.to_string().contains("\n\nRust backtrace:\n"));
}

/// A panic that isn't an SDL error is a renderer bug, not a script
/// mistake: `catch_errors` must let it carry on unwinding (with its
/// usual report) rather than turn it into an error message.
#[test]
fn catch_errors_passes_other_panics_through() {
    let outer = std::panic::catch_unwind(|| {
        let _ = sdl::catch_errors(|| -> () { panic!("renderer bug") });
    });
    let payload = outer.expect_err("a non-SDL panic must propagate");
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"renderer bug"));
}

/// The positive cases (unsurfaced leaves under a wrapper, mixed
/// explicit and inherited surfaces) live in `tests/sdl/with_surface.lisp`
/// and run through the `sdl_test!` harness. This test owns the
/// failure case because the harness has no "expect panic" form;
/// `std::panic::catch_unwind` here turns the expected panic into a
/// passing test.
#[test]
fn with_surface_validation_fails_on_unsurfaced_leaf() {
    // The sphere has neither :surface nor an enclosing with-surface.
    // Scene construction must reject this.
    let source = r#"
(def cam (camera-looking-at [0 0 5] [0 0 0] [0 1 0] 1.0))
(scene {:name "bad"
        :camera cam
        :background [0 0 0]
        :objects [(light-white [10 10 10])
                  (sphere {:center [0 0 0] :r 1.0})]
        :reflect-limit 0
        :min-samples 1
        :max-samples 1})
"#;
    let env = sdl::default_env();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        sdl::eval_source(source, "validation_failure_test.lisp", &env);
    }));
    assert!(
        result.is_err(),
        "scene construction must reject an unsurfaced sphere with no with-surface ancestor",
    );
}

/// Malformed sugar forms (`defn`, `when`, `when-not`, `cond`, `->`,
/// `->>`) must be rejected by the desugaring pass with an error that
/// names the form, rather than falling through to a confusing
/// downstream failure in the core form it expands into. The harness
/// has no "expect panic" form, so the failure cases live here.
#[test]
fn desugar_rejects_malformed_forms() {
    // (source, text the error must contain, description)
    let cases: &[(&str, &str, &str)] = &[
        ("(defn)", "defn", "defn: missing name"),
        ("(defn f)", "defn", "defn: missing parameter vector"),
        ("(defn \"f\" [x] x)", "defn", "defn: non-symbol name"),
        ("(defn f \"doc\")", "defn", "defn: docstring with no parameter vector"),
        ("(defn f x x)", "defn", "defn: non-vector parameters"),
        ("(defn f ([x] x) ([x y] y))", "defn", "defn: multi-arity definition"),
        ("(when)", "when", "when: missing test"),
        ("(when-not)", "when-not", "when-not: missing test"),
        ("(cond true)", "cond", "cond: odd number of forms"),
        ("(cond true 1 false)", "cond", "cond: dangling test"),
        ("(->)", "->", "->: missing value"),
        ("(->>)", "->>", "->>: missing value"),
        ("(-> 1 ())", "->", "->: empty-list step"),
        ("(->> 1 ())", "->>", "->>: empty-list step"),
        ("(for)", "for", "for: missing bindings"),
        ("(for x x)", "for", "for: non-vector bindings"),
        ("(for [])", "for", "for: missing body"),
        ("(for [] 1)", "for", "for: no bindings"),
        ("(for [x] x)", "for", "for: odd binding forms"),
        ("(for [x [1]] x x)", "for", "for: several body forms"),
        ("(for [x [1] :when] x)", "for", "for: :when without a test"),
        ("(for [x [1] :let x] x)", "for", "for: :let without a vector"),
        ("(for [x [1] :while true] x)", "for", "for: unknown modifier"),
    ];
    for &(source, expected, what) in cases.iter() {
        let env = sdl::default_env();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sdl::eval_source(source, "desugar_malformed.lisp", &env);
        }));
        let payload = match result {
            Ok(_) => panic!("must reject {}: {}", what, source),
            Err(p) => p,
        };
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_default();
        assert!(
            message.contains(expected),
            "error for {} ({}) should mention {:?}, got: {}",
            what,
            source,
            expected,
            message
        );
    }
}

/// CSG constructors reject what can't be a CSG operand: fewer than two
/// operands, and anything that isn't a solid (a triangle, a group or
/// transform containing one, or a `load-obj` mesh). A script can't
/// catch these, so they're checked from Rust like the desugar errors.
#[test]
fn csg_rejects_bad_operands() {
    let ball = "(sphere {:center [0 0 0] :r 1})";
    let tri = "(triangle {:vertices [[0 0 0] [1 0 0] [0 1 0]]})";
    let mesh = format!(
        "(load-obj {:?} (surface {{:color [1 1 1]}}))",
        sdl_dir().join("load_obj_fixture.obj").to_string_lossy()
    );
    // (source, text the error must contain, description)
    let cases: Vec<(String, &str, &str)> = vec![
        (format!("(difference {})", ball), "at least 2", "difference: one operand"),
        ("(intersection)".to_string(), "at least 2", "intersection: no operands"),
        (format!("(merge {})", ball), "at least 2", "merge: one operand"),
        (format!("(merge {} {})", ball, tri), "operand 2", "merge: triangle operand"),
        (format!("(difference {} {})", ball, tri), "operand 2", "difference: triangle operand"),
        (format!("(intersection {} {})", tri, ball), "operand 1", "intersection: triangle operand"),
        (format!("(difference {} (translate [1 0 0] (group [{} {}])))", ball, ball, tri),
         "not a solid", "difference: triangle inside a transformed group"),
        (format!("(difference {} {} {})", ball, ball, mesh), "operand 3", "difference: mesh operand"),
    ];
    for (source, expected, what) in cases.iter() {
        let env = sdl::default_env();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sdl::eval_source(source, "csg_bad_operands.lisp", &env);
        }));
        let payload = match result {
            Ok(_) => panic!("must reject {}: {}", what, source),
            Err(p) => p,
        };
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_default();
        assert!(
            message.contains(expected),
            "error for {} ({}) should mention {:?}, got: {}",
            what,
            source,
            expected,
            message
        );
    }
}

/// `torus` rejects parameters that don't describe a ring torus.
#[test]
fn torus_rejects_bad_parameters() {
    // (source, text the error must contain, description)
    let cases: &[(&str, &str, &str)] = &[
        ("(torus {:major 1 :minor 1})", "0 < :minor < :major", "minor == major"),
        ("(torus {:major 1 :minor 2})", "0 < :minor < :major", "minor > major"),
        ("(torus {:major 1 :minor 0})", "0 < :minor < :major", "zero minor"),
        ("(torus {:major 1 :minor 0.5 :axis [0 0 0]})", ":axis", "zero axis"),
        ("(torus {:minor 0.5})", "major", "missing major"),
    ];
    for &(source, expected, what) in cases.iter() {
        let env = sdl::default_env();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sdl::eval_source(source, "torus_bad.lisp", &env);
        }));
        let message = match result {
            Ok(_) => panic!("must reject {}: {}", what, source),
            Err(p) => p.downcast_ref::<String>().cloned().unwrap_or_default(),
        };
        assert!(
            message.contains(expected),
            "error for {} ({}) should mention {:?}, got: {}",
            what, source, expected, message
        );
    }
}

/// A scene built with `(bvh ...)` must render byte-identically to the
/// same shapes in a `(group ...)`: the BVH is purely an acceleration
/// structure. Uses a few hundred jittered spheres (so the tree is several
/// levels deep), a plane (kept outside the tree), reflection (so
/// secondary rays traverse it too) and shadows.
#[test]
fn bvh_render_equivalence() {
    let env = sdl::default_env();
    let pid = std::process::id();
    let path_a = std::env::temp_dir().join(format!("sdl_bvh_eq_a_{}.png", pid));
    let path_b = std::env::temp_dir().join(format!("sdl_bvh_eq_b_{}.png", pid));
    let _ = fs::remove_file(&path_a);
    let _ = fs::remove_file(&path_b);
    env.borrow_mut().define("PATH-A", Value::String(Rc::new(path_a.to_string_lossy().to_string())));
    env.borrow_mut().define("PATH-B", Value::String(Rc::new(path_b.to_string_lossy().to_string())));

    let source = r#"
(def shiny (surface {:color [0.9 0.3 0.2] :ambient 0.2 :specular 0.5 :light 0.6 :reflection 0.3}))
(def floor-s (surface {:color [0.2 0.2 0.2] :ambient 0.2 :specular 0.5 :light 0.6 :checked true}))
(def balls
  (for [i (range 12) j (range 12) :when (< (random 3 i j) 0.8)]
    (sphere {:center [(- i 6) (- j 6) (* 0.3 (random-gaussian 3 i j))]
             :r (+ 0.15 (* 0.3 (random 4 i j)))})))
(defn make-scene [name shapes]
  (scene {:name name
          :camera (camera-looking-at [0 -14 8] [0 0 0] [0 0 1] 1.0)
          :background [0 0 0]
          :reflect-limit 2
          :min-samples 1
          :max-samples 1
          :objects [(light-white [5 -5 10])
                    (with-surface shiny shapes)
                    (plane {:normal [0 0 1] :p0 [0 0 -1] :surface floor-s})]}))
(def t-a (png-target 48 48))
(render (make-scene "bvh-eq-group" (group balls)) t-a 48 48)
(save-png t-a PATH-A)
(def t-b (png-target 48 48))
(render (make-scene "bvh-eq-bvh" (bvh balls)) t-b 48 48)
(save-png t-b PATH-B)
"#;
    sdl::eval_source(source, "bvh_render_equivalence.lisp", &env);

    let a = image::open(&path_a).unwrap_or_else(|e| panic!("decode PATH-A: {}", e)).to_rgb8();
    let b = image::open(&path_b).unwrap_or_else(|e| panic!("decode PATH-B: {}", e)).to_rgb8();
    assert!(a.as_raw().iter().any(|&c| c > 0), "the group render is all black");
    assert_eq!(a.as_raw(), b.as_raw(), "bvh and group renders differ");
    let _ = fs::remove_file(&path_a);
    let _ = fs::remove_file(&path_b);
}

/// `(light {...})` rejects malformed or contradictory keys.
#[test]
fn light_rejects_bad_keys() {
    // (source, text the error must contain, description)
    let cases: &[(&str, &str, &str)] = &[
        ("(light {})", "location", "missing location"),
        ("(light {:location [0 0 0] :colour [1 1 1]})", "unknown key :colour", "typo"),
        ("(light {:location [0 0 0] :direction [0 0 -1] :point-at [0 0 -1] :inner-angle 0 :outer-angle 1})",
         "not both", "direction and point-at"),
        ("(light {:location [0 0 0] :inner-angle 0.1 :outer-angle 0.2})", "need :direction", "angles without a direction"),
        ("(light {:location [0 0 0] :direction [0 0 -1] :inner-angle 0.1})", "both :inner-angle and :outer-angle", "one angle"),
        ("(light {:location [0 0 0] :direction [0 0 -1] :inner-angle 0.5 :outer-angle 0.2})", "must be ≤", "reversed angles"),
        ("(light {:location [0 0 0] :point-at [0 0 0] :inner-angle 0 :outer-angle 1})", "non-zero", "point-at the location"),
        ("(light {:location [0 0 0] :radius 1})", "needs :axis", "disk without an axis"),
        ("(light {:location [0 0 0] :radius 0 :axis [0 0 1]})", "positive", "zero radius"),
        ("(light {:location [0 0 0] :axis [0 0 1]})", ":axis only applies", "axis without radius"),
        ("(light {:location [0 0 0] :radius 1 :axis [0 0 1] :area-u [1 0 0] :area-v [0 1 0]})", "not both", "disk and quad"),
        ("(light {:location [0 0 0] :area-u [1 0 0]})", "both :area-u and :area-v", "one edge"),
        ("(light {:location [0 0 0] :area-u [1 0 0] :area-v [2 0 0]})", "not parallel", "parallel edges"),
    ];
    for &(source, expected, what) in cases.iter() {
        let env = sdl::default_env();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sdl::eval_source(source, "light_bad.lisp", &env);
        }));
        let message = match result {
            Ok(_) => panic!("must reject {}: {}", what, source),
            Err(p) => p.downcast_ref::<String>().cloned().unwrap_or_default(),
        };
        assert!(
            message.contains(expected),
            "error for {} ({}) should mention {:?}, got: {}",
            what, source, expected, message
        );
    }
}

/// A light on the far side of a surface from the viewer contributes
/// nothing: a plane lit only from behind (with a specular term, so a
/// spurious highlight would show) must render exactly like the same
/// plane with no light at all. Found porting redball.pov: the unclamped
/// Lambert term went negative and darkened the backdrop, and the even
/// specular exponent turned a negative half-vector dot product into a
/// highlight. Closed objects mostly hide this because a point facing
/// away from a light is in its own object's shadow; shadowless lights
/// and open surfaces don't.
///
/// Since back faces phase 1 (history entry 62), "behind" means behind
/// relative to the viewer: shading turns the normal toward the ray, so
/// the camera here sees the plane's back and the lights sit beyond it.
/// (Before, the lights were on the camera's side, behind the plane's
/// normal, and that plane now lights up, as POV's two-sided planes do.)
#[test]
fn back_lit_surfaces_get_no_light() {
    let env = sdl::default_env();
    let pid = std::process::id();
    let path_a = std::env::temp_dir().join(format!("sdl_backlit_a_{}.png", pid));
    let path_b = std::env::temp_dir().join(format!("sdl_backlit_b_{}.png", pid));
    let _ = fs::remove_file(&path_a);
    let _ = fs::remove_file(&path_b);
    env.borrow_mut().define("PATH-A", Value::String(Rc::new(path_a.to_string_lossy().to_string())));
    env.borrow_mut().define("PATH-B", Value::String(Rc::new(path_b.to_string_lossy().to_string())));

    let source = r#"
(def wall (plane {:normal [0 0 1] :p0 [0 0 10]
                  :surface (surface {:color [0.5 0.5 0.5] :ambient 0.6 :light 0.8 :specular 1.0})}))
(defn make-scene [lights]
  (scene {:name "backlit"
          :camera (camera-looking-at [0 0 -3] [0 0 0] [0 1 0] 1.0)
          :background [0 0 0]
          :min-samples 1
          :max-samples 1
          :objects (conj lights wall)}))
(def t-a (png-target 24 24))
(render (make-scene [(light-white [4 4 14])
                     (light {:location [-2 1 11] :shadowless true})]) t-a 24 24)
(save-png t-a PATH-A)
(def t-b (png-target 24 24))
(render (make-scene []) t-b 24 24)
(save-png t-b PATH-B)
"#;
    sdl::eval_source(source, "back_lit_surfaces.lisp", &env);

    let a = image::open(&path_a).unwrap_or_else(|e| panic!("decode PATH-A: {}", e)).to_rgb8();
    let b = image::open(&path_b).unwrap_or_else(|e| panic!("decode PATH-B: {}", e)).to_rgb8();
    assert!(b.as_raw().iter().any(|&c| c > 0), "the unlit render is all black");
    assert_eq!(a.as_raw(), b.as_raw(), "lights behind the wall changed its shading");
    let _ = fs::remove_file(&path_a);
    let _ = fs::remove_file(&path_b);
}

/// A mirror reflects toward the side the ray came from. The camera
/// looks along +y at a mirror plane facing it and tilted down, so the
/// true reflection heads down to a green floor, not up to a red
/// ceiling. Before history entry 60 the reflected direction was
/// flipped, and this rendered red.
#[test]
fn mirror_reflects_the_right_way() {
    let env = sdl::default_env();
    let path = std::env::temp_dir().join(format!("sdl_mirror_{}.png", std::process::id()));
    let _ = fs::remove_file(&path);
    env.borrow_mut().define("PATH", Value::String(Rc::new(path.to_string_lossy().to_string())));
    let source = r#"
(def s (scene {:name "mirror"
               :camera (camera-looking-at [0 0 0] [0 1 0] [0 0 1] 1.0)
               :background [0 0 0]
               :min-samples 1 :max-samples 1
               :view {:curve :clip}
               :objects [(plane {:normal (normalize [0 -1 -1]) :p0 [0 5 0]
                                 :surface (surface {:color [0 0 0] :ambient 0 :light 0 :reflection 1.0})})
                         (plane {:normal [0 0 -1] :p0 [0 0 20]
                                 :surface (surface {:color [1 0 0] :ambient 1 :light 0})})
                         (plane {:normal [0 0 1] :p0 [0 0 -20]
                                 :surface (surface {:color [0 1 0] :ambient 1 :light 0})})]}))
(def t (png-target 5 5))
(render s t 5 5)
(save-png t PATH)
"#;
    sdl::eval_source(source, "mirror.lisp", &env);
    let img = image::open(&path).unwrap_or_else(|e| panic!("decode: {}", e)).to_rgb8();
    let centre = img.get_pixel(2, 2).0;
    assert_eq!(centre, [0, 255, 0], "the mirror should show the green floor, got {:?}", centre);
    let _ = fs::remove_file(&path);
}

/// Render `source` (which must `(save-png t PATH)`) and return the
/// image, for tests that compare pixels.
fn render_to_image(source: &str, tag: &str) -> image::RgbImage {
    let env = sdl::default_env();
    let path = std::env::temp_dir().join(format!("sdl_{}_{}.png", tag, std::process::id()));
    let _ = fs::remove_file(&path);
    env.borrow_mut().define("PATH", Value::String(Rc::new(path.to_string_lossy().to_string())));
    sdl::eval_source(source, &format!("{}.lisp", tag), &env);
    let img = image::open(&path).unwrap_or_else(|e| panic!("decode {}: {}", tag, e)).to_rgb8();
    let _ = fs::remove_file(&path);
    img
}

/// Back faces phase 2: a transparent object is applied once, where the
/// ray enters. Looking straight down the axis of a glass cylinder, and
/// of the same cylinder built with CSG, must give the same colour as a
/// glass box whose front face sits at the same place: one blend of the
/// front face with what's behind. Before, the cylinder and the CSG
/// shape were also blended at their far end (an exit), so they came out
/// denser than the box.
#[test]
fn glass_is_blended_once_per_object() {
    let pixel = |object: &str, tag: &str| {
        let source = format!(
            r#"
(def glass (surface {{:color [0.2 0.4 0.9] :ambient 0.2 :light 0.5 :transparency 0.6}}))
(def s (scene {{:name "glass"
               :camera (camera-looking-at [0 0 -5] [0 0 0] [0 1 0] 1.0)
               :background [1 0 0]
               :min-samples 1 :max-samples 1
               :view {{:curve :clip}}
               :objects [(light-white [0 0 -10])
                         (with-surface glass {})]}}))
(def t (png-target 5 5))
(render s t 5 5)
(save-png t PATH)
"#,
            object
        );
        render_to_image(&source, tag).get_pixel(2, 2).0
    };
    let boxed = pixel("(cuboid {:center [0 0 1] :size [2 2 2]})", "glass_box");
    let cylinder = pixel("(cylinder {:p0 [0 0 0] :p1 [0 0 2] :r 1})", "glass_cyl");
    let csg = pixel(
        "(intersection (cylinder {:p0 [0 0 0] :p1 [0 0 2] :r 1}) (cuboid {:center [0 0 1] :size [4 4 4]}))",
        "glass_csg",
    );
    assert_eq!(cylinder, boxed, "glass cylinder vs glass box");
    assert_eq!(csg, boxed, "glass CSG vs glass box");
    // And it is a blend: some of the red background shows through.
    assert!(boxed[0] > boxed[2] / 2 && boxed != [255, 0, 0], "{:?}", boxed);
}

/// Back faces phase 2, shadows: a shadow ray through a glass object is
/// attenuated once (on entry), so a glass cylinder between a light and a
/// wall darkens it exactly as much as a glass box does. The camera looks
/// at the wall from the side, past the glass, so only the shadow ray
/// crosses it.
#[test]
fn glass_shadows_attenuate_once_per_object() {
    let pixel = |object: &str, tag: &str| {
        let source = format!(
            r#"
(def glass (surface {{:color [1 1 1] :ambient 0 :light 0 :transparency 0.5}}))
(def s (scene {{:name "glass-shadow"
               :camera (camera-looking-at [3 4 -5] [3 0 5] [0 1 0] 10.0)
               :background [0 0 0]
               :min-samples 1 :max-samples 1
               :view {{:curve :clip}}
               :objects [(light-white [3 0 -10])
                         (plane {{:normal [0 0 -1] :p0 [0 0 5]
                                 :surface (surface {{:color [1 1 1] :ambient 0 :light 0.8}})}})
                         {}]}}))
(def t (png-target 5 5))
(render s t 5 5)
(save-png t PATH)
"#,
            object
        );
        render_to_image(&source, tag).get_pixel(2, 2).0
    };
    let nothing = pixel("(sphere {:center [100 100 100] :r 0.1 :surface glass})", "shadow_none");
    let boxed = pixel("(with-surface glass (cuboid {:center [3 0 1] :size [1 1 2]}))", "shadow_box");
    let cylinder = pixel("(with-surface glass (cylinder {:p0 [3 0 0] :p1 [3 0 2] :r 0.5}))", "shadow_cyl");
    assert!(boxed[0] < nothing[0] && boxed[0] > 0, "box {:?} vs unshadowed {:?}", boxed, nothing);
    assert_eq!(cylinder, boxed, "glass cylinder's shadow vs glass box's");
}

/// A transparent surface's reflection sits on top of its transparency
/// blend at full strength, as in POV-Ray. A half-transparent mirror with
/// nothing behind it reflects the green floor as brightly as an opaque
/// one; before history entry 63 the reflection was scaled by `1 - T`
/// and came out half as bright.
#[test]
fn transparency_does_not_dim_reflections() {
    let pixel = |transparency: f64, tag: &str| {
        let source = format!(
            r#"
(def s (scene {{:name "glass-mirror"
               :camera (camera-looking-at [0 0 0] [0 1 0] [0 0 1] 1.0)
               :background [0 0 0]
               :min-samples 1 :max-samples 1
               :view {{:curve :clip}}
               :objects [(plane {{:normal (normalize [0 -1 -1]) :p0 [0 5 0]
                                 :surface (surface {{:color [0 0 0] :ambient 0 :light 0
                                                     :reflection 1.0 :transparency {}}})}})
                         (plane {{:normal [0 0 1] :p0 [0 0 -20]
                                 :surface (surface {{:color [0 1 0] :ambient 1 :light 0}})}})]}}))
(def t (png-target 5 5))
(render s t 5 5)
(save-png t PATH)
"#,
            transparency
        );
        render_to_image(&source, tag).get_pixel(2, 2).0
    };
    assert_eq!(pixel(0.0, "mirror_opaque"), [0, 255, 0]);
    assert_eq!(pixel(0.5, "mirror_glass"), [0, 255, 0]);
}

/// A surface's `:pigment` map rejects malformed or contradictory keys.
#[test]
fn pigment_rejects_bad_keys() {
    let wood = "{:pattern :wood :color-map [[0 [1 1 1]] [1 [0 0 0]]]";
    // (source, text the error must contain, description)
    let cases: Vec<(String, &str, &str)> = vec![
        ("(surface {:pigment {:color-map [[0 [1 1 1]]]}})".to_string(), "missing :pattern", "no pattern"),
        ("(surface {:pigment {:pattern :marble :color-map [[0 [1 1 1]]]}})".to_string(), "unknown :pattern", "unknown pattern"),
        ("(surface {:pigment {:pattern \"wood\" :color-map [[0 [1 1 1]]]}})".to_string(), "must be a keyword", "string pattern"),
        ("(surface {:pigment {:pattern :wood}})".to_string(), "needs :color-map", "no colours"),
        ("(surface {:pigment {:pattern :wood :color-map []}})".to_string(), "empty", "empty map"),
        ("(surface {:pigment {:pattern :wood :color-map [[1 [1 1 1]] [0 [0 0 0]]]}})".to_string(), "ascend", "descending map"),
        ("(surface {:pigment {:pattern :wood :color-map [[0 [1 1 1] 2]]}})".to_string(), "[value [r g b]]", "bad entry"),
        ("(surface {:pigment {:pattern :checker :colors [[1 1 1]]}})".to_string(), "two colours", "one checker colour"),
        ("(surface {:pigment {:pattern :checker :colors [[1 1 1] [0 0 0]] :color-map [[0 [1 1 1]]]}})".to_string(),
         "not both", "colors and color-map"),
        (format!("(surface {{:pigment {} :wave :square}}}})", wood), "unknown :wave", "unknown wave"),
        (format!("(surface {{:pigment {} :octaves 0}}}})", wood), "between 1 and 10", "zero octaves"),
        (format!("(surface {{:pigment {} :scale 2}}}})", wood), "unknown key :scale", "typo"),
        (format!("(surface {{:pigment {} :turbulence [1 2]}}}})", wood), "turbulence", "two-component turbulence"),
        (format!("(surface {{:pigment {} :turbulence :lots}}}})", wood), "turbulence", "keyword turbulence"),
        ("(surface {:pigment []})".to_string(), "at least one pigment", "empty layer vector"),
        ("(surface {:pigment {:color [1 0 0 0.5] :pattern :wood}})".to_string(), "takes no other keys", "solid with a pattern"),
        ("(surface {:pigment {:color [1 0 0 0.5 1]}})".to_string(), "[r g b t]", "five-component colour"),
        (format!("(surface {{:pigment [{}}} 7]}})", wood), "expected a map", "non-map layer"),
        (format!("(surface {{:pigment {} :transform [1 2 3]}}}})", wood), "transform", "non-affine transform"),
    ];
    for (source, expected, what) in cases.iter() {
        let env = sdl::default_env();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sdl::eval_source(source, "pigment_bad.lisp", &env);
        }));
        let message = match result {
            Ok(_) => panic!("must reject {}: {}", what, source),
            Err(p) => p.downcast_ref::<String>().cloned().unwrap_or_default(),
        };
        assert!(
            message.contains(expected),
            "error for {} ({}) should mention {:?}, got: {}",
            what, source, expected, message
        );
    }
}

#[test]
fn view_rejects_bad_keys() {
    let scene = |view: &str| {
        format!(
            "(scene {{:name \"v\" :camera (camera-looking-at [0 0 5] [0 0 0] [0 1 0] 1.0) \
             :objects [] :view {}}})",
            view
        )
    };
    // (source, text the error must contain, description)
    let cases: Vec<(String, &str, &str)> = vec![
        (scene("{:curve :filmic}"), "unknown :curve :filmic", "unknown curve"),
        (scene("{:curve \"clip\"}"), "must be a keyword", "string curve"),
        (scene("{:exposure :lots}"), "exposure", "keyword exposure"),
        (scene("{:gamma 2.2}"), "unknown key :gamma", "unknown key"),
        (scene("{:curve :aces}"), "unknown :curve :aces", "unlisted curve"),
        (scene("[:clip]"), "expected a map", "not a map"),
        (scene("{:curve :clip :white 4}"), "only applies to :reinhard", "white without reinhard"),
        (scene("{:curve :reinhard :white 0}"), "positive", "zero white"),
        (scene("{:curve :reinhard :white :bright}"), "white", "keyword white"),
    ];
    for (source, expected, what) in cases.iter() {
        let env = sdl::default_env();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sdl::eval_source(source, "view_bad.lisp", &env);
        }));
        let message = match result {
            Ok(_) => panic!("must reject {}: {}", what, source),
            Err(p) => p.downcast_ref::<String>().cloned().unwrap_or_default(),
        };
        assert!(
            message.contains(expected),
            "error for {} ({}) should mention {:?}, got: {}",
            what, source, expected, message
        );
    }
}

#[test]
fn scene_size_rejects_bad_values() {
    let scene = |size: &str| {
        format!(
            "(scene {{:name \"s\" :camera (camera-looking-at [0 0 5] [0 0 0] [0 1 0] 1.0) \
             :objects [] :size {}}})",
            size
        )
    };
    let cases: Vec<(String, &str, &str)> = vec![
        (scene("[640]"), "[width height]", "one number"),
        (scene("[640 480 3]"), "[width height]", "three numbers"),
        (scene("[0 480]"), "positive", "zero width"),
        (scene("[640 -1]"), "non-negative", "negative height"),
        (scene("[640.5 480]"), "width expected an integer", "non-integer width"),
        (scene("640"), "vector", "not a vector"),
    ];
    for (source, expected, what) in cases.iter() {
        let env = sdl::default_env();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sdl::eval_source(source, "size_bad.lisp", &env);
        }));
        let message = match result {
            Ok(_) => panic!("must reject {}: {}", what, source),
            Err(p) => p.downcast_ref::<String>().cloned().unwrap_or_default(),
        };
        assert!(
            message.contains(expected),
            "error for {} ({}) should mention {:?}, got: {}",
            what, source, expected, message
        );
    }
}

/// Lights-as-shapes affine equivalence: a scene with a bare
/// `(light-white [5 5 5])` in `:objects` must render byte-identically
/// to a scene where the same light is positioned by wrapping a
/// origin-located `(light-white [0 0 0])` in `(translate [5 5 5] ...)`.
/// This pins down the invariant that motivated the whole migration —
/// transforms apply to lights the same way they apply to geometry —
/// and is the regression that would catch any future change to
/// `Shape::collect_lights`'s affine accumulation. Pre-stage-2 this
/// test compared `:lights` (the historical field) against `:objects`;
/// after the stage-2 collapse `:lights` is gone, so both scenes use
/// `:objects` and the comparison is "bare placement" vs
/// "translate-wrapped placement."
///
/// The renderer is per-pixel deterministic, so byte-equality is
/// meaningful even with `parallel = true` — any divergence
/// indicates a real arithmetic difference, not scheduling noise.
#[test]
fn lights_in_objects_equivalence() {
    let env = sdl::default_env();

    let pid = std::process::id();
    let path_a = std::env::temp_dir().join(format!("sdl_lights_eq_a_{}.png", pid));
    let path_b = std::env::temp_dir().join(format!("sdl_lights_eq_b_{}.png", pid));
    // Clean up any leftover from a prior run — we need the existence
    // check below to mean "this run wrote it".
    let _ = fs::remove_file(&path_a);
    let _ = fs::remove_file(&path_b);

    env.borrow_mut().define(
        "PATH-A",
        Value::String(Rc::new(path_a.to_string_lossy().to_string())),
    );
    env.borrow_mut().define(
        "PATH-B",
        Value::String(Rc::new(path_b.to_string_lossy().to_string())),
    );

    // Identical surfaces, camera, and geometry in both scenes —
    // only the *expression* used to place the light differs. A is
    // the direct form (bare light at world coordinates). B is the
    // transform-wrapped form (light at origin in local coordinates,
    // translated to the same world position). Anything other than
    // byte-equality means `Shape::collect_lights` is applying the
    // accumulated affine incorrectly.
    let source = r#"
(def red (surface {:color [1.0 0.2 0.2] :ambient 0.2 :specular 0.5 :light 0.6}))
(def white-c (surface {:color [0.2 0.2 0.2] :ambient 0.2 :specular 0.5
                       :light 0.6 :checked true :reflection 0.0}))
(def cam (camera-looking-at [0 6 3] [0 0 0] [0 0 1] 1.0))

(def s-a
  (scene {:name "lights-eq-a"
          :camera cam
          :background [0 0 0]
          :reflect-limit 0
          :min-samples 1
          :max-samples 1
          :objects [(light-white [5 5 5])
                    (sphere {:center [0 0 0] :r 1.0 :surface red})
                    (plane {:normal [0 0 1] :p0 [0 0 -1] :surface white-c})]}))

(def s-b
  (scene {:name "lights-eq-b"
          :camera cam
          :background [0 0 0]
          :reflect-limit 0
          :min-samples 1
          :max-samples 1
          :objects [(translate [5 5 5] (light-white [0 0 0]))
                    (sphere {:center [0 0 0] :r 1.0 :surface red})
                    (plane {:normal [0 0 1] :p0 [0 0 -1] :surface white-c})]}))

(def t-a (png-target 32 32))
(def t-b (png-target 32 32))
(render s-a t-a 32 32)
(render s-b t-b 32 32)
(save-png t-a PATH-A)
(save-png t-b PATH-B)
"#;

    sdl::eval_source(source, "lights_in_objects_equivalence.lisp", &env);

    let rgb_a = image::open(&path_a)
        .unwrap_or_else(|e| panic!("decode PATH-A: {}", e))
        .to_rgb8();
    let rgb_b = image::open(&path_b)
        .unwrap_or_else(|e| panic!("decode PATH-B: {}", e))
        .to_rgb8();

    assert_eq!(rgb_a.dimensions(), (32, 32));
    assert_eq!(rgb_b.dimensions(), (32, 32));

    // Quick sanity: the scene should be lit (at least one non-black
    // pixel). If neither rendered any light, byte-equality would
    // hold trivially and silently mask a bug.
    let any_lit_a = rgb_a.as_raw().iter().any(|&c| c > 0);
    let any_lit_b = rgb_b.as_raw().iter().any(|&c| c > 0);
    assert!(any_lit_a, "scene A produced an all-black render");
    assert!(any_lit_b, "scene B produced an all-black render");

    assert_eq!(
        rgb_a.as_raw(),
        rgb_b.as_raw(),
        "translate-wrapped light render diverged from bare light render \
         — Shape::collect_lights is applying the affine incorrectly \
         (see {} and {})",
        path_a.display(),
        path_b.display(),
    );

    fs::remove_file(&path_a).ok();
    fs::remove_file(&path_b).ok();
}

// ---------------------------------------------------------------------------
// Scene-load smoke tests
// ---------------------------------------------------------------------------
//
// Phase 8 deleted src/scenes.rs and the byte-equivalence harness that
// pinned each .lisp scene against its Rust counterpart. The SDL is now
// the canonical scene-definition mechanism — there's nothing left to
// compare against. We replace the equivalence tests with a much smaller
// smoke test per scene file: load the script, verify the *-scene
// binding is present and is a Value::Scene. This catches script-level
// breakage (parse errors, missing bindings, type mismatches, broken
// helpers in _common.lisp) without rendering. Visual correctness is
// verified the way the rest of the codebase is verified — by running
// the binary and looking at render.png.

/// Load a `scenes/<relpath>` file in a fresh default_env, look up
/// `binding`, and assert it's a `Value::Scene`. The script path is
/// built absolute (from `CARGO_MANIFEST_DIR`) and passed to
/// `eval_source` as-is, so any `(load "_common.lisp")` the script
/// does resolves correctly via the `CurrentDirGuard`.
fn assert_scene_loads(script_relpath: &str, binding: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scenes")
        .join(script_relpath);
    let source = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("could not read {}: {}", path.display(), e)
    });
    let env = sdl::default_env();
    sdl::eval_source(&source, &path.to_string_lossy(), &env);
    let value = env
        .borrow()
        .lookup(binding)
        .unwrap_or_else(|| {
            panic!(
                "{} did not define `{}`",
                script_relpath, binding
            )
        });
    match value {
        Value::Scene(_) => {}
        other => panic!(
            "{} :: {} expected a scene, got {} ({})",
            script_relpath,
            binding,
            other,
            other.type_name()
        ),
    }
}

// One #[test] per scene file. Order matches `scenes/` directory listing
// for easy visual scan; the `(path, binding)` pairing documents the
// same `<stem>-scene` convention `main.rs`'s `binding_name_for`
// derives. Naming dropped the `phaseN_` prefix that the equivalence
// tests had — post-Phase-8 these are just regular smoke tests, not
// phase deliverables.

#[test]
fn area_light_test_scene_loads() {
    assert_scene_loads("area_light_test.lisp", "area-light-test-scene");
}

#[test]
fn axis_spheres_scene_loads() {
    assert_scene_loads("axis_spheres.lisp", "axis-spheres-scene");
}

#[test]
fn ball_on_plane_scene_loads() {
    assert_scene_loads("ball_on_plane.lisp", "ball-on-plane-scene");
}

#[test]
fn cone_test_scene_loads() {
    assert_scene_loads("cone_test.lisp", "cone-test-scene");
}

#[test]
fn cornell_box_scene_loads() {
    assert_scene_loads("cornell_box.lisp", "cornell-box-scene");
}

#[test]
fn pov_compass_scene_loads() {
    assert_scene_loads("pov_compass.lisp", "pov-compass-scene");
}

#[test]
fn xmastree_scene_loads() {
    assert_scene_loads("xmastree.lisp", "xmastree-scene");
}

#[test]
fn braids_scene_loads() {
    assert_scene_loads("braids.lisp", "braids-scene");
}

#[test]
fn train_scene_loads() {
    assert_scene_loads("train.lisp", "train-scene");
}

#[test]
fn redball_scene_loads() {
    assert_scene_loads("redball.lisp", "redball-scene");
}

#[test]
fn pigment_test_scene_loads() {
    assert_scene_loads("pigment_test.lisp", "pigment-test-scene");
}

#[test]
fn ornament_scene_loads() {
    assert_scene_loads("ornament.lisp", "ornament-scene");
}

#[test]
fn nba_scene_loads() {
    assert_scene_loads("nba.lisp", "nba-scene");
}

#[test]
fn cpot_scene_loads() {
    assert_scene_loads("cpot.lisp", "cpot-scene");
}

#[test]
fn texaco_scene_loads() {
    assert_scene_loads("texaco.lisp", "texaco-scene");
}

#[test]
fn csg_test_scene_loads() {
    assert_scene_loads("csg_test.lisp", "csg-test-scene");
}

#[test]
fn cuboid_test_scene_loads() {
    assert_scene_loads("cuboid_test.lisp", "cuboid-test-scene");
}

#[test]
fn cylinder_test_scene_loads() {
    assert_scene_loads("cylinder_test.lisp", "cylinder-test-scene");
}

#[test]
fn depth_of_field_test_scene_loads() {
    assert_scene_loads("depth_of_field_test.lisp", "depth-of-field-test-scene");
}

#[test]
fn gi_test_scene_loads() {
    assert_scene_loads("gi_test.lisp", "gi-test-scene");
}

#[test]
fn group_test_scene_loads() {
    assert_scene_loads("group_test.lisp", "group-test-scene");
}

#[test]
fn snowman_avatar_scene_loads() {
    assert_scene_loads("snowman_avatar.lisp", "snowman-avatar-scene");
}

#[test]
fn snowman_sphere_scene_loads() {
    assert_scene_loads("snowman_sphere.lisp", "snowman-sphere-scene");
}

#[test]
fn snowman_room_textures_scene_loads() {
    assert_scene_loads("snowman_room_textures.lisp", "snowman-room-textures-scene");
}

#[test]
fn snowman_room_props_scene_loads() {
    assert_scene_loads("snowman_room_props.lisp", "snowman-room-props-scene");
}

#[test]
fn snowman_room_scene_loads() {
    assert_scene_loads("snowman_room.lisp", "snowman-room-scene");
}

#[test]
fn snowman_molding_scene_loads() {
    assert_scene_loads("snowman_molding.lisp", "snowman-molding-scene");
}

#[test]
fn metallic_test_scene_loads() {
    assert_scene_loads("metallic_test.lisp", "metallic-test-scene");
}

#[test]
fn moravian_star_scene_loads() {
    assert_scene_loads("moravian_star.lisp", "moravian-star-scene");
}

#[test]
fn multi_light_test_scene_loads() {
    assert_scene_loads("multi_light_test.lisp", "multi-light-test-scene");
}

#[test]
fn one_sphere_scene_loads() {
    assert_scene_loads("one_sphere.lisp", "one-sphere-scene");
}

#[test]
fn sphere_occlusion_test_scene_loads() {
    assert_scene_loads("sphere_occlusion_test.lisp", "sphere-occlusion-test-scene");
}

#[test]
fn soft_shadow_test_scene_loads() {
    assert_scene_loads("soft_shadow_test.lisp", "soft-shadow-test-scene");
}

#[test]
fn sphere_surface_test_scene_loads() {
    assert_scene_loads("sphere_surface_test.lisp", "sphere-surface-test-scene");
}

#[test]
fn spotlight_test_scene_loads() {
    assert_scene_loads("spotlight_test.lisp", "spotlight-test-scene");
}

#[test]
fn transparency_test_scene_loads() {
    assert_scene_loads("transparency_test.lisp", "transparency-test-scene");
}

#[test]
fn torus_test_scene_loads() {
    assert_scene_loads("torus_test.lisp", "torus-test-scene");
}

#[test]
fn transform_test_scene_loads() {
    assert_scene_loads("transform_test.lisp", "transform-test-scene");
}

/// Teapot smoke test — same shape as the rest, but skips with an
/// `eprintln` when `models/utah_teapot.obj` is absent. Loading the
/// scene calls `(load-obj "../models/utah_teapot.obj" ...)`, which
/// requires the model file. The OBJ isn't committed to the repo
/// (it's a multi-megabyte third-party model you drop in yourself);
/// `main.rs` has the same dependency at runtime.
#[test]
fn teapot_scene_loads() {
    let model_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("utah_teapot.obj");
    if !model_path.exists() {
        eprintln!(
            "teapot_scene_loads: skipped — {} not found. \
             Drop a Utah teapot OBJ at that path to enable this test.",
            model_path.display()
        );
        return;
    }
    assert_scene_loads("teapot.lisp", "teapot-scene");
}

/// Guard test: every `.lisp` file in `tests/sdl/` must have a
/// corresponding `sdl_test!` declaration above. Catches "added a
/// file but forgot the test line" oversights.
#[test]
fn all_scripts_have_a_test() {
    let declared: HashSet<&str> = DECLARED.iter().copied().collect();
    let mut on_disk: HashSet<String> = HashSet::new();
    let entries = fs::read_dir(sdl_dir())
        .unwrap_or_else(|e| panic!("could not read tests/sdl/: {}", e));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("lisp") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                on_disk.insert(stem.to_string());
            }
        }
    }
    let on_disk_refs: HashSet<&str> = on_disk.iter().map(|s| s.as_str()).collect();

    let undeclared: Vec<&&str> = on_disk_refs.difference(&declared).collect();
    let missing_files: Vec<&&str> = declared.difference(&on_disk_refs).collect();

    if !undeclared.is_empty() || !missing_files.is_empty() {
        let mut msg = String::new();
        if !undeclared.is_empty() {
            let mut names: Vec<&&&str> = undeclared.iter().collect();
            names.sort();
            msg.push_str(&format!(
                "\n  scripts on disk with no sdl_test! line: {:?}",
                names
            ));
        }
        if !missing_files.is_empty() {
            let mut names: Vec<&&&str> = missing_files.iter().collect();
            names.sort();
            msg.push_str(&format!(
                "\n  declared sdl_test!s with no script file:  {:?}",
                names
            ));
        }
        panic!("test declarations out of sync with tests/sdl/:{}", msg);
    }
}

/// Renders a unit sphere lit from the camera with `surface` (an SDL
/// surface map) at 21x21, one sample, clip curve.
fn lit_sphere(surface: &str, tag: &str) -> image::RgbImage {
    let source = format!(
        r#"
(def s (scene {{:name "sphere"
               :camera (camera-looking-at [0 0 -4] [0 0 0] [0 1 0] 1.0)
               :background [0 0 0]
               :min-samples 1 :max-samples 1
               :view {{:curve :clip}}
               :objects [(light-white [0 0 -4])
                         (sphere {{:center [0 0 0] :r 1 :surface (surface {})}})]}}))
(def t (png-target 21 21))
(render s t 21 21)
(save-png t PATH)
"#,
        surface
    );
    render_to_image(&source, tag)
}

/// History entry 67: `:metallic` tints the highlight and reflection but
/// keeps the diffuse term, which `:light` controls as for any surface.
/// Before, a metallic surface had no diffuse at all.
#[test]
fn metallic_keeps_its_diffuse() {
    let dim = "{:color [0.8 0.4 0.2] :ambient 0.0 :light 0.0 :metallic true}";
    let lit = "{:color [0.8 0.4 0.2] :ambient 0.0 :light 0.6 :metallic true}";
    let a = lit_sphere(dim, "metal_nodiffuse").get_pixel(10, 7).0;
    let b = lit_sphere(lit, "metal_diffuse").get_pixel(10, 7).0;
    assert_eq!(a, [0, 0, 0], "no diffuse, no highlight: black");
    assert!(b[0] > 100 && b[0] > b[2], "diffuse keeps the body colour: {:?}", b);
}

/// `:shininess` is the highlight's exponent: a larger one gives a
/// smaller highlight. The default (50) is the old fixed exponent.
#[test]
fn shininess_tightens_the_highlight() {
    let spot = |shininess: &str, tag: &str| {
        let surface = format!(
            "{{:color [1 1 1] :ambient 0.0 :light 0.0 :specular 1.0 {}}}",
            shininess
        );
        let img = lit_sphere(&surface, tag);
        img.as_raw().chunks(3).filter(|p| p[0] > 64).count()
    };
    let broad = spot(":shininess 5", "shiny_5");
    let default = spot("", "shiny_default");
    let fifty = spot(":shininess 50", "shiny_50");
    let tight = spot(":shininess 500", "shiny_500");
    assert_eq!(default, fifty, "the default is 50");
    assert!(broad > default && default > tight && tight > 0, "{} {} {}", broad, default, tight);
}

/// `:brilliance` raises the Lambert factor to a power: the point facing
/// the light is unchanged, and the falloff toward the edge is darker.
#[test]
fn brilliance_darkens_the_falloff() {
    let plain = lit_sphere("{:color [1 1 1] :ambient 0.0 :light 0.8}", "brill_1");
    let hard = lit_sphere("{:color [1 1 1] :ambient 0.0 :light 0.8 :brilliance 5}", "brill_5");
    // The centre pixel's sample is a hair off-axis, so allow a little.
    let (pc, hc) = (plain.get_pixel(10, 10).0[0] as i32, hard.get_pixel(10, 10).0[0] as i32);
    assert!((pc - hc).abs() <= 6, "facing the light: plain {} vs brilliance 5 {}", pc, hc);
    // Near the top edge (the sphere covers rows 6 to 16).
    let (p, h) = (plain.get_pixel(10, 7).0[0], hard.get_pixel(10, 7).0[0]);
    assert!(h + 20 < p, "off-centre: plain {} vs brilliance 5 {}", p, h);
}

#[test]
fn surface_rejects_bad_exponents() {
    for (key, value) in [("shininess", "0"), ("shininess", "-2"), ("brilliance", "0"), ("brilliance", "-1")].iter() {
        let source = format!("(surface {{:color [1 1 1] :{} {}}})", key, value);
        let env = sdl::default_env();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sdl::eval_source(&source, "surface_bad.lisp", &env);
        }));
        let message = match result {
            Ok(_) => panic!("must reject :{} {}", key, value),
            Err(p) => p.downcast_ref::<String>().cloned().unwrap_or_default(),
        };
        assert!(message.contains("positive"), "{}: {}", source, message);
    }
}

/// `pov-cone` (_pov.lisp) with two non-zero radii is a truncated cone:
/// radius 1 at y = -1 narrowing to 0.5 at y = 1, flat at both ends. Seen
/// from far down +z (nearly orthographic), with 0.1 units per pixel.
#[test]
fn pov_cone_truncates() {
    let pov = Path::new(env!("CARGO_MANIFEST_DIR")).join("scenes").join("_pov.lisp");
    let source = format!(
        r#"
(load "{}")
(def s (scene {{:name "frustum"
               :camera (camera-looking-at [0 0 100] [0 0 0] [0 1 0] 25)
               :background [0 0 0]
               :min-samples 1 :max-samples 1
               :view {{:curve :clip}}
               :objects [(with-surface (surface {{:color [1 1 1] :ambient 1.0 :light 0.0}})
                           (pov-cone [0 -1 0] 1 [0 1 0] 0.5))]}}))
(def t (png-target 41 41))
(render s t 41 41)
(save-png t PATH)
"#,
        pov.to_string_lossy()
    );
    let img = render_to_image(&source, "pov_cone");
    // Pixel for world (x, y): column 20 + 10x, row 20 - 10y.
    let at = |x: f64, y: f64| img.get_pixel((20.0 + 10.0 * x).round() as u32, (20.0 - 10.0 * y).round() as u32).0[0];
    // Near the top the radius is about 0.53; near the bottom about 0.98.
    assert!(at(0.4, 0.9) > 200 && at(0.7, 0.9) < 50, "top: {} {}", at(0.4, 0.9), at(0.7, 0.9));
    assert!(at(0.9, -0.9) > 200 && at(1.2, -0.9) < 50, "bottom: {} {}", at(0.9, -0.9), at(1.2, -0.9));
    // Flat ends: nothing above y = 1 or below y = -1.
    assert!(at(0.0, 1.2) < 50 && at(0.0, -1.2) < 50, "ends: {} {}", at(0.0, 1.2), at(0.0, -1.2));
    assert!(at(0.0, 0.0) > 200);
}

#[test]
fn blob_rejects_bad_values() {
    let cases = [
        ("(blob {:threshold 0 :components [[[0 0 0] 1 1]]})", "threshold"),
        ("(blob {:threshold 0.1 :components [[[0 0 0] 0 1]]})", "radius"),
        ("(blob {:threshold 0.1 :components [[[0 0 0] 1 -1]]})", "positive strength"),
        ("(blob {:threshold 0.1 :components [[[0 0 0] 1]]})", "[center radius strength]"),
        ("(blob {:components [[[0 0 0] 1 1]]})", ":threshold"),
    ];
    for (source, expected) in cases.iter() {
        let env = sdl::default_env();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sdl::eval_source(source, "blob_bad.lisp", &env);
        }));
        let message = match result {
            Ok(_) => panic!("must reject {}", source),
            Err(p) => p.downcast_ref::<String>().cloned().unwrap_or_default(),
        };
        assert!(message.contains(expected), "{}: {}", source, message);
    }
}

/// A one-component blob renders as the sphere of radius
/// R sqrt(1 - sqrt(t/s)), here exactly 1.
#[test]
fn blob_renders_like_its_sphere() {
    let render = |shape: &str, tag: &str| {
        let source = format!(
            r#"
(def s (scene {{:name "blob"
               :camera (camera-looking-at [0 0 -5] [0 0 0] [0 1 0] 1.0)
               :background [0 0 0]
               :min-samples 1 :max-samples 1
               :view {{:curve :clip}}
               :objects [(light-white [2 3 -5])
                         (with-surface (surface {{:color [1 1 1] :ambient 0.1 :light 0.8}}) {})]}}))
(def t (png-target 31 31))
(render s t 31 31)
(save-png t PATH)
"#,
            shape
        );
        render_to_image(&source, tag)
    };
    // (1 - 1/4)² * 16/9 = 1: the threshold is reached at d = 1.
    let blob = render("(blob {:threshold 1 :components [[[0 0 0] 2 1.7777777777777777]]})", "blob_one");
    let sphere = render("(sphere {:center [0 0 0] :r 1})", "blob_sphere");
    let diff = blob
        .as_raw()
        .iter()
        .zip(sphere.as_raw())
        .filter(|(a, b)| (**a as i32 - **b as i32).abs() > 2)
        .count();
    assert_eq!(diff, 0, "blob and sphere differ in {} channels", diff);
}

/// A `:normal` bump pattern makes an evenly lit sphere shade unevenly
/// without changing its outline; with `:amount 0` it's the plain sphere.
#[test]
fn bump_normals_shade_but_keep_the_outline() {
    let flat = lit_sphere("{:color [1 1 1] :ambient 0.05 :light 0.8}", "bump_none");
    let zero = lit_sphere(
        "{:color [1 1 1] :ambient 0.05 :light 0.8 :normal {:pattern :bumps :amount 0}}",
        "bump_zero",
    );
    let bumped = lit_sphere(
        "{:color [1 1 1] :ambient 0.05 :light 0.8
          :normal {:pattern :wrinkles :amount 0.8 :transform (affine-scale [0.2 0.2 0.2])}}",
        "bump_wrinkles",
    );
    assert_eq!(flat.as_raw(), zero.as_raw(), "amount 0 changes nothing");
    let lit = |img: &image::RgbImage| img.as_raw().chunks(3).map(|p| p[0] > 0).collect::<Vec<_>>();
    assert_eq!(lit(&flat), lit(&bumped), "same outline");
    let changed = flat.as_raw().iter().zip(bumped.as_raw()).filter(|(a, b)| (**a as i32 - **b as i32).abs() > 8).count();
    assert!(changed > 30, "the bumps show: {} channels changed", changed);
}

#[test]
fn normal_rejects_bad_keys() {
    let cases = [
        ("(surface {:color [1 1 1] :normal {:pattern :dents :amount 1}})", ":bumps or :wrinkles"),
        ("(surface {:color [1 1 1] :normal {:pattern :bumps}})", ":amount"),
        ("(surface {:color [1 1 1] :normal {:amount 1}})", "missing :pattern"),
        ("(surface {:color [1 1 1] :normal {:pattern :bumps :amount 1 :size 2}})", "unknown key :size"),
    ];
    for (source, expected) in cases.iter() {
        let env = sdl::default_env();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sdl::eval_source(source, "normal_bad.lisp", &env);
        }));
        let message = match result {
            Ok(_) => panic!("must reject {}", source),
            Err(p) => p.downcast_ref::<String>().cloned().unwrap_or_default(),
        };
        assert!(message.contains(expected), "{}: {}", source, message);
    }
}

/// History entry 71: `:filter` tints what shows through a surface by
/// its colour, where `:transparency` doesn't. Looking through a red
/// sheet at a white background: a red filter shows red, the same amount
/// of transparency shows white, and the body gives up `t + f`.
#[test]
fn filter_tints_what_shows_through() {
    let pixel = |sheet: &str, tag: &str| {
        let source = format!(
            r#"
(def s (scene {{:name "filter"
               :camera (camera-looking-at [0 0 -5] [0 0 0] [0 1 0] 1.0)
               :background [1 1 1]
               :min-samples 1 :max-samples 1
               :view {{:curve :clip}}
               :objects [(with-surface (surface {}) (cuboid {{:center [0 0 0] :size [4 4 0.1]}}))]}}))
(def t (png-target 5 5))
(render s t 5 5)
(save-png t PATH)
"#,
            sheet
        );
        render_to_image(&source, tag).get_pixel(2, 2).0
    };
    let filtered = pixel("{:color [1 0 0] :ambient 0.0 :light 0.0 :filter 1}", "filter_red");
    let clear = pixel("{:color [1 0 0] :ambient 0.0 :light 0.0 :transparency 1}", "filter_clear");
    let half = pixel("{:color [1 0 0] :ambient 1.0 :light 0.0 :filter 0.5}", "filter_half");
    assert_eq!(filtered, [255, 0, 0]);
    assert_eq!(clear, [255, 255, 255]);
    // Half body (red, ambient 1) plus half the white filtered to red.
    assert!(half[0] == 255 && half[1] == 0 && half[2] == 0, "{:?}", half);
}

/// A filter sheet between a light and a white floor casts a shadow
/// tinted by its colour; an equally transparent untinted sheet casts a
/// grey one.
#[test]
fn filter_tints_shadows() {
    let floor = |sheet: &str, tag: &str| {
        let source = format!(
            r#"
(def s (scene {{:name "filter-shadow"
               :camera (camera-looking-at [0 2 -0.001] [0 0 0] [0 1 0] 1.0)
               :background [0 0 0]
               :min-samples 1 :max-samples 1
               :view {{:curve :clip}}
               :objects [(light {{:location [0 10 0] :color [1 1 1]}})
                         (with-surface (surface {}) (cuboid {{:center [0 3 0] :size [20 0.1 20]}}))
                         (plane {{:normal [0 1 0] :p0 [0 0 0]
                                 :surface (surface {{:color [1 1 1] :ambient 0.0 :light 1.0}})}})]}}))
(def t (png-target 5 5))
(render s t 5 5)
(save-png t PATH)
"#,
            sheet
        );
        render_to_image(&source, tag).get_pixel(2, 2).0
    };
    // The camera sits below the sheet, so it sees only the floor.
    let tinted = floor("{:color [0 1 0] :ambient 0.0 :light 0.0 :filter 0.8}", "filter_shadow");
    let grey = floor("{:color [0 1 0] :ambient 0.0 :light 0.0 :transparency 0.8}", "transp_shadow");
    assert!(tinted[1] > 150 && tinted[0] == 0 && tinted[2] == 0, "green shadow: {:?}", tinted);
    assert!(grey[0] > 150 && grey[0] == grey[1] && grey[1] == grey[2], "grey shadow: {:?}", grey);
}

#[test]
fn filter_rejects_bad_values() {
    for source in [
        "(surface {:color [1 1 1] :filter 1.5})",
        "(surface {:color [1 1 1] :filter -0.1})",
        "(surface {:color [1 1 1] :filter 0.6 :transparency 0.6})",
    ] {
        let env = sdl::default_env();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sdl::eval_source(source, "filter_bad.lisp", &env);
        }));
        let message = match result {
            Ok(_) => panic!("must reject {}", source),
            Err(p) => p.downcast_ref::<String>().cloned().unwrap_or_default(),
        };
        assert!(message.contains(":filter"), "{}: {}", source, message);
    }
}

/// A height field from a 2x2 all-white grey TGA is the flat unit square
/// at y = 1: seen from above, the middle is lit and the area beyond the
/// square's edge is background.
#[test]
fn height_field_renders_its_square() {
    let tga = std::env::temp_dir().join(format!("hf_flat_{}.tga", std::process::id()));
    let mut data = vec![0u8; 18];
    data[2] = 3; // uncompressed grey
    data[12] = 2;
    data[14] = 2;
    data[16] = 8;
    data[17] = 0x20;
    data.extend_from_slice(&[255, 255, 255, 255]);
    fs::write(&tga, &data).unwrap();
    // From 4 units above the square at zoom 1 the frame is 4 units
    // across, so the square fills about the middle quarter.
    let source = format!(
        r#"
(def s (scene {{:name "hf"
               :camera (camera-looking-at [0.5 5 0.5001] [0.5 0 0.5] [0 0 1] 1)
               :background [0 0 0]
               :min-samples 1 :max-samples 1
               :view {{:curve :clip}}
               :objects [(with-surface (surface {{:color [1 1 1] :ambient 1.0 :light 0.0}})
                           (height-field {{:image "{}"}}))]}}))
(def t (png-target 21 21))
(render s t 21 21)
(save-png t PATH)
"#,
        tga.to_string_lossy()
    );
    let img = render_to_image(&source, "height_field");
    let _ = fs::remove_file(&tga);
    assert_eq!(img.get_pixel(10, 10).0, [255, 255, 255], "the square");
    assert_eq!(img.get_pixel(2, 2).0, [0, 0, 0], "beyond its edge");
    let lit = img.as_raw().chunks(3).filter(|p| p[0] > 128).count();
    assert!((16..=49).contains(&lit), "about a quarter of the frame across: {} pixels", lit);
}

#[test]
fn height_field_rejects_bad_input() {
    let cases = [
        ("(height-field {:image \"/nonexistent/none.tga\"})", "can't read"),
        ("(height-field {:image \"x.tga\" :smoooth true})", "unknown key :smoooth"),
        ("(height-field {:water-level 0.2})", ":image"),
    ];
    for (source, expected) in cases.iter() {
        let env = sdl::default_env();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sdl::eval_source(source, "hf_bad.lisp", &env);
        }));
        let message = match result {
            Ok(_) => panic!("must reject {}", source),
            Err(p) => p.downcast_ref::<String>().cloned().unwrap_or_default(),
        };
        assert!(message.contains(expected), "{}: {}", source, message);
    }
}

/// A scene `:sky` is a pigment on the ray's direction: sky_sphere's
/// gradient, black above the horizon and red below, seen looking up and
/// looking down (no objects).
#[test]
fn sky_is_a_pigment_on_the_direction() {
    let look = |target: &str, tag: &str| {
        let source = format!(
            r#"
(def s (scene {{:name "sky"
               :camera (camera-looking-at [0 0 0] {} [1 0 0] 4)
               :background [0 0 1]
               :sky {{:pattern :gradient :color-map [[0 [1 0 0]] [0.5 [1 0 0]] [0.5 [0 0 0]] [1 [0 0 0]]]
                      :transform (affine-compose (affine-translation [0 -1 0]) (affine-scale [2 2 2]))}}
               :min-samples 1 :max-samples 1
               :view {{:curve :clip}}
               :objects []}}))
(def t (png-target 3 3))
(render s t 3 3)
(save-png t PATH)
"#,
            target
        );
        render_to_image(&source, tag).get_pixel(1, 1).0
    };
    assert_eq!(look("[0 1 0]", "sky_up"), [0, 0, 0]);
    assert_eq!(look("[0 -1 0]", "sky_down"), [255, 0, 0]);
}

/// A `:brick` pigment choosing whole pigments: mostly the second
/// (brick), with the first (mortar) in the joints.
#[test]
fn brick_pigments_choose_between_pigments() {
    let source = r#"
(def s (scene {:name "brick"
               :camera (camera-looking-at [0 0 -10] [0 0 0] [0 1 0] 1)
               :background [0 0 0]
               :min-samples 1 :max-samples 1
               :view {:curve :clip}
               :objects [(with-surface
                           (surface {:ambient 1.0 :light 0.0
                                     :pigment {:pattern :brick :brick-size [1 0.5 100] :mortar 0.1
                                               :pigments [{:color [1 1 1]}
                                                          [{:color [1 0 0]} {:color [0 0 1 0.5]}]]}})
                           ; The front face at z = 4.95, clear of the joints
                           ; across z (every 100 units).
                           (cuboid {:center [0 0 5] :size [20 20 0.1]}))]}))
(def t (png-target 41 41))
(render s t 41 41)
(save-png t PATH)
"#;
    let img = render_to_image(source, "brick_pigments");
    let (mut mortar, mut brick, mut other) = (0, 0, 0);
    for p in img.as_raw().chunks(3) {
        match p {
            [255, 255, 255] => mortar += 1,
            // Red under a half-clear blue layer.
            [r, 0, b] if *r > 150 && *b > 150 => brick += 1,
            _ => other += 1,
        }
    }
    assert!(brick > 2 * mortar && mortar > 50 && other < 100, "mortar {} brick {} other {}", mortar, brick, other);
}

#[test]
fn pattern_pigments_reject_bad_keys() {
    let cases = [
        ("{:pattern :wood :axis [0 1 0] :colors [[0 0 0] [1 1 1]]}", "only applies to a :gradient"),
        ("{:pattern :bozo :mortar 1 :colors [[0 0 0] [1 1 1]]}", "only applies to a :brick"),
        ("{:pattern :brick :brick-size [0 1 1] :colors [[0 0 0] [1 1 1]]}", "positive"),
        ("{:pattern :checker :colors [[0 0 0] [1 1 1]] :pigments [{:color [0 0 0]} {:color [1 1 1]}]}", "not both"),
        ("{:pattern :checker :pigments [{:color [0 0 0]}]}", "two pigments"),
        ("{:pattern :stripes :colors [[0 0 0] [1 1 1]]}", ":gradient or :brick"),
    ];
    for (pigment, expected) in cases.iter() {
        let source = format!("(surface {{:pigment {}}})", pigment);
        let env = sdl::default_env();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sdl::eval_source(&source, "pigment_bad.lisp", &env);
        }));
        let message = match result {
            Ok(_) => panic!("must reject {}", source),
            Err(p) => p.downcast_ref::<String>().cloned().unwrap_or_default(),
        };
        assert!(message.contains(expected), "{}: {}", source, message);
    }
}

/// `:ambient-light` scales every surface's ambient: at 0.5 an
/// ambient-only surface is half as bright (in linear light), at 0 it's
/// black, and the default is the surface as written.
#[test]
fn ambient_light_scales_ambient() {
    let pixel = |setting: &str, tag: &str| {
        let source = format!(
            r#"
(def s (scene {{:name "ambient"
               :camera (camera-looking-at [0 0 -5] [0 0 0] [0 1 0] 1.0)
               :background [0 0 0]
               :min-samples 1 :max-samples 1
               :view {{:curve :clip}}
               {}
               :objects [(sphere {{:center [0 0 0] :r 1
                                  :surface (surface {{:color [1 1 1] :ambient 0.8 :light 0.0}})}})]}}))
(def t (png-target 5 5))
(render s t 5 5)
(save-png t PATH)
"#,
            setting
        );
        render_to_image(&source, tag).get_pixel(2, 2).0[0]
    };
    let full = pixel("", "ambient_default");
    let one = pixel(":ambient-light 1", "ambient_one");
    let half = pixel(":ambient-light 0.5", "ambient_half");
    let none = pixel(":ambient-light 0", "ambient_none");
    assert_eq!(full, one);
    assert_eq!(none, 0);
    // sRGB of 0.8 is 231; of 0.4 is 170.
    assert!((full as i32 - 231).abs() <= 1 && (half as i32 - 170).abs() <= 1, "{} {}", full, half);
}
