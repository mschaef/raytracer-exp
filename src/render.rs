extern crate image;

mod geometry;

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

pub type Color = [f32; 3];

#[derive(Copy, Clone)]
pub struct Surface {
    pub color: Color
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
    pub camera: Camera,
    pub light: Light,
    pub objects: Vec<Box<dyn Hittable>>,
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
    let (x, y, z) = ray.start;
    let (dx, dy, dz) = ray.delta;

    return (x + dx * t, y + dy * t, z + dz * t);
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

fn nearest_hit(ray: &Vector, objects: &Vec<Box<dyn Hittable>>) -> Option<RayHit> {
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

fn scale_color(color: &Color, s: f32) -> Color {
    [
        color[0] * s,
        color[1] * s,
        color[2] * s
    ]
}

fn shade_pixel(scene: &Scene, hit: &RayHit) -> Color {
    // https://en.wikipedia.org/wiki/Lambertian_reflectance

    match light_vector(&hit.hit_point, &scene) {
        Some(lv) => scale_color(&hit.surface.color, dotp(hit.normal, negp(lv.delta)) as f32),
        None => [0.0, 0.0, 0.0]
    }
}

fn ray_color(ray: &Vector, scene: &Scene) -> Color {
    match nearest_hit(&ray, &scene.objects) {
        Some(hit) => shade_pixel(&scene, &hit),
        None => scene.background
    }
}

fn to_png_color(color: &Color) -> [u8; 3] {
    [
        (color[0] * 256.0) as u8,
        (color[1] * 256.0) as u8,
        (color[2] * 256.0) as u8
    ]
}

pub fn render(
    scene: &Scene, imgx: u32, imgy: u32
) -> image::ImageBuffer<image::Rgb<u8>, Vec<u8>> {

    let mut imgbuf = image::ImageBuffer::new(imgx, imgy);

    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        let ray = camera_ray(&scene.camera, x as f64 / imgx as f64, y as f64 / imgy as f64);
        *pixel = image::Rgb(to_png_color(&ray_color(&ray, &scene)));
    }

    imgbuf
}
