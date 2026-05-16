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
    /// Metallic flag. `false` (the default for every pre-metallic
    /// scene) is an ordinary dielectric surface. When `true`,
    /// `shade_pixel` reinterprets the existing fields the way a metal
    /// behaves: the mirror reflection and the specular highlight are
    /// both tinted component-wise by the surface `color` (a gold
    /// surface reflects gold-tinted, not chrome-white), and the
    /// Lambertian diffuse term is suppressed entirely (metals have
    /// essentially no diffuse lobe). A metallic surface is always
    /// opaque: `transparency` is ignored when `metallic` is `true`.
    /// This is the simplified "metalness" workflow — one base color
    /// drives body, reflection, and highlight. Rough/glossy metal
    /// (scattered reflections) is a deferred follow-on.
    pub metallic: bool,
}

/// Per-variant data for a light source. Phase 1 of the "Light types:
/// spotlights and area lights" plan introduces this as a one-arm enum
/// carrying no variant-specific data; subsequent phases extend it with
/// `Spot { direction, inner_angle, outer_angle }` (Phase 2) and
/// `Area { axis, radius }` (Phase 4).
///
/// The split is a separate field on `Light` rather than turning `Light`
/// itself into an enum: `location`, `color`, and `intensity` are
/// genuinely shared across every light type and are read directly in
/// `shade_pixel` (`l.color`, `l.intensity`) and `collect_lights`
/// (`l.location`). Variant dispatch lives where it actually matters —
/// `collect_lights`, which transforms per-variant fields under the
/// accumulated affine, and the shadow-ray helper `light_vector`, which
/// applies per-variant attenuation on top of the transmittance walk.
/// `shade_pixel` stays light-type-agnostic: it just consumes the
/// `(Vector, f64)` returned by `light_vector` and does Lambert + Phong
/// against the direction.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum LightKind {
    /// Omni-directional point light. Phase-1 default; the only variant
    /// that existed before this work.
    Point,
}

/// A light source. Common fields (`location`, `color`, `intensity`) sit
/// on the struct because every light kind needs them and the shading
/// code reads them directly; the `kind` field carries variant-specific
/// data. See `LightKind` for the design rationale.
///
/// The current shading model is white-implicit when
/// `color = [1.0, 1.0, 1.0]` and `intensity = 1.0`, so existing scenes
/// can be ported by wrapping their location in `Light::white`.
#[derive(Clone, PartialEq, Debug)]
pub struct Light {
    pub location: Point,
    pub color: LinearColor,
    pub intensity: f64,
    pub kind: LightKind,
}

impl Light {
    /// Full-intensity white point light at `location`. Equivalent to the
    /// implicit light parameters in earlier versions of this codebase.
    pub const fn white(location: Point) -> Light {
        Light {
            location,
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            kind: LightKind::Point,
        }
    }

    /// Point light with an explicit color and intensity.
    pub const fn point(location: Point, color: LinearColor, intensity: f64) -> Light {
        Light { location, color, intensity, kind: LightKind::Point }
    }
}

/// Look-at camera in pre-computed form.
///
/// Construct via `Camera::looking_at` (zoom-based), `Camera::with_fov`
/// (field-of-view based), or `Camera::with_dof` (adds a thin-lens
/// aperture) rather than building this struct directly — the
/// constructors derive an orthonormal basis from the user-friendly
/// inputs `(location, look_at, up_hint)` and cache the result here so
/// per-ray work is just additions and scales.
///
/// Fields:
/// - `location`     — world-space camera position.
/// - `forward`      — unit vector pointing from `location` toward the look-at point.
/// - `right`        — unit vector along the camera's right (image +x).
/// - `up`           — unit vector along the camera's up (image −y after inversion).
///                    Re-orthogonalized from the user's `up_hint`.
/// - `half_height`  — half the height of the view plane at unit distance.
///                    Smaller values = more zoomed in.
/// - `aperture_radius` — thin-lens aperture radius in world units.
///                    `0.0` is an ideal pinhole; `camera_ray` takes a
///                    dedicated branch for it that is bit-identical to
///                    the pre-depth-of-field renderer. A positive
///                    radius jitters each primary ray's origin over a
///                    disk of this radius in the `right`/`up` plane,
///                    producing depth-of-field blur. Set to `0.0` by
///                    `looking_at` / `with_fov`.
/// - `focus_distance` — distance from `location` along `forward` to
///                    the focus plane. Geometry at this depth stays
///                    sharp regardless of `aperture_radius`; nearer or
///                    farther geometry blurs. The constructors set
///                    this to the `location`→`look_at` distance —
///                    "focus on what you're aimed at." Only consulted
///                    when `aperture_radius` is nonzero.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Camera {
    pub location: Point,
    pub forward: Point,
    pub right: Point,
    pub up: Point,
    pub half_height: f64,
    pub aperture_radius: f64,
    pub focus_distance: f64,
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
        // `to_look_at` is reused twice: normalized for `forward`, and
        // its length is the default focus distance.
        let to_look_at = subp(look_at, location);
        let forward = normalizep(to_look_at);

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

        Camera {
            location,
            forward,
            right,
            up,
            half_height,
            // Pinhole by default — `camera_ray`'s zero-aperture branch
            // makes this bit-identical to the pre-DOF renderer.
            aperture_radius: 0.0,
            // Focus on the look-at point. Inert while
            // `aperture_radius` is 0; `with_dof` keeps this default
            // and only overrides the aperture.
            focus_distance: lenp(to_look_at),
        }
    }

    /// Construct a depth-of-field camera: the same look-at framing as
    /// `looking_at`, plus a thin-lens aperture of radius
    /// `aperture_radius` (world units). The focus distance is the
    /// `location`→`look_at` distance, so the look-at point is in
    /// focus and geometry nearer or farther blurs by an amount that
    /// grows with `aperture_radius`.
    ///
    /// `aperture_radius = 0.0` produces exactly the camera
    /// `looking_at` would — an ideal pinhole. (Focusing at a depth
    /// other than the look-at point is deliberately not exposed here;
    /// it's a Phase 2 ergonomics item.)
    pub fn with_dof(
        location: Point,
        look_at: Point,
        up_hint: Point,
        zoom: f64,
        aperture_radius: f64,
    ) -> Camera {
        Camera {
            aperture_radius,
            ..Camera::looking_at(location, look_at, up_hint, zoom)
        }
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

/// Generate the primary ray for a sub-pixel sample.
///
/// `xt`, `yt` are normalized view-plane coordinates in `[0, 1]`;
/// `lens` is a point on the unit disk (already mapped through
/// `sampler::concentric_disk`) selecting where on the aperture this
/// ray's origin sits. For a pinhole camera (`aperture_radius == 0.0`)
/// `lens` is ignored — `pixel_color` passes `(0.0, 0.0)` and this
/// function takes its fast path.
fn camera_ray(c: &Camera, aspect: f64, xt: f64, yt: f64, lens: (f64, f64)) -> Vector {
    // Map normalized pixel coordinates [0, 1] to view-plane offsets [-1, 1].
    // The y axis is flipped so that yt=0 (top of image) corresponds to
    // +up in the camera's local frame, matching standard image orientation.
    let sx = 2.0 * xt - 1.0;
    let sy = 1.0 - 2.0 * yt;

    let half_width = c.half_height * aspect;

    // direction = forward + sx*half_width*right + sy*half_height*up
    //
    // This is the unnormalized view-plane direction: its `forward`
    // component is exactly 1, so scaling the whole vector by a
    // distance `d` lands a point exactly `d` units along `forward`.
    let dir = addp(
        addp(
            c.forward,
            scalep(c.right, sx * half_width),
        ),
        scalep(c.up, sy * c.half_height),
    );

    // Pinhole fast path. Bit-identical to the pre-depth-of-field
    // renderer — same `normalizep(dir)`, same origin — and it skips
    // the focal-point and lens-offset arithmetic entirely, so a scene
    // with no depth of field pays nothing for the feature. Keeping
    // this an explicit branch (rather than letting the thin-lens math
    // collapse to the same result at `aperture_radius == 0`) is what
    // guarantees the byte-pinned tests stay byte-identical: scaling
    // `dir` by `focus_distance` and renormalizing is not bitwise the
    // same as renormalizing `dir` directly.
    if c.aperture_radius == 0.0 {
        return Vector {
            start: c.location,
            delta: normalizep(dir),
        };
    }

    // Thin-lens path. The focal point is where the pinhole ray would
    // cross the focus plane: since `dir`'s forward component is 1,
    // scaling it by `focus_distance` puts the point exactly
    // `focus_distance` along `forward`.
    let focal_point = addp(c.location, scalep(dir, c.focus_distance));

    // Jitter the ray origin over the aperture disk, in the camera's
    // right/up plane. `lens` is already a unit-disk point, so scaling
    // by `aperture_radius` gives the world-space offset directly.
    let (lu, lv) = lens;
    let origin = addp(
        c.location,
        addp(
            scalep(c.right, lu * c.aperture_radius),
            scalep(c.up, lv * c.aperture_radius),
        ),
    );

    // Every ray for this sub-pixel sample aims at the same focal
    // point, so geometry at the focus plane converges (stays sharp)
    // while nearer/farther geometry spreads across the aperture
    // (blurs).
    Vector {
        start: origin,
        delta: normalizep(subp(focal_point, origin)),
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

/// Shadow-ray test from `light` to `point`, returning the light's
/// direction-of-travel ray together with the *transmittance* along it
/// — the fraction of the light that survives the trip — for the
/// caller (`shade_pixel`) to scale this light's contribution by.
///
/// Phase 1 of the "Light types: spotlights and area lights" plan
/// makes this a dispatch function on `light.kind`. The shared
/// transmittance walk (from Phase 2 of the transparency plan) lives
/// in `light_vector_point`; Phase 2 of the light-types plan will add
/// `light_vector_spot` next to it, which will reuse the same
/// transmittance walk and fold a smoothstep cone-falloff factor into
/// the returned scalar. `shade_pixel` stays light-type-agnostic — it
/// just multiplies its Lambert + Phong contribution by whatever
/// scalar comes back here.
///
/// Returns:
/// - `None` when the light contributes nothing (e.g. transmittance
///   reached 0 — fully shadowed). The caller can skip this light's
///   shading entirely.
/// - `Some((ray, transmittance))` otherwise, with `transmittance` in
///   `(0.0, 1.0]`. `1.0` means nothing transparent was in the way;
///   the caller scales this light's full contribution by the
///   returned factor.
///
/// Transmittance is a scalar, not a per-channel color: the light is
/// attenuated greyscale, not tinted by the occluder's body color.
/// This keeps Phase 2 of the transparency plan consistent with
/// Phase 1's primary-ray transmission, which is likewise untinted
/// (`lerp(opaque, transmitted, transparency)` in `shade_pixel`).
/// Colored shadows — red glass casting a red-tinted shadow — are
/// deferred to land alongside colored transmission, most naturally
/// with the refraction work.
fn light_vector(point: &Point, scene: &Scene, light: &Light) -> Option<(Vector, f64)> {
    match light.kind {
        LightKind::Point => light_vector_point(point, scene, light),
    }
}

/// Shadow-ray test for a point light: the transmittance walk from the
/// light's `location` toward `point`, with every occluder strictly
/// between them multiplying the running transmittance by its surface
/// `transparency` (0.0 = opaque, 1.0 = fully clear). An opaque
/// occluder drives transmittance to 0 and the walk stops early;
/// transparent occluders attenuate and the walk continues to the next
/// hit.
fn light_vector_point(point: &Point, scene: &Scene, light: &Light) -> Option<(Vector, f64)> {
    let light_direction = subp(*point, light.location);

    let light_distance = lenp(light_direction);

    let ray = Vector {
        start: light.location,
        delta: normalizep(light_direction)
    };

    // Walk the shadow ray from the light toward the shaded point.
    // `cursor` is the current origin for the next segment test; it
    // advances to each occluder's hit point in turn. Because every
    // primitive's `hit_test` rejects `t <= EPSILON`, restarting the
    // test from a hit point never re-finds that same surface — the
    // same self-intersection guard the reflection and transmission
    // rays rely on.
    let mut transmittance = 1.0;
    let mut cursor = ray.start;

    loop {
        let segment = Vector { start: cursor, delta: ray.delta };

        match scene.root.hit_test(&segment) {
            Some(hit) => {
                // Distance of this hit measured from the light along
                // the (unit-length) ray direction. Every hit point
                // lies on the original ray line, so this is just the
                // length from the light's location.
                let dist_from_light = lenp(subp(hit.hit_point, ray.start));

                // A hit at (or beyond) the shaded point itself is not
                // an occluder — it is the surface we are lighting.
                // Stop the walk; whatever transmittance we have is
                // the answer.
                if dist_from_light > light_distance - EPSILON {
                    break;
                }

                // A genuine occluder strictly between light and
                // point. Attenuate by its transparency. An opaque
                // surface (transparency 0.0) zeroes transmittance and
                // we can stop immediately.
                transmittance *= hit.surface.transparency;
                if transmittance <= EPSILON {
                    return None;
                }

                // Advance past this occluder and continue the walk.
                cursor = hit.hit_point;
            }
            // The shadow ray hit nothing further along — no more
            // occluders between here and the light's reach. Done.
            None => break,
        }
    }

    if transmittance <= EPSILON {
        None
    } else {
        Some((ray, transmittance))
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

        let scaled = scale_linear_color(&rcolor, hit.surface.reflection);

        // A metal tints what it reflects by its own color (gold
        // reflects gold-ish); a dielectric reflects untinted, like
        // chrome. The tint uses `scolor` so a checked metal's
        // reflection picks up the checker pattern, consistent with
        // the ambient and diffuse terms below.
        if hit.surface.metallic {
            multiply_linear_color(&scaled, &scolor)
        } else {
            scaled
        }
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
        if let Some((lv, transmittance)) = light_vector(&hit.hit_point, scene, l) {
            let kspecular = f64::powf(dotp(hit.normal, normalizep(addp(ray.delta, lv.delta))), 50.0);
            let lambert = dotp(hit.normal, negp(lv.delta));

            // Per-light tint that scales every contribution by this
            // light's color and intensity.
            let light_tint = scale_linear_color(&l.color, l.intensity);

            // Specular: the highlight takes the color of the light
            // for a dielectric (a white light makes a white highlight
            // on red plastic). A metal additionally tints the
            // highlight by its own color — a gold surface has gold
            // highlights regardless of the light.
            let spec_term = {
                let s = scale_linear_color(&light_tint, kspecular * hit.surface.specular);
                if hit.surface.metallic {
                    multiply_linear_color(&s, &scolor)
                } else {
                    s
                }
            };

            // Diffuse: surface color is modulated by light color
            // (component-wise), then scaled by the Lambert factor and
            // the surface's diffuse coefficient. Metals have
            // essentially no diffuse lobe — all their apparent color
            // comes from the tinted reflection and specular terms —
            // so the diffuse contribution is suppressed entirely for
            // a metallic surface.
            let diff_term = if hit.surface.metallic {
                [0.0, 0.0, 0.0]
            } else {
                scale_linear_color(
                    &multiply_linear_color(&scolor, &light_tint),
                    hit.surface.light * lambert,
                )
            };

            // Scale this light's full contribution by the shadow-ray
            // transmittance: `1.0` for an unobstructed light (the
            // pre-Phase-2 behavior), between 0 and 1 when transparent
            // occluders sit between the point and the light. Opaque
            // occluders never reach here — `light_vector` returns
            // `None` for those.
            let contribution = scale_linear_color(
                &add_linear_color(&spec_term, &diff_term),
                transmittance,
            );

            light = add_linear_color(&light, &contribution);
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
    //
    // A metallic surface is always opaque: `transparency` is ignored
    // when `metallic` is set, so the `!metallic` guard short-circuits
    // the transmitted ray entirely and `opaque` is returned as-is.
    if (hit.surface.transparency > EPSILON) && !hit.surface.metallic && (depth.transmit < scene.transmit_limit) {
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

    // Depth-of-field lens sampling. Only meaningful when the camera
    // has a nonzero aperture; for a pinhole camera we skip the
    // per-sample lens-coordinate work entirely and `camera_ray` takes
    // its bit-identical pinhole branch. This mirrors the `want_time`
    // pattern in `render_one_row` — branch on the feature flag once,
    // outside the hot loop, so the disabled case costs nothing. The
    // lens sample gets its own Cranley-Patterson rotation (distinct
    // hash seed) so the aperture coordinate is decorrelated from the
    // sub-pixel coordinate of the same sample index.
    let dof = camera.camera.aperture_radius != 0.0;
    let (lox, loy) = if dof {
        sampler::cranley_patterson_lens_offset(x, y)
    } else {
        (0.0, 0.0)
    };

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

            // Lens sample for depth of field. When `dof` is false
            // this stays `(0.0, 0.0)` and `camera_ray` ignores it via
            // the pinhole branch — no `halton_lens` / `concentric_disk`
            // calls on the pinhole path. When `dof` is true, the lens
            // Halton point gets the lens CP rotation, then the
            // concentric mapping turns it into a unit-disk aperture
            // offset.
            let lens = if dof {
                let (lhx, lhy) = sampler::halton_lens(i + 1);
                sampler::concentric_disk(
                    (lhx + lox).fract(),
                    (lhy + loy).fract(),
                )
            } else {
                (0.0, 0.0)
            };

            let rc = ray_color(
                &camera_ray(&camera.camera, camera.aspect, xt, yt, lens),
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

