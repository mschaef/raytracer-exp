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

pub mod geometry;
pub mod color;
pub mod shapes;

use rayon::prelude::*;

use geometry::{
    EPSILON,
    Point,
    Vector,
    addp,
    dotp,
    lenp,
    negp,
    normalizep,
    scalep,
    subp,
};

use color::{
    LinearColor,
    scale_linear_color,
    add_linear_color,
    to_png_color,
};

#[derive(Copy, Clone)]
pub struct Surface {
    pub color: LinearColor,
    pub ambient: f64,
    pub specular: f64,
    pub light: f64,
    pub checked: bool,
    pub reflection: f64
}

pub struct Light {
    pub location: Point
}

pub struct Camera {
    pub location: Point,
    pub point_at: Point,
    pub u: Point,
    pub v: Point
}

pub struct Scene {
    pub name: &'static str,
    pub camera: Camera,
    pub light: Light,
    pub objects: Vec<Box<dyn Hittable + Sync + Send>>,
    pub background: LinearColor,

    pub reflect_limit: u32,
    pub oversample: u32,
}

pub trait Hittable {
    fn hit_test(&self, ray: &Vector) -> Option<RayHit>;
}

fn camera_ray(c: &Camera, xt: f64, yt: f64) -> Vector {

    let ray_point_at = addp(addp(c.point_at, scalep(c.u, xt - 0.5)), scalep(c.v, yt - 0.5));

    return Vector {
        start: c.location,
        delta: normalizep(subp(ray_point_at, c.location))
    };
}

fn ray_location(ray: &Vector, t: f64) -> Point {
    let [x, y, z] = ray.start;
    let [dx, dy, dz] = ray.delta;

    return [x + dx * t, y + dy * t, z + dz * t];
}

pub struct RayHit {
    pub distance: f64,
    pub hit_point: Point,
    pub normal: Point,
    pub surface: Surface,
}


fn nearest_hit(ray: &Vector, objects: &Vec<Box<dyn Hittable + Send + Sync>>) -> Option<RayHit> {
    let mut hits = objects.iter().map(| obj | obj.hit_test(&ray))
        .filter_map(| ray_hit | ray_hit )
        .collect::<Vec<RayHit>>();

    hits.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());

    if hits.len() > 0 {
        Some(hits.remove(0))
    } else {
        None
    }
}

fn light_vector(point: &Point, scene: &Scene) -> Option<Vector> {
    let light_direction = subp(*point, scene.light.location);

    let light_distance = lenp(light_direction);

    let ray = Vector {
        start: scene.light.location,
        delta: normalizep(light_direction)
    };

    match nearest_hit(&ray, &scene.objects) {
        Some(hit) =>
            if hit.distance > light_distance - EPSILON {
                Some(ray)
            } else {
                None
            }
        None => None
    }
}


fn shade_pixel(ray: &Vector, scene: &Scene, hit: &RayHit, reflect_count: u32) -> LinearColor {
    // https://en.wikipedia.org/wiki/Lambertian_reflectance

    let scolor = if hit.surface.checked {
        let checkidx = (((hit.hit_point[0] + EPSILON).floor() +
                         (hit.hit_point[1] + EPSILON).floor() +
                         (hit.hit_point[2] + EPSILON).floor()) as i64 % 2).abs();

        scale_linear_color(&hit.surface.color, if checkidx == 0 { 1.0 } else { 0.5 })
    } else {
        hit.surface.color
    };

    let ambient: LinearColor = scale_linear_color(&scolor, hit.surface.ambient);

    let reflected: LinearColor = if (hit.surface.reflection > EPSILON) && (reflect_count >= scene.reflect_limit) {
        let rvec = subp(negp(ray.delta), scalep(hit.normal, 2.0 * dotp(negp(ray.delta), hit.normal)));

        let rcolor = ray_color(&Vector {
            start: hit.hit_point,
            delta: normalizep(rvec)
        }, &scene, reflect_count + 1);

        scale_linear_color(&rcolor, hit.surface.reflection)
    } else {
        [0.0, 0.0, 0.0]
    };


    let light: LinearColor = match light_vector(&hit.hit_point, &scene) {
        Some(lv) => {
            let kspecular = f64::powf(dotp(hit.normal, normalizep(addp(ray.delta, lv.delta))), 50.0) as f64;

            add_linear_color(&scale_linear_color(&[1.0, 1.0, 1.0], kspecular * hit.surface.specular),
                     &scale_linear_color(&scolor, hit.surface.light * dotp(hit.normal, negp(lv.delta)) as f64))

        },
        None => [0.0, 0.0, 0.0]
    };

    add_linear_color(&reflected, &add_linear_color(&ambient, &light))
}

fn ray_color(ray: &Vector, scene: &Scene, reflect_count: u32) -> LinearColor {
    match nearest_hit(&ray, &scene.objects) {
        Some(hit) => shade_pixel(&ray, &scene, &hit, reflect_count),
        None => scene.background
    }
}



pub fn render_into_line(
    scene: &Scene, imgx: u32, imgy: u32, row: image::buffer::EnumeratePixelsMut<image::Rgb<u8>>
) {
    let dx = 1.0 / imgx as f64;
    let dy = 1.0 / imgy as f64;

    let subdx = dx / (scene.oversample as f64 * 2.0);
    let subdy = dy / (scene.oversample as f64 * 2.0);

    for (_, (x, y, pixel)) in row.enumerate() {

        let mut pc = [0.0, 0.0, 0.0];

        let xc = x as f64 * dx - dx / 2.0;
        let yc = y as f64 * dy - dy / 2.0;

        for iix in 0..scene.oversample {
            for iiy in 0..scene.oversample {

                let rc = ray_color(&camera_ray(&scene.camera, xc + subdx * (1.0 + iix as f64), yc + subdy * (1.0 + iiy as f64)), &scene, 0);

                pc = add_linear_color(&pc, &rc)
            }
        }

        *pixel = image::Rgb(to_png_color(&scale_linear_color(&pc, 1.0 / (scene.oversample * scene.oversample) as f64)))
    }
}

pub fn render(
    scene: &Scene, imgx: u32, imgy: u32, parallel: bool
) -> image::ImageBuffer<image::Rgb<u8>, Vec<u8>> {

    let mut imgbuf = image::ImageBuffer::new(imgx, imgy);

    if parallel {
        imgbuf.enumerate_rows_mut()
            .par_bridge()
            .for_each(| (_, row ) | render_into_line(scene, imgx, imgy, row));
    } else {
        for (_, row) in imgbuf.enumerate_rows_mut() {
            render_into_line(scene, imgx, imgy, row)
        }
    }

    imgbuf
}

