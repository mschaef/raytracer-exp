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

struct Plane {
    normal: Point,
    p0: Point,
    color: Color,
}

struct Light {
    location: Point
}

trait Hittable {
    fn hit_test(&self, ray: &Vector) -> Option<RayHit>;
}

struct Scene {
    light: Light,
    objects: Vec<Box<dyn Hittable>>,
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
                color: self.color
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
                    color: self.color
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

fn shade_pixel(hit: &RayHit, lv: &Vector) -> Color {
    // https://en.wikipedia.org/wiki/Lambertian_reflectance

    scale_color(&hit.color, dotp(hit.normal, negp(lv.delta)) as f32)
}

fn ray_color(ray: &Vector, scene: &Scene) -> Color {
    match nearest_hit(&ray, &scene.objects) {
        Some(hit) =>
            match light_vector(&hit.hit_point, &scene) {
                Some(lv) => shade_pixel(&hit, &lv),
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
        objects: vec![
            Box::new(Sphere {
                center: (1.5, 2.0, 0.0),
                r: 0.7,
                color: [1.0, 0.5, 0.0]
            }),
            Box::new(Sphere {
                center: (3.0, 0.0, 0.0),
                r: 1.0,
                color: [1.0, 0.0, 0.0]
            }),
            Box::new(Sphere {
                center: (-3.0, 0.0, 0.0),
                r: 1.0,
                color: [0.0, 0.0, 1.0],
            }),
            Box::new(Sphere {
                center: (0.0, 0.0, 0.0),
                r: 1.0,
                color: [0.0, 1.0, 0.0]
            }),
            Box::new(Sphere {
                center: (0.0, -4.0, 0.0),
                r: 3.0,
                color: [1.0, 1.0, 0.0]
            }),
            Box::new(Sphere { // foreground sphere at back at list - proper occlusion required to make this visible
                center: (-1.5, 2.0, 0.0),
                r: 0.7,
                color: [1.0, 0.0, 1.0]
            }),
        ]
    }
}

fn scene_one_sphere() -> Scene {
    Scene {
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: (10.0, 10.0, 10.0)
        },
        objects: vec![
            Box::new(Sphere {
                center: (0.0, 0.0, 0.0),
                r: 1.0,
                color: [1.0, 0.5, 0.0]
            }),

            Box::new(Plane {
                normal: (1.0, 0.0, 0.0),
                p0: (-3.0, 0.0, 0.0),
                color: [1.0, 0.0, 0.0]
            }),
            Box::new(Plane {
                normal: (0.0, 1.0, 0.0),
                p0: (0.0, -3.0, 0.0),
                color: [0.0, 1.0, 0.0]
            }),
            Box::new(Plane {
                normal: (0.0, 0.0, 1.0),
                p0: (0.0, 0.0, -3.0),
                color: [0.0, 0.0, 1.0]
            }),
        ]
    }
}

fn scene_axis_spheres() -> Scene {
    Scene {
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: (10.0, 10.0, 10.0)
        },
        objects: vec![
            Box::new(Sphere {
                center: (0.0, 0.0, 0.0),
                r: 1.0,
                color: [1.0, 1.0, 1.0]
            }),
            Box::new(Sphere {
                center: (3.0, 0.0, 0.0),
                r: 0.25,
                color: [1.0, 0.0, 0.0]
            }),
            Box::new(Sphere {
                center: (0.0, 3.0, 0.0),
                r: 0.25,
                color: [0.0, 1.0, 0.0]
            }),
            Box::new(Sphere {
                center: (0.0, 0.0, 3.0),
                r: 0.25,
                color: [0.0, 0.0, 1.0]
            }),
        ]
    }
}


fn scene_ball_on_plane() -> Scene {
    Scene {
        background: [0.0, 0.0, 0.0],
        light: Light {
            location: (10.0, 10.0, 10.0)
        },
        objects: vec![
            Box::new(Sphere {
                center: (0.0, -2.0, -1.0),
                r: 0.66,
                color: [0.0, 0.0, 1.0]
            }),
            Box::new(Plane {
                normal: (0.0, 0.0, 1.0),
                p0: (0.0, 0.0, -2.0),
                color: [1.0, 1.0, 1.0]
            }),

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
