// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

pub mod geometry;
pub mod color;
pub mod transform;
pub mod shapes;
pub mod mesh;
pub mod output;

use std::convert::TryFrom;
use std::time::Instant;

use shapes::{Shape, nearest_hit};
use output::{RenderTarget, HeatmapTarget};

use rayon::prelude::*;

/// Build a `Vec<Shape>` from a comma-separated list of shape literals
/// (e.g. `Sphere { ... }`, `Plane { ... }`, `Cuboid { ... }`). Each entry
/// is converted to `Shape` via its `From` impl, so scene definitions don't
/// have to spell out `Box::new(...)` or `Shape::Sphere(...)` for every
/// object.
#[macro_export]
macro_rules! scene_objects {
    ($($shape:expr),* $(,)?) => {
        vec![$(<$crate::render::shapes::Shape>::from($shape)),*]
    };
}

use geometry::{
    EPSILON,
    Point,
    Vector,
    addp,
    crossp,
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
    multiply_linear_color,
    to_png_color,
};

use std::cmp::Ordering;

#[derive(Copy, Clone)]
pub struct Surface {
    pub color: LinearColor,
    pub ambient: f64,
    pub specular: f64,
    pub light: f64,
    pub checked: bool,
    pub reflection: f64
}

/// A point light source. Carries an emitted color and a scalar intensity
/// so that scenes can use multiple visually distinct lights for testing
/// and artistic control. The current shading model is white-implicit
/// when `color = [1.0, 1.0, 1.0]` and `intensity = 1.0`, so existing
/// scenes can be ported by wrapping their location in `Light::white`.
pub struct Light {
    pub location: Point,
    pub color: LinearColor,
    pub intensity: f64,
}

impl Light {
    /// Full-intensity white point light at `location`. Equivalent to the
    /// implicit light parameters in earlier versions of this codebase.
    pub const fn white(location: Point) -> Light {
        Light {
            location,
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
        }
    }

    /// Point light with an explicit color and intensity.
    pub const fn point(location: Point, color: LinearColor, intensity: f64) -> Light {
        Light { location, color, intensity }
    }
}

/// Look-at camera in pre-computed form.
///
/// Construct via `Camera::looking_at` (zoom-based) or `Camera::with_fov`
/// (field-of-view based) rather than building this struct directly — the
/// constructors derive an orthonormal basis from the user-friendly inputs
/// `(location, look_at, up_hint)` and cache the result here so per-ray
/// work is just additions and scales.
///
/// Fields:
/// - `location`     — world-space camera position.
/// - `forward`      — unit vector pointing from `location` toward the look-at point.
/// - `right`        — unit vector along the camera's right (image +x).
/// - `up`           — unit vector along the camera's up (image −y after inversion).
///                    Re-orthogonalized from the user's `up_hint`.
/// - `half_height`  — half the height of the view plane at unit distance.
///                    Smaller values = more zoomed in.
#[derive(Copy, Clone)]
pub struct Camera {
    pub location: Point,
    pub forward: Point,
    pub right: Point,
    pub up: Point,
    pub half_height: f64,
}

impl Camera {
    /// Construct a camera by location, target, up-direction hint, and zoom.
    ///
    /// `up_hint` need not be perpendicular to the view direction — the
    /// component along `forward` is projected out and the result is
    /// renormalized. The only constraint is that `up_hint` must not be
    /// parallel to `(look_at - location)`, which would leave no
    /// orientation degree of freedom.
    ///
    /// `zoom` is a positive multiplier: `zoom = 1.0` corresponds to a
    /// vertical FOV of about 53° (a "normal" lens, equivalent to the
    /// previous default camera). `zoom = 2.0` is twice as zoomed in,
    /// `zoom = 0.5` is wider-angle.
    pub fn looking_at(
        location: Point,
        look_at: Point,
        up_hint: Point,
        zoom: f64,
    ) -> Camera {
        let forward = normalizep(subp(look_at, location));

        // right = up_hint × forward, then renormalize. This convention
        // gives the intuitive result for a camera placed above the scene
        // looking down at the origin (right = +x).
        let right_unnorm = crossp(up_hint, forward);
        if lenp(right_unnorm) < EPSILON {
            panic!("Camera::looking_at: up_hint is parallel to view direction");
        }
        let right = normalizep(right_unnorm);

        // Re-orthogonalize up: project user's up_hint onto the plane
        // perpendicular to forward by taking forward × right.
        let up = crossp(forward, right);

        // half_height = 0.5 / zoom puts a 1.0-tall view plane at unit
        // distance when zoom = 1, giving 2*atan(0.5) ≈ 53° vertical FOV.
        let half_height = 0.5 / zoom;

        Camera { location, forward, right, up, half_height }
    }

    /// Construct a camera using a vertical field of view (in radians)
    /// instead of a zoom factor. Internally maps to
    /// `half_height = tan(fov / 2)`.
    pub fn with_fov(
        location: Point,
        look_at: Point,
        up_hint: Point,
        fov_radians: f64,
    ) -> Camera {
        let half_height = (fov_radians / 2.0).tan();
        // Equivalent zoom = 0.5 / half_height; route through `looking_at`
        // so all the basis math lives in one place.
        Camera::looking_at(location, look_at, up_hint, 0.5 / half_height)
    }
}

struct CameraDetails {
    pub camera: Camera,
    pub dx: f64,
    pub dy: f64,
    pub aspect: f64,
    pub oversample: u32,
}

pub struct Scene {
    pub name: &'static str,
    pub camera: Camera,
    pub lights: Vec<Light>,
    pub objects: Vec<Shape>,
    pub background: LinearColor,

    pub reflect_limit: u32,
    pub oversample: u32,
}

pub trait Hittable {
    fn hit_test(&self, ray: &Vector) -> Option<RayHit>;
}

fn camera_ray(c: &Camera, aspect: f64, xt: f64, yt: f64) -> Vector {
    // Map normalized pixel coordinates [0, 1] to view-plane offsets [-1, 1].
    // The y axis is flipped so that yt=0 (top of image) corresponds to
    // +up in the camera's local frame, matching standard image orientation.
    let sx = 2.0 * xt - 1.0;
    let sy = 1.0 - 2.0 * yt;

    let half_width = c.half_height * aspect;

    // direction = forward + sx*half_width*right + sy*half_height*up
    let dir = addp(
        addp(
            c.forward,
            scalep(c.right, sx * half_width),
        ),
        scalep(c.up, sy * c.half_height),
    );

    Vector {
        start: c.location,
        delta: normalizep(dir),
    }
}

fn ray_location(ray: &Vector, t: f64) -> Point {
    let [x, y, z] = ray.start;
    let [dx, dy, dz] = ray.delta;

    [x + dx * t, y + dy * t, z + dz * t]
}

pub struct RayHit {
    pub distance: f64,
    pub hit_point: Point,
    pub normal: Point,
    pub surface: Surface,
}

impl PartialOrd for RayHit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let order = if self.distance < other.distance {
            Ordering::Greater
        } else if self.distance > other.distance {
            Ordering::Less
        } else {
            Ordering::Equal
        };

        Some(order)
    }
}

impl PartialEq for RayHit {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance
    }
}

fn light_vector(point: &Point, scene: &Scene, light: &Light) -> Option<Vector> {
    let light_direction = subp(*point, light.location);

    let light_distance = lenp(light_direction);

    let ray = Vector {
        start: light.location,
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
        }, scene, reflect_count + 1);

        scale_linear_color(&rcolor, hit.surface.reflection)
    } else {
        [0.0, 0.0, 0.0]
    };

    // Sum direct lighting contributions from every light in the scene
    // that can see this surface point. Each visible light contributes
    // a Phong specular highlight and a Lambertian diffuse term, both
    // tinted by `light.color * light.intensity`. With one white,
    // unit-intensity light this is identical to the earlier behavior;
    // with multiple lights the contributions just add. Empty `lights`
    // gives a pure ambient + reflection render, useful as a debug mode.
    let mut light: LinearColor = [0.0, 0.0, 0.0];
    for l in &scene.lights {
        if let Some(lv) = light_vector(&hit.hit_point, scene, l) {
            let kspecular = f64::powf(dotp(hit.normal, normalizep(addp(ray.delta, lv.delta))), 50.0);
            let lambert = dotp(hit.normal, negp(lv.delta));

            // Per-light tint that scales every contribution by this
            // light's color and intensity.
            let light_tint = scale_linear_color(&l.color, l.intensity);

            // Specular: highlight takes the color of the light.
            let spec_term = scale_linear_color(&light_tint, kspecular * hit.surface.specular);

            // Diffuse: surface color is modulated by light color
            // (component-wise), then scaled by the Lambert factor and
            // the surface's diffuse coefficient.
            let diff_term = scale_linear_color(
                &multiply_linear_color(&scolor, &light_tint),
                hit.surface.light * lambert,
            );

            light = add_linear_color(&light, &add_linear_color(&spec_term, &diff_term));
        }
    }

    add_linear_color(&reflected, &add_linear_color(&ambient, &light))
}

fn ray_color(ray: &Vector, scene: &Scene, reflect_count: u32) -> LinearColor {
    match nearest_hit(ray, &scene.objects) {
        Some(hit) => shade_pixel(ray, scene, &hit, reflect_count),
        None => scene.background
    }
}

fn pixel_color(
    camera: &CameraDetails,
    scene: &Scene,
    x: u32,
    y: u32,
) -> LinearColor {
    let subdx = camera.dx / (camera.oversample as f64 * 2.0);
    let subdy = camera.dy / (camera.oversample as f64 * 2.0);

    let xc = x as f64 * camera.dx - camera.dx / 2.0;
    let yc = y as f64 * camera.dy - camera.dy / 2.0;

    let mut pc = [0.0, 0.0, 0.0];
    for iix in 0..scene.oversample {
        for iiy in 0..scene.oversample {

            let xt = xc + subdx * (1 + 2 * iix) as f64;
            let yt = yc + subdy * (1 + 2 * iiy) as f64;

            let rc = ray_color(&camera_ray(&camera.camera, camera.aspect, xt, yt), scene, 0);

            pc = add_linear_color(&pc, &rc)
        }
    }

    scale_linear_color(&pc, 1.0 / (scene.oversample * scene.oversample) as f64)
}

fn render_one_row<T: RenderTarget + ?Sized>(
    target: &T,
    heatmap: Option<&dyn HeatmapTarget>,
    camera: &CameraDetails,
    scene: &Scene,
    imgx: u32,
    y: u32,
) {
    let mut row = vec![[0u8; 3]; imgx as usize];

    // Branch outside the per-pixel loop so the heatmap-disabled case
    // compiles to the same machine code as before this feature existed
    // — no Instant::now calls, no per-pixel allocation, no extra
    // bookkeeping. The hot path stays hot when the user isn't asking
    // for a heat map.
    if let Some(h) = heatmap {
        let mut timings = vec![0u32; imgx as usize];
        for x in 0..imgx {
            let start = Instant::now();
            let pc = pixel_color(camera, scene, x, y);
            // Saturate at u32::MAX nanoseconds (~4.29 s) rather than
            // wrapping silently. A pixel that takes longer than that
            // shows up as "max-bright" on the heat map, which is
            // accurate; wrapping would alias it to a small value and
            // misreport an outlier as a fast pixel.
            let elapsed_ns = start.elapsed().as_nanos();
            timings[x as usize] = u32::try_from(elapsed_ns).unwrap_or(u32::MAX);
            row[x as usize] = to_png_color(&pc);
        }
        target.submit_row(0, y, &row);
        h.submit_timing_row(0, y, &timings);
    } else {
        for x in 0..imgx {
            let pc = pixel_color(camera, scene, x, y);
            row[x as usize] = to_png_color(&pc);
        }
        target.submit_row(0, y, &row);
    }
}

/// Render `scene` at resolution `imgx`×`imgy`, pushing finished pixel rows
/// into `target`. The renderer no longer allocates an image of its own —
/// where pixels go and what becomes of them is the target's concern.
///
/// If `heatmap` is `Some(_)`, the renderer additionally measures the wall
/// time of each `pixel_color` call and submits per-pixel timings as `u32`
/// nanoseconds to the heatmap target. With `heatmap = None` there is zero
/// per-pixel overhead — no `Instant::now` calls, no extra allocation, and
/// no extra branching in the hot loop.
///
/// Under `parallel = true`, rows are computed across Rayon's thread pool;
/// `target.submit_row` will be called concurrently from multiple threads
/// (in unspecified order). The traits' `Send + Sync` bounds are what
/// make this safe.
pub fn render<T: RenderTarget + ?Sized>(
    scene: &Scene,
    imgx: u32,
    imgy: u32,
    target: &T,
    heatmap: Option<&dyn HeatmapTarget>,
    parallel: bool,
) {
    let camera = CameraDetails {
        camera: scene.camera,
        dx: 1.0 / imgx as f64,
        dy: 1.0 / imgy as f64,
        aspect: imgx as f64 / imgy as f64,
        oversample: scene.oversample,
    };

    if parallel {
        (0..imgy).into_par_iter().for_each(
            | y | render_one_row(target, heatmap, &camera, scene, imgx, y)
        );
    } else {
        (0..imgy).for_each(
            | y | render_one_row(target, heatmap, &camera, scene, imgx, y)
        );
    }

    // Signal end-of-render to the target. Default impl is a no-op;
    // ProgressTarget uses this to emit a final newline, future
    // streaming targets will use it to send a "done" message, etc.
    target.finish();
    if let Some(h) = heatmap {
        h.finish();
    }
}

