// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

use std::env;
use std::fs;
use std::path::Path;
use std::time::Instant;

use raytracer::render::{render, Scene};
use raytracer::render::output::{
    PngTarget,
    StreamTarget,
    OffsetTarget,
    ProgressTarget,
    RenderTarget,
    HeatmapTarget,
    HeatmapScale,
    PngHeatmapTarget,
    OffsetHeatmapTarget,
};
use raytracer::sdl;
use raytracer::sdl::value::Value;

fn is_parallel() -> bool {
    match env::var("PARALLEL") {
        Ok(val) => val.to_lowercase() == "y",
        Err(_) => true
    }
}

/// Read an SDL scene script from the repo's `scenes/` directory and
/// extract the named `Value::Scene` binding from it.
///
/// `rel_path` is relative to `scenes/` (e.g. `"teapot.lisp"`) and
/// `binding` is the symbol the script `def`s its scene to (e.g.
/// `"teapot-scene"`). The script path is built absolute via
/// `env!("CARGO_MANIFEST_DIR")` so the `(load "_common.lisp")` calls
/// inside each scene resolve correctly regardless of what CWD the
/// binary was launched from — the SDL's `CurrentDirGuard` anchors
/// against the scene file's directory.
///
/// Panics on read / parse / eval failure or if the script doesn't
/// `def` the expected binding to a `Value::Scene`. Scene definition
/// is part of program startup; a missing or malformed scene file is
/// a fatal config error rather than something to recover from.
fn load_sdl_scene(rel_path: &str, binding: &str) -> Scene {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scenes")
        .join(rel_path);
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
                path.display(),
                binding
            )
        });
    match value {
        // Clone out of the Rc — render_quadrants takes [Scene; 4] by
        // value. The clone is one-shot at startup and Scene's Vec
        // contents (Light, Shape) all derive Clone, so this is cheap
        // enough to not be worth restructuring around.
        Value::Scene(rc) => (*rc).clone(),
        other => panic!(
            "{} :: {} expected a scene, got {} ({})",
            path.display(),
            binding,
            other,
            other.type_name()
        ),
    }
}

fn render_into<T: RenderTarget + ?Sized>(
    target: &T,
    heatmap: Option<&dyn HeatmapTarget>,
    scene: &Scene, sx: u32, sy: u32,
) {
    let parallel = is_parallel();

    // Wrap the destination in a ProgressTarget so the user sees live
    // row-completion updates while the render is in flight. The wrapper
    // forwards every submit_row to the underlying target unchanged and
    // emits a closing newline via finish() (called by render() at end).
    let progress = ProgressTarget::new(target, sy, &scene.name);

    let start = Instant::now();
    render(scene, sx, sy, &progress, heatmap, parallel);
    let duration = start.elapsed();

    println!("Time elapsed in {} is: {:?} (parallel: {})", scene.name, duration, parallel);
}

/// Run all four quadrant renders against the given pixel target. Factored
/// out of `main` so the streaming and on-disk paths can share the same
/// rendering logic — the only difference between them is which concrete
/// `RenderTarget` they construct.
fn render_quadrants<T: RenderTarget + ?Sized>(
    target: &T,
    heatmap: &PngHeatmapTarget,
    scenes: &[Scene; 4],
    half: u32,
) {
    render_into(
        &OffsetTarget::new(target, 0, 0),
        Some(&OffsetHeatmapTarget::new(heatmap, 0, 0)),
        &scenes[0], half, half,
    );
    render_into(
        &OffsetTarget::new(target, half, 0),
        Some(&OffsetHeatmapTarget::new(heatmap, half, 0)),
        &scenes[1], half, half,
    );
    render_into(
        &OffsetTarget::new(target, 0, half),
        Some(&OffsetHeatmapTarget::new(heatmap, 0, half)),
        &scenes[2], half, half,
    );
    render_into(
        &OffsetTarget::new(target, half, half),
        Some(&OffsetHeatmapTarget::new(heatmap, half, half)),
        &scenes[3], half, half,
    );
}

fn main() {
    let imgdim = 2048;
    let half = imgdim / 2;

    // Parallel heatmap target: same shape as the pixel target, accumulating
    // per-pixel render times in nanoseconds. Saved as a separate
    // single-channel PNG (`render-heatmap.png`) at the end. The renderer
    // currently always populates this; if heatmap collection ever needs to
    // be opt-in, swap `Some(&heatmap_offset)` for `None` in the calls below.
    let heatmap = PngHeatmapTarget::new(imgdim, imgdim);

    // Scenes are now defined in `scenes/<name>.lisp`. To swap one in or
    // out, change the (path, binding-name) pair below — same role as
    // the commented-out `scene_*()` calls used to play before Phase 8
    // deleted `src/scenes.rs`. Available scenes (binding name in
    // parens):
    //   sphere_occlusion_test.lisp ("sphere-occlusion-test-scene")
    //   sphere_surface_test.lisp   ("sphere-surface-test-scene")
    //   one_sphere.lisp            ("one-sphere-scene")
    //   axis_spheres.lisp          ("axis-spheres-scene")
    //   cuboid_test.lisp           ("cuboid-test-scene")
    //   group_test.lisp            ("group-test-scene")
    //   transform_test.lisp        ("transform-test-scene")
    //   multi_light_test.lisp      ("multi-light-test-scene")
    //   ball_on_plane.lisp         ("ball-on-plane-scene")
    //   cylinder_test.lisp         ("cylinder-test-scene")
    //   teapot.lisp                ("teapot-scene")
    let scenes = [
        load_sdl_scene("cuboid_test.lisp",      "cuboid-test-scene"),
        load_sdl_scene("multi_light_test.lisp", "multi-light-test-scene"),
        load_sdl_scene("cylinder_test.lisp",    "cylinder-test-scene"),
        load_sdl_scene("teapot.lisp",           "teapot-scene"),
    ];

    // RTVIEW_ADDR=host:port routes pixels to a streaming receiver instead
    // of writing render.png. Stage one of the rtview integration: the
    // streamed bytes are validated against the on-disk path with a tiny
    // standalone receiver (`cargo run --bin rtview_receiver`); the real
    // Cocoa GUI lands in a later stage.
    match env::var("RTVIEW_ADDR") {
        Ok(addr) => {
            // One backing StreamTarget for the whole composite. Each
            // scene renders into its quadrant via an OffsetTarget — same
            // shape as the on-disk path, just a different sink.
            let target = StreamTarget::connect(&addr, imgdim, imgdim)
                .expect("rtview receiver not reachable at RTVIEW_ADDR");
            render_quadrants(&target, &heatmap, &scenes, half);
            // Skip the inter-quadrant crosshair under streaming so the
            // received image is composed only of pixels that went through
            // submit_row — this is what makes byte-parity testing against
            // render.png meaningful.
        }
        Err(_) => {
            // One backing PngTarget for the whole composite. Each scene
            // renders into its quadrant via an OffsetTarget that
            // re-routes coordinates; no intermediate sub-buffers are
            // allocated.
            let target = PngTarget::new(imgdim, imgdim);
            render_quadrants(&target, &heatmap, &scenes, half);

            // Crosshair lines between quadrants. PngTarget exposes
            // put_pixel for exactly this kind of compositing operation
            // that doesn't fit the row-at-a-time pattern. Color is
            // linear, same as submit_row; PngTarget handles the sRGB
            // encode internally.
            for ii in 0..imgdim - 1 {
                target.put_pixel(ii, imgdim / 2, [1.0, 1.0, 1.0]);
                target.put_pixel(imgdim / 2, ii, [1.0, 1.0, 1.0]);
            }

            target.save("render.png").unwrap();
        }
    }

    // `HeatmapScale::Log` compresses the bright end so the body of the
    // distribution gets more grayscale gradient — useful when scenes
    // contain a complex mesh alongside cheap primitives. Swap to
    // `HeatmapScale::Linear` to see direct proportional brightness.
    heatmap.save("render-heatmap.png", HeatmapScale::Log).unwrap();
}
