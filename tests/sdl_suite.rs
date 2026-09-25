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
