// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

extern crate image;

use std::env;
use std::time::Instant;

use crate::image::GenericImage;

mod render;
mod scenes;

use render::{render, Scene};

use scenes::{
    //scene_sphere_occlusion_test,
    scene_sphere_surface_test,
    scene_axis_spheres,
    scene_one_sphere,
    scene_ball_on_plane
};

fn render_into(parallel: bool,
               output_imgbuf: &mut image::ImageBuffer<image::Rgb<u8>, Vec<u8>>,
               scene: &Scene, sx: u32, sy: u32, x: u32, y: u32) {

    let start = Instant::now();

    output_imgbuf.copy_from(&render(&scene, sx, sy, parallel), x, y)
        .map_err(|err| println!("{:?}", err)).ok();

    let duration = start.elapsed();
    println!("Time elapsed in {} is: {:?} (parallel: {})", scene.name, duration, parallel);
}

fn is_parallel() -> bool {
    match env::var("PARALLEL") {
        Ok(val) => val.to_lowercase() == "y",
        Err(_) => true
    }
}

fn main() {
    let imgdim = 2048;
    let half = imgdim / 2;
    let parallel = is_parallel();


    let mut output_imgbuf = image::ImageBuffer::new(imgdim, imgdim);

    let scene = [
        //scene_sphere_occlusion_test(),
        scene_sphere_surface_test(),
        scene_axis_spheres(),
        scene_one_sphere(),
        scene_ball_on_plane()
    ];

    render_into(parallel, &mut output_imgbuf, &scene[0], half, half, 0, 0);
    render_into(parallel, &mut output_imgbuf, &scene[1], half, half, half, 0);
    render_into(parallel, &mut output_imgbuf, &scene[2], half, half, 0, half);
    render_into(parallel, &mut output_imgbuf, &scene[3], half, half, half, half);

    for ii in 0..imgdim - 1 {
        *output_imgbuf.get_pixel_mut(ii, imgdim / 2) = image::Rgb([255, 255, 255]);
        *output_imgbuf.get_pixel_mut(imgdim / 2, ii) = image::Rgb([255, 255, 255]);
    }

    output_imgbuf.save("render.png").unwrap();
}
