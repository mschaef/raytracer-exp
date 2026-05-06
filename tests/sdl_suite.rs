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
    sdl::read_and_eval(&source, &format!("{}.lisp", name));
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

/// Phase 5 — port-equivalence test for `scenes/transform_test.lisp`.
///
/// Renders the SDL-defined scene and the original `scene_transform_test`
/// from `scenes.rs` into two `PngTarget`s at the same dimensions, saves
/// each as PNG, decodes both, and asserts pixel-by-pixel equality.
///
/// The two pipelines run identical math on identical inputs (same
/// `Surface` field values, same `Camera::looking_at` arguments, same
/// transform composition order, identical f64 representations of `pi`),
/// so the rendered bytes should match exactly. `TOLERANCE = 0` reflects
/// that — any divergence is a real bug in the binding layer or the
/// port, not floating-point drift. If a future change introduces an
/// unavoidable LSB-level mismatch we'd loosen the tolerance, but the
/// failure message preserves enough detail to diagnose either case.
///
/// Resolution is 64×64 to keep the test fast while still exercising
/// every transform path in the scene (each transformed object covers
/// at least a few pixels at this size). `parallel = false` removes any
/// scheduler-order variability — the renderer is per-pixel deterministic
/// regardless, but serial execution makes that property load-bearing for
/// the test rather than incidental.
///
/// On failure both PNGs are kept on disk and their paths surfaced in
/// the panic message so the user can `open` them and diff visually.
/// On success they're removed.
#[test]
fn phase5_transform_test_scene_matches_rust() {
    use raytracer::render::output::PngTarget;
    use raytracer::render::render;
    use raytracer::scenes::scene_transform_test;

    const W: u32 = 64;
    const H: u32 = 64;
    const TOLERANCE: u8 = 0;

    let pid = std::process::id();
    let rust_path = std::env::temp_dir()
        .join(format!("sdl_phase5_rust_{}.png", pid));
    let sdl_path = std::env::temp_dir()
        .join(format!("sdl_phase5_sdl_{}.png", pid));
    let _ = fs::remove_file(&rust_path);
    let _ = fs::remove_file(&sdl_path);

    // Render the Rust version directly. PngTarget writes sRGB-encoded
    // 8-bit pixels, which matches what we'll get when we decode either
    // saved file below — the comparison is symmetric.
    let rust_scene = scene_transform_test();
    let rust_target = PngTarget::new(W, H);
    render(&rust_scene, W, H, &rust_target, None, false);
    rust_target
        .save(&rust_path)
        .expect("save Rust render");

    // Load the .lisp scene file, evaluate, look up `transform-test-scene`,
    // render. The script lives at the repo root under `scenes/` —
    // CARGO_MANIFEST_DIR resolves to the workspace root at compile time
    // so this works regardless of where `cargo test` is invoked from.
    let script_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scenes")
        .join("transform_test.lisp");
    let source = fs::read_to_string(&script_path).unwrap_or_else(|e| {
        panic!("could not read {}: {}", script_path.display(), e)
    });

    let env = sdl::default_env();
    sdl::eval_source(&source, "scenes/transform_test.lisp", &env);

    let scene_value = env
        .borrow()
        .lookup("transform-test-scene")
        .expect("script did not define `transform-test-scene`");
    let sdl_scene = match scene_value {
        Value::Scene(s) => s,
        other => panic!(
            "transform-test-scene must be a scene, got {} ({})",
            other,
            other.type_name()
        ),
    };

    let sdl_target = PngTarget::new(W, H);
    render(&*sdl_scene, W, H, &sdl_target, None, false);
    sdl_target
        .save(&sdl_path)
        .expect("save SDL render");

    let rust_img = image::open(&rust_path)
        .unwrap_or_else(|e| panic!("decode Rust PNG: {}", e))
        .to_rgb8();
    let sdl_img = image::open(&sdl_path)
        .unwrap_or_else(|e| panic!("decode SDL PNG: {}", e))
        .to_rgb8();

    assert_eq!(rust_img.dimensions(), (W, H), "Rust PNG dimensions");
    assert_eq!(sdl_img.dimensions(), (W, H), "SDL PNG dimensions");

    // Walk the buffers in parallel. Track both the worst per-channel
    // delta seen anywhere and the first pixel that exceeds tolerance —
    // the former is a quick sanity check ("is this off by 1 LSB or
    // wildly wrong?"), the latter points the user at a specific pixel
    // to investigate.
    let mut max_diff: u8 = 0;
    let mut first_offender: Option<(u32, u32, [u8; 3], [u8; 3])> = None;
    for y in 0..H {
        for x in 0..W {
            let r = rust_img.get_pixel(x, y).0;
            let s = sdl_img.get_pixel(x, y).0;
            for c in 0..3 {
                let d = r[c].abs_diff(s[c]);
                if d > max_diff {
                    max_diff = d;
                }
                if d > TOLERANCE && first_offender.is_none() {
                    first_offender = Some((x, y, r, s));
                }
            }
        }
    }

    if let Some((x, y, r, s)) = first_offender {
        panic!(
            "SDL render diverges from Rust render at ({}, {}): \
             rust={:?} sdl={:?}, max diff = {} (tolerance {}). \
             rust PNG: {}, sdl PNG: {}",
            x,
            y,
            r,
            s,
            max_diff,
            TOLERANCE,
            rust_path.display(),
            sdl_path.display(),
        );
    }

    // All pixels matched within tolerance — clean up the temp files.
    let _ = fs::remove_file(&rust_path);
    let _ = fs::remove_file(&sdl_path);
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
