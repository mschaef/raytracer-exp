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
pub mod sampler;

use std::convert::TryFrom;
use std::time::Instant;

use shapes::Shape;
use output::{RenderTarget, HeatmapTarget};
use transform::Affine;

use rayon::prelude::*;

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
};

use std::cmp::Ordering;

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Surface {
    pub color: LinearColor,
    pub ambient: f64,
    pub specular: f64,
    pub light: f64,
    pub checked: bool,
    pub reflection: f64,
    /// Transmission coefficient in `[0.0, 1.0]`. `0.0` is fully
    /// opaque (the default for every pre-transparency scene); `1.0`
    /// is fully see-through. `shade_pixel` blends the surface's own
    /// opaque shading with the color seen *through* the surface as
    /// `lerp(opaque, transmitted, transparency)`.
    ///
    /// Phase 1 transmission is *non-refractive*: the transmitted ray
    /// continues in the incoming direction without bending. Refraction
    /// (Snell's law, per-surface IOR) is deferred to a later phase.
    /// Note also that Phase 1 shadow rays do not yet honor
    /// transparency — a transparent object still casts a solid
    /// shadow; that's Phase 2.
    pub transparency: f64,
}

/// A point light source. Carries an emitted color and a scalar intensity
/// so that scenes can use multiple visually distinct lights for testing
/// and artistic control. The current shading model is white-implicit
/// when `color = [1.0, 1.0, 1.0]` and `intensity = 1.0`, so existing
/// scenes can be ported by wrapping their location in `Light::white`.
#[derive(Clone, PartialEq, Debug)]
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
#[derive(Copy, Clone, PartialEq, Debug)]
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
}

#[derive(Clone, PartialEq, Debug)]
pub struct Scene {
    /// Human-readable name used in progress output and debugging.
    /// `String` rather than `&'static str` so script-built scenes (from
    /// the SDL) can carry runtime-derived names; static-string scene
    /// literals just use `.to_string()` at construction.
    pub name: String,
    pub camera: Camera,
    /// The whole scene is a single top-level `Shape`. Typically a
    /// `Shape::Group` containing the geometry and lights at the top
    /// level, but the renderer doesn't care about the shape — it just
    /// dispatches `hit_test` and `collect_lights` against it. Lights
    /// live inside this tree as `Shape::Light` nodes alongside
    /// geometry, with the SDL constructor auto-wrapping `Value::Light`
    /// values that appear in `:objects`.
    ///
    /// Stage 2 of the lights-as-shapes migration consolidated this:
    /// the previous `lights: Vec<Light>` + `objects: Vec<Shape>` pair
    /// became one field. Existing scenes were rewritten to put their
    /// lights in the `:objects` list (which the SDL now exposes as
    /// the canonical place for everything in the scene graph).
    pub root: Shape,
    pub background: LinearColor,

    pub reflect_limit: u32,

    /// Maximum transmission recursion depth for transparent surfaces.
    /// A primary ray that passes through a transparent surface spawns
    /// a transmitted ray; that ray can hit another transparent
    /// surface and spawn another, and so on. `transmit_limit` caps
    /// that chain — at the cap a transparent surface renders as if
    /// it were opaque.
    ///
    /// This is a *separate* budget from `reflect_limit` (tracked by
    /// the independent `transmit` counter in `Depth`) because the two
    /// kinds of recursion have different natural depths: a ray
    /// passing through N stacked transparent panes legitimately needs
    /// N transmission levels, whereas mirror bounces rarely need more
    /// than a handful. The SDL default is 8.
    pub transmit_limit: u32,

    /// Adaptive oversampling parameters. The per-pixel sample loop in
    /// `pixel_color` takes at least `min_samples` samples, then keeps
    /// going batch-by-batch while the per-channel min/max spread
    /// exceeds `variance_threshold`, up to a cap of `max_samples`.
    /// A pixel sitting on a flat surface usually terminates at
    /// `min_samples`; pixels on a geometric edge or in a high-contrast
    /// region keep sampling until they either stabilize or hit the cap.
    ///
    /// Phase 2 of the adaptive-oversampling plan replaced the previous
    /// single `oversample: u32` field. The old "fixed N×N grid" model
    /// is gone — sample positions come from the Halton sampler in
    /// `render::sampler`, and total count per pixel is variable.
    ///
    /// Sample positions: see `pixel_color` / `render::sampler`.
    /// The convention is that `min_samples == max_samples` reproduces
    /// the previous "fixed sample count, take exactly N samples and
    /// move on" behavior, which is useful for benchmarking and for
    /// byte-pinning equivalence tests that want determinism.
    pub min_samples: u32,
    pub max_samples: u32,
    /// Per-channel min/max spread threshold for early termination.
    /// Linear-color units, so `0.005` ≈ "half a percent of full
    /// channel range." Lower → more samples in noisy regions
    /// (better quality, slower); higher → cheaper, more apparent
    /// noise.
    pub variance_threshold: f64,
}

/// Optional diagnostic heatmap targets that `render()` populates
/// alongside the main pixel target. Each slot is independent: pass
/// `None` for the ones you don't want, and the renderer skips the
/// per-pixel bookkeeping for that metric entirely (no `Instant::now`
/// calls when `time` is `None`, no sample-count counter writes when
/// `samples` is `None`). `HeatmapTargets::default()` is "neither" —
/// equivalent to the old `heatmap: None` parameter, and what call
/// sites that don't care about diagnostics should use.
///
/// Each heatmap is a separate `&dyn HeatmapTarget` so they can be
/// different storage choices if a caller wants — e.g. an in-memory
/// PNG buffer for time but a streaming target for sample count, or
/// the same `PngHeatmapTarget` for both (though sharing a single
/// target between the two slots would conflate the metrics in one
/// buffer and isn't usually what you want).
#[derive(Default, Clone, Copy)]
pub struct HeatmapTargets<'a> {
    pub time: Option<&'a dyn HeatmapTarget>,
    pub samples: Option<&'a dyn HeatmapTarget>,
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

    match scene.root.hit_test(&ray) {
        Some(hit) =>
            if hit.distance > light_distance - EPSILON {
                Some(ray)
            } else {
                None
            }
        None => None
    }
}

/// Recursion-budget tracker threaded through `ray_color` /
/// `shade_pixel`. Reflection and transmission carry *independent*
/// depth counters, checked against `Scene::reflect_limit` and
/// `Scene::transmit_limit` respectively — see the doc comment on
/// `Scene::transmit_limit` for why the two budgets are kept separate.
///
/// `Copy` (two `u32`s), so it threads through the recursion by value
/// with no ceremony; `..depth` struct-update syntax bumps one counter
/// while carrying the other through unchanged.
#[derive(Copy, Clone, Debug)]
struct Depth {
    reflect: u32,
    transmit: u32,
}

impl Depth {
    /// The starting budget for a primary (camera) ray: no reflection
    /// or transmission bounces spent yet.
    fn zero() -> Depth {
        Depth { reflect: 0, transmit: 0 }
    }
}

fn shade_pixel(ray: &Vector, scene: &Scene, lights: &[Light], hit: &RayHit, depth: Depth) -> LinearColor {
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

    let reflected: LinearColor = if (hit.surface.reflection > EPSILON) && (depth.reflect < scene.reflect_limit) {
        let rvec = subp(negp(ray.delta), scalep(hit.normal, 2.0 * dotp(negp(ray.delta), hit.normal)));

        let rcolor = ray_color(&Vector {
            start: hit.hit_point,
            delta: normalizep(rvec)
        }, scene, lights, Depth { reflect: depth.reflect + 1, ..depth });

        scale_linear_color(&rcolor, hit.surface.reflection)
    } else {
        [0.0, 0.0, 0.0]
    };

    // Sum direct lighting contributions from every light extracted
    // from `scene.root`. Each visible light contributes a Phong
    // specular highlight and a Lambertian diffuse term, both tinted
    // by `light.color * light.intensity`. With one white,
    // unit-intensity light this is identical to the earlier behavior;
    // with multiple lights the contributions just add. An empty list
    // gives a pure ambient + reflection render, useful as a debug
    // mode.
    let mut light: LinearColor = [0.0, 0.0, 0.0];
    for l in lights {
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

    // The surface's opaque shading: ambient + direct lighting +
    // mirror reflection. For an opaque surface (transparency == 0)
    // this is the final color.
    let opaque = add_linear_color(&reflected, &add_linear_color(&ambient, &light));

    // Transmission. When the surface is at all transparent (and we
    // haven't hit the recursion cap), cast a *straight-through* ray —
    // same direction as the incoming ray, originating at the hit
    // point — and blend its color in by the transparency coefficient.
    //
    // Phase 1 is deliberately non-refractive: the transmitted ray
    // doesn't bend, so a solid transparent sphere shows the geometry
    // behind it undistorted, plus its own back surface. Refraction is
    // a later phase.
    //
    // No epsilon offset is needed on the transmitted ray's start
    // point: every primitive's `hit_test` already rejects `t <=
    // EPSILON`, so the surface we're leaving is discarded and the ray
    // continues to the next surface — exactly the same self-
    // intersection guard the reflection ray above relies on.
    //
    // At the recursion cap (`depth.transmit >= scene.transmit_limit`)
    // a transparent surface falls back to rendering fully opaque,
    // which is a graceful, bounded degradation.
    if (hit.surface.transparency > EPSILON) && (depth.transmit < scene.transmit_limit) {
        let transmitted = ray_color(
            &Vector {
                start: hit.hit_point,
                delta: ray.delta,
            },
            scene,
            lights,
            Depth { transmit: depth.transmit + 1, ..depth },
        );

        // lerp(opaque, transmitted, transparency)
        let t = hit.surface.transparency;
        add_linear_color(
            &scale_linear_color(&opaque, 1.0 - t),
            &scale_linear_color(&transmitted, t),
        )
    } else {
        opaque
    }
}

fn ray_color(ray: &Vector, scene: &Scene, lights: &[Light], depth: Depth) -> LinearColor {
    match scene.root.hit_test(ray) {
        Some(hit) => shade_pixel(ray, scene, lights, &hit, depth),
        None => scene.background
    }
}

/// Batch size for the adaptive sample loop. The variance check runs
/// once per batch rather than per individual sample, so the check's
/// cost is amortized; 4 also happens to match the previous fixed
/// `oversample = 2` case exactly, which keeps the "starting" point
/// of the new loop intuitive ("two samples deep in each axis").
const SAMPLE_BATCH: u32 = 4;

/// Compute one pixel's color via the adaptive sampling loop, and
/// return the sample count alongside it.
///
/// The sample count is what the Phase 3 sample-count heatmap renders.
/// We return it always (rather than gating on whether a heatmap is
/// active) — it's a single `u32` per pixel and the renderer's caller
/// is free to ignore it. Threading "maybe collect the count" through
/// here as a flag or builder would be more bookkeeping than just
/// always returning it.
fn pixel_color(
    camera: &CameraDetails,
    scene: &Scene,
    lights: &[Light],
    x: u32,
    y: u32,
) -> (LinearColor, u32) {
    // Phase 2 of the adaptive-oversampling plan: sample count per
    // pixel is variable, driven by a per-channel min/max spread
    // check. The loop takes at least `min_samples` samples, then
    // keeps going one batch at a time while max channel spread
    // exceeds `variance_threshold`, up to a cap of `max_samples`.
    //
    // Sample positions still come from the Phase 1 Halton-(2, 3)
    // sampler in `render::sampler`, with per-pixel Cranley-Patterson
    // rotation to decorrelate neighbors. The Halton sequence stays
    // well-distributed at arbitrary index, which is what makes the
    // open-ended sample count work without quality cliffs as
    // `samples` grows.
    let (ox, oy) = sampler::cranley_patterson_offset(x, y);

    // Min/max spread metric: track per-channel min and max across
    // all samples taken so far. After each batch, the per-channel
    // spread is `max - min`; the loop terminates when the largest
    // channel's spread drops below `variance_threshold`. This is
    // simpler and cheaper than computing statistical variance, and
    // the threshold is intuitive — "any channel allowed to differ
    // by this much across samples." A scene's flat regions usually
    // terminate at `min_samples`; edges and high-contrast areas
    // sample further.
    let mut sum: LinearColor = [0.0, 0.0, 0.0];
    let mut min_c: LinearColor = [f64::INFINITY; 3];
    let mut max_c: LinearColor = [f64::NEG_INFINITY; 3];
    let mut samples: u32 = 0;

    // Sample-count contract for the adaptive loop:
    //
    // * At least `min_samples` samples are always taken (even if
    //   the early ones already agree to within the threshold —
    //   variance is a *noisy* estimate at very low sample counts,
    //   and the floor protects against terminating on spurious
    //   agreement).
    //
    // * After that, additional batches of `SAMPLE_BATCH` are taken
    //   whenever max channel spread is above the threshold, up to
    //   a hard cap of `max_samples`.
    //
    // * `min_samples == max_samples` reproduces the previous
    //   fixed-count behavior, which is what the byte-pinned tests
    //   in `tests/sdl_suite.rs` rely on for determinism.
    let min_samples = scene.min_samples.max(1);
    let max_samples = scene.max_samples.max(min_samples);
    let threshold = scene.variance_threshold;

    loop {
        // Batch size: normally `SAMPLE_BATCH`, capped at the remaining
        // budget so a `max_samples` smaller than the batch (e.g. the
        // byte-pinned tests using `min == max == 1` for determinism)
        // takes exactly the requested count, not a batch-rounded count.
        let batch = SAMPLE_BATCH.min(max_samples - samples);

        // Take this batch's worth of samples. Each sample's sub-pixel
        // offset comes from `(halton_pair(i + 1) + (ox, oy)) mod 1` —
        // Halton index 0 sits at the pixel corner so we shift past it.
        for k in 0..batch {
            let i = samples + k;
            let (hx, hy) = sampler::halton_pair(i + 1);
            let sx = (hx + ox).fract();
            let sy = (hy + oy).fract();

            // Pixel-center convention is "pixel x is centered at
            // view-plane coordinate `x * dx`" (see camera_ray, and
            // the image-y note in CLAUDE.md). Sample at fractional
            // offset `s` in [0, 1) lands at view-plane coordinate
            // `(x + s - 0.5) * dx`.
            let xt = (x as f64 + sx - 0.5) * camera.dx;
            let yt = (y as f64 + sy - 0.5) * camera.dy;

            let rc = ray_color(
                &camera_ray(&camera.camera, camera.aspect, xt, yt),
                scene,
                lights,
                Depth::zero(),
            );

            sum = add_linear_color(&sum, &rc);
            for c in 0..3 {
                if rc[c] < min_c[c] {
                    min_c[c] = rc[c];
                }
                if rc[c] > max_c[c] {
                    max_c[c] = rc[c];
                }
            }
        }
        samples += batch;

        // Hit the cap → done regardless of variance.
        if samples >= max_samples {
            break;
        }

        // Below the min floor → keep going regardless of variance.
        if samples < min_samples {
            continue;
        }

        // Past the floor and under the cap → check spread.
        let spread = (max_c[0] - min_c[0])
            .max(max_c[1] - min_c[1])
            .max(max_c[2] - min_c[2]);
        if spread <= threshold {
            break;
        }
    }

    (scale_linear_color(&sum, 1.0 / samples as f64), samples)
}

fn render_one_row<T: RenderTarget + ?Sized>(
    target: &T,
    heatmaps: HeatmapTargets<'_>,
    camera: &CameraDetails,
    scene: &Scene,
    lights: &[Light],
    imgx: u32,
    y: u32,
) {
    let mut row = vec![[0.0f64; 3]; imgx as usize];

    // Branch outside the per-pixel loop so the heatmap-disabled case
    // compiles to nearly the same machine code as before this feature
    // existed — no `Instant::now` calls when time isn't requested, no
    // per-pixel sample-count writes when that heatmap isn't requested,
    // no allocation for either. The hot path stays hot when the
    // caller isn't asking for diagnostics.
    //
    // Allocating the per-metric buffers conditionally keeps the
    // common "both heatmaps requested" path one Vec per metric per
    // row (same cost characteristics as before, when the single
    // timing buffer was conditional). "Neither heatmap" stays
    // allocation-free for the metric buffers; just the pixel-color
    // row remains.
    let want_time = heatmaps.time.is_some();
    let want_samples = heatmaps.samples.is_some();

    let mut timings: Vec<u32> = if want_time { vec![0u32; imgx as usize] } else { Vec::new() };
    let mut sample_counts: Vec<u32> =
        if want_samples { vec![0u32; imgx as usize] } else { Vec::new() };

    for x in 0..imgx {
        // Timer is started conditionally: when `want_time` is false,
        // there's no `Instant::now` call at all, which is what made
        // the original "heatmap disabled" path compile to the same
        // code as the pre-feature renderer. We preserve that property
        // for the time metric.
        let start_ns = if want_time { Some(Instant::now()) } else { None };

        let (pc, samples) = pixel_color(camera, scene, lights, x, y);
        row[x as usize] = pc;

        if let Some(start) = start_ns {
            // Saturate at u32::MAX nanoseconds (~4.29 s) rather than
            // wrapping silently. A pixel that takes longer than that
            // shows up as "max-bright" on the heat map, which is
            // accurate; wrapping would alias it to a small value and
            // misreport an outlier as a fast pixel.
            let elapsed_ns = start.elapsed().as_nanos();
            timings[x as usize] = u32::try_from(elapsed_ns).unwrap_or(u32::MAX);
        }
        if want_samples {
            sample_counts[x as usize] = samples;
        }
    }

    target.submit_row(0, y, &row);
    if let Some(h) = heatmaps.time {
        h.submit_metric_row(0, y, &timings);
    }
    if let Some(h) = heatmaps.samples {
        h.submit_metric_row(0, y, &sample_counts);
    }
}

/// Render `scene` at resolution `imgx`×`imgy`, pushing finished pixel rows
/// into `target`. The renderer no longer allocates an image of its own —
/// where pixels go and what becomes of them is the target's concern.
///
/// `heatmaps` selects which (if any) diagnostic per-pixel metrics get
/// collected and submitted alongside the main render. The `time` slot
/// is per-pixel wall time in nanoseconds; the `samples` slot is the
/// per-pixel adaptive sample count. `HeatmapTargets::default()` is
/// the zero-overhead "neither" case — no `Instant::now` calls when
/// time is `None`, no per-pixel sample-count writes when samples is
/// `None`, and no allocation for either metric buffer. With both
/// requested, the cost is one Vec<u32> per metric per row, dwarfed
/// by ray-tracing cost.
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
    heatmaps: HeatmapTargets<'_>,
    parallel: bool,
) {
    let camera = CameraDetails {
        camera: scene.camera,
        dx: 1.0 / imgx as f64,
        dy: 1.0 / imgy as f64,
        aspect: imgx as f64 / imgy as f64,
    };

    // Build the flat world-space list of lights once at render entry
    // by walking `scene.root`. Every `Shape::Light` leaf in the tree
    // contributes a world-space `Light`, with the affines of any
    // enclosing `Shape::Transform` nodes accumulated on the way down.
    // After the stage-2 collapse the scene is a single top-level
    // `Shape`; the renderer no longer carries any distinction between
    // "top-level lights" and "lights inside the object tree."
    let mut effective_lights: Vec<Light> = Vec::new();
    scene.root.collect_lights(Affine::identity(), &mut effective_lights);
    let lights = effective_lights.as_slice();

    if parallel {
        (0..imgy).into_par_iter().for_each(
            | y | render_one_row(target, heatmaps, &camera, scene, lights, imgx, y)
        );
    } else {
        (0..imgy).for_each(
            | y | render_one_row(target, heatmaps, &camera, scene, lights, imgx, y)
        );
    }

    // Signal end-of-render to every active output target. Default
    // impl on each trait is a no-op; ProgressTarget emits a final
    // newline, future streaming targets will send a "done" message,
    // and so on.
    target.finish();
    if let Some(h) = heatmaps.time {
        h.finish();
    }
    if let Some(h) = heatmaps.samples {
        h.finish();
    }
}

