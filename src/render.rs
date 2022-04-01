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

mod geometry;

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

const REFLECT_LIMIT: i32 = 2;

pub type Color = [f64; 3];

#[derive(Copy, Clone)]
pub struct Surface {
    pub color: Color,
    pub ambient: f64,
    pub specular: f64,
    pub light: f64,
    pub checked: bool,
    pub reflection: f64
}

pub struct Sphere {
    pub center: Point,
    pub r: f64,
    pub surface: Surface,
}

pub struct Plane {
    pub normal: Point,
    pub p0: Point,
    pub surface: Surface,
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
    pub background: Color,
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

impl Hittable for Sphere {
    fn hit_test(&self, ray: &Vector) -> Option<RayHit> {
        // Hit test algorithm taken from this website and translated to
        // Rust:
        //
        // https://viclw17.github.io/2018/07/16/raytracing-ray-sphere-intersection

        let oc = subp(ray.start, self.center);
        let a = dotp(ray.delta, ray.delta);
        let b = 2.0 * dotp(oc, ray.delta);
        let c = dotp(oc, oc) - self.r * self.r;
        let discriminant = b*b - 4.0*a*c;

        if discriminant < 0.0 {
            None
        } else {
            let t = (-b - discriminant.sqrt()) / (2.0*a);
            let hit_point = ray_location(&ray, t);

            Some(RayHit {
                distance: t,
                hit_point: hit_point,
                normal: normalizep(subp(hit_point, self.center)),
                surface: self.surface
            })
        }
    }
}

impl Hittable for Plane {
    fn hit_test(&self, ray: &Vector) -> Option<RayHit> {
        let denom = dotp(self.normal, ray.delta);

        if denom.abs() < EPSILON {
            None
        } else {
            let p0l0 = subp(self.p0, ray.start);
            let t = dotp(p0l0, self.normal) / denom;

            if t <= EPSILON {
                None
            } else {
                let hit_point = ray_location(&ray, t);

                Some(RayHit {
                    distance: t,
                    hit_point: hit_point,
                    normal: self.normal,
                    surface: self.surface
                })
            }
        }
    }
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

fn scale_color(color: &Color, s: f64) -> Color {
    [
        color[0] * s,
        color[1] * s,
        color[2] * s
    ]
}

fn addcolor(colora: &Color, colorb: &Color) -> Color {
    [
        colora[0] + colorb[0],
        colora[1] + colorb[1],
        colora[2] + colorb[2],
    ]
}

fn shade_pixel(ray: &Vector, scene: &Scene, hit: &RayHit, reflect_limit: i32) -> Color {
    // https://en.wikipedia.org/wiki/Lambertian_reflectance

    let scolor = if hit.surface.checked {
        let checkidx = (((hit.hit_point[0] + EPSILON).floor() +
                         (hit.hit_point[1] + EPSILON).floor() +
                         (hit.hit_point[2] + EPSILON).floor()) as i64 % 2).abs();

        scale_color(&hit.surface.color, if checkidx == 0 { 1.0 } else { 0.5 })
    } else {
        hit.surface.color
    };

    let ambient: Color = scale_color(&scolor, hit.surface.ambient);

    let reflected: Color = if (hit.surface.reflection > EPSILON) && (reflect_limit > 0) {
        let rvec = subp(negp(ray.delta), scalep(hit.normal, 2.0 * dotp(negp(ray.delta), hit.normal)));

        let rcolor = ray_color(&Vector {
            start: hit.hit_point,
            delta: normalizep(rvec)
        }, &scene, reflect_limit - 1);

        scale_color(&rcolor, hit.surface.reflection)
    } else {
        [0.0, 0.0, 0.0]
    };


    let light: Color = match light_vector(&hit.hit_point, &scene) {
        Some(lv) => {
            let kspecular = f64::powf(dotp(hit.normal, normalizep(addp(ray.delta, lv.delta))), 50.0) as f64;

            addcolor(&scale_color(&[1.0, 1.0, 1.0], kspecular * hit.surface.specular),
                     &scale_color(&scolor, hit.surface.light * dotp(hit.normal, negp(lv.delta)) as f64))

        },
        None => [0.0, 0.0, 0.0]
    };


    addcolor(&reflected, &addcolor(&ambient, &light))
}

fn ray_color(ray: &Vector, scene: &Scene, reflect_limit: i32) -> Color {
    match nearest_hit(&ray, &scene.objects) {
        Some(hit) => shade_pixel(&ray, &scene, &hit, reflect_limit),
        None => scene.background
    }
}


fn linear_to_srgb(x: f64) -> f64 {
    if x < 0.0 {
        0.0
    } else if x < 0.0031308	{
        x * 12.92
    } else if x < 1.0 {
        1.055 * x.powf(1.0/2.4) - 0.055
    } else {
        1.0
    }
}

fn to_png_color(color: &Color) -> [u8; 3] {
    [
        (linear_to_srgb(color[0]) * 256.0) as u8,
        (linear_to_srgb(color[1]) * 256.0) as u8,
        (linear_to_srgb(color[2]) * 256.0) as u8
    ]
}

pub fn render_into_line(
    scene: &Scene, imgx: u32, imgy: u32, row: image::buffer::EnumeratePixelsMut<image::Rgb<u8>>
) {
    for (_, (x, y, pixel)) in row.enumerate() {
        let ray = camera_ray(&scene.camera, x as f64 / imgx as f64, y as f64 / imgy as f64);
        *pixel = image::Rgb(to_png_color(&ray_color(&ray, &scene, REFLECT_LIMIT)));
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

