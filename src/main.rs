extern crate image;

use crate::image::GenericImage;

const EPSILON: f64 = 0.0001;

type Point = (f64, f64, f64);

struct Vector {
    start: Point,
    delta: Point
}

fn lenp(pt: Point) -> f64 {
    let (x, y, z) = pt;

    ((x * x) + (y * y) + (z * z)).sqrt()
}

fn normalizep(pt: Point) -> Point {
    let (x, y, z) = pt;

    let len = lenp(pt);

    if len < EPSILON {
        panic!("Cannot normalize point vector of length 0.");
    }

    (x / len, y / len, z / len)
}

fn scalep(pt: Point, t: f64) -> Point {
    let (x, y, z) = pt;

    return (x * t, y * t, z * t);
}

fn addp(pt0: Point, pt1: Point) -> Point {
    let (x0, y0, z0) = pt0;
    let (x1, y1, z1) = pt1;

    return (x0 + x1, y0 + y1, z0 + z1);
}

fn subp(pt0: Point, pt1: Point) -> Point {
    let (x0, y0, z0) = pt0;
    let (x1, y1, z1) = pt1;

    return (x0 - x1, y0 - y1, z0 - z1);
}

fn dotp(pt0: Point, pt1: Point) -> f64 {
    let (x0, y0, z0) = pt0;
    let (x1, y1, z1) = pt1;

    return x0 * x1 + y0 * y1 + z0 * z1;
}

fn negp(pt: Point) -> Point {
    let (x, y, z) = pt;

    return (-x, -y, -z);
}

type Color = [f32; 3];

struct Sphere {
    center: Point,
    r: f64,
    color: Color,
}

struct Light {
    location: Point
}

struct Scene {
    light: Light,
    spheres: Vec<Sphere>,
    background: Color,
}

struct Camera {
    location: Point,
    point_at: Point,
    u: Point,
    v: Point
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

struct RayHit {
    distance: f64,
    hit_point: Point,
    normal: Point,
    color: Color,
}

fn hit_sphere(s: &Sphere, ray: &Vector) -> Option<RayHit> {

    // Hit test algorithm taken from this website and translated to
    // Rust:
    //
    // https://viclw17.github.io/2018/07/16/raytracing-ray-sphere-intersection

    let oc = subp(ray.start, s.center);
    let a = dotp(ray.delta, ray.delta);
    let b = 2.0 * dotp(oc, ray.delta);
    let c = dotp(oc, oc) - s.r * s.r;
    let discriminant = b*b - 4.0*a*c;

    if discriminant < 0.0 {
        None
    } else {
        let t = (-b - discriminant.sqrt()) / (2.0*a);
        let hit_point = ray_location(&ray, t);

        Some(RayHit {
            distance: t,
            hit_point: hit_point,
            normal: normalizep(subp(hit_point, s.center)),
            color: s.color
        })
    }
}

fn nearest_hit(ray: &Vector, spheres: &Vec<Sphere>) -> Option<RayHit> {
    let mut hits = spheres.iter().map(| s | hit_sphere(&s, &ray))
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

    match nearest_hit(&ray, &scene.spheres) {
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

fn ray_color(ray: &Vector, scene: &Scene) -> Color {
    match nearest_hit(&ray, &scene.spheres) {
        Some(hit) =>
            match light_vector(&hit.hit_point, &scene) {
                Some(lv) => scale_color(&hit.color, dotp(hit.normal, negp(lv.delta)) as f32),
                None => [0.0, 0.0, 0.0]
            },
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

fn scene_sphere_occlusion_test() -> Scene {
    Scene {
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: (5.0, 5.0, 5.0)
        },
        spheres: vec![
            Sphere { // foreground sphere - visible b/c first in list
                center: (1.5, 2.0, 0.0),
                r: 0.7,
                color: [1.0, 0.5, 0.0]
            },
            Sphere {
                center: (3.0, 0.0, 0.0),
                r: 1.0,
                color: [1.0, 0.0, 0.0]
            },
            Sphere {
                center: (-3.0, 0.0, 0.0),
                r: 1.0,
                color: [0.0, 0.0, 1.0],
            },
            Sphere {
                center: (0.0, 0.0, 0.0),
                r: 1.0,
                color: [0.0, 1.0, 0.0]
            },
            Sphere {
                center: (0.0, -4.0, 0.0),
                r: 3.0,
                color: [1.0, 1.0, 0.0]
            },
            Sphere { // foreground sphere at back at list - proper occlusion required to make this visible
                center: (-1.5, 2.0, 0.0),
                r: 0.7,
                color: [1.0, 0.0, 1.0]
            },
        ]
    }
}

fn scene_one_sphere() -> Scene {
    Scene {
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: (10.0, 10.0, 10.0)
        },
        spheres: vec![
            Sphere {
                center: (0.0, 0.0, 0.0),
                r: 1.0,
                color: [1.0, 0.5, 0.0]
            }
        ]
    }
}

fn scene_axis_spheres() -> Scene {
    Scene {
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: (10.0, 10.0, 10.0)
        },
        spheres: vec![
            Sphere {
                center: (0.0, 0.0, 0.0),
                r: 1.0,
                color: [1.0, 1.0, 1.0]
            },
            Sphere {
                center: (3.0, 0.0, 0.0),
                r: 0.25,
                color: [1.0, 0.0, 0.0]
            },
            Sphere {
                center: (0.0, 0.0, 3.0),
                r: 0.25,
                color: [0.0, 0.0, 1.0]
            },
            Sphere {
                center: (0.0, 3.0, 0.0),
                r: 0.25,
                color: [0.0, 1.0, 0.0]
            }
        ]
    }
}

fn render(
    c: &Camera, scene: &Scene, imgx: u32, imgy: u32
) -> image::ImageBuffer<image::Rgb<u8>, Vec<u8>> {

    let mut imgbuf = image::ImageBuffer::new(imgx, imgy);

    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        let ray = camera_ray(&c, x as f64 / imgx as f64, y as f64 / imgy as f64);
        *pixel = image::Rgb(to_png_color(&ray_color(&ray, &scene)));
    }

    imgbuf
}

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
        scene_one_sphere(),
        scene_axis_spheres()
    ];

    output_imgbuf.copy_from(&render(&c, &scene[0], imgdim / 2, imgdim / 2), 0, 0)
        .map_err(|err| println!("{:?}", err)).ok();
    output_imgbuf.copy_from(&render(&c, &scene[1], imgdim / 2, imgdim / 2), imgdim / 2, 0)
        .map_err(|err| println!("{:?}", err)).ok();
    output_imgbuf.copy_from(&render(&c, &scene[2], imgdim / 2, imgdim / 2), 0, imgdim / 2)
        .map_err(|err| println!("{:?}", err)).ok();

    for ii in 0..imgdim - 1 {
        *output_imgbuf.get_pixel_mut(ii, imgdim / 2) = image::Rgb([255, 255, 255]);
        *output_imgbuf.get_pixel_mut(imgdim / 2, ii) = image::Rgb([255, 255, 255]);
    }

    output_imgbuf.save("render.png").unwrap();
}
