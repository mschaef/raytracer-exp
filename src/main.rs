extern crate image;

use crate::image::GenericImage;

mod render;
mod scenes;

use render::{Camera, render};

use scenes::{
    scene_sphere_occlusion_test,
    scene_axis_spheres,
    scene_one_sphere,
    scene_ball_on_plane
};

fn main() {
    let imgdim = 1024;

    let c = Camera {
        location: (0.0, 10.0, 0.0),
        point_at: (0.0, 0.0, 0.0),
        u: (10.0, 0.0, 0.0),
        v: (0.0, 0.0, -10.0)
    };

    let mut output_imgbuf = image::ImageBuffer::new(imgdim, imgdim);

    let scene = [
        scene_sphere_occlusion_test(),
        scene_axis_spheres(),
        scene_one_sphere(),
        scene_ball_on_plane()
    ];

     output_imgbuf.copy_from(&render(&c, &scene[0], imgdim / 2, imgdim / 2), 0, 0)
         .map_err(|err| println!("{:?}", err)).ok();
    output_imgbuf.copy_from(&render(&c, &scene[1], imgdim / 2, imgdim / 2), imgdim / 2, 0)
        .map_err(|err| println!("{:?}", err)).ok();
     output_imgbuf.copy_from(&render(&c, &scene[2], imgdim / 2, imgdim / 2), 0, imgdim / 2)
        .map_err(|err| println!("{:?}", err)).ok();
     output_imgbuf.copy_from(&render(&c, &scene[3], imgdim / 2, imgdim / 2), imgdim / 2, imgdim / 2)
        .map_err(|err| println!("{:?}", err)).ok();

    for ii in 0..imgdim - 1 {
        *output_imgbuf.get_pixel_mut(ii, imgdim / 2) = image::Rgb([255, 255, 255]);
        *output_imgbuf.get_pixel_mut(imgdim / 2, ii) = image::Rgb([255, 255, 255]);
    }

    output_imgbuf.save("render.png").unwrap();
}
