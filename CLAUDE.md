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
                         `OffsetTarget`, and `ProgressTarget` impls, AND
                         the parallel `HeatmapTarget` trait with
                         `PngHeatmapTarget` and `OffsetHeatmapTarget`. The
                         renderer pushes finished rows into a target rather
                         than returning an image, and (optionally) per-pixel
                         timings into a heatmap target; `image` crate use is
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
    Bounded(Box<Bounded>),             // AABB-accelerated subtree
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

`Bounded::hit_test` does a cheap boolean ray-AABB test first (slab method,
no normal/distance computation); if the ray misses the box the entire
subtree is skipped without recursing. This is the BVH primitive — both
the single-level "wrap a mesh in bounded()" usage and any future
multi-level BVH built by `bvh(...)` compose out of `Bounded(Group(...))`
nodes.

`Shape::bounds() -> Option<AABB>` returns the smallest AABB enclosing a
shape, or `None` if the shape is genuinely unbounded. Plane returns
`None` (infinite); Group returns `None` if any child is unbounded;
Transform transforms the eight corners of the child's local AABB by the
cached forward affine and takes the AABB enclosing the result (a
conservative bound — not the tightest possible for shapes other than
boxes, but always sufficient). Used by `bounded(...)` to auto-compute
bounds, and useful directly for visualization via `AABB::to_cuboid(surface)`.

The `Transformed` struct caches the forward affine, the inverse affine,
and a precomputed inverse-transpose `Mat3` for normal transformation.
The forward is used by `Shape::bounds()` to compute world-space bounds
from the child's local-space AABB (transform the 8 corners, take min/max);
hit-testing only uses the inverse and `normal_xform`. Its hit_test:

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
   pub fn bounded(child: impl Into<Shape>) -> Shape;
   pub fn bounded_with(bounds: AABB, child: impl Into<Shape>) -> Shape;
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

10. **Per-pixel render-time heatmap.** Second observability piece. New
    `HeatmapTarget` trait alongside `RenderTarget`, with `PngHeatmapTarget`
    (Mutex-protected `Vec<u32>` of nanoseconds per pixel; `save(path)`
    normalizes against the 99th percentile of timings via
    `select_nth_unstable`, clamping the brightest 1% to white so a few
    outlier pixels don't wash the rest of the image to black, and writes
    a single-channel grayscale PNG) and `OffsetHeatmapTarget` (same role
    and convention as `OffsetTarget` — does not propagate `finish()`
    because multiple offsets share one inner). `render()` gained an
    `Option<&dyn HeatmapTarget>` parameter; when `Some`, it brackets
    each `pixel_color` call with `Instant::now()` and submits per-row
    timing arrays. Pixels exceeding `u32::MAX` ns saturate via
    `try_from` rather than wrapping silently. The disabled
    (`heatmap = None`) path branches outside the per-pixel loop so the
    no-heatmap case has zero per-pixel overhead — same machine code as
    before this feature existed. `main.rs` now creates a
    `PngHeatmapTarget` alongside the `PngTarget` and saves the
    accompanying file as `render-heatmap.png`. Color mapping below the
    99th percentile is selectable via a `HeatmapScale` enum passed to
    `save`: `Linear` for direct proportional brightness, `Log` for
    `ln(1 + t) / ln(1 + cutoff)` which compresses the bright end and
    brings out gradient detail in the body of the distribution.
    `main.rs` currently uses `Log` since scenes mixing a teapot with
    cheap primitives are still heavy-tailed even after percentile
    clamping; flip to `Linear` to see direct proportional brightness.

11. **BVH primitive: `Bounded` variant + `Shape::bounds()`.** Phase 1 of
    BVH support: a new `AABB` type in `shapes.rs` (separate from the
    renderable `Cuboid` despite identical geometry — `AABB::intersects`
    is a boolean slab test, faster than `Cuboid::hit_test` because it
    skips normal/distance/surface computation), a `Shape::Bounded`
    variant that does the AABB test before recursing into its child,
    and a `Shape::bounds() -> Option<AABB>` method covering every
    variant. Plane returns `None` (genuinely infinite); Group returns
    `None` if any child is unbounded. Constructors: `bounded(child)`
    auto-computes the bound (panics on unbounded child) and
    `bounded_with(bounds, child)` lets callers reuse a precomputed bound
    for both acceleration and visualization. `AABB::to_cuboid(surface)`
    is the visualization bridge: turns a bound into a renderable
    `Cuboid` for diagnosing bounds. `scene_teapot` was updated to wrap
    the loaded mesh in `bounded(...)`.

12. **Transformed bounds (BVH phase 3).** `Transformed` now caches the
    forward affine alongside the inverse and `normal_xform` (a single
    `Affine`-by-value field, ~96 bytes per Transform node).
    `Shape::Transform::bounds()` transforms the eight corners of the
    child's local-space AABB by the forward affine and takes the
    enclosing world-space AABB. The bound is conservative — for shapes
    other than boxes (especially rotated spheres), it can be looser
    than the optimal world-space bound, but it is always a valid
    enclosing bound, which is what correctness requires. Lets scenes
    wrap whole transformed subtrees in `bounded(...)`, which is
    slightly faster than wrapping inside the transforms because rays
    that miss the world-space AABB skip the per-Transform inverse-ray
    work entirely. `scene_teapot` updated to demonstrate this form.
    Phase 2 (the BVH builder) deferred — manual annotation is fine
    for the model sizes this codebase handles.

13. **SDL phase 1: language core.** New `src/sdl/` module tree implements
    a small Clojure-subset Lisp: `reader.rs` (tokenizer + s-expression
    reader with source positions), `ast.rs`, `value.rs`, `env.rs`
    (parent-linked `Rc<RefCell<HashMap>>`), `eval.rs` (tree walker,
    special forms `def`/`let`/`fn`/`if`/`do`/`quote`/`recur`/`and`/`or`,
    function application with a recur loop, vector-pattern destructuring
    in `let` and `fn`), `builtins.rs` (arithmetic + comparison with
    int/float promotion, vector and map ops, predicates, `assert` /
    `assert=`), `error.rs` (position-tagged panics via `sdl_panic!`).
    A small `sdl_run` binary in `src/bin/` evaluates ad-hoc scripts.
    Verification is a per-file test convention departing from the
    rest of the codebase: each `tests/sdl/<topic>.lisp` script becomes
    a `#[test]` via the `sdl_test!` macro in `tests/sdl_suite.rs`,
    plus an `all_scripts_have_a_test` guard that catches drift between
    the on-disk files and declared tests. To make the SDL crate
    consumable from outside the binary, `src/lib.rs` was introduced
    exposing `pub mod sdl;` initially, then expanded to also expose
    `pub mod render;` and `pub mod scenes;` when phase 2 began (see
    next entry).

14. **SDL phase 2: host bindings.** Script-callable constructors for
    every ray tracer host type live in `src/sdl/bindings.rs` and get
    installed into the default environment alongside the Phase 1
    built-ins. New `Value` variants — `Surface`, `Camera`, `Affine`,
    `Aabb` (Copy types, inlined) and `Light(Rc<...>)`, `Shape(Rc<...>)`,
    `Scene(Rc<...>)` — make host values first-class in the language,
    structurally comparable via `assert=` (the new variants reuse the
    `PartialEq` derives added to the host structs). Multi-field
    constructors are map-keyed: `(surface {:color [...] :ambient n
    :specular n :light n :checked b :reflection n})`,
    `(sphere {:center [...] :r n :surface S})`, `(plane {:normal [...]
    :p0 [...] :surface S})`, `(cuboid {:center [...] :size [...]
    :surface S})`, `(triangle {:vertices [...] :normals [...]
    :surface S})` (`:normals` optional — falls back to the geometric
    face normal), `(cylinder {:p0 [...] :p1 [...] :r n :surface S})`,
    `(scene {:name "..." :camera C :background [...] :lights [...]
    :objects [...] :reflect-limit n :oversample n})`. Single-purpose
    constructors are positional: `(light-white p)`, `(light-point p
    c i)`, `(camera-looking-at loc look up zoom)`, `(camera-with-fov
    loc look up fov)`, `(translate d s)`, `(scale s s)`, `(rotate-x
    θ s)` / `-y` / `-z`, `(rotate-axis axis θ s)`, `(transform a s)`,
    `(group [s1 s2 ...])`, `(bounded s)`, `(bounded-with aabb s)`. The
    `Affine` and `AABB` types are exposed via their own constructors:
    `(affine-identity)`, `(affine-translation d)`, `(affine-scale s)`,
    `(affine-rotation-x θ)` / `-y` / `-z`, `(affine-rotation-axis a θ)`,
    `(affine-compose a b)`, `(affine-inverse a)`, `(aabb min max)`.
    Type predicates `surface?` / `camera?` / `affine?` / `aabb?` /
    `light?` / `shape?` / `scene?` complete the surface area. Numbers
    are accepted as int or float and coerced — scripts can write
    `:r 1` instead of `:r 1.0`. `Scene::name` was changed from
    `&'static str` to `String` so script-built scenes carry runtime
    names without leaking memory; existing scene literals in
    `scenes.rs` use `.to_string()`. New tests in
    `tests/sdl/bindings_*.lisp` (one per surface / lights / camera /
    leaf shapes / transforms / scene) verify construction works,
    type predicates classify correctly, and equivalent-input scenes
    compare structurally equal.

15. **SDL phase 3: render dispatch.** Closes the loop — a script can
    now drive a render to disk without touching Rust. New
    `Value::Target(Rc<SdlTarget>)` variant, with `SdlTarget` (in
    `src/sdl/target.rs`) wrapping an `Arc<dyn RenderTarget>` for the
    renderer-facing handle plus an optional `Arc<PngTarget>` for
    `save-png`. The Arc wrapper is what makes runtime composition
    work: the host `OffsetTarget` and `ProgressTarget` are
    lifetime-parameterized for stack-allocated quadrant compositing
    in `main.rs`, which doesn't fit a heap-allocated SDL value.
    `src/render/output.rs` gained two owned counterparts —
    `ArcOffsetTarget` and `ArcProgressTarget` — that take
    `Arc<dyn RenderTarget>` instead of a borrowed reference; same
    `submit_row`/`finish` semantics as the existing borrowed forms.
    `PngTarget::save` was changed from `self`-consuming to `&self`
    (it's a thin shim over `image::ImageBuffer::save`, which already
    takes `&self`); this keeps the SDL ownership model simple and
    main.rs's call site is unaffected. Bindings:
    `(png-target w h)` constructs a fresh PNG buffer;
    `(offset-target inner dx dy)` and
    `(progress-target inner total-rows label)` wrap an existing
    target, propagating the optional `PngTarget` handle so
    `save-png` keeps working through wrappers;
    `(render scene target w h)` drives an end-to-end parallel render
    and returns the target so calls can be chained;
    `(save-png target path)` writes the accumulated buffer to disk.
    Equality on `Value::Target` is `Rc` pointer identity (matching
    `Fn`) since targets are stateful and structural equality
    wouldn't be meaningful. Tests:
    `tests/sdl/render_dispatch.lisp` exercises constructors,
    predicates, and `(render ...)` without touching disk; a separate
    Rust-side `render_dispatch_save` test in `tests/sdl_suite.rs`
    injects an `OUTPUT-PATH` binding into the env, evaluates an
    inline script that calls `(save-png ...)`, and verifies the
    resulting file is a 16×16 PNG with at least one lit pixel near
    the center.

16. **SDL phase 4: stdlib and ergonomics.** In-language conveniences
    on top of Phase 1 primitives, split between Rust and a small
    bundled lisp file. New special forms in `src/sdl/eval.rs`:
    `cond` (test/expr pairs, returns first truthy match's expr or
    nil), `when` / `when-not` (gated implicit-do), `->` /
    `->>` (thread-first / thread-last, rewriting each subsequent
    form's first or last argument slot — implemented as special
    forms because the SDL has no macros). New built-in functions in
    `src/sdl/builtins.rs`: HOFs `map`, `filter`, `reduce` (2- and
    3-arity), `range` (1/2/3-arity), `repeat`, and `apply` (with
    Clojure's variadic shape: leading positional args + trailing
    spread vector). All HOFs go through `eval::apply` so they work
    uniformly over native and interpreted callables. Math helpers:
    variadic `min` / `max` (preserve int-ness when every arg is
    int), `abs` (preserves int-ness), `sqrt` / `sin` / `cos` /
    `tan` (always return float). New `src/sdl/stdlib.lisp`,
    bundled into the binary via `include_str!` and evaluated by
    `default_env` after the Rust built-ins and host bindings, holds
    the lisp-side conveniences: `pi`, `tau` constants;
    `deg->rad` / `rad->deg`; `point` / `x` / `y` / `z` accessors;
    component-wise point arithmetic `p+` / `p-` / `p*` (scalar
    multiply). Tests:
    `tests/sdl/control_flow.lisp` extended with `when` /
    `when-not` / `cond` cases (including laziness checks); new
    scripts `threading.lisp`, `hofs.lisp`, `math.lisp`,
    `points.lisp`. Phase ends here: scripts have enough leverage
    to write idiomatic scenes without dropping back to Rust for
    common idioms.

17. **SDL phase 5: ported scene + visual-equivalence harness.**
    First "real" scene written entirely in the SDL plus a
    byte-equality harness that pins the SDL pipeline against the
    canonical Rust version. New top-level `scenes/` directory holds
    portable scene definitions; the first inhabitant is
    `scenes/transform_test.lisp`, a port of `scene_transform_test`
    from `scenes.rs`. The script is *pure data* — it `def`s
    `transform-test-scene` to a `Value::Scene` and stops there, no
    `(render ...)` call — so it's safe to evaluate from anywhere
    without side effects, and the Rust harness drives both sides of
    the comparison itself. Idiomatic surface area: a script-side
    `(def glossy (fn [color] (surface {...})))` plays the role of
    `surface_glossy` from `scenes.rs`, named `surface-red` /
    `-green` / etc. plus a separate `surface-white-c` for the
    reflective checkered ground; the camera is built once via
    `(def default-camera (camera-looking-at ...))`; rotations use
    `(/ pi N)` against the stdlib's `pi`. New
    `phase5_transform_test_scene_matches_rust` test in
    `tests/sdl_suite.rs` reads the script via
    `CARGO_MANIFEST_DIR/scenes/transform_test.lisp`, evaluates it in
    a `default_env`, looks up `transform-test-scene`, renders both
    that and `scene_transform_test()` to 64×64 `PngTarget`s with
    `parallel = false`, saves and decodes each, and walks the
    buffers asserting per-channel equality (`TOLERANCE = 0`).
    Failure preserves both PNGs on disk and surfaces their paths in
    the panic so `open` produces a side-by-side diff for debugging.
    The pipelines run identical math on identical inputs (same
    `Surface` field values, same `Camera::looking_at` arguments,
    same transform composition order, identical f64 representation
    of `pi`), so a divergence is a bug in the binding layer or the
    port — not floating-point drift. Phase ends here: the SDL is
    pleasant enough to write a real scene in, and a regression in
    any binding immediately breaks a fast test.

18. **SDL phase 6: scene-port parity (no-mesh subset).** Every scene
    in `src/scenes.rs` except `scene_teapot` now has a parallel
    SDL definition in `scenes/`, each pinned by a
    `phase6_<name>_scene_matches_rust` test in `tests/sdl_suite.rs`.
    The eventual goal is to delete the Rust scene definitions
    entirely; this phase is the parity step, the teapot port (which
    needs a `load-obj` mesh binding) is the next step, and removal
    follows that. Three pieces of supporting infrastructure landed
    alongside the ports:
      * **`mod` and `quot` builtins** in `src/sdl/builtins.rs`. Both
        are int-preserving when both args are ints, float-promoting
        otherwise, and panic on division by zero. `mod` follows
        Clojure semantics (sign of result matches sign of divisor),
        distinct from Rust's `%` which matches the dividend.
        `quot` truncates toward zero. Together they let
        `scenes/sphere_surface_test.lisp` express the original
        `(0..25).map(|x| ...)` 5×5 grid generator from `scenes.rs`
        without dropping back to a hand-rolled list. Tests live in
        `tests/sdl/math.lisp` (extended).
      * **`(load <path-expr>)` special form** in `src/sdl/eval.rs`.
        Reads the file at the resolved path and evaluates each
        top-level form in the *current* environment (the
        binding-into-caller-scope semantic is what required it to
        be a special form rather than a builtin — `NativeFn` has no
        `env` parameter). Path resolution is relative to the
        directory of the loading file via a thread-local
        `CURRENT_DIR` set by an RAII guard in
        `crate::sdl::eval_source`, with absolute paths used as-is.
        `tests/sdl_suite.rs::run_script` was updated to pass the
        absolute script path to `eval_source` so the load form
        works under the test harness too. Tests:
        `tests/sdl/load_form.lisp` plus `load_form_fixture.lisp`
        (the loaded helper, also a standalone passing test).
      * **`scenes/_common.lisp`** — single source of truth for the
        surface coefficients (`ambient`/`specular`/`light`), the
        `glossy` and `reflective` helpers, the `surface-*` presets,
        and `default-camera`. Every scene file starts with
        `(load "_common.lisp")` and pulls these into its env.
        Underscore-prefixed filename signals "not a standalone
        scene"; the test harness only references the unprefixed
        files. The harness itself was refactored: the per-scene
        90-line equivalence test from Phase 5 collapsed into a
        single `assert_sdl_scene_matches_rust` helper, with one
        `#[test]` per ported scene calling it with three arguments
        (script relpath, binding name, Rust scene fn). Default
        comparison is byte-equal at 64×64 with `parallel = false`,
        same conventions as Phase 5.

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

## Scene definition language: implementation plan

The next major piece of work is a script-driven layer for building and
rendering scenes. This section captures the design and a phased delivery
plan; once each phase ships, its summary moves into "Recent work history"
and the corresponding plan content here is trimmed.

### Goals and shape

The SDL is **lower-level than a POV-Ray-style declarative scene file**.
A script owns control flow: it constructs surfaces, lights, cameras, and
geometry, builds a render target, and explicitly calls `render`. This is
what enables the existing four-quadrant style of output, and eventually
animation — where a script builds geometry once and drives it across a
frame loop into a streaming target.

The language is a small Clojure-subset Lisp:

- **Syntax:** s-expressions. `;` line comments. The reader supports
  Clojure-style literals — vector `[...]`, map `{...}`, keyword `:foo`,
  string `"..."` — and the `'x` reader macro for `(quote x)`. No other
  reader macros.
- **Types:** `Int (i64)`, `Float (f64)`, `Bool`, `String`, `Keyword`,
  `Symbol`, `Vec` (`Rc<Vec<Value>>`), `Map` (`Rc<HashMap<Key, Value>>`),
  `Nil`, `Fn` (interpreted or native), plus the ray tracer's host types
  as enumerated `Value` variants. No seq abstraction, no full numeric
  tower (auto-promotion in arithmetic only), no rationals or bignums.
- **Special forms:** `def`, `let`, `fn`, `if`, `do`, `quote`, `recur`.
- **Evaluation:** eager, single-threaded, downward closures only via
  parent-linked `Rc<RefCell<Environment>>`.
- **Memory:** reference counting via `Rc`. Process lifetime is short.
- **Ergonomics:** vector destructuring in `let` bindings and `fn`
  parameter lists, including nesting (essential for 3D math). AST nodes
  carry source positions for line/column error reporting.
- **Error model:** panics on script errors with a reported source
  position, matching the rest of the codebase. No `Result` plumbing
  through the interpreter.
- **Skipped:** macros, dynamic vars, namespaces, multimethods,
  protocols, lazy seqs, transducers, varargs, atoms.

### Module layout

A new top-level module `sdl` alongside `render`. Approximate breakdown:

```
src/sdl/
  mod.rs       Re-exports and the public entry point: read + eval a file.
  reader.rs    Tokenizer + s-expression reader producing AST with source positions.
  ast.rs       AST node definitions (literal, symbol, list, vector, map, ...).
  value.rs     The runtime Value enum, including host-type variants.
  env.rs       Environment: parent-linked Rc<RefCell<HashMap>>.
  eval.rs      Evaluator: dispatch on AST node type, special forms, apply.
  builtins.rs  Pure-language built-in functions (arithmetic, vec, map, etc.).
  bindings.rs  Native function bindings to the ray tracer API.
  target.rs    SdlTarget — Arc-wrapped render-target value for the SDL.
  stdlib.lisp  In-language standard library, bundled via include_str!.
  error.rs     Error type with source positions; pretty printer.
```

The `render` module's public API is unchanged in shape; the SDL is a
layer above it. Phase 3 added two owned counterparts to existing
output types (`ArcOffsetTarget`, `ArcProgressTarget`) and relaxed
`PngTarget::save` from consuming to borrowing — both additive
changes.

### Test suite

A unit-test convention is introduced specifically for the SDL. This is a
deliberate departure from the rest of the codebase, which is verified
visually. The interpreter's correctness is too detailed to verify by render
comparison, and the language is too small not to test thoroughly.

- Test scripts live in `tests/sdl/` with one file per topic (e.g.
  `arithmetic.lisp`, `let_destructuring.lisp`, `recur.lisp`,
  `closures.lisp`).
- Each script uses `(assert <expr>)` and `(assert= <actual> <expected>)`,
  which are built-in forms. A failed assertion panics with the source
  position.
- A Rust integration test (`tests/sdl_suite.rs`) walks the directory,
  evaluates each file in a fresh interpreter, and fails the test run if
  any script panics.
- Each phase grows the suite. A phase is "done" when its tests pass and
  prior phases' tests still pass.

For Phase 3 onward, tests additionally cover constructor output (build a
value from script, debug-format it, compare to an expected snapshot) and
end-to-end rendering (render a known scene to a temp path; assert file
existence, dimensions, and a handful of pixel values).

### Phases

**Phase 1 — Language core (no host bindings).** Done; see "Recent work
history."

**Phase 2 — Host bindings: scene construction.** Done; see "Recent work
history."

**Phase 3 — Render dispatch.** Done; see "Recent work history."

**Phase 4 — Standard library and ergonomics.** Done; see "Recent work
history."

**Phase 5 — Port a real scene.** Done; see "Recent work history."

**Phase 6 — Scene-port parity (no-mesh subset).** Done; see "Recent
work history." Every scene in `scenes.rs` except `scene_teapot` has
a parallel `.lisp` definition pinned by an equivalence test. Phase 6
also delivered the supporting `mod`/`quot` builtins and the
`(load ...)` special form.

**Phase 7 — `load-obj` mesh binding + teapot port.** The last
ingredient blocking SDL parity with `scenes.rs`. Add
`(load-obj <path-string> <surface>)` as a new SDL host binding
calling through to `crate::render::mesh::load_obj`, which returns a
`Shape::Group` of triangles. Path resolution should match
`(load ...)` (relative to the current file via `CURRENT_DIR`).
Then port `scene_teapot` to `scenes/teapot.lisp` and add an
equivalence test. The `models/teapot.obj` file is already on disk
but not committed — same setup the Rust scene assumes.

**Phase 8 — Remove `src/scenes.rs` (and the surrounding plumbing).**
Once Phase 7 lands and every scene in `scenes/*.lisp` is verified
byte-equal to its Rust counterpart, delete `src/scenes.rs`,
`pub mod scenes;` in `src/lib.rs`, the corresponding imports in
`src/main.rs`, and the `assert_sdl_scene_matches_rust` helper that
becomes unanchored. `main.rs` will need its own way to drive the
SDL scenes (probably a small loop that loads each `scenes/*.lisp`
and calls `(render ...)` from script — or extracts each scene
binding and renders from Rust). The `scene_objects!` macro becomes
unused once `scenes.rs` is gone (the .lisp files reach the host
shape constructors via the SDL bindings, not the macro), so it
can be deleted too.

**Phase 9+ (deferred).** Heatmap target binding; animation
(timestep loops, a video or sequence-of-PNGs target, per-frame
mutation of geometry); interpreter optimizations.

### Decisions still open

To be settled when each phase begins, not committed to in this plan:

- File extension for SDL scripts: settled on `.lisp` for now (matches
  the test suite); revisit if the SDL grows enough to deserve its own
  extension.

Settled in earlier phases:

- `Value` carries host types directly: small Copy types (`Surface`,
  `Camera`, `Affine`, `Aabb`) inline, larger ones (`Light`, `Shape`,
  `Scene`) wrapped in `Rc` for cheap cloning. `Shape` derives Clone
  so binding-side extraction can deep-clone through the `Rc` when the
  host constructor needs ownership.
- Multi-field constructors are map-keyed; single-purpose constructors
  are positional. Map keys are keyword-only.
- `Scene::name` is `String` so script-built scenes carry runtime
  names; existing scene literals use `.to_string()`.
- Scripts can be either pure-data (define a Scene, return) or
  side-effecting (call `(render ...)` / `(save-png ...)`). Phase 5
  used pure-data so the test harness owns the render call and can
  compare against the Rust pipeline byte-for-byte; production usage
  is expected to be side-effecting (rendering then saving).
- Portable scenes live at `scenes/<name>.lisp` at the repo root
  (mirroring `models/` for OBJ files). The `tests/sdl/` directory
  remains for language unit-test scripts that use `(assert ...)`;
  scenes are a separate concern even though they share the `.lisp`
  extension and the test harness reads them.

## Future directions

The README's own "Potential Futures" list overlaps these but is now somewhat
out of date.

**Performance: real BVH (bounding-volume hierarchy).** Phases 1 and 3 of
BVH support are in: `Shape::Bounded` is the wrapper that does a ray-AABB
test before recursing, `bounded(...)` auto-computes the bound, and
`Shape::Transform` correctly bounds itself by transforming the child's
eight AABB corners. What's missing is a BVH *builder*: a
`bvh(children: Vec<Shape>) -> Shape` function that recursively splits a
flat list of children into a balanced tree of `Bounded(Group(...))`
nodes. Standard splitting heuristic is "median split along the longest
axis" — find the axis with the largest spread of centroid positions,
sort by centroid on that axis, split at the median, recurse until
leaves are small enough (typically 4–8 items). That turns the teapot
from O(n) per ray into O(log n), which is what makes large meshes
pleasant to render. Manual annotation (wrapping a known mesh in
`bounded(...)`) is the workaround until then; for the kinds of models
this codebase deals with, that's tractable.

**Transform collapsing.** A nested `translate(rotate(scale(leaf)))` produces
three separate `Transform` nodes, each doing its own ray-transform on the way
down. `transform()` could peek at its child and, if it's already a
`Shape::Transform`, multiply the inverse affines and skip a level. Trivial
local optimization. **Explicitly on the radar** — flagged after Phase 6 as
the next perf item to tackle once SDL parity is complete and the Rust
scene definitions are gone (deeper transform stacks will be more common
as SDL scenes get richer, since the SDL constructor functions don't
currently fold). Work item lives here in the plan rather than in
"Phases" because it's a self-contained optimization, not a sequenced
SDL milestone.

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

**Scene definition language.** See the dedicated "Scene definition
language: implementation plan" section above — this is the next major
piece of work, and the design and phasing are captured there rather than
in this list.

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
