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

mod render;
mod scenes;

use render::{render, Scene};
use render::output::{PngTarget, OffsetTarget, RenderTarget};

use scenes::{
    //scene_sphere_occlusion_test,
    //scene_sphere_surface_test,
    scene_cuboid_test,
    //scene_axis_spheres,
    scene_multi_light_test,
    //scene_one_sphere,
    scene_transform_test,
    //scene_ball_on_plane
    scene_teapot
};

fn is_parallel() -> bool {
    match env::var("PARALLEL") {
        Ok(val) => val.to_lowercase() == "y",
        Err(_) => true
    }
}

fn render_into<T: RenderTarget + ?Sized>(
    target: &T, scene: &Scene, sx: u32, sy: u32,
) {
    let parallel = is_parallel();

    let start = Instant::now();
    render(scene, sx, sy, target, parallel);
    let duration = start.elapsed();

    println!("Time elapsed in {} is: {:?} (parallel: {})", scene.name, duration, parallel);
}

fn main() {
    let imgdim = 2048;
    let half = imgdim / 2;

    // One backing PngTarget for the whole composite. Each scene renders
    // into its quadrant via an OffsetTarget that re-routes coordinates;
    // no intermediate sub-buffers are allocated.
    let target = PngTarget::new(imgdim, imgdim);

    let scene = [
        //scene_sphere_occlusion_test(),
        //scene_sphere_surface_test(),
        scene_cuboid_test(),
        //scene_axis_spheres(),
        scene_multi_light_test(),
        //scene_one_sphere(),
        scene_transform_test(),
        //scene_ball_on_plane()
        scene_teapot()
    ];

    render_into(&OffsetTarget::new(&target, 0,    0   ), &scene[0], half, half);
    render_into(&OffsetTarget::new(&target, half, 0   ), &scene[1], half, half);
    render_into(&OffsetTarget::new(&target, 0,    half), &scene[2], half, half);
    render_into(&OffsetTarget::new(&target, half, half), &scene[3], half, half);

    // Crosshair lines between quadrants. PngTarget exposes put_pixel for
    // exactly this kind of compositing operation that doesn't fit the
    // row-at-a-time pattern.
    for ii in 0..imgdim - 1 {
        target.put_pixel(ii, imgdim / 2, [255, 255, 255]);
        target.put_pixel(imgdim / 2, ii, [255, 255, 255]);
    }

    target.save("render.png").unwrap();
}
