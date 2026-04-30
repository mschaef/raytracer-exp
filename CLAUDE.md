# Project Notes for Claude

A small ray tracer written in Rust. The README frames it as a Rust-learning
exercise; this document is the operating manual for working on it productively
in a Claude-assisted session.

## What this is

A CPU ray tracer that renders simple scenes of analytic primitives (spheres,
planes, axis-aligned boxes) into PNGs. It supports ambient/diffuse/specular
shading, hard shadows, mirror reflections, hierarchical scene composition with
affine transforms, and a look-at camera. Rendering is parallelized with Rayon.
Output is a 2048×2048 image (`render.png`) split into four quadrants, each
showing a different scene; this is configured in `main.rs`.

`cargo run --release` produces `render.png`. Set `PARALLEL=n` to disable Rayon.

## Module layout

```
src/
  main.rs              Entry point. Builds 4 scenes, lays them out in quadrants,
                       writes render.png. Reads PARALLEL env var.

  render.rs            Top-level render module. Defines:
                         - Scene, Camera, Light, Surface, RayHit
                         - Hittable trait
                         - the scene_objects! macro (#[macro_export])
                         - the rendering pipeline:
                             render → render_into_line → pixel_color
                                    → camera_ray, ray_color
                                    → shade_pixel, light_vector
                       Sub-modules:
    render/geometry.rs   Point = [f64; 3], Vector { start, delta }, EPSILON,
                         and pointwise ops: addp, subp, scalep, dotp, crossp,
                         lenp, normalizep, negp.
    render/color.rs      LinearColor and conversions to/from PNG sRGB.
    render/transform.rs  Affine 3D transforms as (3×3 linear, 3-vec translation).
                         Mat3 type alias, mat3_apply/multiply/transpose/inverse,
                         and Affine constructors: identity, translation, scale,
                         rotation_x/y/z, rotation_axis. Plus compose, inverse,
                         transform_point, transform_vector.
    render/shapes.rs     The Shape enum and everything related. See below.
    render/mesh.rs       Wavefront OBJ loader (`load_obj`). Returns a
                         `Shape::Group` of `Shape::Triangle`s, so loaded
                         meshes integrate with the rest of the scene tree
                         without any special-casing.
    render/output.rs     The `RenderTarget` trait plus the `PngTarget`,
                         `OffsetTarget`, and `ProgressTarget` impls. The
                         renderer pushes finished rows into a target rather
                         than returning an image; `image` crate use is
                         fully encapsulated here.

  scenes.rs            Hand-written scene definitions, surface presets,
                       and default_camera(). Each scene is a `pub fn` returning
                       a Scene value, marked #[allow(dead_code)] since main.rs
                       only wires up four of them at a time.
```

## The Shape enum

Central abstraction. Closed enumeration, no dynamic dispatch:

```rust
pub enum Shape {
    Sphere(Sphere),
    Plane(Plane),
    Cuboid(Cuboid),                    // axis-aligned box, slab method
    Triangle(Triangle),                // Möller–Trumbore, smooth normals
    Group(Vec<Shape>),                 // hierarchical container
    Transform(Box<Transformed>),       // affine-transformed subtree
}
```

`Hittable for Shape` is a single match dispatching to per-variant logic.
`Sphere`/`Plane`/`Cuboid` implement `Hittable` with the standard analytic ray
tests. `Triangle` uses Möller–Trumbore and interpolates per-vertex normals
via the barycentric coordinates returned by the test (smooth shading falls
out for free; flat shading is the same algorithm with all three vertex
normals equal). `Group::hit_test` is `nearest_hit(ray, &children)` — same
fold the top-level scene traversal uses, so flat scenes and arbitrarily-nested
groups share the exact same hit-testing path.

The `Transformed` struct caches the inverse affine and a precomputed
inverse-transpose `Mat3` for normal transformation. Its hit_test:

1. Inverse-transforms the ray into the child's local space (deliberately
   *without* renormalizing the local direction — see "Pitfalls" below).
2. Recursively calls the child's hit_test.
3. On a hit, recomputes `world_hit_point` directly from the *world* ray and
   the returned `t` (free, since `t` is preserved across the transform), and
   transforms the local normal back via the cached `normal_xform`, then
   renormalizes.

`Box<Transformed>` is what breaks the otherwise-recursive size of `Shape`.
The `Group` variant is naturally recursive without an explicit Box because
`Vec<Shape>` is heap-indirected.

## Constructing scenes ergonomically

Three layered conveniences let scene definitions stay clean:

1. **`From<T> for Shape`** for each leaf type (`Sphere`, `Plane`, `Cuboid`).
   Plus the standard library's reflexive `From<T> for T`, so `Shape::from(s)`
   works on any leaf or on an existing `Shape`.

2. **The `scene_objects!` macro** (defined in `render.rs`, `#[macro_export]`):

   ```rust
   scene_objects![
       Sphere { ... },
       Plane  { ... },
       translate([1,0,0], Cuboid { ... }),
   ]
   ```

   Expands each entry through `<Shape>::from(_)`, returning `Vec<Shape>`. Used
   directly for `Scene::objects` and as the input to `group(...)`.

3. **Constructor functions** (in `shapes.rs`) that accept `impl Into<Shape>`
   for their `child` argument, so leaf primitives can be passed directly:

   ```rust
   pub fn group(children: Vec<Shape>) -> Shape;
   pub fn transform(forward: Affine, child: impl Into<Shape>) -> Shape;
   pub fn translate(d: Point,        child: impl Into<Shape>) -> Shape;
   pub fn scale(s: Point,            child: impl Into<Shape>) -> Shape;
   pub fn rotate_x(theta: f64,       child: impl Into<Shape>) -> Shape;
   pub fn rotate_y(theta: f64,       child: impl Into<Shape>) -> Shape;
   pub fn rotate_z(theta: f64,       child: impl Into<Shape>) -> Shape;
   pub fn rotate_axis(axis: Point, theta: f64, child: impl Into<Shape>) -> Shape;
   ```

   Outer-most call applies last, so `translate(t, rotate_z(θ, scale(s, leaf)))`
   reads naturally: scale first, then rotate, then translate. Each layer
   inverse-transforms the ray on the way down; the math comes out equivalent
   to a single composed transform without anyone having to think about
   matrix multiplication order.

   The bare `transform(matrix, child)` is the escape hatch for hand-built
   `Affine` values via `Affine::translation(...).compose(...)` etc.

## The Camera

Standard look-at model with a precomputed orthonormal basis:

```rust
pub struct Camera {
    pub location: Point,
    pub forward: Point,    // unit
    pub right: Point,      // unit
    pub up: Point,         // unit, re-orthogonalized from up_hint
    pub half_height: f64,  // half of view-plane height at unit distance
}
```

Built via `Camera::looking_at(location, look_at, up_hint, zoom)` or
`Camera::with_fov(location, look_at, up_hint, fov_radians)`. The user's
`up_hint` doesn't have to be perpendicular to `forward`; the constructor
projects out the parallel component. It will panic if `up_hint` is *parallel*
to `forward` (no orientation degree of freedom).

`zoom = 1.0` corresponds to vertical FOV ≈ 53° and matches the framing of the
older fixed camera. `with_fov` is a thin wrapper that converts to zoom and
forwards to `looking_at`.

Aspect ratio is the renderer's concern, not the camera's. `CameraDetails`
caches `aspect = imgx / imgy` and `camera_ray` computes `half_width` per call.
`camera_ray` flips image-y (`sy = 1.0 - 2.0 * yt`) so pixel y=0 is the top of
the image — required for existing scenes to render right-side-up.

## Rendering pipeline

```
render(scene, imgx, imgy, &target, parallel)
  └─ for each y in 0..imgy (rayon::into_par_iter if parallel):
       render_one_row
         └─ for each x in 0..imgx:
              pixel_color   ← oversample loop (2×2 by default)
                └─ camera_ray (uses cached aspect, basis)
                └─ ray_color
                     └─ nearest_hit (fold over scene.objects: &[Shape])
                          → Hittable::hit_test on each Shape
                     └─ shade_pixel (lambert + specular + reflection)
                          └─ light_vector  (shadow ray)
                          └─ recursive ray_color for reflections
                            (capped by Scene::reflect_limit)
         └─ target.submit_row(0, y, &row)
```

`nearest_hit` lives in `shapes.rs` and is shared between the renderer's
top-level traversal and `Shape::Group`'s hit_test. The renderer doesn't know
or care about the shape of the scene tree; it just dispatches through
`Hittable::hit_test`.

`render()` no longer returns an image — it writes finished rows of `[u8; 3]`
pixels into the supplied `RenderTarget`. The renderer is therefore agnostic
to where pixels eventually go (PNG file today, possibly a streaming UI in
the future).

## Surface model

`Surface { color, ambient, specular, light, checked, reflection }`. Lighting is
Lambertian diffuse + Phong specular (50-power), with ambient as a flat
multiplier of the surface color and a single bounce of mirror reflection
(recursion gated by `Scene::reflect_limit`). The `checked` flag enables a
simple world-space checker pattern keyed off `floor(x+y+z)`.

Surface presets and the `surface_glossy` / `reflective` const fns live in
`scenes.rs`. Common ones: `SURFACE_RED`, `SURFACE_GREEN`, …, `SURFACE_WHITE_C`
(the reflective checkered ground used by most scenes).

## Lights

`Light { location, color, intensity }`. Point lights only (no directional /
spotlight / area light variants yet). `color` is the emitted color; `intensity`
is a scalar multiplier. The two are conceptually distinct knobs even though
their numerical effect overlaps — color is hue, intensity is brightness.
Convenience constructors: `Light::white(location)` for full-intensity white
(matches the legacy implicit defaults), `Light::point(location, color,
intensity)` for the general case.

`Scene::lights: Vec<Light>` is a list, summed in `shade_pixel`. Each visible
light contributes a Phong specular highlight and a Lambertian diffuse term,
both multiplied by `light.color * light.intensity`. The diffuse term has the
surface color modulated component-wise by the light tint; the specular term
takes on the pure light color (i.e. a red light produces a red highlight on
any surface, regardless of body color, which is physically right for
microfacet specularity). Empty `lights: vec![]` yields ambient + reflection
only — useful as a debug mode.

## Recent work history

Approximate order of recent commits, oldest first:

1. **Cuboid primitive added.** Axis-aligned box, specified by center and
   per-axis size. Slab method intersection. Per-face normals fall out of
   tracking which axis "won" the t_enter maximum.

2. **`Box::new` boilerplate removed.** Scene definitions previously read
   `Box::new(Sphere { ... }) as Box<dyn Hittable + Send + Sync>` for every
   object. Replaced `Vec<Box<dyn Hittable>>` with the closed `Shape` enum,
   added `From<T> for Shape` impls, introduced the `scene_objects!` macro.
   Faster (no boxing, no vtable), cleaner at the call site.

3. **`Group` variant added.** Hierarchical scene composition. The shared
   `nearest_hit` helper became `pub` and got used in both places. Verifying
   correctness was a matter of confirming a grouped scene rendered
   identically to the same scene with no grouping.

4. **`Transform` variant added.** New `transform.rs` module with the affine
   math (Mat3, Affine, all ops). `Transformed` struct caches inverse +
   normal_xform. Per-axis constructor functions (`translate`, `scale`,
   `rotate_x/y/z`, `rotate_axis`) plus the `transform(affine, child)` escape
   hatch. Took a follow-up pass to make the constructors accept
   `impl Into<Shape>` so leaf primitives could be passed directly.

5. **Camera rewrite.** Replaced the ad-hoc `(location, point_at, u, v)` form
   (where `u`/`v` were full view-plane span vectors) with a look-at camera
   parameterized by `(location, look_at, up_hint, zoom)`. Added `crossp` to
   `geometry.rs`. `Camera::with_fov` provides a degrees/radians alternative.
   Aspect is now derived from image dimensions at render time, not encoded
   in the camera. `camera_ray` performs the image-y flip.

6. **Pluggable render targets.** Factored the disk-writing concern out of
   the renderer. New `output.rs` module defines the `RenderTarget` trait
   (`submit_row(&self, x, y, &[[u8; 3]])` + default `finish()`), plus
   `PngTarget` (Mutex-protected `image::ImageBuffer`, `save(path)` /
   `put_pixel`) and `OffsetTarget` (wraps another target, adds an `(dx, dy)`
   to coordinates). `render()` now takes `&impl RenderTarget` and returns
   `()`; it writes rows directly to the target instead of building an
   `ImageBuffer`. `main.rs` composes the four-quadrant output by sharing one
   `PngTarget` and pointing four `OffsetTarget`s at it. The `image` crate
   dependency now lives only inside `output.rs`. Streaming output (e.g. to
   a windowed UI) plugs in by implementing `RenderTarget` over a channel
   or socket; no renderer changes needed.

7. **Multiple lights with color and intensity.** `Scene::light: Light` was
   replaced with `Scene::lights: Vec<Light>`. The `Light` struct gained
   `color: LinearColor` and `intensity: f64` fields, with `Light::white(loc)`
   and `Light::point(loc, color, intensity)` const-fn constructors so
   existing scenes can stay one-line. `shade_pixel` now loops over visible
   lights and sums their contributions; `light_vector` takes a `&Light`
   parameter instead of pulling from `scene.light`. Added
   `multiply_linear_color` to `color.rs` for component-wise color
   modulation. New `scene_multi_light_test` puts a red and a blue light
   on opposite sides of a white sphere as a visual smoke test for the
   summed-contribution and shadow-tinting math.

8. **Triangle primitive and OBJ mesh loading.** Added `Triangle` as a
   `Shape` variant with three vertices, three per-vertex normals, and a
   surface. Hit test is Möller–Trumbore; barycentric weights from the
   intersection are reused to interpolate per-vertex normals (smooth
   shading). New `render/mesh.rs` module exports `load_obj(path, surface)`
   which uses the `tobj` crate to parse a Wavefront OBJ file and produces
   a `Shape::Group` of `Shape::Triangle`s. Polygon faces are
   fan-triangulated by tobj; OBJs without per-vertex normals get
   geometric face normals computed at load time (flat shading). Loading
   panics on I/O or parse error — scene definition is part of program
   startup, so a missing model is a fatal config error. Will revisit
   error handling when the scene DSL lands. New `scene_teapot` loads
   `models/teapot.obj`. Note that without a BVH this will be slow:
   `nearest_hit` is O(n) and the teapot is ~6000 triangles.

9. **Live progress reporting.** First step of an observability program
   intended to inform performance work. New `ProgressTarget` in
   `output.rs` wraps another `RenderTarget` and prints
   `"  {label}: {n}/{total} rows"` to stderr with `\r` for in-place
   updates as rows complete. Print is serialized via `stderr().lock()`
   to keep concurrent worker output from interleaving. Renderer now
   calls `target.finish()` at the end of `render()`; `ProgressTarget`'s
   override emits a closing newline so the subsequent "Time elapsed"
   line lands cleanly. `ProgressTarget` propagates `finish()` to the
   inner target (so a future streaming target wrapped in progress still
   gets its "done" signal); `OffsetTarget` does not propagate (because
   multiple offsets share one inner). `main.rs::render_into` now
   constructs a `ProgressTarget` per scene with `scene.name` as the
   label.

## Pitfalls and conventions

These are the things that have bitten or might bite someone working on the
codebase. Keep them in mind.

**Don't renormalize ray.delta inside `Transformed::hit_test`.** Under
non-uniform scale the inverse-transformed direction's magnitude changes; if
you renormalize, the local-space `t` and the world-space `t` diverge, and
nearest-hit selection across mixed transformed/untransformed objects breaks.
The convention in this codebase is that all primitive `hit_test`s already
handle non-unit-magnitude directions correctly (sphere uses
`a = dot(delta, delta)`; plane and cuboid are scale-invariant in `t`). This
is a load-bearing property — be careful adding new primitives.

**Normals transform by the inverse-transpose, not by the forward matrix.**
Cached as `normal_xform` on `Transformed`. Equal to `transpose(inverse.linear)`.
If you see "lit faces dark and dark faces lit" or "highlight in the wrong
place under non-uniform scale," this is the suspect.

**`EPSILON = 0.0001` is in local-ray-t units.** For typical scale factors near
1, equivalent to 0.0001 world units. Under extreme non-uniform scale (1e-3 or
1e3), the self-intersection rejection threshold drifts in world terms. Hasn't
been an issue yet.

**`scene_objects!` macro requires explicit import in submodules.** It's
`#[macro_export]` so it lives at the crate root; modules using it need
`use crate::scene_objects;` at the top. This is already in place in
`scenes.rs`.

**Image-y is inverted.** `camera_ray` uses `sy = 1.0 - 2.0 * yt` so that
pixel y=0 is the top of the image. Don't "fix" this unless you also flip
every existing scene's `up_hint`.

**There are no unit tests.** Verification is visual: render and look. When
making changes, the smell test is "does the output look the same as before
for cases that shouldn't have changed, and right for cases that should?" The
default `main.rs` quadrant layout is useful for side-by-side comparisons.

**`#[allow(dead_code)]` on every scene fn.** `main.rs` only references four
scenes at a time; the unused ones generate warnings without it. When
introducing a new scene, swap it into the `scene` array in `main.rs` to view
it (or comment one out — the existing pattern shows both styles).

## Future directions

The README's own "Potential Futures" list overlaps these but is now somewhat
out of date.

**Performance: BVH (bounding-volume hierarchy).** `nearest_hit` is currently
O(n) in the number of objects per ray. The hierarchical `Group` structure is
exactly what makes adding a BVH straightforward: precompute an AABB per
`Group` (or per subtree), early-out if the ray misses the AABB. This is the
single largest perf improvement available for non-trivial scenes.

**Transform collapsing.** A nested `translate(rotate(scale(leaf)))` produces
three separate `Transform` nodes, each doing its own ray-transform on the way
down. `transform()` could peek at its child and, if it's already a
`Shape::Transform`, multiply the inverse affines and skip a level. Trivial
local optimization.

**More primitives.** Cylinder, cone, torus. Triangle is already in.
Each new primitive is a struct + `Hittable` impl + a new `Shape` variant
+ `From` impl + match arm.

**More mesh formats.** PLY would be a clean addition (fits academic
test models like the Stanford bunny); the loader interface is already
shaped right — add a `load_ply` to `mesh.rs` that returns `Shape` the
same way `load_obj` does.

**Shadow-ray "any-hit" optimization.** `light_vector` currently uses
`nearest_hit` to test occlusion, but a shadow ray only needs to know
*whether* something is in the way, not what's nearest. Splitting the helper
into an `any_hit` variant that early-exits on the first occluder would
speed up shadow tests substantially, especially in scenes with many lights
or many objects. Independent of any other work; cleanly self-contained.

**Light types beyond point.** `Light` is currently a single struct. To
add directional lights (parallel rays from infinity, like the sun),
spotlights (cones), or area lights (sampled emitters for soft shadows),
turn `Light` into an enum following the same pattern as `Shape`. Each
variant gets its own `light_vector` arm; the rest of the pipeline doesn't
change. Area lights specifically open the door to soft shadows and
require multiple shadow-ray samples per shading point.

**Refraction / transparency.** Substantially more involved — requires Fresnel
equations, IOR per surface, and accounting for the medium the ray is
currently traveling through.

**Depth of field.** Aperture-based ray jittering at `camera_ray` time, with a
focus distance on the camera. The existing `oversample` loop is the right
place to integrate aperture sampling.

**Text-based scene definition language.** Mentioned in the README. The
current scene-definition style (Rust source, with `scene_objects!` and
constructor functions) is already pretty close to a DSL; a parser that
produces `Shape` values from text would slot in cleanly. The
`impl Into<Shape>` ergonomics would not survive a parser, but `Shape::from`
+ `scene_objects!` over runtime data does.

**Camera animation.** Now that `default_camera()` is a function returning a
fresh `Camera`, varying its parameters per frame is one new function call.
Render multiple frames, encode as video.

**Refactor: `Scene::objects: Vec<Shape>` → `Scene::root: Shape`.** The scene
is conceptually a top-level group; making it literally one would remove a
small special case in the renderer (top-level fold vs. recursive Group case).
Cosmetic, not load-bearing.

## Build / dev notes

- Edition: check `Cargo.toml`. The `#[macro_export]` + `use crate::macro;`
  idiom requires Rust 2018 or later.
- Dependencies: `image` for PNG output, `rayon` for parallelism. No math
  crate — everything is hand-rolled on `[f64; 3]` arrays via `geometry.rs`.
- This sandbox typically does not have `cargo` available, so cargo check /
  build / run must be done on the user's machine. Past sessions have caught
  compile issues by careful reading; verification is the user's job.
- Branch naming convention: recent feature work has been merged onto
  `ai-main`. The README mentions a `scene-definition-language` branch as
  a future direction.
