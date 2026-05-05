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

use raytracer::scenes::{
    //scene_sphere_occlusion_test,
    //scene_sphere_surface_test,
    scene_cuboid_test,
    //scene_axis_spheres,
    scene_multi_light_test,
    //scene_one_sphere,
    //scene_transform_test,
    //scene_ball_on_plane
    scene_cylinder_test,
    scene_teapot
};

fn is_parallel() -> bool {
    match env::var("PARALLEL") {
        Ok(val) => val.to_lowercase() == "y",
        Err(_) => true
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

    let scenes = [
        //scene_sphere_occlusion_test(),
        //scene_sphere_surface_test(),
        scene_cuboid_test(),
        //scene_axis_spheres(),
        scene_multi_light_test(),
        //scene_one_sphere(),
        //scene_transform_test(),
        //scene_ball_on_plane()
        scene_cylinder_test(),
        scene_teapot()
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
