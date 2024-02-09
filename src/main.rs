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

use render::{render, Scene, Surface};

use scenes::{
    SURFACE_BLACK,
    SURFACE_BLUE,
    SURFACE_GREEN,
    SURFACE_ORANGE,
    SURFACE_PURPLE,
    SURFACE_RED,
    SURFACE_WHITE,
    SURFACE_YELLOW,
    scene_one_sphere
};

fn render_into(output_imgbuf: &mut image::ImageBuffer<image::Rgb<u8>, Vec<u8>>,
               scene: &Scene, sx: u32, sy: u32, x: u32, y: u32) {

    let parallel = is_parallel();

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

fn render_sphere(sphere_surface: Surface, imgdim: u32, filename: &str) {
    let mut output_imgbuf = image::ImageBuffer::new(imgdim, imgdim);
    render_into(&mut output_imgbuf, &scene_one_sphere(sphere_surface), 128, 128, 0, 0);
    output_imgbuf.save(filename).unwrap();
}

fn main() {
    render_sphere(SURFACE_BLACK , 128, "sphere-black.png");
    render_sphere(SURFACE_BLUE  , 128, "sphere-blue.png");
    render_sphere(SURFACE_GREEN , 128, "sphere-green.png");
    render_sphere(SURFACE_ORANGE, 128, "sphere-orange.png");
    render_sphere(SURFACE_PURPLE, 128, "sphere-purple.png");
    render_sphere(SURFACE_RED   , 128, "sphere-red.png");
    render_sphere(SURFACE_WHITE , 128, "sphere-white.png");
    render_sphere(SURFACE_YELLOW, 128, "sphere-yellow.png");
}
