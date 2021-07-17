extern crate image;

use crate::image::GenericImage;

mod render;
mod scenes;

use render::{render, Scene};

use scenes::{
    scene_sphere_occlusion_test,
    scene_axis_spheres,
    scene_one_sphere,
    scene_ball_on_plane
};

fn render_into(output_imgbuf: &mut image::ImageBuffer<image::Rgb<u8>, Vec<u8>>,
               scene: &Scene, sx: u32, sy: u32, x: u32, y: u32) {

    output_imgbuf.copy_from(&render(&scene, sx, sy), x, y)
        .map_err(|err| println!("{:?}", err)).ok();
}

fn main() {
    let imgdim = 1024;
    let half = imgdim / 2;

    let mut output_imgbuf = image::ImageBuffer::new(imgdim, imgdim);

    let scene = [
        scene_sphere_occlusion_test(),
        scene_axis_spheres(),
        scene_one_sphere(),
        scene_ball_on_plane()
    ];

    render_into(&mut output_imgbuf, &scene[0], half, half, 0, 0);
    render_into(&mut output_imgbuf, &scene[1], half, half, half, 0);
    render_into(&mut output_imgbuf, &scene[2], half, half, 0, half);
    render_into(&mut output_imgbuf, &scene[3], half, half, half, half);

    for ii in 0..imgdim - 1 {
        *output_imgbuf.get_pixel_mut(ii, imgdim / 2) = image::Rgb([255, 255, 255]);
        *output_imgbuf.get_pixel_mut(imgdim / 2, ii) = image::Rgb([255, 255, 255]);
    }

    output_imgbuf.save("render.png").unwrap();
}
