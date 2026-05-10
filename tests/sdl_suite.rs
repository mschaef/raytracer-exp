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
    "bindings_camera",
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
    "destructuring",
    "fn_form",
    "hofs",
    "literals",
    "load_form",
    "load_form_fixture",
    "logic",
    "map_ops",
    "math",
    "points",
    "predicates",
    "quote",
    "recur",
    "render_dispatch",
    "strings",
    "threading",
    "vec_ops",
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
sdl_test!(bindings_camera);
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
sdl_test!(destructuring);
sdl_test!(fn_form);
sdl_test!(hofs);
sdl_test!(literals);
sdl_test!(load_form);
sdl_test!(load_form_fixture);
sdl_test!(logic);
sdl_test!(map_ops);
sdl_test!(math);
sdl_test!(points);
sdl_test!(predicates);
sdl_test!(quote);
sdl_test!(recur);
sdl_test!(render_dispatch);
sdl_test!(strings);
sdl_test!(threading);
sdl_test!(vec_ops);

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
               :lights [(light-white [10 10 10])]
               :objects [(sphere {:center [0 0 0] :r 1.0 :surface red})]
               :reflect-limit 0
               :oversample 1}))
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

// ---------------------------------------------------------------------------
// Scene-port equivalence harness
// ---------------------------------------------------------------------------
//
// Each ported scene gets a one-line `#[test]` that delegates to
// `assert_sdl_scene_matches_rust`. The helper renders both the
// SDL-defined scene (looked up by binding name in the script's env)
// and the canonical Rust scene to two `PngTarget`s at the same
// dimensions, saves each as PNG, decodes both, and asserts
// pixel-by-pixel equality.
//
// The two pipelines run identical math on identical inputs (same
// `Surface` field values, same `Camera::looking_at` arguments, same
// transform composition order, identical f64 representation of `pi`),
// so the rendered bytes should match exactly. `TOLERANCE = 0` reflects
// that — any divergence is a real bug in the binding layer or the
// port, not floating-point drift. If a future change introduces an
// unavoidable LSB-level mismatch we'd loosen the tolerance, but the
// failure message preserves enough detail to diagnose either case.
//
// Resolution is 64×64 to keep the suite fast while still exercising
// every transform path in each scene. `parallel = false` removes any
// scheduler-order variability — the renderer is per-pixel deterministic
// regardless, but serial execution makes that property load-bearing for
// the test rather than incidental.
//
// On failure both PNGs are kept on disk and their paths surfaced in
// the panic message so the user can `open` them and diff visually.
// On success they're removed.

const EQUIV_W: u32 = 64;
const EQUIV_H: u32 = 64;
const EQUIV_TOLERANCE: u8 = 0;

/// Render a Rust `Scene` and an SDL-defined scene from a `.lisp`
/// script and assert byte-equal output.
///
/// Defaults to 64×64 with `parallel = false` — the convention used by
/// the Phase 5 / Phase 6 ports where every transformed object covers
/// at least a few pixels and serial execution is well under a second.
/// For ports where that's too slow (a 6000-triangle teapot at 64×64
/// serial would dominate suite runtime), call
/// [`assert_sdl_scene_matches_rust_with`] directly with smaller
/// dimensions and/or `parallel = true`.
fn assert_sdl_scene_matches_rust(
    script_relpath: &str,
    binding_name: &str,
    rust_scene: raytracer::render::Scene,
    suffix: &str,
) {
    assert_sdl_scene_matches_rust_with(
        script_relpath,
        binding_name,
        rust_scene,
        suffix,
        EQUIV_W,
        EQUIV_H,
        false,
    );
}

/// Like [`assert_sdl_scene_matches_rust`] but takes the render
/// dimensions and `parallel` flag explicitly. The renderer is
/// per-pixel deterministic regardless of whether work is dispatched
/// across Rayon's worker threads or run serially in the main thread —
/// each pixel reads from immutable scene data and writes a single
/// non-overlapping row to the target — so byte-equality holds with
/// `parallel = true` too. Use that for expensive scenes (e.g. the
/// teapot) where serial execution would dominate suite runtime.
///
/// `script_relpath` is the path relative to `scenes/` (e.g.
/// `"transform_test.lisp"`). `binding_name` is the symbol the script
/// `def`s its `Value::Scene` to (e.g. `"transform-test-scene"`).
/// `suffix` is appended to the temp PNG filenames so concurrent runs
/// of different ports don't clobber each other.
///
/// The script path is built absolute (from `CARGO_MANIFEST_DIR`) and
/// passed to `eval_source` as-is, so any `(load "_common.lisp")` the
/// script does resolves correctly via the `CurrentDirGuard` thread-
/// local installed inside `eval_source`.
fn assert_sdl_scene_matches_rust_with(
    script_relpath: &str,
    binding_name: &str,
    rust_scene: raytracer::render::Scene,
    suffix: &str,
    width: u32,
    height: u32,
    parallel: bool,
) {
    use raytracer::render::output::PngTarget;
    use raytracer::render::render;

    let pid = std::process::id();
    let rust_path = std::env::temp_dir()
        .join(format!("sdl_equiv_{}_rust_{}.png", suffix, pid));
    let sdl_path = std::env::temp_dir()
        .join(format!("sdl_equiv_{}_sdl_{}.png", suffix, pid));
    let _ = fs::remove_file(&rust_path);
    let _ = fs::remove_file(&sdl_path);

    // Render the Rust version directly into a PngTarget and save.
    // PngTarget writes sRGB-encoded 8-bit pixels, which matches what
    // we'll get when we decode either saved file below — the
    // comparison is symmetric.
    let rust_target = PngTarget::new(width, height);
    render(&rust_scene, width, height, &rust_target, None, parallel);
    rust_target
        .save(&rust_path)
        .expect("save Rust render");

    // Read and evaluate the .lisp scene file in a fresh default_env.
    // CARGO_MANIFEST_DIR is the workspace root, so this works
    // regardless of where `cargo test` is invoked from.
    let script_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scenes")
        .join(script_relpath);
    let source = fs::read_to_string(&script_path).unwrap_or_else(|e| {
        panic!("could not read {}: {}", script_path.display(), e)
    });
    let env = sdl::default_env();
    // Pass the absolute path so (load "_common.lisp") in the script
    // resolves to scenes/_common.lisp via the CurrentDirGuard.
    sdl::eval_source(&source, &script_path.to_string_lossy(), &env);

    let scene_value = env
        .borrow()
        .lookup(binding_name)
        .unwrap_or_else(|| panic!("script did not define `{}`", binding_name));
    let sdl_scene = match scene_value {
        Value::Scene(s) => s,
        other => panic!(
            "{} must be a scene, got {} ({})",
            binding_name,
            other,
            other.type_name()
        ),
    };

    let sdl_target = PngTarget::new(width, height);
    render(&*sdl_scene, width, height, &sdl_target, None, parallel);
    sdl_target
        .save(&sdl_path)
        .expect("save SDL render");

    let rust_img = image::open(&rust_path)
        .unwrap_or_else(|e| panic!("decode Rust PNG: {}", e))
        .to_rgb8();
    let sdl_img = image::open(&sdl_path)
        .unwrap_or_else(|e| panic!("decode SDL PNG: {}", e))
        .to_rgb8();

    assert_eq!(
        rust_img.dimensions(),
        (width, height),
        "{}: Rust PNG dimensions",
        script_relpath
    );
    assert_eq!(
        sdl_img.dimensions(),
        (width, height),
        "{}: SDL PNG dimensions",
        script_relpath
    );

    // Walk the buffers in parallel. Track both the worst per-channel
    // delta seen anywhere and the first pixel that exceeds tolerance —
    // the former is a quick sanity check ("is this off by 1 LSB or
    // wildly wrong?"), the latter points the user at a specific pixel
    // to investigate.
    let mut max_diff: u8 = 0;
    let mut first_offender: Option<(u32, u32, [u8; 3], [u8; 3])> = None;
    for y in 0..height {
        for x in 0..width {
            let r = rust_img.get_pixel(x, y).0;
            let s = sdl_img.get_pixel(x, y).0;
            for c in 0..3 {
                let d = r[c].abs_diff(s[c]);
                if d > max_diff {
                    max_diff = d;
                }
                if d > EQUIV_TOLERANCE && first_offender.is_none() {
                    first_offender = Some((x, y, r, s));
                }
            }
        }
    }

    if let Some((x, y, r, s)) = first_offender {
        panic!(
            "{}: SDL render diverges from Rust render at ({}, {}): \
             rust={:?} sdl={:?}, max diff = {} (tolerance {}). \
             rust PNG: {}, sdl PNG: {}",
            script_relpath,
            x,
            y,
            r,
            s,
            max_diff,
            EQUIV_TOLERANCE,
            rust_path.display(),
            sdl_path.display(),
        );
    }

    // All pixels matched within tolerance — clean up the temp files.
    let _ = fs::remove_file(&rust_path);
    let _ = fs::remove_file(&sdl_path);
}

// One #[test] per ported scene. Phase 5 was the original; Phase 6 added
// the rest of the no-mesh `scenes.rs` entries. `scene_teapot` is
// deferred until the `load-obj` mesh binding lands.

#[test]
fn phase5_transform_test_scene_matches_rust() {
    assert_sdl_scene_matches_rust(
        "transform_test.lisp",
        "transform-test-scene",
        raytracer::scenes::scene_transform_test(),
        "transform_test",
    );
}

#[test]
fn phase6_sphere_occlusion_test_scene_matches_rust() {
    assert_sdl_scene_matches_rust(
        "sphere_occlusion_test.lisp",
        "sphere-occlusion-test-scene",
        raytracer::scenes::scene_sphere_occlusion_test(),
        "sphere_occlusion_test",
    );
}

#[test]
fn phase6_sphere_surface_test_scene_matches_rust() {
    assert_sdl_scene_matches_rust(
        "sphere_surface_test.lisp",
        "sphere-surface-test-scene",
        raytracer::scenes::scene_sphere_surface_test(),
        "sphere_surface_test",
    );
}

#[test]
fn phase6_one_sphere_scene_matches_rust() {
    assert_sdl_scene_matches_rust(
        "one_sphere.lisp",
        "one-sphere-scene",
        raytracer::scenes::scene_one_sphere(),
        "one_sphere",
    );
}

#[test]
fn phase6_axis_spheres_scene_matches_rust() {
    assert_sdl_scene_matches_rust(
        "axis_spheres.lisp",
        "axis-spheres-scene",
        raytracer::scenes::scene_axis_spheres(),
        "axis_spheres",
    );
}

#[test]
fn phase6_cuboid_test_scene_matches_rust() {
    assert_sdl_scene_matches_rust(
        "cuboid_test.lisp",
        "cuboid-test-scene",
        raytracer::scenes::scene_cuboid_test(),
        "cuboid_test",
    );
}

#[test]
fn phase6_group_test_scene_matches_rust() {
    assert_sdl_scene_matches_rust(
        "group_test.lisp",
        "group-test-scene",
        raytracer::scenes::scene_group_test(),
        "group_test",
    );
}

#[test]
fn phase6_multi_light_test_scene_matches_rust() {
    assert_sdl_scene_matches_rust(
        "multi_light_test.lisp",
        "multi-light-test-scene",
        raytracer::scenes::scene_multi_light_test(),
        "multi_light_test",
    );
}

#[test]
fn phase6_ball_on_plane_scene_matches_rust() {
    assert_sdl_scene_matches_rust(
        "ball_on_plane.lisp",
        "ball-on-plane-scene",
        raytracer::scenes::scene_ball_on_plane(),
        "ball_on_plane",
    );
}

#[test]
fn phase6_cylinder_test_scene_matches_rust() {
    assert_sdl_scene_matches_rust(
        "cylinder_test.lisp",
        "cylinder-test-scene",
        raytracer::scenes::scene_cylinder_test(),
        "cylinder_test",
    );
}

/// Phase 7 — teapot port equivalence test.
///
/// Renders both `scene_teapot()` and `scenes/teapot.lisp` to PNGs
/// and asserts byte-equality, same as the other ports — but with two
/// concessions to the teapot's cost:
///
/// 1. **Skip if the model isn't on disk.** The teapot OBJ isn't
///    committed to the repo (it's a multi-megabyte third-party model
///    you drop in yourself), so a fresh clone has no way to run this
///    test. We check for `models/utah_teapot.obj` up front and skip
///    with an explanatory `eprintln` if it's missing — same posture
///    as `main.rs`, which also fails at runtime if the model is
///    absent rather than bringing it in as a build-time dependency.
///
/// 2. **Smaller render and `parallel = true`.** A 6000-triangle mesh
///    rendered serially at 64×64 takes seconds; at 32×32 with rayon
///    it's well under one. The renderer is per-pixel deterministic
///    regardless of dispatch strategy (each pixel reads from
///    immutable scene data and writes a non-overlapping row to the
///    target), so byte-equality still holds.
#[test]
fn phase7_teapot_scene_matches_rust() {
    let model_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("utah_teapot.obj");
    if !model_path.exists() {
        eprintln!(
            "phase7_teapot_scene_matches_rust: skipped — {} not found. \
             Drop a Utah teapot OBJ at that path to enable this test.",
            model_path.display()
        );
        return;
    }
    assert_sdl_scene_matches_rust_with(
        "teapot.lisp",
        "teapot-scene",
        raytracer::scenes::scene_teapot(),
        "teapot",
        32,
        32,
        true,
    );
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
