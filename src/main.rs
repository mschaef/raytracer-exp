//! An example of generating julia fractals.
extern crate image;
extern crate num_complex;

const EPSILON: f64 = 0.0001;

type Point = (f64, f64, f64);

struct Vector {
    start: Point,
    delta: Point
}

fn normalizep(pt: Point) -> Point {
    let (x, y, z) = pt;

    let len = ((x * x) + (y * y) + (z * z)).sqrt();

    if len < EPSILON {
        panic!("Cannot normalize point vector of length 0.");
    }

    return (x / len, y / len, z / len);
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

fn normalizev(vec: Vector) -> Vector {
    return Vector {
        start: vec.start,
        delta: normalizep(vec.delta)
    };
}

struct Sphere {
    center: Point,
    r: f64,
    color: [u8; 3]
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

fn solve_quadratic(a: f64, b: f64, c: f64) -> Option<(f64, f64)> {

    let discr = b * b - 4.0 * a * c;

    if discr < 0.0 {
        None
    } else if discr < EPSILON {
        let x = - 0.5 * b / a;
        Some((x, x))
    } else {
        let q = if b > 0.0 {
            -0.5 * (b + discr.sqrt())
        } else {
            -0.5 * (b - discr.sqrt())
        };

        let x0 = q / a;
        let x1 = c / q;

        if x0 > x1 {
            Some((x1, x0))
        } else {
            Some((x0, x1))
        }
    }
}

fn hit_sphere(s: &Sphere, ray: &Vector) -> bool {

    // Hit test algorithm taken from this website and translated to
    // Rust:
    //
    // https://viclw17.github.io/2018/07/16/raytracing-ray-sphere-intersection

    let oc = subp(ray.start, s.center);
    let a = dotp(ray.delta, ray.delta);
    let b = 2.0 * dotp(oc, ray.delta);
    let c = dotp(oc, oc) - s.r * s.r;
    let discriminant = b*b - 4.0*a*c;

    discriminant > 0.0
}

fn main() {
    let imgx = 800;
    let imgy = 800;

    let c = Camera {
        location: (0.0, 10.0, 0.0),
        point_at: (0.0, 0.0, 0.0),
        u: (10.0, 0.0, 0.0),
        v: (0.0, 0.0, 10.0)
    };

    let spheres = [
        Sphere {
            center: (3.0, 0.0, 0.0),
            r: 1.0,
            color: [255, 0, 0]
        },
        Sphere {
            center: (-3.0, 0.0, 0.0),
            r: 1.0,
            color: [0, 0, 255]
        },
        Sphere {
            center: (0.0, 0.0, 0.0),
            r: 1.0,
            color: [0, 255, 0]
        },
        Sphere {
            center: (0.0, -4.0, 0.0),
            r: 5.0,
            color: [255, 255, 0]
        },
    ];

    // Create a new ImgBuf with width: imgx and height: imgy
    let mut imgbuf = image::ImageBuffer::new(imgx, imgy);

    for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
        let ray = camera_ray(&c, x as f64 / imgx as f64, y as f64 / imgy as f64);

        let s = spheres.iter().find(| &s | hit_sphere(&s, &ray));

        let cvec = match s {
            None => [255 as u8, 255 as u8, 255 as u8],
            Some(hs) => hs.color
        };

        *pixel = image::Rgb(cvec);
    }

    imgbuf.save("fractal.png").unwrap();
}
