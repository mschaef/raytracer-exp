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
pub mod poly;
pub mod noise;
pub mod pigment;

use std::cell::Cell;
use std::convert::TryFrom;
use std::time::Instant;

use shapes::Shape;
use output::{RenderTarget, HeatmapTarget};
use transform::Affine;
use pigment::Pigment;

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
    /// A procedural pigment. When set, it replaces `color` (and the
    /// `checked` pattern) as the surface colour, evaluated at the hit's
    /// texture point. Pigments are built once, when a scene is
    /// constructed, and leaked to get a `'static` reference, which keeps
    /// `Surface` small and `Copy`; the leak is bounded by the number of
    /// pigmented surfaces a script creates.
    pub pigment: Option<&'static Pigment>,
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
    /// Directed spotlight. `direction` is the cone axis as a unit
    /// vector pointing *away* from the light's `location` (i.e. the
    /// direction the light shines). `inner_angle` and `outer_angle`
    /// are half-angles in radians measured from `direction`:
    ///
    /// - Inside the inner cone (angle ≤ `inner_angle`): full intensity.
    /// - Outside the outer cone (angle ≥ `outer_angle`): zero (the
    ///   light contributes nothing to that point).
    /// - In the transition band: a smoothstep falloff between the two.
    ///
    /// Stored as angles rather than cosines for debuggability — the
    /// cone-falloff helper derives `cos(inner_angle)` / `cos(outer_angle)`
    /// at the dot-product comparison site. `inner_angle ≤ outer_angle`
    /// is a precondition the SDL binding enforces; constructed lights
    /// pass that constraint through without re-checking.
    Spot {
        direction: Point,
        inner_angle: f64,
        outer_angle: f64,
    },
    /// Disk-shaped area emitter centered at the light's `location`.
    /// `axis` is the disk's normal (unit vector) and the direction
    /// the light emits along (i.e. the front face of the disk is the
    /// `+axis` side). `radius` is the disk's radius in world units.
    ///
    /// Phase 4 of the "Light types: spotlights and area lights" plan
    /// renders this as a *hard-shadowed* light: the shadow ray is
    /// cast from the disk center exactly, and the light's
    /// contribution is scaled by a Lambertian cosine factor
    /// `max(0, dot(axis, light→point))`. The half-space behind the
    /// disk (where the dot product is non-positive) receives no
    /// direct light from this source — same physics as a real disk
    /// emitter only being visible from its front side. Phase 5 will
    /// replace the center-only sampling with per-pixel-sample
    /// jittered shadow rays across the disk for soft shadows; the
    /// cosine attenuation and `axis` transform stay unchanged.
    ///
    /// `radius` is the only field unused by Phase 4 itself — it's
    /// stored for Phase 5's sampler to pick a point on the disk.
    /// Threading it through the renderer as `_radius` in the
    /// Phase-4 helper keeps the function signature stable across
    /// phases.
    Area {
        axis: Point,
        radius: f64,
        /// Optional spot cone, measured from the disk's centre: a disk
        /// light that is also a spotlight (POV-Ray's `spotlight` with
        /// `area_light`). `None` is the plain disk.
        cone: Option<SpotCone>,
    },
    /// Parallelogram area emitter centred at `location`, spanned by the
    /// edge vectors `u` and `v` (full edge lengths, as POV-Ray's
    /// `area_light <u>, <v>, ...`). Soft shadows come from the same
    /// per-pixel-sample light coordinate as the disk.
    ///
    /// Unlike the disk, a quad emits equally in every direction, from
    /// both faces, with no Lambertian cosine factor: that's how POV-Ray
    /// treats area lights (a point light spread over a rectangle). Give
    /// it a `cone` to aim it, as POV does with `spotlight`.
    Quad {
        u: Point,
        v: Point,
        cone: Option<SpotCone>,
    },
}

/// A spotlight's cone: `direction` (unit, pointing the way the light
/// shines) and half-angles in radians. Inside `inner_angle` the light
/// is at full strength, beyond `outer_angle` it contributes nothing,
/// and between them it falls off with a smoothstep. Used by area
/// lights that are also spotlights; `LightKind::Spot` carries the same
/// three fields inline and shares `SpotCone::falloff`.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct SpotCone {
    pub direction: Point,
    pub inner_angle: f64,
    pub outer_angle: f64,
}

impl SpotCone {
    /// The cone's strength, in `[0, 1]`, toward a point in the unit
    /// direction `light_to_point_unit` from the light.
    pub fn falloff(&self, light_to_point_unit: Point) -> f64 {
        let cos_theta = dotp(light_to_point_unit, self.direction);
        let cos_inner = self.inner_angle.cos();
        let cos_outer = self.outer_angle.cos();
        // Hermite-cubic smoothstep with explicit clamping at both edges.
        // The first arm handles "outside the outer cone"; the second
        // handles "inside the inner cone"; the third is the transition
        // band. Splitting it this way also avoids a 0/0 when
        // `inner_angle == outer_angle` (the denominator vanishes but
        // every input has already matched one of the clamp arms).
        if cos_theta <= cos_outer {
            0.0
        } else if cos_theta >= cos_inner {
            1.0
        } else {
            let t = (cos_theta - cos_outer) / (cos_inner - cos_outer);
            t * t * (3.0 - 2.0 * t)
        }
    }
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
    /// A shadowless light illuminates every point it faces, ignoring
    /// anything in between (POV-Ray's `shadowless`): no shadow ray is
    /// cast. Typically a fill light. `false` for every constructor
    /// below.
    pub shadowless: bool,
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
            shadowless: false,
        }
    }

    /// Point light with an explicit color and intensity.
    pub const fn point(location: Point, color: LinearColor, intensity: f64) -> Light {
        Light { location, color, intensity, kind: LightKind::Point, shadowless: false }
    }

    /// Spotlight at `location` aimed along `direction`, with cone
    /// half-angles `inner_angle` (full intensity) and `outer_angle`
    /// (cutoff) in radians measured from the axis.
    ///
    /// `direction` is the caller's responsibility to supply
    /// unit-length — the cone-falloff helper assumes it. Likewise the
    /// caller is responsible for `inner_angle ≤ outer_angle`. The SDL
    /// `(light-spot ...)` binding normalizes the direction and
    /// validates the angle ordering at the script boundary, which is
    /// the intended construction path; direct Rust callers either
    /// match that contract or accept the consequences (a non-unit
    /// `direction` makes the cone falloff non-physical; reversed
    /// angles collapse the smoothstep band to zero width, which is
    /// mathematically defined — both edges return 0 — but means the
    /// inner cone has a hard rather than soft edge).
    pub const fn spot(
        location: Point,
        direction: Point,
        color: LinearColor,
        intensity: f64,
        inner_angle: f64,
        outer_angle: f64,
    ) -> Light {
        Light {
            location,
            color,
            intensity,
            kind: LightKind::Spot { direction, inner_angle, outer_angle },
            shadowless: false,
        }
    }

    /// Disk area light centered at `location` with normal `axis` and
    /// `radius`. `axis` is the caller's responsibility to supply
    /// unit-length and is the direction the disk emits (light shines
    /// in `+axis`; the back side is dark). `radius > 0` is the
    /// caller's responsibility too.
    ///
    /// The SDL `(light-area ...)` binding normalizes `axis` and
    /// validates `radius > 0` at the script boundary; direct Rust
    /// callers either match that contract or accept the
    /// consequences (a non-unit `axis` makes the cosine attenuation
    /// non-physical; a zero or negative radius is meaningless to
    /// Phase 5's disk sampler).
    pub const fn area(
        location: Point,
        axis: Point,
        radius: f64,
        color: LinearColor,
        intensity: f64,
    ) -> Light {
        Light {
            location,
            color,
            intensity,
            kind: LightKind::Area { axis, radius, cone: None },
            shadowless: false,
        }
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

/// Render-view diagnostic selector. Picks which component of
/// `shade_pixel`'s output the renderer returns; the rest are
/// computed and thrown away. Used for understanding what each
/// shading term contributes to the final image — especially
/// useful for verifying path-traced indirect lighting visually
/// ("is GI actually doing anything?") and for isolating
/// transparency / reflection / direct lighting bugs.
///
/// `Full` (the default) means "everything," and reproduces the
/// renderer's normal output. The byte-pinned tests in
/// `tests/sdl_suite.rs` rely on this default — SDL-constructed
/// scenes always carry `ViewMode::Full` unless main.rs overrides
/// from the `RAYTRACER_VIEW` environment variable, which the
/// tests don't set.
///
/// Phase 4 of the path-tracing plan. Not exposed in the SDL: this
/// is a render-time diagnostic knob, not a scene-authoring
/// concern (the SDL is the canonical "what does this scene look
/// like?" definition; switching views from inside a script would
/// muddy that).
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub enum ViewMode {
    /// The default: combine ambient + direct lighting + reflection
    /// + indirect + transmission as `shade_pixel` normally would.
    #[default]
    Full,

    /// Direct lighting only — ambient + the per-light Lambert /
    /// Phong contribution at the primary hit. No reflection, no
    /// indirect, no transmission. Approximately what the renderer
    /// would produce *before* the GI / reflection / transparency
    /// features landed: useful as a baseline "what does direct
    /// lighting alone look like?" view.
    Local,

    /// Indirect (path-traced) contribution only. The answer to
    /// "is global illumination doing anything, and where?" Pixels
    /// near the colored walls in a Cornell-style scene should
    /// glow with tinted indirect light; corners should show soft
    /// fill from multi-bounce paths.
    Indirect,

    /// Mirror-reflection contribution only. Shows what the
    /// recursive reflection ray brought back from the rest of the
    /// scene. A scene with no reflective surfaces renders black
    /// in this mode.
    Reflection,

    /// Transmitted contribution only, transparency-weighted.
    /// Multiplied by `surface.transparency`, so an opaque surface
    /// contributes nothing here even though `shade_pixel` doesn't
    /// fire a transmission ray for it. Useful for isolating
    /// glass / transparency behavior.
    Transmission,
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

    /// Maximum indirect-bounce recursion depth for diffuse global
    /// illumination. `0` (the default) is "feature off" — no indirect
    /// rays are fired and the renderer's output is byte-identical to
    /// the pre-GI renderer. A positive value enables path-traced
    /// indirect lighting: at each diffuse, non-metallic hit
    /// `shade_pixel` fires one cosine-weighted hemisphere ray (using
    /// the per-pixel-sample `indirect_coord` threaded through the
    /// renderer) and accumulates the incoming radiance, tinted by the
    /// surface's color, as the indirect contribution. The counter is
    /// `Depth::indirect`, separate from `reflect` and `transmit`, for
    /// the same reason those two are separate: a path bouncing
    /// diffusely through a room has a different natural depth than
    /// either a chain of mirrors or a stack of transparent panes.
    ///
    /// Phase 1 of the "Path tracing / GI" plan adds this field and
    /// the surrounding infrastructure (sampler, threaded coordinate,
    /// hoisted hot-path flag) but does *not* yet consume them — the
    /// renderer's output is byte-identical to before. Phase 2 wires
    /// the indirect branch in `shade_pixel`.
    pub indirect_limit: u32,

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

    /// Render-view diagnostic selector. Defaults to `ViewMode::Full`
    /// (everything combined); main.rs overrides from the
    /// `RAYTRACER_VIEW` environment variable when set. Not exposed
    /// in the SDL — the SDL is the canonical "what is this scene"
    /// definition; per-view debugging is a render-time concern.
    /// Phase 4 of the path-tracing plan.
    pub view_mode: ViewMode,
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
    /// Per-pixel *average indirect-bounce depth*, scaled by 100 and
    /// stored as `u32`. The scale lets a fractional average (e.g.
    /// 2.4 bounces) survive the u32 round-trip with two decimals of
    /// resolution; PngHeatmapTarget's 99th-percentile normalization
    /// renders the resulting values into grayscale just like the
    /// time and samples heatmaps. Phase 4 of the path-tracing plan
    /// — "per-surface convergence visualization" in the plan's
    /// language. Filled by `pixel_color` from the per-sample max
    /// indirect depth tracked in the `MAX_INDIRECT_DEPTH`
    /// thread-local; `None` (the default) skips the bookkeeping
    /// entirely.
    pub depth: Option<&'a dyn HeatmapTarget>,
}

thread_local! {
    /// The maximum `Depth::indirect` reached during the current
    /// pixel sample's ray-tracing recursion. Used by the depth
    /// heatmap to visualize where path-tracing rays go deep into
    /// the scene before terminating — a proxy for "where is the
    /// renderer doing more bounce work per sample."
    ///
    /// Thread-local because rayon parallelizes rendering across
    /// rows: each worker thread keeps its own `MAX_INDIRECT_DEPTH`
    /// and there's no cross-thread sharing. `pixel_color` resets
    /// this to 0 before each sample and reads it after; the
    /// indirect branch in `shade_pixel` updates it after the
    /// Russian-roulette survival check (i.e. only counted when a
    /// bounce actually fires). When `scene.indirect_limit == 0`
    /// the indirect branch never fires, so this stays 0 and the
    /// resulting depth heatmap is uniformly zero.
    ///
    /// Implemented via `Cell` rather than `RefCell` because the
    /// payload is `Copy`; the borrow checker doesn't need to
    /// mediate access to a `u32`.
    static MAX_INDIRECT_DEPTH: Cell<u32> = const { Cell::new(0) };
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
    /// The hit point in *texture space*: the local coordinates of
    /// whatever gave the hit its surface, i.e. the leaf primitive if it
    /// carries its own surface, otherwise the nearest enclosing
    /// `Shape::Surfaced`. Pigments are evaluated here, so a pattern
    /// moves, turns and scales with its object. `Transformed::hit_test`
    /// maintains it: while the surface is still `None` the point is
    /// re-expressed at each level on the way out; once a surface is
    /// set, it's final.
    pub texture_point: Point,
    pub normal: Point,
    /// `Option<Surface>` rather than `Surface` so a leaf primitive
    /// can return `None` to indicate "no explicit surface" — the
    /// deepest enclosing `Shape::Surfaced` wrapper then fills it in
    /// on the way back out. By the time a `RayHit` reaches
    /// `shade_pixel` it should always be `Some(_)` in a well-formed
    /// scene (`Shape::validate_surfaces` enforces that at scene
    /// construction); `shade_pixel` carries a defensive hot-pink
    /// fallback for malformed input as a safety net.
    pub surface: Option<Surface>,
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
fn light_vector(
    point: &Point,
    scene: &Scene,
    light: &Light,
    light_coord: (f64, f64),
) -> Option<(Vector, f64)> {
    match light.kind {
        // Point and spot lights ignore `light_coord` — they sample a
        // single fixed point (`light.location`) regardless of which
        // pixel sample drives them. Only area lights consume the
        // coordinate (for the per-sample disk-point pick), so the
        // routing is "always thread, only one arm reads."
        LightKind::Point => light_vector_point(point, scene, light),
        LightKind::Spot { direction, inner_angle, outer_angle } => {
            light_vector_spot(point, scene, light, direction, inner_angle, outer_angle)
        }
        LightKind::Area { axis, radius, cone } => {
            light_vector_area(point, scene, light, axis, radius, cone, light_coord)
        }
        LightKind::Quad { u, v, cone } => {
            light_vector_quad(point, scene, light, u, v, cone, light_coord)
        }
    }
}

/// The shared shadow-ray transmittance walk used by every light kind:
/// march from `origin` toward `target`, multiplying a running
/// transmittance by each occluder's surface `transparency` (0.0 =
/// opaque, 1.0 = fully clear). An opaque occluder drives transmittance
/// to 0 and the walk stops early; transparent occluders attenuate and
/// the walk continues to the next hit.
///
/// Returns the unit-direction ray from `origin` to `target` paired
/// with the surviving transmittance in `(0.0, 1.0]`, or `None` if
/// the path is fully shadowed. The returned `ray.delta` is the
/// shadow-ray direction that `shade_pixel` uses for the Lambert and
/// Phong terms; `ray.start` is `origin`.
///
/// Phase 5 of the "Light types: spotlights and area lights" plan
/// extracted this from `light_vector_point` so that area-light
/// soft-shadow sampling (which picks a per-sample point on the
/// emitter disk and uses that as the shadow-ray origin) can share
/// the same walk. Point and spot lights still go through this with
/// `origin = light.location`, producing bit-identical behavior to
/// the pre-refactor code.
fn shadow_ray_walk(origin: Point, target: &Point, scene: &Scene) -> Option<(Vector, f64)> {
    let direction = subp(*target, origin);
    let distance = lenp(direction);

    let ray = Vector {
        start: origin,
        delta: normalizep(direction),
    };

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
                // Distance of this hit measured from `origin` along
                // the (unit-length) ray direction. Every hit point
                // lies on the original ray line, so this is just the
                // length from the origin.
                let dist_from_origin = lenp(subp(hit.hit_point, ray.start));

                // A hit at (or beyond) the shaded point itself is not
                // an occluder — it is the surface we are lighting.
                // Stop the walk; whatever transmittance we have is
                // the answer.
                if dist_from_origin > distance - EPSILON {
                    break;
                }

                // A genuine occluder strictly between origin and
                // target. Attenuate by its transparency. An opaque
                // surface (transparency 0.0) zeroes transmittance and
                // we can stop immediately.
                //
                // A `None` surface here would mean a leaf bypassed
                // `Shape::validate_surfaces` (direct Rust construction
                // outside the SDL). Treat it as opaque so it still
                // occludes — failing closed is safer than failing
                // open (a "leaked" None as transparent would let a
                // shadow ray skip a real geometric occluder).
                let occluder_transparency = hit
                    .surface
                    .as_ref()
                    .map(|s| s.transparency)
                    .unwrap_or(0.0);
                transmittance *= occluder_transparency;
                if transmittance <= EPSILON {
                    return None;
                }

                // Advance past this occluder and continue the walk.
                cursor = hit.hit_point;
            }
            // The shadow ray hit nothing further along — no more
            // occluders between here and the target's reach. Done.
            None => break,
        }
    }

    if transmittance <= EPSILON {
        None
    } else {
        Some((ray, transmittance))
    }
}

/// Shadow-ray test for a point light: thin wrapper that walks from
/// `light.location` toward `point`. The full transmittance-walk logic
/// lives in `shadow_ray_walk`; this function exists to give the
/// `light_vector` dispatcher a uniform per-kind handler shape.
fn light_vector_point(point: &Point, scene: &Scene, light: &Light) -> Option<(Vector, f64)> {
    light_ray(light.location, point, scene, light.shadowless)
}

/// The ray from a point on a light (`origin`) to the shaded `point`,
/// with the transmittance along it: the shadow walk, or for a
/// shadowless light just the ray at full transmittance.
fn light_ray(origin: Point, point: &Point, scene: &Scene, shadowless: bool) -> Option<(Vector, f64)> {
    if shadowless {
        Some((Vector { start: origin, delta: normalizep(subp(*point, origin)) }, 1.0))
    } else {
        shadow_ray_walk(origin, point, scene)
    }
}

/// The unit direction from `origin` to `point`, or `None` when they
/// coincide (the direction is undefined).
fn unit_toward(origin: Point, point: &Point) -> Option<Point> {
    let d = subp(*point, origin);
    let len = lenp(d);
    if len < EPSILON {
        None
    } else {
        Some([d[0] / len, d[1] / len, d[2] / len])
    }
}

/// Shadow-ray test for a spotlight: cone falloff on top of the
/// transmittance walk.
///
/// The falloff factor is computed *before* the transmittance walk so
/// that a shaded point outside the outer cone (where the spotlight
/// contributes nothing regardless of occlusion) skips the walk
/// entirely — an early-out analogous to the opaque-occluder early-out
/// in `light_vector_point`. Inside the cone, the walk runs identically
/// to the point-light path and the cone factor is folded into the
/// returned transmittance.
///
/// Cone falloff:
/// `smoothstep(cos(outer_angle), cos(inner_angle), cos_theta)` where
/// `cos_theta = dot(normalize(point - light.location), direction)`.
/// Comparison is done in cosine space because `cos` is monotonically
/// decreasing on `[0, π]`: a larger angle means a smaller cosine, so
/// "angle ≤ inner_angle" becomes "cos_theta ≥ cos(inner_angle)". The
/// edges-clamped branches handle the degenerate `inner_angle ==
/// outer_angle` case (hard-edged cone) without dividing by zero.
fn light_vector_spot(
    point: &Point,
    scene: &Scene,
    light: &Light,
    direction: Point,
    inner_angle: f64,
    outer_angle: f64,
) -> Option<(Vector, f64)> {
    let light_to_point = subp(*point, light.location);
    let dist = lenp(light_to_point);
    if dist < EPSILON {
        // Shaded point coincident with the light's location: the
        // light→point direction is undefined and the cone test
        // doesn't apply. Fall through to the point-light path,
        // which handles dist-near-zero gracefully and produces
        // whatever shading the point light would. This is a
        // degenerate case mostly relevant to authoring mistakes —
        // a light placed exactly on a surface — rather than a real
        // rendering scenario.
        return light_vector_point(point, scene, light);
    }
    let light_to_point_unit = [
        light_to_point[0] / dist,
        light_to_point[1] / dist,
        light_to_point[2] / dist,
    ];
    let cone_falloff = SpotCone { direction, inner_angle, outer_angle }.falloff(light_to_point_unit);

    if cone_falloff <= EPSILON {
        return None;
    }

    // Inside the cone: run the same shadow-ray transmittance walk
    // as the point-light path. Reusing the helper keeps the
    // transparent-shadow behavior identical for both kinds (an
    // important property — a glass pane should attenuate a
    // spotlight the same way it attenuates a point light).
    let (ray, transmittance) = light_vector_point(point, scene, light)?;
    Some((ray, transmittance * cone_falloff))
}

/// Build an orthonormal basis `(u, v)` perpendicular to a unit-length
/// `axis`. Used by the area-light disk sampler to orient a unit-disk
/// sample (in `(dx, dy)` coordinates) into the world-space plane the
/// disk lies in: the world-space offset is
/// `radius * dx * u + radius * dy * v`.
///
/// Picks the world axis least aligned with `axis` as the hint vector
/// to avoid a degenerate cross product when `axis` happens to line up
/// with a world axis (a common case — a ceiling disk light usually
/// has `axis = [0, 0, ±1]`). With the least-aligned hint, the first
/// cross product is always well-conditioned; the second falls out
/// orthogonal-and-unit by construction (axis and the first basis
/// vector are both unit and orthogonal, so their cross product is
/// also unit).
fn disk_basis(axis: Point) -> (Point, Point) {
    let absx = axis[0].abs();
    let absy = axis[1].abs();
    let absz = axis[2].abs();
    let hint: Point = if absx <= absy && absx <= absz {
        [1.0, 0.0, 0.0]
    } else if absy <= absz {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let u = normalizep(crossp(axis, hint));
    let v = crossp(axis, u);
    (u, v)
}

/// Shadow-ray test for a disk area light: half-space check +
/// Lambertian cosine attenuation, plus (in Phase 5) per-pixel-sample
/// jittered shadow-ray origin sampling for soft shadows.
///
/// The light's contribution is scaled by `max(0, cos_theta)` where
/// `cos_theta = dot(axis, normalize(point - light.location))` —
/// physically, this is how much radiance a disk emitter with normal
/// `axis` directs toward a shaded point at angle `theta` off-axis.
/// The shaded point is on the *back* of the disk exactly when
/// `cos_theta ≤ 0`, in which case the light contributes nothing and
/// we return `None`. (Same early-out shape as the spotlight cone
/// falloff and the opaque-occluder transmittance walk.) The cosine
/// is computed against the disk *center* — using the disk center
/// (rather than the per-sample disk point) for the radiance
/// attenuation matches the Phase 4 baseline and is consistent across
/// all per-pixel samples.
///
/// Phase 5 — soft shadows. `light_coord` is a per-pixel-sample
/// coordinate in `[0, 1)²` generated by `pixel_color` from a
/// the R2 sequence (`sampler::area_sample`) plus a dedicated CP rotation;
/// the `concentric_disk` map turns it into a uniform unit-disk
/// point, which `disk_basis` orients into the disk's world-space
/// plane to produce a sample point on the emitter. That sample
/// point replaces `light.location` as the shadow-ray origin in
/// `shadow_ray_walk`. One pixel sample takes one shadow ray to one
/// disk point; thirty-two pixel samples take thirty-two shadow rays
/// to thirty-two well-distributed disk points. Penumbra pixels —
/// where some samples reach the light and some don't — see high
/// variance and the adaptive oversampler keeps sampling them until
/// they stabilize; fully shadowed and fully lit pixels terminate at
/// `min_samples` and pay almost nothing for the feature.
///
/// `Point` and `Spot` lights ignore `light_coord` entirely; the
/// dispatcher in `light_vector` only routes it here.
fn light_vector_area(
    point: &Point,
    scene: &Scene,
    light: &Light,
    axis: Point,
    radius: f64,
    cone: Option<SpotCone>,
    light_coord: (f64, f64),
) -> Option<(Vector, f64)> {
    let light_to_point = subp(*point, light.location);
    let dist = lenp(light_to_point);
    if dist < EPSILON {
        // Shaded point coincident with the disk center: the cosine
        // test doesn't apply (the light→point direction is
        // undefined). Fall through to the point-light path, which
        // handles dist-near-zero gracefully. Same degenerate-case
        // handling as `light_vector_spot`.
        return light_vector_point(point, scene, light);
    }
    let light_to_point_unit = [
        light_to_point[0] / dist,
        light_to_point[1] / dist,
        light_to_point[2] / dist,
    ];

    // Lambertian cosine attenuation. A disk emitter facing `axis`
    // sends radiance proportional to `cos(theta)` toward a point at
    // angle `theta` off the normal. The back hemisphere
    // (`cos_theta ≤ 0`) is fully dark from this light: no
    // contribution, return `None` so `shade_pixel` skips it
    // entirely.
    let cosine = dotp(axis, light_to_point_unit);
    if cosine <= EPSILON {
        return None;
    }
    // A disk that is also a spotlight: the cone is measured from the
    // disk's centre, and points outside it get nothing (checked before
    // any shadow work, like `light_vector_spot`).
    let strength = match cone {
        None => cosine,
        Some(c) => {
            let falloff = c.falloff(light_to_point_unit);
            if falloff <= EPSILON {
                return None;
            }
            cosine * falloff
        }
    };

    // Sample a point on the disk. `light_coord` is the per-pixel-
    // sample `[0, 1)²` value (R2 point + per-pixel CP
    // rotation, both fresh each pixel sample); `concentric_disk`
    // maps it to a uniform unit-disk point with low distortion;
    // `disk_basis` gives an orthonormal basis perpendicular to
    // `axis`, in which the disk lies; multiply through to land the
    // sample at the right place on the world-space emitter disk.
    // That sample point is the shadow ray's origin.
    let (lu, lv) = light_coord;
    let (dx, dy) = sampler::concentric_disk(lu, lv);
    let (basis_u, basis_v) = disk_basis(axis);
    let origin = addp(
        light.location,
        addp(
            scalep(basis_u, radius * dx),
            scalep(basis_v, radius * dy),
        ),
    );

    // Walk from the sampled disk origin toward the shaded point,
    // through any transparent occluders, then fold the cosine
    // attenuation into the surviving transmittance.
    let (ray, transmittance) = light_ray(origin, point, scene, light.shadowless)?;
    Some((ray, transmittance * strength))
}

/// Shadow-ray helper for `LightKind::Quad`: a parallelogram emitter
/// centred on `light.location` with edges `u` and `v`. The per-sample
/// `light_coord` in `[0, 1)²` picks a uniformly distributed point on it,
/// the shadow ray starts there, and the result is scaled by the spot
/// cone if there is one. No cosine factor: the quad emits equally in
/// all directions, as POV-Ray's area lights do.
fn light_vector_quad(
    point: &Point,
    scene: &Scene,
    light: &Light,
    u: Point,
    v: Point,
    cone: Option<SpotCone>,
    light_coord: (f64, f64),
) -> Option<(Vector, f64)> {
    let strength = match cone {
        None => 1.0,
        Some(c) => {
            let falloff = match unit_toward(light.location, point) {
                Some(dir) => c.falloff(dir),
                None => 1.0,
            };
            if falloff <= EPSILON {
                return None;
            }
            falloff
        }
    };
    let (lu, lv) = light_coord;
    let origin = addp(light.location, addp(scalep(u, lu - 0.5), scalep(v, lv - 0.5)));
    let (ray, transmittance) = light_ray(origin, point, scene, light.shadowless)?;
    Some((ray, transmittance * strength))
}

/// Recursion-budget tracker threaded through `ray_color` /
/// `shade_pixel`. Reflection, transmission, and indirect (diffuse
/// path-tracing) bounces carry *independent* depth counters, checked
/// against `Scene::reflect_limit`, `Scene::transmit_limit`, and
/// `Scene::indirect_limit` respectively — see the doc comment on
/// `Scene::transmit_limit` for why these budgets are kept separate.
///
/// Phase 1 of the path-tracing plan adds the `indirect` counter
/// alongside the existing two; it's threaded through the recursion
/// but not yet consumed (Phase 2 wires it into the indirect branch
/// in `shade_pixel`). Default-zero means "no bounces spent," same
/// shape as the other two — `Depth::zero()` returns all three at 0.
///
/// `Copy` (three `u32`s), so it threads through the recursion by
/// value with no ceremony; `..depth` struct-update syntax bumps one
/// counter while carrying the others through unchanged.
#[derive(Copy, Clone, Debug)]
struct Depth {
    reflect: u32,
    transmit: u32,
    indirect: u32,
}

impl Depth {
    /// The starting budget for a primary (camera) ray: no reflection,
    /// transmission, or indirect bounces spent yet.
    fn zero() -> Depth {
        Depth { reflect: 0, transmit: 0, indirect: 0 }
    }
}

/// Defensive fallback used by `shade_pixel` when a `RayHit` arrives
/// with `surface: None`. In a well-formed scene this never happens —
/// `Shape::validate_surfaces` rejects unsurfaced leaves at scene
/// construction — but the renderer treats it as a recoverable bug
/// rather than a panic: the offending geometry shades as a flat,
/// opaque, fully-saturated magenta, which is loud enough to spot in
/// a render and impossible to confuse with any of the existing
/// surface presets. This is the "safety net," not the primary
/// check.
const MISSING_SURFACE: Surface = Surface {
    color: [1.0, 0.0, 1.0],
    ambient: 1.0,
    specular: 0.0,
    light: 0.0,
    checked: false,
    reflection: 0.0,
    transparency: 0.0,
    metallic: false,
    pigment: None,
};

fn shade_pixel(
    ray: &Vector,
    scene: &Scene,
    lights: &[Light],
    hit: &RayHit,
    depth: Depth,
    light_coord: (f64, f64),
    indirect_coord: (f64, f64),
) -> LinearColor {
    // https://en.wikipedia.org/wiki/Lambertian_reflectance

    // Unwrap the leaf's surface once at the top, falling back to
    // the magenta missing-surface sentinel if a `None` somehow made
    // it past validation. Every subsequent reference to surface
    // fields goes through this local rather than `hit.surface`.
    let surface = hit.surface.unwrap_or(MISSING_SURFACE);

    let scolor = if let Some(pigment) = surface.pigment {
        pigment.color_at(hit.texture_point)
    } else if surface.checked {
        let checkidx = (((hit.hit_point[0] + EPSILON).floor() +
                         (hit.hit_point[1] + EPSILON).floor() +
                         (hit.hit_point[2] + EPSILON).floor()) as i64 % 2).abs();

        scale_linear_color(&surface.color, if checkidx == 0 { 1.0 } else { 0.5 })
    } else {
        surface.color
    };

    let ambient: LinearColor = scale_linear_color(&scolor, surface.ambient);

    let reflected: LinearColor = if (surface.reflection > EPSILON) && (depth.reflect < scene.reflect_limit) {
        let rvec = subp(negp(ray.delta), scalep(hit.normal, 2.0 * dotp(negp(ray.delta), hit.normal)));

        // Reuse the same `light_coord` and `indirect_coord` for
        // recursive rays rather than re-deriving them per bounce.
        // Bounces are rare enough that the bias is invisible (mirrors
        // the DOF "reflection rays don't get fresh aperture jitter"
        // call); plumbing fresh per-bounce coords would require
        // threading sampler state through recursion proper.
        let rcolor = ray_color(&Vector {
            start: hit.hit_point,
            delta: normalizep(rvec)
        }, scene, lights, Depth { reflect: depth.reflect + 1, ..depth }, light_coord, indirect_coord);

        let scaled = scale_linear_color(&rcolor, surface.reflection);

        // A metal tints what it reflects by its own color (gold
        // reflects gold-ish); a dielectric reflects untinted, like
        // chrome. The tint uses `scolor` so a checked metal's
        // reflection picks up the checker pattern, consistent with
        // the ambient and diffuse terms below.
        if surface.metallic {
            multiply_linear_color(&scaled, &scolor)
        } else {
            scaled
        }
    } else {
        [0.0, 0.0, 0.0]
    };

    // Indirect (path-traced) lighting. Phases 2+3 of the path-
    // tracing plan. At each diffuse, non-metallic hit, fire a
    // single cosine-weighted hemisphere ray and accumulate the
    // incoming radiance, tinted by the surface's body color. This
    // is what produces color bleeding (a red wall tinting a nearby
    // white box red on the facing side) and the soft fill in
    // shadowed areas that makes corner darkening look natural —
    // neither of which the direct + reflection terms above can
    // reproduce.
    //
    // Why one ray, not N: more *primary* rays are how variance is
    // resolved in this codebase (the adaptive oversampler in
    // `pixel_color` already routes extra samples to noisy
    // pixels). Firing N indirect rays per shade would hide the
    // per-sample variance the adaptive loop relies on and lock in
    // a fixed cost everywhere.
    //
    // Cosine-weighted sampling: `cosine_hemisphere_sample` returns
    // a direction with PDF `cos(theta)/π`, and the Lambertian
    // BRDF contributes `albedo/π * cos(theta)`. The PDF cancels
    // the cosine, so the unbiased estimator collapses to
    // `incoming * surface.light * scolor` — no explicit cosine
    // term, no π factor, no division by PDF.
    //
    // Why `scolor` (not `surface.color`): a checked surface's
    // local color depends on which check we hit. Tinting the
    // bounce by `scolor` means the indirect light leaving a check
    // carries the right tint — the same checker-aware behavior
    // the ambient, diffuse, and metallic-reflection terms above
    // already use.
    //
    // Russian roulette (Phase 3): rather than relying on the hard
    // depth cap (`scene.indirect_limit`) as the only termination
    // mechanism, every bounce decides probabilistically whether
    // the path continues. Survival probability is the maximum
    // channel of the local throughput `surface.light * scolor`
    // clamped to `[0, 0.95]`: bright surfaces survive more, dark
    // surfaces terminate more — and the contribution of every
    // surviving path is scaled by `1/p` to maintain expectation,
    // which is what keeps the estimator unbiased. The expected
    // path length is `1/(1 - p)`; on Cornell-style matte surfaces
    // (`p ≈ 0.72`) this is about 3-4 bounces, well below the
    // depth cap that now serves only as a worst-case safety net.
    //
    // `scene.indirect_limit > 0` remains the off-switch gate: when
    // 0 (the default), `indirect` is `[0; 3]`, the `opaque` sum
    // collapses to the pre-GI form, and existing byte-pinned
    // tests stay bit-identical via `x + 0.0 == x`.
    //
    // Why guard on `surface.light > EPSILON` and `!surface.metallic`:
    // a perfect-mirror dielectric (`light == 0`) has no diffuse
    // lobe; a metal is similarly suppressed at the direct-lighting
    // level. Either way the bounce would multiply by zero, so
    // skipping the recursive `ray_color` entirely is a free win.
    let indirect: LinearColor = if scene.indirect_limit > 0
        && depth.indirect < scene.indirect_limit
        && surface.light > EPSILON
        && !surface.metallic
    {
        // Russian-roulette survival probability. Take the maximum
        // channel of the local throughput `surface.light * scolor`
        // — what the indirect contribution would be multiplied by
        // before propagating up — and clamp to `0.95` so even an
        // albedo-1 surface eventually terminates (bounding the
        // worst-case path length).
        //
        // No explicit lower clamp: a dim surface (max ≈ 0.01)
        // legitimately should terminate ~99% of the time because
        // its contribution to the final pixel is correspondingly
        // small. Adding a floor would *under-terminate* dim
        // bounces, raising variance for negligible expected gain.
        let max_albedo = scolor[0].max(scolor[1]).max(scolor[2]);
        let p = (surface.light * max_albedo).min(0.95);

        // RR sample, decorrelated per bounce via golden-ratio
        // offset (see `rr_sample` doc). When `p` is effectively
        // zero, terminate without consulting the sample —
        // dividing by zero would blow up the contribution, and
        // the estimator is mathematically `0/0 * 0` either way.
        let u = sampler::rr_sample(indirect_coord, depth.indirect);
        if p <= EPSILON || u >= p {
            // Path terminates here: no indirect contribution from
            // this bounce. The path is *unbiased* — surviving
            // contributions below scale by `1/p` to compensate
            // for the (1-p) fraction that terminate.
            [0.0, 0.0, 0.0]
        } else {
            // Bounce will fire — record the depth we're about to
            // reach in the thread-local max, for the depth
            // heatmap (Phase 4 / "per-surface convergence
            // visualization"). The bounce ray will recurse with
            // `depth.indirect + 1`, which is the deepest level
            // *this* sub-path reaches at this hit; deeper levels
            // inside the recursion update the max from their own
            // shade_pixel calls. The thread-local is reset by
            // `pixel_color` between samples, so the running max
            // is per-pixel-sample.
            let new_depth = depth.indirect + 1;
            MAX_INDIRECT_DEPTH.with(|c| {
                if new_depth > c.get() {
                    c.set(new_depth);
                }
            });
            // Pick a hemisphere direction in the *local* frame
            // (z = up), cosine-weighted via Malley's method
            // (concentric disk + z lift). The `indirect_coord` is
            // per-pixel-sample, derived in `pixel_color` from a
            // Halton-(17,19) point shifted by the pixel's CP
            // rotation (seed 3) — independent of sub-pixel, lens,
            // and area-light coords at the same sample index.
            let (iu, iv) = indirect_coord;
            let (lx, ly, lz) = sampler::cosine_hemisphere_sample(iu, iv);

            // Rotate the local sample into world space via an
            // orthonormal basis built around the surface normal.
            // The local z = up is the surface's outward normal in
            // world space; the local x and y span the tangent
            // plane.
            let (basis_u, basis_v) = sampler::hemisphere_basis(hit.normal);
            let dir = addp(
                addp(scalep(basis_u, lx), scalep(basis_v, ly)),
                scalep(hit.normal, lz),
            );

            // Cast the bounce ray. Self-intersection: every
            // primitive's `hit_test` rejects `t <= EPSILON`, so
            // the surface we're leaving is discarded — same guard
            // the reflection and transmission branches use. Reuse
            // the pixel sample's `light_coord` and `indirect_coord`
            // for the recursion; per-bounce sampler state would
            // mean threading sampler state through recursion
            // proper. RR survival, on the other hand, *is*
            // decorrelated per bounce via `rr_sample`'s
            // golden-ratio depth offset.
            let incoming = ray_color(
                &Vector { start: hit.hit_point, delta: dir },
                scene,
                lights,
                Depth { indirect: depth.indirect + 1, ..depth },
                light_coord,
                indirect_coord,
            );

            // Modulate the incoming radiance by the diffuse
            // coefficient and the (checker-aware) surface color —
            // this is the color-bleed term — and scale by `1/p`
            // to maintain the RR estimator's unbiasedness:
            //
            //   E[indirect] = p * (1/p) * tint * incoming
            //              + (1-p) * 0
            //              = tint * incoming
            //
            // which is the same expectation as the no-RR Phase 2
            // estimator, computed cheaper on average because
            // (1-p) of paths terminate at this depth.
            let tint = multiply_linear_color(
                &scale_linear_color(&incoming, surface.light),
                &scolor,
            );
            scale_linear_color(&tint, 1.0 / p)
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
        if let Some((lv, transmittance)) = light_vector(&hit.hit_point, scene, l, light_coord) {
            let lambert = dotp(hit.normal, negp(lv.delta));
            // A light behind the surface contributes nothing. Without
            // this check the Lambert factor goes negative and darkens
            // the surface below its ambient level, and the specular
            // power of a negative dot product comes out positive and
            // adds a spurious highlight. Closed objects mostly hide
            // this, because a point facing away from a light is usually
            // in its own object's shadow and never gets here; a plane
            // or an open mesh lit from behind doesn't.
            if lambert <= 0.0 {
                continue;
            }
            // Blinn-Phong: the half vector between the directions toward
            // the viewer and toward the light is the negation of
            // `normalize(ray.delta + lv.delta)`, since both of those point
            // *away from* the light and viewer. The even exponent used to
            // hide the sign; clamping needs it the right way round.
            let half_dot = -dotp(hit.normal, normalizep(addp(ray.delta, lv.delta)));
            let kspecular = f64::powf(half_dot.max(0.0), 50.0);

            // Per-light tint that scales every contribution by this
            // light's color and intensity.
            let light_tint = scale_linear_color(&l.color, l.intensity);

            // Specular: the highlight takes the color of the light
            // for a dielectric (a white light makes a white highlight
            // on red plastic). A metal additionally tints the
            // highlight by its own color — a gold surface has gold
            // highlights regardless of the light.
            let spec_term = {
                let s = scale_linear_color(&light_tint, kspecular * surface.specular);
                if surface.metallic {
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
            let diff_term = if surface.metallic {
                [0.0, 0.0, 0.0]
            } else {
                scale_linear_color(
                    &multiply_linear_color(&scolor, &light_tint),
                    surface.light * lambert,
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
    // mirror reflection + indirect (path-traced) lighting. For an
    // opaque surface (transparency == 0) this is the final color.
    // When `scene.indirect_limit == 0` (the default) `indirect` is
    // identically `[0; 3]` and this sum collapses to the pre-GI
    // form `reflected + ambient + light` — what every byte-pinned
    // test in `tests/sdl_suite.rs` exercises.
    let opaque = add_linear_color(
        &add_linear_color(&reflected, &add_linear_color(&ambient, &light)),
        &indirect,
    );

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
    //
    // `transmitted_alpha` is `surface.transparency` when the
    // transmission ray actually fires, `0.0` otherwise (recursion
    // cap reached, surface opaque, or metallic). Both the
    // `Full`-mode lerp below and the `Transmission` view-mode
    // dispatch read this same value, so the transmission ray is
    // computed exactly once regardless of view mode.
    let (transmitted_color, transmitted_alpha) =
        if (surface.transparency > EPSILON) && !surface.metallic && (depth.transmit < scene.transmit_limit) {
            let transmitted = ray_color(
                &Vector {
                    start: hit.hit_point,
                    delta: ray.delta,
                },
                scene,
                lights,
                Depth { transmit: depth.transmit + 1, ..depth },
                light_coord,
                indirect_coord,
            );
            (transmitted, surface.transparency)
        } else {
            ([0.0, 0.0, 0.0], 0.0)
        };

    // Full-mode combined color: lerp(opaque, transmitted,
    // transparency). When `transmitted_alpha == 0.0` (the
    // common case — every fully opaque surface, plus any
    // transparent surface at the recursion cap), the lerp
    // collapses to `opaque` bit-identically (the
    // `(1 - 0) * opaque + 0 * [0,0,0]` simplifies to `opaque`).
    // So byte-pinned tests against opaque scenes don't see the
    // refactor.
    let combined = if transmitted_alpha > EPSILON {
        let t = transmitted_alpha;
        add_linear_color(
            &scale_linear_color(&opaque, 1.0 - t),
            &scale_linear_color(&transmitted_color, t),
        )
    } else {
        opaque
    };

    // Render-view dispatch (Phase 4 of the path-tracing plan).
    // The view-mode selector applies ONLY at the *primary* hit —
    // recursive `shade_pixel` calls from inside the reflection,
    // transmission, and indirect branches always return the
    // full combined color, because their callers need the full
    // radiance at the bounce point. If a recursive call returned
    // only one component, the upstream branch (e.g. the indirect
    // bounce reading the secondary surface's radiance) would
    // see only a fraction of what's actually there.
    //
    // "Primary" means all three depth counters are zero —
    // `Depth::zero()` — i.e. this is the first shade in the
    // chain from the camera ray. The default `ViewMode::Full`
    // path is bit-identical to the pre-Phase-4 renderer: it
    // returns `combined` exactly as the old code did.
    let is_primary = depth.reflect == 0 && depth.transmit == 0 && depth.indirect == 0;
    if !is_primary {
        return combined;
    }
    match scene.view_mode {
        ViewMode::Full => combined,
        ViewMode::Local => add_linear_color(&ambient, &light),
        ViewMode::Indirect => indirect,
        ViewMode::Reflection => reflected,
        // Transparency-weighted: an opaque surface contributes
        // exactly zero here (because `transmitted_alpha` is 0.0,
        // and the renderer never fires a transmission ray for it).
        // A glass surface contributes `transmitted_color *
        // transparency`, which is exactly that surface's
        // *contribution to the Full pixel* via the lerp above —
        // so the Transmission view shows "the visible portion of
        // the scene that's reached via transmission," not "the
        // raw transmitted-ray output."
        ViewMode::Transmission => scale_linear_color(&transmitted_color, transmitted_alpha),
    }
}

fn ray_color(
    ray: &Vector,
    scene: &Scene,
    lights: &[Light],
    depth: Depth,
    light_coord: (f64, f64),
    indirect_coord: (f64, f64),
) -> LinearColor {
    match scene.root.hit_test(ray) {
        Some(hit) => shade_pixel(ray, scene, lights, &hit, depth, light_coord, indirect_coord),
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
/// return the sample count and the depth-heatmap metric alongside it.
///
/// The sample count is what the adaptive-oversampling Phase 3
/// sample-count heatmap renders. The third return value is the
/// *average max indirect depth per sample, scaled by 100* — the
/// path-tracing Phase 4 depth heatmap. Both are returned
/// unconditionally rather than gated on whether a heatmap is
/// active: they're a u32 each per pixel and the renderer's caller
/// is free to ignore them. Threading "maybe collect" flags down
/// here would be more bookkeeping than just always returning the
/// values.
///
/// "Average max indirect depth ×100": for each sample, the
/// `MAX_INDIRECT_DEPTH` thread-local captures the deepest
/// `Depth::indirect` reached by any branch of the ray-tracing
/// recursion (set by the indirect branch in `shade_pixel`). The
/// per-pixel result is the average across samples, multiplied by
/// 100 so two decimals of resolution survive the u32 round-trip
/// to `PngHeatmapTarget`. When `scene.indirect_limit == 0` (the
/// default) the indirect branch never fires and this is always 0.
fn pixel_color(
    camera: &CameraDetails,
    scene: &Scene,
    lights: &[Light],
    x: u32,
    y: u32,
) -> (LinearColor, u32, u32) {
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

    // Area-light disk sampling. Same hoisted-flag pattern as DOF: if
    // no light in the scene is an area light, we skip the per-sample
    // area-coordinate work entirely — `light_vector_area` is the
    // only consumer, and `LightKind::Point` / `LightKind::Spot` just
    // ignore the coordinate. The flag is computed once per pixel
    // rather than per sample because the lights slice is fixed for
    // the entire frame (and indeed for the whole render call), and
    // the matches!() probe is too cheap to bother lifting further.
    // Phase 5 of the "Light types: spotlights and area lights" plan.
    let has_area_light = lights
        .iter()
        .any(|l| matches!(l.kind, LightKind::Area { .. } | LightKind::Quad { .. }));
    let (aox, aoy) = if has_area_light {
        sampler::cranley_patterson_area_offset(x, y)
    } else {
        (0.0, 0.0)
    };

    // Indirect-bounce (path-tracing) sampling. Same hoisted-flag
    // pattern as DOF and area-light sampling: when `indirect_limit`
    // is 0 (the default), no indirect rays will ever fire and we
    // skip the per-sample `halton_indirect` / CP work entirely.
    // The flag is computed once per pixel; `indirect_limit` is a
    // scene-wide constant, so this is just a clean shape that
    // mirrors the other two features. Phase 1 of the path-tracing
    // plan: the coordinate is threaded through `ray_color` /
    // `shade_pixel` but has no consumer yet — Phase 2 adds the
    // indirect branch to `shade_pixel` that reads it.
    let has_indirect = scene.indirect_limit > 0;
    let (iox, ioy) = if has_indirect {
        sampler::cranley_patterson_indirect_offset(x, y)
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

    // Per-sample max-indirect-depth bookkeeping (Phase 4 depth
    // heatmap). The thread-local `MAX_INDIRECT_DEPTH` is reset to
    // 0 before each sample's `ray_color` call and read after; we
    // accumulate the per-sample reads here and average at the end.
    // When `scene.indirect_limit == 0` (the default) the indirect
    // branch never fires and the running max stays 0 every
    // sample, so `depth_sum` ends at 0 — bit-identical to "no
    // depth heatmap" for any pre-Phase-2 byte-pinned test.
    let mut depth_sum: u32 = 0;

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

            // Per-pixel-sample area-light coordinate. Same pattern as
            // the lens sample but on the R2 sequence (see
            // `sampler::area_sample` for why not Halton) and its own
            // CP rotation seed. `light_vector_area` and
            // `light_vector_quad` are the only consumers; point/spot
            // lights ignore it. When no area
            // light is in the scene this stays `(0.0, 0.0)` and
            // `area_sample` is never called on the hot path.
            let light_coord = if has_area_light {
                let (ahx, ahy) = sampler::area_sample(i + 1);
                ((ahx + aox).fract(), (ahy + aoy).fract())
            } else {
                (0.0, 0.0)
            };

            // Per-pixel-sample indirect-bounce coordinate. Same
            // pattern as the lens and area-light samples but on
            // Halton bases (17, 19) and CP rotation seed 3. Phase 2
            // of the path-tracing plan will consume this in
            // `shade_pixel` to pick the next-bounce direction at a
            // diffuse hit; in Phase 1 the coordinate is threaded
            // through but unused. When `indirect_limit == 0` (the
            // default) this stays `(0.0, 0.0)` and `halton_indirect`
            // is never called on the hot path — the off case pays
            // nothing.
            let indirect_coord = if has_indirect {
                let (ihx, ihy) = sampler::halton_indirect(i + 1);
                ((ihx + iox).fract(), (ihy + ioy).fract())
            } else {
                (0.0, 0.0)
            };

            // Reset the per-sample max-indirect-depth tracker
            // before tracing this sample. The `shade_pixel`
            // indirect branch will bump it whenever a bounce
            // fires; we read the final max immediately after
            // `ray_color` returns. When indirect lighting is off
            // (`scene.indirect_limit == 0`) the branch never
            // fires and this stays 0 — same shape as the
            // pre-Phase-4 renderer.
            MAX_INDIRECT_DEPTH.with(|c| c.set(0));

            let rc = ray_color(
                &camera_ray(&camera.camera, camera.aspect, xt, yt, lens),
                scene,
                lights,
                Depth::zero(),
                light_coord,
                indirect_coord,
            );

            // Capture the sample's max indirect depth and add it
            // to the running total. Averaged across samples
            // below; the result is the depth-heatmap metric.
            depth_sum = depth_sum.saturating_add(MAX_INDIRECT_DEPTH.with(|c| c.get()));

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

    // Average max indirect depth × 100. The ×100 scaling preserves
    // two decimals of fractional resolution in the u32 metric: an
    // average of 2.4 bounces lands as 240. The PNG heatmap
    // normalizes against the 99th percentile at save time, so the
    // absolute scale doesn't matter — only ratios between pixels
    // do, and 100× has plenty of headroom for the modest depths
    // path tracing reaches in practice (8 max means at most 800
    // here).
    let depth_metric = if samples > 0 {
        ((depth_sum as f64) * 100.0 / samples as f64).round() as u32
    } else {
        0
    };

    (
        scale_linear_color(&sum, 1.0 / samples as f64),
        samples,
        depth_metric,
    )
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
    let want_depth = heatmaps.depth.is_some();

    let mut timings: Vec<u32> = if want_time { vec![0u32; imgx as usize] } else { Vec::new() };
    let mut sample_counts: Vec<u32> =
        if want_samples { vec![0u32; imgx as usize] } else { Vec::new() };
    let mut depths: Vec<u32> =
        if want_depth { vec![0u32; imgx as usize] } else { Vec::new() };

    for x in 0..imgx {
        // Timer is started conditionally: when `want_time` is false,
        // there's no `Instant::now` call at all, which is what made
        // the original "heatmap disabled" path compile to the same
        // code as the pre-feature renderer. We preserve that property
        // for the time metric.
        let start_ns = if want_time { Some(Instant::now()) } else { None };

        let (pc, samples, depth_metric) = pixel_color(camera, scene, lights, x, y);
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
        if want_depth {
            depths[x as usize] = depth_metric;
        }
    }

    target.submit_row(0, y, &row);
    if let Some(h) = heatmaps.time {
        h.submit_metric_row(0, y, &timings);
    }
    if let Some(h) = heatmaps.samples {
        h.submit_metric_row(0, y, &sample_counts);
    }
    if let Some(h) = heatmaps.depth {
        h.submit_metric_row(0, y, &depths);
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
    if let Some(h) = heatmaps.depth {
        h.finish();
    }
}


#[cfg(test)]
mod light_tests {
    use super::*;
    use crate::render::shapes::{group, Plane, Sphere};

    fn opaque() -> Surface {
        Surface {
            color: [1.0, 1.0, 1.0],
            ambient: 0.1,
            specular: 0.0,
            light: 0.6,
            checked: false,
            reflection: 0.0,
            transparency: 0.0,
            metallic: false,
            pigment: None,
        }
    }

    /// A scene whose only geometry is an opaque unit sphere at the
    /// origin, which blocks light between +z and -z.
    fn blocker_scene() -> Scene {
        Scene {
            name: "light tests".to_string(),
            camera: Camera::looking_at([0.0, -5.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0),
            root: group(vec![Shape::Sphere(Sphere { center: [0.0, 0.0, 0.0], r: 1.0, surface: Some(opaque()) })]),
            background: [0.0, 0.0, 0.0],
            reflect_limit: 0,
            transmit_limit: 0,
            indirect_limit: 0,
            min_samples: 1,
            max_samples: 1,
            variance_threshold: 0.0,
            view_mode: ViewMode::default(),
        }
    }

    const BELOW: Point = [0.0, 0.0, -3.0];

    fn cone(direction: Point, inner_deg: f64, outer_deg: f64) -> SpotCone {
        SpotCone {
            direction: normalizep(direction),
            inner_angle: inner_deg.to_radians(),
            outer_angle: outer_deg.to_radians(),
        }
    }

    #[test]
    fn spot_cone_falloff() {
        let c = cone([0.0, 0.0, -1.0], 10.0, 30.0);
        assert_eq!(c.falloff([0.0, 0.0, -1.0]), 1.0);
        assert_eq!(c.falloff([1.0, 0.0, 0.0]), 0.0);
        let at = |deg: f64| c.falloff([deg.to_radians().sin(), 0.0, -deg.to_radians().cos()]);
        assert_eq!(at(5.0), 1.0);
        assert_eq!(at(35.0), 0.0);
        let mid = at(20.0);
        assert!(mid > 0.0 && mid < 1.0);
        assert!(at(15.0) > mid && mid > at(25.0));
        // A hard edge: equal angles give 1 inside and 0 outside.
        let hard = cone([0.0, 0.0, -1.0], 20.0, 20.0);
        assert_eq!(hard.falloff([0.0, 0.0, -1.0]), 1.0);
        assert_eq!(hard.falloff([30f64.to_radians().sin(), 0.0, -30f64.to_radians().cos()]), 0.0);
    }

    #[test]
    fn shadowless_lights_ignore_occluders() {
        let scene = blocker_scene();
        let mut light = Light::white([0.0, 0.0, 3.0]);
        assert!(light_vector(&BELOW, &scene, &light, (0.5, 0.5)).is_none(), "the sphere blocks it");
        light.shadowless = true;
        let (ray, t) = light_vector(&BELOW, &scene, &light, (0.5, 0.5)).unwrap();
        assert_eq!(t, 1.0);
        assert_eq!(ray.start, [0.0, 0.0, 3.0]);
        assert_eq!(ray.delta, [0.0, 0.0, -1.0]);
        // Shadowless area lights too.
        let quad = Light {
            location: [0.0, 0.0, 3.0],
            color: [1.0; 3],
            intensity: 1.0,
            kind: LightKind::Quad { u: [0.5, 0.0, 0.0], v: [0.0, 0.5, 0.0], cone: None },
            shadowless: true,
        };
        assert!(light_vector(&BELOW, &scene, &quad, (0.3, 0.7)).is_some());
    }

    #[test]
    fn quad_samples_span_the_parallelogram() {
        // No geometry, so every sample reaches the point; the shadow
        // ray starts at the sampled point on the quad.
        let mut scene = blocker_scene();
        scene.root = group(vec![]);
        let quad = Light {
            location: [1.0, 2.0, 3.0],
            color: [1.0; 3],
            intensity: 1.0,
            kind: LightKind::Quad { u: [4.0, 0.0, 0.0], v: [0.0, 2.0, 0.0], cone: None },
            shadowless: false,
        };
        let origin = |lu, lv| light_vector(&BELOW, &scene, &quad, (lu, lv)).unwrap().0.start;
        assert_eq!(origin(0.5, 0.5), [1.0, 2.0, 3.0]);
        assert_eq!(origin(0.0, 0.0), [-1.0, 1.0, 3.0]);
        assert_eq!(origin(1.0, 1.0), [3.0, 3.0, 3.0]);
        // Full strength with no cosine factor, even at a steep angle
        // and from behind: a quad emits equally in all directions.
        let (_, t) = light_vector(&[20.0, 2.0, 3.5], &scene, &quad, (0.5, 0.5)).unwrap();
        assert_eq!(t, 1.0);
    }

    #[test]
    fn a_quad_with_a_cone_is_a_spotlight() {
        let mut scene = blocker_scene();
        scene.root = group(vec![]);
        let quad = Light {
            location: [0.0, 0.0, 3.0],
            color: [1.0; 3],
            intensity: 1.0,
            kind: LightKind::Quad { u: [1.0, 0.0, 0.0], v: [0.0, 1.0, 0.0], cone: Some(cone([0.0, 0.0, -1.0], 10.0, 20.0)) },
            shadowless: false,
        };
        assert_eq!(light_vector(&[0.0, 0.0, 0.0], &scene, &quad, (0.2, 0.9)).unwrap().1, 1.0);
        assert!(light_vector(&[3.0, 0.0, 0.0], &scene, &quad, (0.5, 0.5)).is_none(), "outside the cone");
    }

    #[test]
    fn a_disk_with_a_cone_is_a_spotlight() {
        let mut scene = blocker_scene();
        scene.root = group(vec![Shape::Plane(Plane { normal: [0.0, 0.0, 1.0], p0: [0.0, 0.0, -10.0], surface: Some(opaque()) })]);
        let axis = [0.0, 0.0, -1.0];
        let plain = Light::area([0.0, 0.0, 3.0], axis, 0.5, [1.0; 3], 1.0);
        let coned = Light {
            kind: LightKind::Area { axis, radius: 0.5, cone: Some(cone(axis, 10.0, 20.0)) },
            ..plain.clone()
        };
        // Straight below, both are the same.
        let p = [0.0, 0.0, 0.0];
        let (a, ta) = light_vector(&p, &scene, &plain, (0.3, 0.6)).unwrap();
        let (b, tb) = light_vector(&p, &scene, &coned, (0.3, 0.6)).unwrap();
        assert_eq!((a.start, a.delta, ta), (b.start, b.delta, tb));
        // Off to the side, the plain disk still lights it (with its
        // cosine falloff), the coned one doesn't.
        let side = [3.0, 0.0, 0.0];
        assert!(light_vector(&side, &scene, &plain, (0.5, 0.5)).is_some());
        assert!(light_vector(&side, &scene, &coned, (0.5, 0.5)).is_none());
    }
}
