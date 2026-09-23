# Project Notes for Claude

A small ray tracer written in Rust. The README frames it as a Rust-learning
exercise; this document is the operating manual for working on it productively
in a Claude-assisted session.

## What this is

A CPU ray tracer that renders simple scenes of analytic primitives (spheres,
planes, axis-aligned boxes) into PNGs. It supports ambient/diffuse/specular
shading, hard shadows, mirror reflections, hierarchical scene composition with
affine transforms, and a look-at camera. Rendering is parallelized with Rayon.

The binary takes a single scene file path on the command line and writes
`render.png` (and `render-heatmap.png`, `render-samples.png`) in the
current directory:

```
cargo run --release -- scenes/teapot.lisp
```

The binding looked up inside the script is derived from the filename:
`cuboid_test.lisp` → `cuboid-test-scene` (file stem with `_` → `-`, plus
the `-scene` suffix). Every scene in `scenes/` follows this convention.
`SIZE=N` or `SIZE=WxH` overrides the default 1024×1024 output; `PARALLEL=n`
disables Rayon; `RTVIEW_ADDR=host:port` streams pixels to a live receiver
instead of writing `render.png`.

## Module layout

```
src/
  main.rs              Entry point. Takes a single SDL file path on
                       the command line, derives the canonical
                       `<stem>-scene` binding from the filename, and
                       renders the scene to render.png alongside two
                       diagnostic heatmaps (render-heatmap.png for
                       per-pixel render time, render-samples.png for
                       per-pixel adaptive-sample count). Reads SIZE,
                       PARALLEL, and RTVIEW_ADDR env vars. Multi-scene
                       compositing and animation are expected to move
                       into the SDL — see "Future directions."

  render.rs            Top-level render module. Defines:
                         - Scene, Camera, Light, Surface, RayHit
                         - Hittable trait
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
    render/poly.rs       Real roots of quadratics, cubics and quartics
                         (closed form, with Newton polishing for the
                         quartic). Used by the torus intersection.
    render/mesh.rs       Wavefront OBJ loader (`load_obj`). Returns a
                         `Shape::Group` of `Shape::Triangle`s, so loaded
                         meshes integrate with the rest of the scene tree
                         without any special-casing.
    render/output.rs     The `RenderTarget` trait plus the `PngTarget`,
                         `OffsetTarget`, and `ProgressTarget` impls, AND
                         the parallel `HeatmapTarget` trait (one method,
                         `submit_metric_row(x, y, &[u32])`, where the
                         metric's meaning — render time, sample count,
                         anything else — is determined by the renderer
                         slot it's fed through) with `PngHeatmapTarget`
                         and `OffsetHeatmapTarget`. The renderer pushes
                         finished rows into a target rather than returning
                         an image, and (optionally) per-pixel diagnostic
                         metrics into one or more heatmap targets via the
                         `HeatmapTargets` struct; `image` crate use is
                         fully encapsulated here.

  sdl/                 The scene definition language. See the
                       "Scene definition language" section below for
                       a full module breakdown. Phase 8 deleted the
                       older `src/scenes.rs` once every scene had a
                       parallel `.lisp` definition; the SDL is now
                       the canonical scene-definition mechanism.
```

Scene definitions live at the repo root:

```
scenes/
  _common.lisp         Surface coefficients (ambient/specular/light), the
                       glossy/reflective/glassy/metallic helpers, surface-*
                       presets (including the surface-gold/-silver/-copper
                       metals), and default-camera. Loaded by every scene
                       file via (load "_common.lisp"). Underscore prefix
                       signals "not a standalone scene."

  axis_spheres.lisp    Each scene file defines a single `<name>-scene`
  ball_on_plane.lisp   binding (Value::Scene). main.rs derives the
  cuboid_test.lisp     binding name from the script's filename (e.g.
  cylinder_test.lisp   `cuboid_test.lisp` → `cuboid-test-scene`);
  group_test.lisp      tests/sdl_suite.rs has a smoke test per file
  moravian_star.lisp   that verifies the script evaluates and
  multi_light_test.lisp produces a Scene.
  one_sphere.lisp
  sphere_occlusion_test.lisp  teapot.lisp uses (load-obj ...) to pull
  sphere_surface_test.lisp    in models/utah_teapot.obj — that file
  teapot.lisp                 isn't committed; the smoke test
  transform_test.lisp         gracefully handles its absence.
```

## The Shape enum

Central abstraction. Closed enumeration, no dynamic dispatch:

```rust
pub enum Shape {
    Sphere(Sphere),
    Plane(Plane),
    Cuboid(Cuboid),                    // axis-aligned box, slab method
    Triangle(Triangle),                // Möller–Trumbore, smooth normals
    Cylinder(Cylinder),                // closed cylinder, body + caps
    Cone(Cone),                        // closed cone, lateral surface + base cap
    Torus(Torus),                      // solid ring torus (quartic)
    Group(Vec<Shape>),                 // hierarchical container
    Transform(Box<Transformed>),       // affine-transformed subtree
    Bounded(Box<Bounded>),             // AABB-accelerated subtree
    Surfaced(Box<SurfacedShape>),      // default surface for a subtree
    Csg(Box<Csg>),                     // difference / intersection / merge of solids
    Light(Light),                      // positioned light source (invisible)
}
```

`Csg { op: CsgOp, a, b }` is a binary difference or intersection of two
*solids* (`Shape::is_solid`: everything except triangles, and so
meshes). It works on **spans**, the intervals along a ray where the
ray is inside a solid. `Shape::spans` produces them for every variant:
a `Plane` counts as a half-space, and a `Group` as a union. The
operands' span lists are combined with interval set operations, and
`hit_test` returns the first boundary in front of the ray. See "CSG:
implementation plan".

`Hittable for Shape` is a single match dispatching to per-variant logic.
`Sphere`/`Plane`/`Cuboid` implement `Hittable` with the standard analytic ray
tests. `Triangle` uses Möller–Trumbore and interpolates per-vertex normals
via the barycentric coordinates returned by the test (smooth shading falls
out for free; flat shading is the same algorithm with all three vertex
normals equal). `Cylinder` and `Cone` are analytic closed solids:
`Cylinder` tests the curved side plus two end-cap disks; `Cone` tests
the curved lateral surface (a quadratic in the `cos²θ` cone equation,
with an `s ≥ 0` check that rejects the infinite double cone's second
nappe) plus a single base-cap disk — `Cone`'s `p0` is the base center
of radius `r` and `p1` is the apex point, and unlike `Cylinder` the two
ends are not interchangeable. `Group::hit_test` is
`nearest_hit(ray, &children)` — same fold the top-level scene traversal
uses, so flat scenes and arbitrarily-nested groups share the exact same
hit-testing path.

`Light::hit_test` returns `None`: lights are invisible to every kind of
ray (primary, shadow, reflection). They occupy a position in the scene
graph for the sake of being affected by enclosing transforms; the renderer
collects them up-front via `Shape::collect_lights` rather than reaching
them through hit-testing. See the "Lights" section below.

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
boxes, but always sufficient). Light returns a degenerate point AABB at
`light.location` (`min == max`) so that Group/Bounded composition stays
well-defined; lights aren't hit-tested so the "useless" bound has no
runtime consequence. Used by `bounded(...)` to auto-compute bounds, and
useful directly for visualization via `AABB::to_cuboid(surface)`.

`Shape::collect_lights(&Affine, &mut Vec<Light>)` walks the tree and
pushes every `Shape::Light` leaf's world-space `Light` into the output
vec. Affines accumulate through `Transform` nodes via
`world_from_local.compose(t.forward)`; `Group` and `Bounded` recurse
into their children with the same affine (Bounded *unconditionally* —
the AABB early-out is per-ray work that would just hide lights with
degenerate bounds for no benefit). The renderer calls this once at
`render()` entry to build a flat `Vec<Light>` for shading; details
under "Lights" below.

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

## Constructing scenes

Scenes are written in the SDL — see `scenes/*.lisp` for examples and
"Scene definition language" below for the language and bindings. The
underlying Rust constructors are still public on `crate::render::shapes`
for direct use:

- `From<T> for Shape` for each leaf type (`Sphere`, `Plane`, `Cuboid`,
  `Cylinder`, `Triangle`). Auto-promotes a leaf primitive to the `Shape`
  enum variant; reflexive `From<Shape> for Shape` means `Shape::from(s)`
  works uniformly.
- Constructor functions in `shapes.rs` for the composite/transformed
  variants, each accepting `impl Into<Shape>` so leaves and existing
  shapes both work as the child argument:

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
  to a single composed transform without anyone having to think about matrix
  multiplication order. The bare `transform(matrix, child)` is the escape
  hatch for hand-built `Affine` values via
  `Affine::translation(...).compose(...)` etc.

The SDL bindings call these constructors under the hood — `(translate ...)`,
`(rotate-z ...)`, `(group ...)`, `(bounded ...)`, etc. all map directly to
the Rust functions above with the same composition semantics.

## The Camera

Standard look-at model with a precomputed orthonormal basis, plus an
optional thin-lens aperture for depth of field:

```rust
pub struct Camera {
    pub location: Point,
    pub forward: Point,        // unit
    pub right: Point,          // unit
    pub up: Point,             // unit, re-orthogonalized from up_hint
    pub half_height: f64,      // half of view-plane height at unit distance
    pub aperture_radius: f64,  // 0.0 = ideal pinhole; >0 = depth-of-field blur
    pub focus_distance: f64,   // depth (along forward) that stays sharp
}
```

Built via `Camera::looking_at(location, look_at, up_hint, zoom)`,
`Camera::with_fov(location, look_at, up_hint, fov_radians)`, or
`Camera::with_dof(location, look_at, up_hint, zoom, aperture_radius)`.
The user's `up_hint` doesn't have to be perpendicular to `forward`; the
constructor projects out the parallel component. It will panic if
`up_hint` is *parallel* to `forward` (no orientation degree of freedom).

`zoom = 1.0` corresponds to vertical FOV ≈ 53° and matches the framing of the
older fixed camera. `with_fov` is a thin wrapper that converts to zoom and
forwards to `looking_at`.

`looking_at` and `with_fov` set `aperture_radius = 0.0` — an ideal
pinhole — and `focus_distance` to the `location`→`look_at` distance
(inert while the aperture is 0). `with_dof` is `looking_at` with the
aperture overridden: the look-at point is the focus plane, geometry
nearer or farther blurs by an amount that grows with
`aperture_radius`. `camera_ray` takes an explicit pinhole fast-path
branch when `aperture_radius == 0.0` that is bit-identical to the
pre-depth-of-field renderer; the thin-lens path aims every sub-pixel
sample's ray at the same focal point from a jittered origin on the
aperture disk. See "Depth of field: implementation plan."

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
              pixel_color   ← adaptive sample loop (min_samples..max_samples)
                              with Halton-(2,3) + CP rotation per
                              sample, batch-checked variance for early
                              termination — see `render::sampler` and
                              Scene::{min,max}_samples / variance_threshold.
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

`Surface { color, ambient, specular, light, checked, reflection, transparency,
metallic }`.
Lighting is Lambertian diffuse + Phong specular (50-power), with ambient as a
flat multiplier of the surface color and a single bounce of mirror reflection
(recursion gated by `Scene::reflect_limit`). The `checked` flag enables a
simple world-space checker pattern keyed off `floor(x+y+z)`.

`metallic` (bool, default `false`) flags a metal surface. The default
`false` is an ordinary dielectric — every pre-metallic scene renders
byte-identically. When `true`, `shade_pixel` reinterprets the existing
fields the way a metal behaves: the mirror reflection and the Phong
specular highlight are both tinted component-wise by the surface color
(a gold surface reflects gold-tinted rather than chrome-white, and has
gold highlights regardless of light color), and the Lambertian diffuse
term is suppressed entirely (metals have essentially no diffuse lobe,
so all their apparent color comes from the tinted reflection and
specular). A metallic surface is always opaque: the `!metallic` guard
on the transmission branch means `transparency` is ignored when
`metallic` is set. This is the simplified "metalness" workflow — one
base color drives body, reflection, and highlight — and reuses the
existing `reflection` field as the reflection strength. Rough/glossy
metal (scattered reflections, needing a `roughness` field and
per-sample reflection-ray jittering) is a deferred follow-on; so is
Fresnel.

`transparency` (0.0 = opaque, 1.0 = fully see-through) is the
transmission coefficient. `shade_pixel` computes the surface's opaque
shading (ambient + direct lighting + reflection), then — if the surface
is at all transparent — casts a *straight-through* transmitted ray
(same direction as the incoming ray, originating at the hit point) and
returns `lerp(opaque, transmitted, transparency)`. Transmission is
non-refractive: the transmitted ray does not bend, so geometry behind
a transparent surface shows up undistorted. Transmission recursion is
gated by `Scene::transmit_limit` — a separate budget from `reflect_limit`,
tracked by the independent `transmit` counter in the private `Depth`
struct that threads through `ray_color` / `shade_pixel`.

Shadow rays honor transparency too (Phase 2): `light_vector` walks the
shadow ray from the light to the shaded point with a
repeated-nearest-hit loop, multiplying a running *transmittance* by
each occluder's `transparency`. An opaque occluder zeroes it (full
shadow, early-out); transparent occluders attenuate it. `shade_pixel`
scales each light's contribution by that transmittance, so transparent
objects cast lightened shadows rather than solid ones. Transmittance
is greyscale, not tinted by the occluder's body color — consistent
with the untinted primary-ray transmission above; colored shadows are
deferred to land with colored transmission (see the refraction note in
"Future directions").

Surface presets and the `glossy` / `reflective` / `glassy` / `metallic`
constructor helpers live in `scenes/_common.lisp`. Common ones:
`surface-red`, `surface-green`, …, `surface-white-c` (the reflective
checkered ground used by most scenes), and the `surface-gold` /
`surface-silver` / `surface-copper` metal presets. Every
`scenes/<name>.lisp` file pulls these in via
`(load "_common.lisp")`.

## Lights

`Light { location, color, intensity, kind: LightKind }`. `color` is the
emitted color; `intensity` is a scalar multiplier. The two are
conceptually distinct knobs even though their numerical effect overlaps
— color is hue, intensity is brightness. `kind` carries variant-specific
data: as of Phase 4 of the "Light types: spotlights and area lights"
plan, `LightKind` is `Point` (omni-directional, the original behavior),
`Spot { direction, inner_angle, outer_angle }` (directed cone with
smooth falloff), or `Area { axis, radius }` (disk emitter, hard
shadows in Phase 4, soft shadows in Phase 5). The shared fields stay
on the struct (rather than turning `Light` itself into an enum)
because `shade_pixel` and `collect_lights` read `location` / `color` /
`intensity` directly — a pure enum would force accessor methods or
per-call-site `match` arms for fields every variant has.

Convenience constructors: `Light::white(location)` for full-intensity
white (matches the legacy implicit defaults), `Light::point(location,
color, intensity)` for the general case, `Light::spot(location,
direction, color, intensity, inner_angle, outer_angle)` for a
spotlight, and `Light::area(location, axis, radius, color, intensity)`
for a disk emitter. All four are `const fn`. The Rust constructors
assume caller-supplied direction/axis vectors are unit-length,
`inner_angle ≤ outer_angle`, and `radius > 0`; the SDL bindings
(`light-spot` / `light-area`) normalize the unit vectors and validate
the scalar constraints at the script boundary, so script-built lights
satisfy every invariant by construction.

Variant dispatch on `kind` lives in two places. `light_vector`
delegates to a per-kind helper that returns the `(Vector,
transmittance)` pair `shade_pixel` consumes:

- `light_vector_point` does the transmittance walk (Phase 2 of the
  transparency plan).
- `light_vector_spot` computes a cone falloff factor — `smoothstep(
  cos(outer_angle), cos(inner_angle), cos_theta)` where `cos_theta`
  is the dot product of the spotlight's `direction` with the unit
  light→point ray — *before* the walk, so a shaded point outside
  the outer cone returns `None` immediately without hit-testing.
  Inside the cone, it delegates to the point-light walk and folds
  the cone factor into the returned transmittance.
- `light_vector_area` computes a Lambertian cosine factor
  `max(0, dot(axis, normalize(point - light.location)))` against the
  disk's normal. Shaded points in the back hemisphere of the disk
  (`cos_theta ≤ 0`) return `None` immediately — the disk only
  illuminates the half-space its front face points into. Inside the
  lit half-space, the helper picks a per-pixel-sample point on the
  emitter disk (Phase 5), runs the shadow-ray walk from *that*
  point (not the disk center) toward the shaded point, and folds
  the cosine factor into the returned transmittance. Penumbra
  pixels — where some pixel samples reach the light and some don't
  — see high variance and the adaptive oversampler keeps sampling
  them until they stabilize; fully shadowed and fully lit pixels
  terminate at `min_samples`. The disk point is sampled via Halton
  bases (11, 13) plus a dedicated Cranley-Patterson rotation,
  decorrelated from the sub-pixel (bases 2, 3) and lens (5, 7)
  coordinates of the same sample index; the `concentric_disk` map
  in `render::sampler` turns the `[0, 1)²` value into a uniform
  unit-disk point, which is then scaled by `radius` and oriented
  into the disk's world-space plane by `disk_basis(axis)`.

The transparency behavior of intervening occluders is therefore
identical for all three light kinds (a glass pane attenuates a
spotlight or an area light the same way it attenuates a point
light), and `shade_pixel` stays light-type-agnostic: it just
multiplies its Lambert + Phong contribution by whatever scalar the
helper returned.

`Shape::collect_lights` walks the scene graph extracting world-space
lights. The variant arm transforms the per-variant geometric fields
under the accumulated affine: a spotlight's `direction` and an area
light's `axis` are both transformed by the linear part of the affine
and renormalized (translations don't apply to vectors, and non-
uniform scale can change a unit vector's magnitude). The spotlight's
`inner_angle` / `outer_angle` and the area light's `radius` are
unaffected — they're scalars, not vectors. (Uniform-scale-aware
radius scaling for area lights would be a Phase 6 ergonomics item,
same posture as Cylinder/Cone radii.)

Lights live inside `Scene::root` as `Shape::Light` nodes alongside
geometry. The renderer reaches them via `Shape::collect_lights`,
called once at `render()` entry against `scene.root` with the
identity affine; the result is a `Vec<Light>` of world-space lights
that gets threaded as `&[Light]` through `render_one_row` →
`pixel_color` → `ray_color` → `shade_pixel` (which iterates the
slice). `shade_pixel` sums each visible light's Phong specular
highlight and Lambertian diffuse contribution, both multiplied by
`light.color * light.intensity`. The diffuse term has the surface
color modulated component-wise by the light tint; the specular term
takes on the pure light color (a red light produces a red highlight
on any surface regardless of body color, which is physically right
for microfacet specularity). A scene with no lights renders ambient
+ reflection only — useful as a debug mode.

The point of `Shape::Light` is that lights inherit affine transforms
from enclosing `Shape::Transform` wrappers, the same way geometry
does. `(translate [5 5 5] (light-white [0 0 0]))` in SDL puts a
light at world `[5 5 5]`. Useful for two reasons: positioning lights
in the same coordinate system as the surrounding geometry (e.g. an
`(rotate-z θ (group [body lamp]))` rotates the "lamp" — geometry
plus its light — around the body); and a future diagnostic-imaging
pass that wants to render visible markers at light positions can
build them from the same `Shape::Light` nodes the renderer extracts
from.

At the SDL surface, `:objects` is a flat list that the scene
constructor wraps in `Shape::Group` to form `Scene::root`. Bare
`(light-white ...)` / `(light-point ...)` values auto-wrap into
`Shape::Light` at the binding boundary via `require_shape_value`, so
no explicit conversion is needed. The constructor rejects the
historical `:lights` key with a migration error so unmigrated scenes
fail loudly instead of silently dropping their lights.

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

19. **SDL phase 7: `(load-obj ...)` mesh binding + teapot port.**
    Closes the last gap before SDL parity with `scenes.rs`. New
    `(load-obj <path-string> <surface>)` host binding in
    `src/sdl/bindings.rs` calls through to
    `crate::render::mesh::load_obj`, returning a `Value::Shape`
    wrapping the `Shape::Group` of triangles. Path resolution
    matches `(load ...)`: relative paths join `CURRENT_DIR` (the
    loading file's directory), absolute paths used as-is — so
    scenes can write `(load-obj "../models/foo.obj" surface)`
    portably rather than depending on CWD. Positional arity
    (path, surface) rather than map-keyed since two args is
    obvious from order. Tests: a tiny
    `tests/sdl/load_obj_fixture.obj` with two flat triangles
    (no per-vertex normals, exercising the loader's
    geometric-normal fallback) and `tests/sdl/bindings_mesh.lisp`
    confirms the binding loads, the result is a Shape, and the
    Shape composes through `translate`/`scale`/`bounded` and
    into a renderable scene. The `.obj` lives under `tests/sdl/`
    alongside `.lisp` files; the
    `all_scripts_have_a_test` guard only walks `.lisp` extensions
    so the fixture doesn't trip it. New `scenes/teapot.lisp`
    ports `scene_teapot` end-to-end:
    `(bounded (translate [0 0 -2] (scale [0.5 0.5 0.5]
    (load-obj "../models/utah_teapot.obj" surface-blue))))`.
    Equivalence test
    `phase7_teapot_scene_matches_rust` uses a new
    `assert_sdl_scene_matches_rust_with` helper variant (32×32,
    `parallel = true`) — a 6000-triangle mesh at the default
    64×64 serial would dominate suite runtime, but the renderer
    is per-pixel deterministic so byte-equality still holds with
    parallel dispatch. The model file isn't committed (same
    posture as `main.rs`'s use of `scene_teapot`); the test
    skips with an `eprintln` when `models/utah_teapot.obj` is
    absent rather than failing. Phase ends here: every scene in
    `scenes.rs` now has a parallel `.lisp` definition pinned by
    an equivalence test, and the next step (Phase 8) is removing
    `scenes.rs` itself.

20. **SDL phase 8: SDL is the only scene-definition mechanism.**
    Deletes `src/scenes.rs` (10 hand-written Rust scenes), the
    `pub mod scenes;` declaration in `src/lib.rs`, and the
    `scene_objects!` macro in `src/render.rs` (used only inside
    `scenes.rs`). The doc comment in `src/render/shapes.rs` was
    trimmed to mention only the `From` impls — the macro
    reference no longer applies. `src/main.rs` now drives the
    renderer from SDL scenes via a small `load_sdl_scene(rel,
    binding)` helper that reads `scenes/<rel>` (path built
    absolute from `CARGO_MANIFEST_DIR`), evaluates in a fresh
    `default_env`, looks up the named scene binding, and clones
    the inner `Scene` out of the `Rc`. The four-quadrant
    composition and the on-disk vs. streaming path are
    structurally unchanged. `tests/sdl_suite.rs` lost its
    `assert_sdl_scene_matches_rust*` byte-equivalence helpers
    along with the 11 `phase{5,6,7}_<scene>_scene_matches_rust`
    tests they powered — there's no Rust scene left to compare
    against. They were replaced with much smaller smoke tests
    (`<scene>_scene_loads`) that load each `.lisp` file in a
    fresh `default_env`, look up the scene binding, and verify
    it's a `Value::Scene` without rendering. Catches script-
    level breakage (parse errors, missing bindings,
    `_common.lisp` regressions) for free; visual correctness
    falls back to the rest of the codebase's "render and look"
    convention. Phase ends here: the SDL is the canonical
    scene-definition mechanism. Future phases (heatmap target
    binding, animation, interpreter optimizations, transform
    collapsing) are all additive on top of this baseline.

21. **Lights as scene-graph shapes (stage 1).** Lights can now live
    inside `Scene::objects` and inherit affine transforms from
    enclosing `Shape::Transform` nodes, the same way geometry does
    — `(translate [5 5 5] (light-white [0 0 0]))` puts a light at
    world `[5 5 5]`. New `Shape::Light(Light)` variant;
    `Hittable::hit_test` returns `None` (lights are invisible to
    primary, shadow, and reflection rays); `Shape::bounds()` returns
    a degenerate point AABB at `light.location` so Group/Bounded
    composition stays well-defined without special-casing. New
    `Shape::collect_lights(&Affine, &mut Vec<Light>)` walks the tree,
    accumulating affines through `Transform` nodes via
    `world_from_local.compose(t.forward)`, and pushes world-space
    `Light` values into the output vec. `Bounded` wrappers are
    descended into unconditionally — the AABB early-out is per-ray
    work that would just hide lights with degenerate bounds for no
    speedup. `render()` builds an "effective lights" `Vec<Light>`
    once at entry by cloning `scene.lights` and extending it with
    `collect_lights` over `scene.objects`, then threads a
    `&[Light]` slice through `render_one_row` → `pixel_color` →
    `ray_color` → `shade_pixel`. `shade_pixel` iterates the slice
    instead of `scene.lights`. Stage 1 keeps `Scene::lights`
    working — scenes that put lights there get the same render
    they always got, scenes that move lights into the object tree
    (or mix both styles) render identically. SDL side:
    `require_shape_value` auto-wraps `Value::Light` into
    `Shape::Light`, so the same `(light-white ...)` /
    `(light-point ...)` constructors flow through `(translate ...)`,
    `(rotate-* ...)`, `(scale ...)`, `(group ...)`, `(bounded ...)`,
    and the scene `:objects` field without any new surface area.
    Predicates unchanged: a bare `(light-white ...)` is `light?`
    and not `shape?`; wrapping it in a transform makes the result
    `shape?`. `scenes/multi_light_test.lisp` was ported as a
    visual smoke test — both lights now live in `:objects` with
    `:lights []`; re-rendering should yield an identical quadrant.
    New tests: `tests/sdl/lights_in_objects.lisp` exercises the
    SDL surface (coercion, predicates, composition through every
    transform constructor, scene construction in both new and
    mixed styles), and a new `lights_in_objects_equivalence` test
    in `tests/sdl_suite.rs` renders two 32×32 scenes — one with
    a light at world `[5 5 5]` in `:lights`, one with
    `(translate [5 5 5] (light-white [0 0 0]))` in `:objects` —
    and asserts byte-equality. That single byte-equality test
    pins down both that the collection pass runs and that the
    accumulated affine is being applied correctly to light
    positions; the renderer is per-pixel deterministic so
    equality holds with `parallel = true`. Stage 2 (deferred):
    the `Scene::root: Shape` collapse called for in "Future
    directions" — once the scene is a single top-level `Shape`,
    the render-entry call becomes `scene.root.collect_lights(...)`
    and `Scene::lights` comes out entirely.

22. **Lights as scene-graph shapes (stage 2): `Scene::root` collapse.**
    Finishes the migration started in entry 21. `Scene::lights:
    Vec<Light>` and `Scene::objects: Vec<Shape>` are gone, replaced
    by a single `Scene::root: Shape` (typically a `Shape::Group`).
    The renderer's top-level traversal is just `scene.root.hit_test(ray)`
    in `ray_color` and `light_vector`; the standalone `nearest_hit`
    call sites in `render.rs` are gone (the function itself stays in
    `shapes.rs`, used internally by `Shape::Group::hit_test`).
    `render()` builds the effective lights with one
    `scene.root.collect_lights(Affine::identity(), &mut lights)` call
    and no separate `clone()` of a top-level list. SDL surface:
    `builtin_scene` drops the `:lights` key, keeps `:objects` as the
    flat vector it has always been, wraps it in `Shape::Group(...)`
    internally and stores as `root`. Bare `(light-white ...)` /
    `(light-point ...)` values continue to auto-wrap via
    `require_shape_value`, so the script surface for placing lights
    is unchanged from stage 1 — the only thing scenes had to do to
    migrate was move the `:lights` contents into `:objects` and drop
    the key. The constructor rejects `:lights` with an explicit
    migration error rather than silently ignoring it: unmigrated
    scenes panic on load, which is what `<scene>_scene_loads` smoke
    tests in `tests/sdl_suite.rs` will surface. Every `scenes/*.lisp`
    file was migrated; `sphere_surface_test.lisp` is the one that
    needed real lisp work (`(apply conj [light] (map make-sphere
    (range 25)))` to prepend the light to the 5×5 grid generator
    instead of dropping into a hand-written list). `bindings_scene.lisp`,
    `bindings_mesh.lisp`, `render_dispatch.lisp`, and the inline
    sources of `render_dispatch_save` and `lights_in_objects_equivalence`
    in `tests/sdl_suite.rs` all got the same treatment.
    `lights_in_objects_equivalence` was repurposed: pre-stage-2 it
    compared `:lights` vs `:objects` placements; after stage 2 it
    compares "bare light at world `[5 5 5]` in `:objects`" vs
    "`(translate [5 5 5] (light-white [0 0 0]))` in `:objects`",
    which is the same affine-application invariant from a different
    direction. Added `moravian_star_scene_loads` smoke test (the
    scene file landed between sessions and didn't have one). The
    `require_light_value` helper in `bindings.rs` is gone — it was
    only ever called by the `:lights` parser, which no longer exists.

23. **CLI single-scene rendering; quadrant layout removed.**
    `main.rs` rewritten around a single positional argument: the path
    to an SDL script. The four-scene quadrant compositing
    (`render_quadrants`, the `OffsetTarget` wrapping, the per-quadrant
    crosshair) is gone, along with the hard-coded 2048×2048 dimensions
    and the in-source list of scenes to render. The binding name to
    look up inside the script is derived from the filename
    (`cuboid_test.lisp` → `cuboid-test-scene`: file stem with `_` →
    `-`, plus the `-scene` suffix), matching the convention every
    `scenes/*.lisp` already follows. New `SIZE` env var (`SIZE=N`
    shorthand for `NxN`, `SIZE=WxH` separate dimensions) overrides
    the new 1024×1024 default; `PARALLEL` and `RTVIEW_ADDR` carry over
    unchanged from the pre-rewrite. Error paths print to stderr and
    exit non-zero rather than panicking — missing file, missing
    binding, wrong-typed binding, malformed `SIZE`, and bad usage all
    get clean messages. The path passed to `eval_source` is
    canonicalized so the SDL's `CurrentDirGuard` anchors `(load ...)`
    calls against the script's real directory even when the CLI arg
    was relative. Imports cleaned up: `OffsetTarget` and
    `OffsetHeatmapTarget` are no longer used by `main.rs` (they
    remain in `output.rs` for future SDL-driven compositing). The
    pre-existing `sdl_run` binary is still the right tool for
    side-effecting scripts that call `(render ...)` /
    `(save-png ...)` directly; `main.rs` retains the "pure-data
    scene definition + render-from-Rust" shape. Multi-scene
    compositing and animation are expected to move into the SDL — a
    script can construct an off-screen target, render each scene
    into an `(offset-target ...)` of it, and save the composed
    result — at which point `main.rs` reduces to "evaluate this
    script" and the convention-based binding lookup goes away.

24. **Adaptive oversampling phase 1: Halton sampler.** Swapped the
    fixed N×N sub-pixel grid in `pixel_color` for a Halton-(2, 3)
    low-discrepancy sequence indexed by sample number, decorrelated
    across pixels via a per-pixel Cranley-Patterson rotation. New
    `src/render/sampler.rs` exposes `halton_pair(i)` (radical inverse
    base 2 / base 3, returns a point in `[0, 1)²`; callers start at
    `i = 1` since `i = 0` sits at the pixel corner) and
    `cranley_patterson_offset(x, y)` (splitmix64-style mix of the
    pixel coordinates yielding a per-pixel `(ox, oy)` rotation, also
    in `[0, 1)²`). The actual sub-pixel sample position is
    `(halton_pair(i + 1) + (ox, oy)) mod 1`, evaluated in
    `pixel_color`'s newly flat `for i in 0..oversample² { ... }`
    loop. CP offset is hoisted outside the loop because it's
    invariant across samples for a given pixel. Sample count per
    pixel is unchanged at `oversample²` — Phase 1 only changes
    *where* within the pixel the samples land; the grid arithmetic
    (`subdx`, `iix`/`iiy`, the 2× stride) is gone. The module ships
    with unit tests pinning the first few base-2 / base-3 radical
    inverses to known fractions, asserting all outputs stay in
    `[0, 1)²` across a 64×64 grid, and checking that neighboring
    pixels receive distinct CP rotations (guards against a
    degenerate xor-only hash). Existing byte-pinned tests carry
    through unchanged: `lights_in_objects_equivalence` compares
    two scenes that both go through the same deterministic
    `(x, y, i) → sample-position` map, so byte-equality holds
    for any reason geometry would; `render_dispatch_save`'s
    "center pixel is lit" check survives any reasonable sub-pixel
    offset. Visual output for existing scenes is comparable — the
    intent isn't a quality change, it's the foundation for Phase 2's
    variance-driven termination, which only works cleanly because
    sample `i` now has a well-defined offset regardless of total
    sample count.

25. **Adaptive oversampling phase 2: variance-driven termination.**
    `pixel_color`'s per-pixel sample count is now adaptive. The loop
    takes a batch of `SAMPLE_BATCH = 4` samples at a time, tracks
    per-channel min/max across all samples so far, and terminates
    when the largest channel's `max - min` spread drops below
    `variance_threshold` — subject to a `min_samples` floor (the
    metric is too noisy at very low sample counts to trust) and a
    `max_samples` cap (so a stubbornly noisy pixel doesn't sample
    forever). Flat regions of typical scenes terminate at
    `min_samples`; edges and high-contrast areas keep sampling until
    they stabilize. Min/max spread was chosen over running statistical
    variance because the threshold is easier to reason about and the
    per-sample tracking is two channels of `min`/`max` rather than
    `sum` + `sum_of_squares`. `Scene::oversample` was deleted and
    replaced with three new fields: `min_samples: u32`, `max_samples:
    u32`, `variance_threshold: f64`. `CameraDetails` lost its
    `oversample` field at the same time (no longer needed since
    `pixel_color` reads adaptive parameters straight from `Scene`).
    Defaults: `min_samples = 4` (matches the previous fixed
    `oversample = 2` cost), `max_samples = 32`, `variance_threshold
    = 0.005` linear-color units. Setting `min_samples == max_samples`
    reproduces the previous fixed-count behavior exactly; the
    byte-pinned tests in `tests/sdl_suite.rs` rely on this for
    determinism (both `render_dispatch_save` and
    `lights_in_objects_equivalence` set `min = max = 1`). SDL side:
    `builtin_scene` in `src/sdl/bindings.rs` drops the `:oversample`
    key and adds `:min-samples`, `:max-samples`, `:variance-threshold`,
    all optional with the same defaults as the Rust struct. Scripts
    still carrying `:oversample` get an explicit migration error
    (same pattern as the stage-2 `:lights` rejection) directing them
    at the replacement keys. Every `scenes/*.lisp` file had its
    `:oversample 2` line dropped — the defaults match its cost on
    flat geometry and adapt up on the parts that need more. The
    test scripts `bindings_scene.lisp`, `bindings_mesh.lisp`,
    `render_dispatch.lisp`, `lights_in_objects.lisp`, and the inline
    sources for `render_dispatch_save` and
    `lights_in_objects_equivalence` were also migrated:
    `:oversample 1` (tests that wanted exactly one sample per pixel
    for determinism) became `:min-samples 1 :max-samples 1`;
    `:oversample 2` in `bindings_scene.lisp` became a slightly more
    interesting `:min-samples 4 :max-samples 16 :variance-threshold
    0.01` to exercise the new keys. The byte-pinned equivalence
    test holds for the same reason it always did — both scenes go
    through the same deterministic sampler and now the same
    deterministic batch-of-1 sample-count path.

26. **Adaptive oversampling phase 3: sample-count heatmap.** Second
    diagnostic output. `main.rs` now writes a `render-samples.png`
    grayscale heatmap alongside `render-heatmap.png` — black where
    the adaptive loop terminated at `min_samples` (flat regions),
    bright where it kept going (geometric edges, high-contrast
    areas, the silhouette of a complex mesh). The two heatmaps
    correlate strongly but aren't redundant: time picks up
    per-sample cost variation (a ray through the teapot's BVH is
    expensive even at one sample), sample count isolates "where is
    the sampler actually working harder per sample." Implementation:
    the `HeatmapTarget` trait was generalized — the lone method got
    renamed from `submit_timing_row` (with `timings_ns: &[u32]`) to
    `submit_metric_row` (with `metric: &[u32]`), and `PngHeatmapTarget`'s
    docstring relaxed from "per-pixel timings" to "u32 per pixel
    metric, whatever the renderer fed in." The storage and normalization
    code didn't change at all — both metrics are u32-per-pixel and
    fit the same 99th-percentile clamp + grayscale-PNG output
    machinery. `render()` traded its `heatmap: Option<&dyn
    HeatmapTarget>` parameter for a new `HeatmapTargets<'a>` struct
    with two slots (`time` and `samples`), each `Option<&'a dyn
    HeatmapTarget>`; `HeatmapTargets::default()` is the zero-overhead
    "neither" case. `pixel_color`'s return type became
    `(LinearColor, u32)` (color + sample count taken); always
    returning the count is cheaper than threading a "do you want
    it?" flag through to gate the assignment. `render_one_row` now
    conditionally allocates timing and sample-count buffers based
    on which slots are populated, preserving the original "no
    `Instant::now` calls when time is disabled" property and
    extending it to samples. `main.rs::main` builds both
    heatmaps unconditionally and routes them through
    `HeatmapTargets { time: Some(&t), samples: Some(&s) }`; the
    sample-count save uses `HeatmapScale::Linear` (the distribution
    is bounded between `min_samples` and `max_samples`, so the log
    compression that helps the time heatmap would mislead here).
    SDL render binding (`builtin_render`) passes
    `HeatmapTargets::default()` — heatmaps aren't exposed at the
    SDL surface yet, future work. No test changes: the byte-pinned
    tests still go through render() unchanged in the no-heatmap
    case, and the SDL test scripts use the SDL binding which now
    passes `HeatmapTargets::default()` instead of `None`.

27. **Transparency phase 1: non-refractive transmission.** Surfaces
    can now be partially see-through. `Surface` gained a
    `transparency: f64` field (0.0 = opaque, 1.0 = fully
    transmissive); `Scene` gained `transmit_limit: u32` (the
    transmission-recursion cap, default 8 at the SDL surface). A
    new private `Depth { reflect, transmit }` struct replaced the
    bare `reflect_count: u32` parameter threaded through
    `ray_color` / `shade_pixel` — reflection and transmission carry
    independent depth counters checked against `reflect_limit` and
    `transmit_limit` respectively, because a ray through N stacked
    transparent surfaces legitimately needs N transmission levels,
    a different scale of depth than mirror bounces. `shade_pixel`
    computes the opaque shading exactly as before (now bound to a
    local `opaque`), then — when `transparency > EPSILON` and the
    transmit budget isn't spent — casts a *straight-through*
    transmitted ray (incoming direction unchanged, origin at the
    hit point; no epsilon offset needed since every primitive's
    `hit_test` already rejects `t <= EPSILON`, discarding the
    surface being left) and returns
    `lerp(opaque, transmitted, transparency)`. At the recursion cap
    a transparent surface falls back to rendering fully opaque.
    Phase 1 is deliberately non-refractive — the transmitted ray
    doesn't bend — and shadow rays still treat any hit as full
    occlusion, so transparent objects cast solid shadows for now
    (Phase 2). SDL surface: `(surface ...)` gained an optional
    `:transparency` key (default 0.0), `(scene ...)` gained an
    optional `:transmit-limit` key (default 8), and
    `scenes/_common.lisp` gained a `glassy` helper alongside
    `glossy` / `reflective`. Since `transparency` defaults to 0.0,
    every existing scene renders byte-identically — the
    byte-pinned tests in `tests/sdl_suite.rs` are unaffected. New
    `scenes/transparency_test.lisp` (a glassy sphere in front of an
    opaque red one, both on the checker ground) with a
    `transparency_test_scene_loads` smoke test;
    `tests/sdl/bindings_surface.lisp` extended to exercise the
    `:transparency` key and its default.

28. **Transparency phase 2: transparent shadows.** Shadow rays now
    honor surface transparency, so a transparent object casts a
    lightened shadow instead of a solid black one. `light_vector`
    changed from a binary reaches / fully-occluded test
    (`Option<Vector>`) to an accumulated-transmittance walk
    (`Option<(Vector, f64)>`): it steps the shadow ray from the
    light toward the shaded point with a repeated-nearest-hit loop,
    advancing the segment origin to each hit point in turn (the
    `t <= EPSILON` rejection in every primitive's `hit_test` keeps
    the walk from re-finding the surface it just left, same guard
    the reflection / transmission rays use), and multiplies a
    running transmittance by each occluder's `surface.transparency`.
    An opaque occluder (`transparency == 0.0`) zeroes transmittance
    and the function returns `None` immediately — the early-out that
    keeps the common opaque case as cheap as the old single
    `hit_test`. A hit at or beyond the shaded point itself
    (`dist_from_light > light_distance - EPSILON`) ends the walk
    without counting as an occluder. `shade_pixel` destructures the
    new `(lv, transmittance)` pair and scales each light's combined
    specular + diffuse contribution by `transmittance` —
    `transmittance == 1.0` reproduces the pre-Phase-2 unobstructed
    result exactly. Two judgment calls settled here: (1) the
    transmittance is a *scalar*, not a per-channel color — the light
    is attenuated greyscale, not tinted by the occluder's body
    color, matching Phase 1's untinted primary-ray transmission;
    colored shadows are deferred to land with colored transmission
    (most naturally with refraction). (2) The plan's draft formula
    said "multiply in `(1 - occluder.transparency)`," which has the
    polarity backwards — opaque is `transparency == 0.0`, and an
    opaque occluder must drive transmittance to 0, so the factor is
    `occluder.transparency` directly; implemented that way. No new
    `Surface` / `Scene` fields and no SDL surface change — Phase 2 is
    purely a renderer-internal change to the shadow-ray traversal.
    Determinism: the byte-pinned tests in `tests/sdl_suite.rs` use
    only opaque surfaces, where an opaque occluder still drives
    transmittance to exactly 0 (→ `None`, same as the old binary
    "occluded") and an unobstructed light still yields
    `transmittance == 1.0` (→ identity scale), so those renders are
    bit-for-bit unchanged. Visual check: render
    `scenes/transparency_test.lisp` — the glassy sphere should now
    cast a soft, partial shadow on the checker floor rather than a
    solid one.

29. **Depth of field phase 1: thin-lens camera.** The camera can now
    have a finite aperture, producing depth-of-field blur. `Camera`
    gained `aperture_radius: f64` (world units; `0.0` = ideal
    pinhole) and `focus_distance: f64` (depth along `forward` that
    stays sharp). `Camera::looking_at` / `with_fov` set
    `aperture_radius = 0.0` and `focus_distance` to the
    `location`→`look_at` distance; a new `Camera::with_dof(location,
    look_at, up_hint, zoom, aperture_radius)` is `looking_at` with
    the aperture overridden (focus stays on the look-at point —
    focusing at some other depth is a Phase 2 ergonomics item).
    `camera_ray` gained a `lens: (f64, f64)` parameter (a unit-disk
    point) and an explicit pinhole fast-path branch: when
    `aperture_radius == 0.0` it returns exactly the pre-DOF
    `Vector { start: location, delta: normalizep(dir) }`, which is
    what keeps the byte-pinned tests bit-identical — scaling `dir`
    by `focus_distance` and renormalizing is *not* bitwise the same
    as renormalizing `dir` directly, so the zero-aperture case can't
    just fall out of the thin-lens math. The thin-lens path computes
    the focal point (`location + focus_distance * dir`, exploiting
    that `dir`'s forward component is exactly 1) and jitters the ray
    origin over the aperture disk in the `right`/`up` plane.
    `render::sampler` gained the lens-sampling pieces: `halton_lens`
    (bases 5, 7 — distinct from `halton_pair`'s 2, 3 so the lens and
    sub-pixel coordinates of a given sample index are uncorrelated),
    `concentric_disk` (Shirley–Chiu unit-square→unit-disk mapping,
    for smooth bokeh), and `cranley_patterson_lens_offset` (the lens
    CP rotation; the existing `cranley_patterson_offset` was
    refactored to share a `cp_hash(x, y, seed)` helper, with seed 0
    folded in as `+ 0` so its output is bit-identical to before).
    `pixel_color` computes the lens sample per sub-pixel sample, but
    only when `aperture_radius != 0.0` — the `dof` flag is hoisted
    outside the loop exactly like `render_one_row`'s `want_time`, so
    the pinhole path makes no `halton_lens` / `concentric_disk`
    calls and pays nothing for the feature. SDL: a positional
    `(camera-dof location look-at up-hint zoom aperture-radius)`
    binding alongside `camera-looking-at` / `camera-with-fov`; a
    zero aperture argument makes it structurally equal to the
    `camera-looking-at` camera. New `scenes/depth_of_field_test.lisp`
    (three spheres at staggered depths, the middle one on the focus
    plane) with a `depth_of_field_test_scene_loads` smoke test;
    `tests/sdl/bindings_camera.lisp` extended for `camera-dof`, and
    new unit tests in `src/render/sampler.rs` for the lens sampler
    functions. Existing scenes render byte-identically — aperture
    defaults to 0.0 and the pinhole branch is bit-identical — so the
    byte-pinned tests in `tests/sdl_suite.rs` are unaffected.

30. **Metallic surfaces (basic case).** `Surface` gained a
    `metallic: bool` field (default `false` = ordinary dielectric).
    When `true`, `shade_pixel` reinterprets the existing fields the
    way a metal behaves: the mirror reflection term and the Phong
    specular highlight are both tinted component-wise by the surface
    color (via `multiply_linear_color` against `scolor`, so a checked
    metal's reflection picks up the checker pattern consistently with
    the ambient/diffuse terms), and the Lambertian diffuse term is
    suppressed entirely — metals have essentially no diffuse lobe, so
    all their apparent color comes from the tinted reflection and
    specular. A metallic surface is also forced opaque: a `!metallic`
    guard was added to the transmission branch's condition, so
    `transparency` is ignored when `metallic` is set (the two are
    physically contradictory). The change is three localized edits to
    `shade_pixel` plus the one field; no new recursion, no pipeline
    changes, reuses the existing `reflection` field as the reflection
    strength. SDL surface: `(surface ...)` gained an optional
    `:metallic` key (default `false`), and `scenes/_common.lisp`
    gained a `metallic` helper (alongside `glossy` / `reflective` /
    `glassy`) plus `surface-gold` / `surface-silver` /
    `surface-copper` presets. New `scenes/metallic_test.lisp` (gold,
    silver, copper spheres on the checker ground, `reflect-limit 3`
    so metal-to-metal reflections resolve) with a
    `metallic_test_scene_loads` smoke test;
    `tests/sdl/bindings_surface.lisp` extended to exercise the
    `:metallic` key and its default. Since `metallic` defaults to
    `false`, every existing scene renders byte-identically and the
    byte-pinned tests in `tests/sdl_suite.rs` are unaffected.
    Rough/glossy metal (scattered reflections — a `roughness` field
    and per-sample reflection-ray jittering, with the sample-index
    wrinkle that reflection happens inside the non-sample-indexed
    `ray_color` recursion) and Fresnel are deferred follow-ons.

31. **Cone primitive.** New `Cone` shape — a closed solid cone
    parameterized exactly like `Cylinder` (`p0`, `p1`, `r`, `surface`)
    but with `p0` as the base center (radius `r`) and `p1` as the apex
    point. The two ends are *not* interchangeable, which is the one
    semantic difference from `Cylinder` worth keeping in mind. Same
    transform caveat as `Cylinder`: uniform scale stays a cone,
    non-uniform scale would need an elliptical cone the primitive can't
    represent — wrap in `Transform`. `Hittable for Cone` tests two
    surfaces: the curved lateral surface and a single flat base cap at
    `p0` (the apex end has no cap). The lateral test substitutes the ray
    into the `((P-apex)·axis_unit)² = cos²θ·|P-apex|²` cone equation and
    solves the resulting quadratic; `a` can be positive, negative, or
    ~0 (ray parallel to a generator line — handled as a linear
    fallback), so both roots are gathered without assuming an ordering.
    Each root is trimmed by `s = (P-apex)·axis_unit ∈ [0, axis_len]` —
    the `s ≥ 0` half is load-bearing, it discards the infinite double
    cone's second nappe behind the apex, which the `cos²θ` form also
    admits. The lateral normal is `normalize(perp_unit - slope·axis_unit)`
    (`slope = r/axis_len`): radially outward *and* tilted toward the
    apex by the half-angle — the "lit faces dark" suspect if the sign
    is wrong. Apex-tip hits (degenerate `perp`) are skipped rather than
    emitting a garbage normal. The base cap reuses `Cylinder`'s cap
    test verbatim (ray-plane + squared-radius check), normal
    `+axis_unit`. `Shape::bounds()` returns the union of the base
    disk's AABB (the same `r·√(1-axis_unit[i]²)` per-axis trick
    `Cylinder` uses, applied only at `p0`) with `p1` as a degenerate
    point — tight, not just conservative. Touch points were the usual
    closed-enum set: the `Shape::Cone` variant, `From<Cone>`, the
    `hit_test`/`bounds`/`collect_lights` match arms, the SDL
    `value.rs` Display arm, and a map-keyed `(cone {:p0 :p1 :r
    :surface})` constructor in `bindings.rs`. New `scenes/cone_test.lisp`
    (three cones — vertical, apex-on at the camera, diagonal reflective —
    mirroring `cylinder_test.lisp`'s layout) with a
    `cone_test_scene_loads` smoke test; `tests/sdl/bindings_shapes.lisp`
    extended with cone construction, structural-equality, and
    end-not-interchangeable checks. No existing scene or byte-pinned
    test is affected — `Cone` is purely additive.

32. **Light types phase 1: `LightKind` refactor.** Phase 1 of the
    "Light types: spotlights and area lights" plan: introduces the
    enum scaffolding for variant-dispatched lights with no behavior
    change. `Light` gained a `kind: LightKind` field; `LightKind` is a
    one-arm `#[derive(Copy, Clone, PartialEq, Debug)]` enum with only
    `Point` for now (Phase 2 adds `Spot { direction, inner_angle,
    outer_angle }`; Phase 4 adds `Area { axis, radius }`). Constructors
    `Light::white` / `Light::point` set `kind: LightKind::Point`.
    `light_vector` is now a thin dispatcher that matches on
    `light.kind` and delegates to a per-kind helper; the existing
    transmittance walk moved into `light_vector_point` unchanged, so
    Phase 2's `light_vector_spot` slots in as a sibling. `shade_pixel`
    stays light-type-agnostic — it just consumes the `(Vector, f64)`
    pair the helper returns. `Shape::collect_lights`'s
    `Shape::Light(l)` arm propagates `l.kind` through to the
    world-space `Light` it constructs; later phases extend this arm to
    transform per-variant geometric fields (a spotlight's direction,
    an area light's axis) under the accumulated affine. SDL surface
    unchanged — `(light-white ...)` / `(light-point ...)` still
    produce `Value::Light` wrapping a `Light` that just happens to
    carry the new `kind` field; `light?` predicate, `require_shape_value`
    auto-wrap, scene-graph integration all untouched. The hybrid
    struct + `LightKind` shape was a deliberate departure from the
    plan's original "turn `Light` into an enum like `Shape`" framing:
    `location` / `color` / `intensity` are genuinely shared and read
    directly across the shading code, so a pure enum would force
    accessor methods or per-site `match` arms for fields every variant
    has. Variant dispatch lives where it actually matters —
    `collect_lights` and `light_vector` — and the plan section's
    "Decisions still open" flagged this choice for review at
    implementation time; landing it gives the next phase the smallest
    possible delta. Determinism: every existing scene renders
    byte-identically (only `kind: LightKind::Point` exists; the
    dispatch match has exactly one arm and the helper body is the
    pre-Phase-1 `light_vector` verbatim), which the byte-pinned tests
    in `tests/sdl_suite.rs` pin down without changes. No new scenes,
    no SDL surface area, no test changes — Phase 1 is purely an
    infrastructure checkpoint shaped for Phases 2 and 4.

33. **Light types phase 2: spotlights (`Spot` variant).** Phase 2 of
    the "Light types: spotlights and area lights" plan. `LightKind`
    gained `Spot { direction: Point, inner_angle: f64, outer_angle:
    f64 }`: `direction` is the cone axis (unit vector pointing the
    way the light shines), `inner_angle` and `outer_angle` are
    half-angles in radians measured from the axis. Angles stored
    rather than cosines for debuggability — cosines are derived at
    the comparison site. New `Light::spot` `const fn` constructor.
    Cone falloff is `smoothstep(cos(outer), cos(inner), cos_theta)`
    where `cos_theta = dot(direction, normalize(point - location))`.
    Comparison is done in cosine space because `cos` is monotonically
    decreasing on `[0, π]`. Implemented as explicit clamp-at-edge
    branches (outside outer → 0, inside inner → 1, transition band
    → Hermite cubic), which both reads cleaner than a clamp-and-cube
    and dodges a 0/0 when `inner_angle == outer_angle` (a
    hard-edged cone — both edges coincide, every input matches one
    of the clamp arms before hitting the band's division).
    Renderer: `light_vector` matched the new arm to a fresh helper
    `light_vector_spot`, which computes the cone factor *first* and
    returns `None` for points outside the outer cone before doing
    any hit-testing — the early-out keeps spotlight scenes from
    paying transmittance-walk cost for pixels the spotlight can't
    reach. Inside the cone, it delegates to `light_vector_point`
    for the existing transmittance walk and folds the cone factor
    into the returned scalar, so transparent occluders attenuate
    spotlights the same way they attenuate point lights (a glass
    pane in a spotlight beam behaves consistently with the same
    pane in front of a point light). `Shape::collect_lights`'s
    `Spot` arm transforms `direction` by the linear part of the
    accumulated affine (`Affine::transform_vector`) and
    renormalizes — non-uniform scale can change a unit vector's
    magnitude even when the source was unit-length, so the
    renormalize is load-bearing. Angles aren't vectors and don't
    transform. SDL: `(light-spot location direction color intensity
    inner-angle outer-angle)` positional binding installed
    alongside `light-white` and `light-point`. Validates
    `lenp(direction) >= EPSILON` (rejects zero vectors) and
    `inner-angle <= outer-angle` (rejects reversed angles, which
    would produce a hard rather than soft inner edge — bounded but
    surprising) with explicit `sdl_panic!` messages naming the
    bad value, then normalizes the direction so direct Rust
    callers using the `Light::spot` constructor and script callers
    coming through the binding both end up with the same
    unit-direction invariant. Required two new imports in
    `src/sdl/bindings.rs` (`lenp`, `normalizep`, `EPSILON` from
    `crate::render::geometry`) and a new use of `LightKind` in
    `src/render/shapes.rs` for the `collect_lights` match.
    New `scenes/spotlight_test.lisp` (a spotlight pointed straight
    down at the checker floor with three spheres along +x at
    progressively larger angular offsets — red inside the inner
    cone, green in the transition band, blue outside the outer
    cone) and a `spotlight_test_scene_loads` smoke test in
    `tests/sdl_suite.rs`. `tests/sdl/bindings_lights.lisp`
    extended with spotlight construction, structural equality
    (covering each of `direction` / `inner-angle` / `outer-angle`
    / `intensity` separately so a regression in any one shows up
    distinctly), the boundary normalization check (two spotlights
    built from parallel direction vectors of different magnitudes
    compare equal because the binding normalizes both at
    construction), and a Spot-vs-Point inequality. Existing
    byte-pinned tests in `tests/sdl_suite.rs` are unaffected:
    point lights still flow through the `LightKind::Point` arm,
    and `light_vector_point`'s body is unchanged — Phase 2 added
    code but didn't modify any path the point-light renderer
    takes. Phase 3 (spotlight ergonomics: aim-at-target
    constructor, possible distance attenuation) remains deferred.

34. **Light types phase 4: area lights (`Area` variant, hard-shadow
    baseline).** Phase 4 of the "Light types: spotlights and area
    lights" plan. `LightKind` gained `Area { axis: Point, radius:
    f64 }` — a disk emitter centered at the light's `location` with
    normal `axis` (unit vector pointing the way the light shines)
    and `radius` (world units). Disk over quad was the planned
    Phase-4 shape: one axis vector vs. two basis vectors is simpler
    to construct, simpler to transform (no orthogonality constraint
    to preserve), and the `concentric_disk` sample map already in
    `render::sampler` (DOF Phase 1) is the canonical map for
    sampling a uniform point on the disk in Phase 5. New
    `Light::area` `const fn` constructor. Renderer: `light_vector`
    matched the new arm to a fresh helper `light_vector_area`,
    which (1) computes a Lambertian cosine attenuation `cosine =
    dot(axis, normalize(point - light.location))` against the
    disk's normal, (2) returns `None` immediately for points in the
    back hemisphere (`cosine ≤ EPSILON`) — same early-out shape as
    the spotlight cone falloff — and (3) inside the lit half-space
    delegates to `light_vector_point` for the transmittance walk
    and folds the cosine factor into the returned scalar.
    Transparent occluders attenuate area lights the same way they
    attenuate point and spot lights. Phase 4 deliberately samples
    the disk *center* only — `light.location` — so shadows are
    hard, looking like a directional point light with cosine
    falloff against `axis`. Phase 5 will replace the center-only
    sampling with per-pixel-sample jittered points on the disk for
    soft shadows; the helper's signature already threads `_radius`
    through (unused in Phase 4) so the Phase-5 diff is the body,
    not the signature. `Shape::collect_lights`'s `Area` arm
    transforms `axis` by the linear part of the accumulated affine
    and renormalizes (same pattern as `Spot::direction`); `radius`
    is left alone, matching the existing Cylinder/Cone-radius
    posture (uniform-scale-aware radius scaling is a Phase 6
    nicety). SDL: positional `(light-area location axis radius
    color intensity)` (5-arg, distinct from `light-spot`'s 6-arg
    shape). The binding validates `lenp(axis) >= EPSILON` and
    `radius >= EPSILON` with explicit `sdl_panic!` messages naming
    the bad value, then normalizes the axis. Radius validation is
    *forward-looking* — Phase 4 doesn't consult `radius`, but a
    non-positive radius is meaningless to Phase 5's disk sampler,
    so we reject at construction rather than wait for Phase 5.
    New `scenes/area_light_test.lisp` (disk light at `[0 0 4]`
    aimed down with radius 1.0, three spheres along ±x at
    progressively larger angular offsets to show the cosine
    falloff, on the reflective checker floor) and an
    `area_light_test_scene_loads` smoke test in
    `tests/sdl_suite.rs`. `tests/sdl/bindings_lights.lisp`
    extended with area-light construction, structural equality
    (each of `axis` / `radius` / `intensity` exercised separately),
    the boundary normalization check (non-unit axis input produces
    a structurally equal area light), and Area-vs-Point /
    Area-vs-Spot inequality. Existing byte-pinned tests are
    unaffected: point and spot lights still flow through their
    respective `LightKind` arms unchanged; Phase 4 added code paths
    but didn't modify any of the existing ones. The next chunk —
    Phase 5 (soft shadows via per-pixel-sample jittered shadow
    rays threaded through the existing Halton sampler) — is the
    one real architectural change in the light-types plan.

35. **Light types phase 5: area-light soft shadows.** The payoff
    phase of the "Light types: spotlights and area lights" plan
    and the one real architectural change in it. Per-pixel-sample
    jittered shadow rays across an area light's disk produce soft
    shadows that the adaptive oversampler resolves automatically:
    penumbra pixels see high variance and keep sampling until they
    stabilize; fully-shadowed and fully-lit pixels terminate at
    `min_samples` and pay almost nothing for the feature.
    Implementation breakdown:
      * **Sampler additions** (`src/render/sampler.rs`).
        `halton_area(i)` on bases 11 and 13 — a third distinct
        pair so the area-light coord is uncorrelated with both
        the sub-pixel (2, 3) and lens (5, 7) coords of the same
        sample index. `cranley_patterson_area_offset(x, y)` uses
        the existing `cp_hash` helper with seed `2`, distinct
        from pixel's `0` and lens's `1`. New unit tests pin the
        first-index radical inverses, the in-range invariant, and
        decorrelation from both other CP rotations.
      * **`shadow_ray_walk` extraction** (`src/render.rs`). The
        transmittance-walk loop that lived in `light_vector_point`
        moved into a generic `shadow_ray_walk(origin, target,
        scene)` helper. `light_vector_point` is now a one-line
        wrapper passing `light.location` as `origin`; the math
        is the same Rust operations on the same f64 inputs, so
        point-light output is bit-identical to the pre-refactor
        code — what the byte-pinned tests in `tests/sdl_suite.rs`
        rely on. Area lights now share the same walk through the
        helper, just with a per-sample disk point as the origin.
      * **`disk_basis(axis)` helper.** Returns an orthonormal
        basis `(u, v)` perpendicular to a unit `axis`. Uses the
        "pick the world axis least aligned with `axis` as the
        hint" trick to avoid a degenerate cross product when the
        axis lines up with a world axis (the common case for
        ceiling/floor disk lights with `axis = [0, 0, ±1]`).
      * **`light_vector_area` Phase-5 body.** `light_coord` is
        unpacked as a `[0, 1)²` value, mapped through
        `concentric_disk` to a uniform unit-disk point, scaled by
        `radius`, and oriented into world space by
        `disk_basis(axis)` to produce the shadow-ray origin —
        replacing `light.location` from Phase 4. The cosine
        attenuation against `axis` (computed against the disk
        center, not the per-sample point — matches the Phase 4
        behavior and is consistent across all samples for a given
        pixel) is unchanged. The walk delegates to
        `shadow_ray_walk(origin, point, scene)`.
      * **Coordinate threading.** `light_vector`, `shade_pixel`,
        and `ray_color` gained a `light_coord: (f64, f64)`
        parameter, kept separate from the `Depth` blob (the plan
        recommended hybrid combine-into-one-blob would have made
        the architectural delta wider for no real win — depth and
        light coord rarely change at the same call site). Point
        and spot lights ignore the coordinate; only
        `light_vector_area` consumes it. Recursive `ray_color`
        calls in `shade_pixel` (reflection, transmission) reuse
        the *same* pixel-sample `light_coord` rather than
        re-deriving one per bounce — same "good enough" judgment
        call DOF made for reflection rays not getting fresh
        aperture jitter, and per-bounce sample state would mean
        threading sampler state through recursion proper.
      * **`pixel_color` integration.** Computes `has_area_light`
        once per pixel (`lights.iter().any(matches!
        LightKind::Area)`), hoisted outside the sample loop the
        same way `dof` is. When false, the area CP offset stays
        `(0.0, 0.0)` and per-sample `halton_area`/CP work is
        skipped entirely — point/spot-only scenes pay nothing for
        the feature. When true, each pixel sample gets a fresh
        `light_coord = (halton_area(i + 1) + (aox, aoy)) mod 1`,
        threaded into `ray_color`.
    New `scenes/soft_shadow_test.lisp` (a disk light radius 1.5 at
    `[0 0 4]` aimed down at a single sphere at `[0 0 1.5]` over
    the checker floor) with a `soft_shadow_test_scene_loads`
    smoke test. The existing `scenes/area_light_test.lisp` from
    Phase 4 transitions in this commit from a hard shadow at the
    disk center to a soft shadow sampled across the disk — that's
    the intended behavior change, and the smoke test only
    verifies that the scene still loads, so nothing in the test
    suite breaks. Determinism: point and spot lights ignore the
    new coordinate and still go through their respective
    `LightKind` arms with the same shadow-ray geometry as before;
    `light_vector_point` calls `shadow_ray_walk(light.location,
    ...)` which is the same math as the pre-refactor body. The
    byte-pinned tests in `tests/sdl_suite.rs` (which use only
    point lights) are bit-for-bit unaffected. Visual check:
    re-render `scenes/area_light_test.lisp` and confirm shadows
    are now visibly soft instead of hard; render
    `scenes/soft_shadow_test.lisp` and the
    `render-samples.png` heatmap; the penumbra region around the
    shadow should be distinctly brighter than the fully-lit and
    fully-shadowed regions, which is the "adaptive sampler doing
    the soft-shadow work" verification the CLAUDE.md anticipated
    when DOF Phase 1 introduced the sampler abstraction.

36. **SDL desugaring pass + `defn`.** Phase 1 of "`defn` and the
    desugaring pass." New `src/sdl/desugar.rs` sits between the
    reader and the evaluator: `eval_source` now runs
    `desugar::desugar(form)` on each top-level form immediately
    before evaluating it. The pass walks the whole form tree
    (lists, vectors, maps), leaves `(quote ...)` untouched, and
    rewrites any list whose head symbol names a sugar form, then
    re-walks the result so nested sugar is fully expanded. The
    only sugar so far is `(defn name docstring? [params] body...)`
    → `(def name (fn name [params] body...))`. The docstring is
    accepted and discarded; a string after the parameter vector is
    an ordinary body form; multi-arity is rejected (since `fn`
    doesn't support it); malformed forms panic with an error that
    names `defn`. Synthesized `def`/`fn` symbols carry the `defn`
    head's source position, and everything else keeps its
    original position. Expanding one top-level form at a time is
    deliberate: it's the order a future `defmacro` needs, since a
    macro defined by one form must be visible when the next is
    expanded. Tests: new `tests/sdl/defn.lisp` (basic use,
    equivalence with `def` + `fn`, naming, docstrings, rest args,
    destructuring, recursion, `recur`, closures, nested `defn`,
    quote passthrough), plus `defn_rejects_malformed_forms` in
    `tests/sdl_suite.rs` for the error cases. Existing
    `(def x (fn ...))` definitions are unchanged and still work;
    migrating them is Phase 2.

37. **`defn` migration.** Phase 2 of "`defn` and the desugaring
    pass." All 37 `(def name (fn [...] ...))` definitions were
    rewritten as `(defn name [...] ...)`: the stdlib's angle and
    point helpers, the surface helpers in `scenes/_common.lisp`,
    the per-scene helpers in `cornell_box`, `gi_test`,
    `moravian_star` and `sphere_surface_test`, and the test scripts
    `bindings_scene`, `closures`, `destructuring`, `hofs`, `points`,
    `recur` and `threading`. Bodies were re-indented to the usual
    two-space `defn` style. `tests/sdl/fn_form.lisp` keeps the
    long-hand form on purpose (a header comment says why), and
    `defn.lisp` keeps one long-hand definition as its equivalence
    reference. `defn` is now the idiom for named functions in SDL
    code.

38. **Control-flow sugar moved into the desugaring pass.** Phase 3
    of "`defn` and the desugaring pass." `when`, `when-not`, `cond`,
    `->` and `->>` are no longer special forms in `eval.rs`; they
    are source rewrites in `src/sdl/desugar.rs`:
    `(when t body...)` → `(if t (do body...) nil)`,
    `(when-not t body...)` → `(if t nil (do body...))`,
    `(cond t1 e1 ...)` → nested `if`s ending in `nil`, and
    `(-> x f (g a))` → `(g (f x) a)` (`->>` puts the value last).
    The evaluator lost `eval_cond`, `eval_when`, `eval_when_not`,
    `eval_thread_first`, `eval_thread_last` and `thread_step`, and
    now handles only the core forms plus `and`, `or` and `load`.
    Error messages for malformed forms are kept (they still name
    the form), and are now raised when a top-level form is
    expanded rather than when the bad subform is reached. Two
    behavior changes, both matching Clojure's macros: a threading
    step can itself be a special form or sugar
    (`(-> x (if :yes :no))` is `(if x :yes :no)`; before, the head
    was evaluated as a function and failed), and evaluation order
    follows the rewritten call, so in `(-> x (g a))` the head `g` is
    evaluated before `x`. Only side-effecting threaded code can tell.
    `and` and `or` stay special forms: rewriting them into core
    forms needs a temporary binding for the value being tested,
    which in turn needs a `gensym`-style hygiene story the SDL
    doesn't have yet. Tests:
    `threading.lisp` and `control_flow.lisp` gained composition
    cases (sugar inside sugar, special-form thread steps, `recur`
    from `when`/`cond` bodies, long `cond` chains, quote
    passthrough), and the Rust malformed-form test became
    `desugar_rejects_malformed_forms`, covering every sugar form.

39. **CSG phase 1: span query.** Phase 1 of the "CSG: implementation
    plan": the query CSG needs, with no change to any render. New in
    `src/render/shapes.rs`: `SpanEnd { t, normal, surface }` and
    `Span { enter, exit }`, and `Shape::spans(ray, &mut Vec<Span>)`,
    which appends the whole-line intervals (negative `t` included)
    where the ray is inside a solid, sorted and non-overlapping.
    `Sphere`, `Cuboid`, `Cylinder` and `Cone` each give at most one
    span. They reuse their `hit_test` math but keep both crossings and
    the exit normal, and the cylinder and cone share a `convex_span`
    helper that takes the smallest and largest valid crossing. `Plane`
    is a half-space (solid opposite `normal`) with infinite endpoints.
    Of the wrappers: `Transform` maps the ray without renormalizing (so
    `t` carries straight across) and maps normals through
    `normal_xform`; `Surfaced` fills missing surfaces; `Bounded` reuses
    `AABB::intersects`, which is valid because spans entirely behind
    the origin can't matter; and `Group` is a union. `Triangle` and
    `Light` give no spans. The set operations `span_union`,
    `span_intersection` and `span_difference` are free functions over
    sorted span lists. Difference flips the normals of the boundaries
    the subtracted solid contributes and keeps its surface.
    `first_span_hit(spans, ray)`, the first finite endpoint with
    `t > EPSILON` as a `RayHit`, landed early because the tests need
    it; Phase 2's `Csg::hit_test` will use it. One deliberate
    difference from the cone's `hit_test`: a lateral root on the apex
    tip gets a normal pointing out through the apex instead of being
    skipped, so a ray down the axis still has two endpoints. New
    `#[cfg(test)] mod span_tests` (18 tests): the spans of each
    primitive through, from inside, and missing; the plane half-space
    in every direction; the wrappers; the set operations on
    hand-built lists, including touching spans and the normal flip; a
    sphere-minus-sphere-minus-plane bowl; and a property check, over
    2,000 deterministic rays per solid (bare and under a non-uniform
    rotate/scale/translate stack), that the first span endpoint in
    front of an outside origin matches `hit_test`'s distance and
    normal. Nothing outside this section calls the new code, so every
    scene renders byte-identically. **Verification caveat:** the
    session that wrote this had no crates.io access, so the tests ran
    in a dependency-free harness crate (edition 2018, same as this
    crate) that compiled the real `shapes.rs`, `geometry.rs`,
    `transform.rs` and `color.rs` against verbatim copies of the
    `render.rs` types they use. A full `cargo test` here is still
    needed. (Superseded by entry 40: Phase 2's session compiled the
    whole crate and ran the full suite against stand-in libraries.)

40. **CSG phase 2: the `Csg` node and SDL bindings.** New variant
    `Shape::Csg(Box<Csg>)`, where `Csg { op: CsgOp, a, b }` and `CsgOp`
    is `Difference` or `Intersection`. There's no union variant because
    `Group` already is one.
    - **Evaluation.** `Csg` computes its operands' span lists, skipping
      `b` when `a` is empty, and combines them with
      `span_difference` / `span_intersection`. Its `hit_test` is
      `first_span_hit` over the result. Because the node also answers
      `spans`, CSG nests.
    - **Other match arms.** `bounds()`: a difference is bounded by
      `a`; an intersection by the overlap of the two bounds (new
      `AABB::intersection`, which clamps to a degenerate box when there
      is no overlap), or by whichever operand is bounded when the other
      is a half-space. `validate_surfaces` and `collect_lights` recurse
      into both operands, and the SDL `Display` arm prints
      `#<shape difference>` / `#<shape intersection>`.
    - **Solidity.** New `Shape::is_solid()`: false for `Triangle`,
      true for the other primitives, `Plane`, `Csg` and `Light`; a
      `Group` is solid when all its children are; the wrappers pass
      through. The Rust constructors `difference(a, b)` and
      `intersection(a, b)` assert it.
    - **SDL.** `(difference a b c …)` means `a − (b ∪ c ∪ …)`: the
      binding wraps the extra operands in one `group`, which is POV's
      n-ary form. `(intersection a b c …)` folds left. Both need at
      least two operands, and a non-solid operand (a triangle, or a
      `load-obj` mesh) is rejected with a positioned `sdl_panic!` that
      names it by number. Faces cut by an operand show that operand's
      surface if it has one, otherwise the nearest `with-surface`.
    - **Tests.**
      - Seven new Rust unit tests in `shapes.rs`: the bowl's interior,
        back and rim; a multi-cutter difference and nesting, with exact
        `t` and normals; intersection hits and bounds; hits unchanged
        whether a transform wraps the whole CSG node or each operand;
        cut-face surfaces; `is_solid`; and the constructor panic.
      - New `tests/sdl/bindings_csg.lisp` (registered in both
        `DECLARED` and the `sdl_test!` list): construction, display,
        equality, the n-ary forms, composition and nesting, and a
        small render.
      - New `csg_rejects_bad_operands` Rust test in
        `tests/sdl_suite.rs`.
      - New `scenes/csg_test.lisp` with a `csg_test_scene_loads` smoke
        test. It shows a cube minus a sphere, with the sphere's own
        yellow surface on the cut faces, a glassy sphere∩cube, and a
        gold bowl (sphere − sphere − half-space) opening upward.
    - **Visual check.** A render of `csg_test` looked right: cut faces
      lit on the correct side, glass showing its back faces, and the
      bowl's rim and interior visible.
    - **Pitfall.** Plane normals must be unit length, both for
      `hit_test` (which returns `normal` as is) and for half-space
      spans. The SDL `plane` binding doesn't normalize.
    - **How it was verified.** This session had no crates.io access
      either. The whole crate was built against small local stand-ins
      for `image` (raw-pixel save/open), `rayon` (sequential) and
      `tobj` (a `v`/`vn`/`f` parser), with `num-complex` dropped as
      unused. All 47 unit tests and all 61 `sdl_suite` tests pass
      there, and the renders were made with those stand-ins. It still
      needs a `cargo test` with the real crates.
    - **Unchanged.** Scenes without CSG don't reach any new code, so
      they render byte-identically.

41. **CSG phase 3: Texaco port.** The acceptance test for CSG, and
    the first scene ported from the POV-Ray projects
    (github.com/mschaef/povray-projects; see
    `docs/povray_gap_analysis.md`).
    - **`scenes/_pov.lisp`.** Porting helpers:
      - `pov-white` / `-black` / `-red` / `-green` / `-blue`.
      - `(box a b)`, a cuboid from two corners in either order.
      - `pov-arrow`, the POV projects' `makeArrow`.
      - `(pov-camera loc look-at)`, which is zoom 1.0 with up `+y`.
        That's POV's default camera; `direction k*z` is zoom k.
      - Starting-point surfaces: `pov-plain`, `pov-plain-specular`,
        `pov-metal-a` and `pov-metal-c`. The metals borrow `F_MetalA` /
        `F_MetalC`'s ambient, diffuse (`:light`), specular and
        reflection numbers, and deliberately aren't `:metallic`,
        because this renderer's metallic model drops diffuse.
      - The header records the porting conventions.
    - **Handedness confirmed.** `scenes/pov_compass.lisp` renders the
      projects' arrow compass from a camera at -z: red +x points right,
      green +y up, and blue +z away, as in POV-Ray. POV's z-rotation
      matrix (`source/backend/math/matrices.cpp`, applied to row
      vectors) is the same rotation as `Affine::rotation_z`. So POV
      coordinates and `rotate` angles port unchanged, with degrees
      converted, and POV's transform order, where the first one written
      applies first, becomes the innermost call.
    - **`scenes/texaco.lisp`.** A structural port of `texaco.pov`:
      - `na-108` wedge cutters.
      - `star` is a cylinder minus five cutters via
        `(apply difference disk cutters)`.
      - `texaco-star` subtracts the T.
      - The bowl is sphere − sphere(0.999) − cylinder.
      - The star is scaled to 0.16 deep and turned about y.
      - `(texaco-at angle backdrop?)` builds any animation frame;
        `texaco-scene` is `(texaco-at 0 false)`.
    - **The backdrop plane.** It's optional because only the newest
      `texaco.pov` has the white `ambient 1` plane at z = 10. The older
      `texacobackup.pov` and `texaco_animation.pov` don't, and the
      black-background reference GIFs were rendered without it.
    - **`scenes/texaco_frames.lisp`.** A side-effecting script for
      `sdl_run` that renders the original's 24-frame animation (star
      0° → -180° about y) to `texaco00.png` … `texaco23.png`.
    - **Visual check.** Against `texaco.gif`, the shape, framing, T cut
      and star-in-bowl match, and the 0.001-thick rim renders cleanly.
      The star's shadow inside the bowl is stronger than in the
      reference, and the reference shows more reflected star in the
      walls; both are surface tuning, left for later by choice.
    - **Sampling note.** A near-pixel-aligned horizontal edge on the
      star shows faint speckle at the default adaptive settings. It
      renders as a smooth anti-aliased row at a fixed 64 samples, so
      it's sampler noise (four samples agreeing early), not geometry.
    - **Tests.** New `pov_compass_scene_loads` and `texaco_scene_loads`
      smoke tests. The frames script isn't smoke-tested because it
      writes files.
    - **How it was verified.** As in entry 40: built against stand-in
      libraries, with 47 unit and 63 suite tests passing, and the
      renders and all 24 frames made with that build.

42. **CSG phase 4: torus, performance, merge.**
    - **Torus primitive.**
      - `Torus { center, axis (unit), major, minor, surface }`: the
        points within `minor` of a circle of radius `major` around
        `center`, perpendicular to `axis`.
      - The intersection is a quartic, solved by the new
        `render::poly` module. That module has Cardano/trigonometric
        cubics and a Ferrari quartic via the resolvent cubic
        (Schwarze's Graphics Gems structure), with Newton polishing,
        and its own unit tests, including 500 random four-root
        quartics.
      - To keep the quartic well conditioned, `Torus::local_roots`
        moves the ray into the torus's frame, normalizes the direction,
        and re-origins it where it enters the bounding sphere (radius
        `major + minor`). A ray that misses that sphere can't hit the
        torus.
      - Spans come from the sorted roots. Each gap between consecutive
        roots is classified by testing its midpoint against the implicit
        equation, rather than pairing roots by position, so a tangent
        ray's double root can't flip inside and outside. That gives up
        to two spans.
      - Normals point from the nearest point on the core circle.
        `bounds()` is tight: `major * sqrt(1 - axis[i]²) + minor` per
        axis.
      - SDL: `(torus {:major R :minor r :center [..] :axis [..]
        :surface S})`. `:center` defaults to the origin and `:axis` to
        +y (POV's `torus { R, r }`). The axis is normalized, and the
        binding requires `0 < minor < major`.
      - Tests: unit tests for two-span crossings with exact `t` and
        normals, the tube, the hole, starting inside, a tilted torus,
        bounds, and the xmastree-style grooved stand as CSG. The torus
        was also added to both 2,000-ray span-vs-`hit_test` agreement
        checks. `bindings_shapes.lisp` gained torus cases, and there's
        a new `torus_rejects_bad_parameters` Rust test.
      - New `scenes/torus_test.lisp` (upright ring, grooved stand under
        a gold ring, a ring with a quarter cut away) with a smoke test.
    - **Performance.** Profiling Texaco under callgrind showed CSG span
      evaluation at about 83% of render time, with about a quarter of
      the total in `malloc`/`free`. Two changes, each timed:
      - Span buffers come from a thread-local pool
        (`with_span_buffer`). The set operations append into the
        caller's buffer (`span_*_into`), and groups and tori normalize
        their appended range in place (`normalize_union_tail`, which
        replaces `span_union_of`). Texaco went from 3.8 s to 3.1 s.
      - `csg()` wraps compound operands (group, transform, surfaced,
        CSG) that have finite bounds in `Bounded`. Texaco then took
        2.65 s.
      - Output was byte-identical for texaco, torus_test and csg_test
        after each change. Timings are single-threaded, from the
        sequential `rayon` stand-in.
    - **`merge`.** `CsgOp::Merge`: the union of the operands' spans.
      Unlike a `Group`, whose `hit_test` still sees each child's buried
      surface, a merge has no internal faces, which shows with glass.
      Its bounds are the union of the operands' (none if either is
      unbounded). SDL `(merge a b c …)` groups the extra operands, like
      `difference`. New unit tests, `bindings_csg.lisp` cases, and
      rejection cases.
    - **Not done.** `inverse` was skipped (no ported scene uses it).
      Back faces of transparent primitives were prototyped and reverted
      pending a decision; see the CSG plan's Phase 4.
    - **How it was verified.** As in entries 40–41: built against
      stand-in libraries, with 59 unit and 65 suite tests passing.

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

**Image-y is inverted.** `camera_ray` uses `sy = 1.0 - 2.0 * yt` so that
pixel y=0 is the top of the image. Don't "fix" this unless you also flip
every existing scene's `up_hint`.

**Visual verification of scenes; SDL has unit tests.** The renderer and
the scene definitions are verified visually — render `render.png` and
look. The SDL itself has a thorough integration suite at
`tests/sdl_suite.rs`; that catches script-level breakage (parse errors,
missing bindings, language regressions) but doesn't cover visual
correctness. When changing the renderer or a `scenes/*.lisp` file, the
smell test is "does the output look the same as before for cases that
shouldn't have changed, and right for cases that should?" Render the
same scene before and after the change and diff the outputs.

**Running a scene.** `cargo run --release -- scenes/<name>.lisp`. The
binding is derived from the filename: `<stem>-scene` with `_` → `-`.
`SIZE=N` or `SIZE=WxH` overrides the default 1024×1024 (the teapot at
1024² is slow without a real BVH — bump down to 512 for fast iteration).
`PARALLEL=n` disables Rayon. `RTVIEW_ADDR=host:port` routes pixels to a
live receiver instead of writing render.png.

## Scene definition language: implementation plan

The next major piece of work is a script-driven layer for building and
rendering scenes. This section captures the design and a phased delivery
plan; once each phase ships, its summary moves into "Recent work history"
and the corresponding plan content here is trimmed.

### Goals and shape

The SDL is **lower-level than a POV-Ray-style declarative scene file**.
A script owns control flow: it constructs surfaces, lights, cameras, and
geometry, builds a render target, and explicitly calls `render`. This
is what makes scripted multi-scene compositing and animation possible —
a script builds geometry once and drives it across multiple `(render ...)`
calls or a frame loop into a streaming target.

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
  Syntactic sugar (`defn`, `when`, `when-not`, `cond`, `->`, `->>`)
  is rewritten into these by the desugaring pass in
  `src/sdl/desugar.rs` before evaluation.
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
  protocols, lazy seqs, transducers, varargs, atoms. (The desugaring
  pass is shaped like macroexpansion so user macros can plug into it
  later — see "`defn` and the desugaring pass" below.)

### Module layout

A new top-level module `sdl` alongside `render`. Approximate breakdown:

```
src/sdl/
  mod.rs       Re-exports and the public entry point: read + eval a file.
  reader.rs    Tokenizer + s-expression reader producing AST with source positions.
  ast.rs       AST node definitions (literal, symbol, list, vector, map, ...).
  desugar.rs   Per-top-level-form rewrite of sugar (`defn`, `when`, `cond`,
               `->`, ...) into core forms.
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

**Phase 7 — `load-obj` mesh binding + teapot port.** Done; see
"Recent work history."

**Phase 8 — Remove `src/scenes.rs` (and the surrounding plumbing).**
Done; see "Recent work history." The SDL is now the canonical
scene-definition mechanism.

**Phase 9+ (deferred).** Heatmap target binding; animation
(timestep loops, a video or sequence-of-PNGs target, per-frame
mutation of geometry); interpreter optimizations; transform
collapsing (see "Future directions" — first perf item to tackle
now that SDL parity is complete).

### `defn` and the desugaring pass

Clojure-style `defn` as sugar for `(def name (fn ...))`, implemented
as a separate desugaring pass rather than a special form so that the
same hook can later host real macros.

- **Phase 1 — desugaring pass + `defn`.** Done; see "Recent work
  history" entry 36.
- **Phase 2 — migrate existing definitions.** Done; see "Recent
  work history" entry 37.
- **Phase 3 — move existing sugar into the pass.** Done; see
  "Recent work history" entry 38. `and` / `or` remain special forms
  until there's a hygiene mechanism (`gensym`) for the temporary
  they need.

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
- File-system paths in SDL scripts (`(load ...)`, `(load-obj ...)`)
  resolve relative to the loading file's directory via the
  thread-local `CURRENT_DIR`. Absolute paths are used as-is.
  Falling through to CWD only happens when there's no anchor —
  e.g. an inline script eval'd from Rust without a meaningful
  filename. The script-relative convention lets `scenes/teapot.lisp`
  reference `../models/utah_teapot.obj` without depending on
  what CWD the renderer was invoked from, which is what makes the
  ports portable across `cargo test`, `cargo run`, and any future
  external invocation.

## Adaptive oversampling: implementation plan

The current oversampling does anti-aliasing via a fixed N×N sub-pixel
grid — every pixel costs the same whether it's in a flat region or
sitting on a geometric edge. Adaptive oversampling redirects work to
where it matters: take a few samples, check variance, and only continue
sampling pixels that need more. The expected payoff is a substantial
speed-up on every existing scene with no visual quality loss, plus a
sampling infrastructure that future DOF work (which introduces a second
source of pixel-level variance — out-of-focus regions) hooks into
directly.

The work splits into three phases. Phase 1 swaps the fixed sub-pixel
grid for a sample-index-keyed quasi-random sequence — same sample count
per pixel, different sample positions — to establish the "sample i has
a well-defined offset regardless of total count" abstraction that
adaptive needs. Phase 2 adds variance-driven termination. Phase 3 is
diagnostics and polish.

### Phase 1 — Sample-indexed sampling foundation

Done; see "Recent work history."

### Phase 2 — Adaptive termination

Done; see "Recent work history."

### Phase 3 — Diagnostics and polish

Sample-count heatmap landed — see "Recent work history." Remaining
candidate:

- A way to disable adaptive sampling for benchmark comparisons —
  either an env var (`ADAPTIVE=0` forces uniform sampling at
  `max_samples`) or a Scene field. Useful for sanity-checking the
  speed-up and for visual diffing against the pre-adaptive renderer.
  Hasn't shipped because there's no concrete need yet; landing the
  sample-count heatmap was the more useful diagnostic.

### Decisions still open

Nothing structural — Phases 1 and 2 settled the sampler shape and the
adaptive-loop shape. Phase 3 candidates above are all opt-in tuning
rather than required design choices.

Settled in Phase 1:

- **Sequence choice.** Halton (2, 3). Hand-rolled radical inverse in
  `src/render/sampler.rs`; no new dependency.
- **Cranley-Patterson rotation.** Yes. Splitmix64-style hash of
  `(x, y)` shifts each pixel's Halton sequence by a distinct
  `(ox, oy)` in `[0, 1)²`, decorrelating neighbors.

Settled in Phase 2:

- **Variance metric.** Per-channel min/max spread, `max - min`
  compared against `variance_threshold`. Simpler than running
  statistical variance, threshold is intuitive ("any channel allowed
  to differ by this much across samples"), and the per-sample
  bookkeeping is two channels of min and max rather than `sum +
  sum_of_squares`.
- **SDL migration shape.** `:oversample` deleted, `:min-samples`,
  `:max-samples`, `:variance-threshold` added with defaults that
  reproduce the previous fixed-grid cost on flat regions and adapt
  up on the parts that need more. Legacy `:oversample` rejected
  with an explicit migration error.
- **Batch size for adaptive checks.** `SAMPLE_BATCH = 4`. Variance
  estimates are noisy at very low sample counts; batching amortizes
  the metric cost and lets the floor on `min_samples` filter out
  spurious early-termination on accidental sample agreement.

### Verification

Phase 2 should produce noticeably faster `cargo run --release` timings
on every existing scene with no visible quality regression. The two
heatmap outputs in `main.rs` give complementary views: `render-heatmap.png`
shows per-pixel render time (where the renderer spent wall clock),
`render-samples.png` shows per-pixel adaptive sample count (where the
sampler kept going past `min_samples`). Bright pixels on geometric edges,
dim pixels in flat regions — that's adaptive sampling working as
intended. End-to-end check is rendering `scenes/teapot.lisp` (or another
non-trivial scene) before and after a change and comparing the
elapsed-time line and the rendered PNG. PNG should look equivalent;
time should improve.

### Relationship to depth of field

DOF Phase 1 has since landed on top of this work — see "Depth of
field: implementation plan". The sampler abstraction from Phase 1 is
what made it cheap: aperture jittering uses the same
sample-index-keyed sequence (`halton_lens`, distinct bases from the
sub-pixel `halton_pair`), so the lens sample for sample `i` has a
well-defined position no matter how many samples a pixel takes. The
claim that adaptive termination "routes the extra samples DOF needs
to the out-of-focus pixels for free" is exactly what DOF Phase 2
sets out to verify against the sample-count heatmap.

## Transparency / transmission: implementation plan

Adds see-through surfaces to the renderer. The work split into two
phases that shipped back-to-back, and both are now done — this plan
is complete. Phase 1 delivered transmission for primary and
reflection rays (non-refractive); Phase 2 made shadow rays honor
transparency. Refraction — Snell's-law bending and per-surface IOR —
was explicitly *not* part of this plan; it's a later, larger piece
of work that builds on the transmission machinery landed here (see
the refraction note in "Future directions").

### Phase 1 — Transmission through surfaces

Done; see "Recent work history" entry 27. Summary: `Surface` gained
`transparency: f64`, `Scene` gained `transmit_limit: u32`, a private
`Depth { reflect, transmit }` struct replaced the bare
`reflect_count` parameter, and `shade_pixel` blends
`lerp(opaque, transmitted, transparency)` using a straight-through
(non-refractive) transmitted ray. SDL surface: `:transparency` on
`(surface ...)`, `:transmit-limit` on `(scene ...)`, a `glassy`
helper in `_common.lisp`.

### Phase 2 — Transparent shadows

Done; see "Recent work history" entry 28. Summary: `light_vector`
changed from a binary `Option<Vector>` test to an
accumulated-transmittance walk `Option<(Vector, f64)>` — it steps
the shadow ray from light to shaded point with a
repeated-nearest-hit loop, multiplying a running transmittance by
each occluder's `transparency`, with an opaque occluder zeroing it
(early-out → `None`). `shade_pixel` scales each light's contribution
by the returned transmittance. Two judgment calls settled at
implementation time: transmittance is a *scalar* (greyscale
attenuation, not colored-shadow tinting — consistent with Phase 1's
untinted transmission; colored shadows deferred to land with colored
transmission); and the per-occluder factor is `occluder.transparency`
directly, not the `(1 - transparency)` the plan draft had — the
draft's polarity was backwards (opaque is `transparency == 0.0` and
must drive transmittance to 0). No new `Surface` / `Scene` fields,
no SDL surface change — Phase 2 is purely internal to the shadow-ray
traversal. The byte-pinned tests use only opaque surfaces, so they
are bit-for-bit unaffected.

### Verification

Phase 1: render `scenes/transparency_test.lisp` and confirm the red
sphere and checker floor are visible through the glassy sphere,
blended by the 0.7 coefficient, with the glassy sphere's own shading
still present. Phase 2: the same scene's glassy sphere should cast a
*lightened*, partial shadow on the checker floor rather than a solid
one. Existing scenes must render byte-identically across both phases
(they do — `transparency` defaults to 0.0 and opaque occluders
reproduce the old binary shadow behavior exactly), which the
byte-pinned tests in `tests/sdl_suite.rs` pin down.

## Depth of field: implementation plan

Adds a thin-lens camera so scenes can have depth-of-field blur —
geometry away from the focus plane goes soft, the amount of blur set
by an aperture radius. The work splits into two phases. Phase 1 (the
complete rendering-side feature) is done; Phase 2 (ergonomics and
adaptive-sampling interplay) is optional polish that benefits from
landing after Phase 1 is renderable.

### The model

A thin-lens camera replaces the pinhole. A pinhole camera originates
every primary ray at the single point `camera.location`; the
thin-lens version picks a jittered origin on a disk of
`aperture_radius` around `location` (in the `right`/`up` plane) and
aims it at the **focal point** — the spot on the focus plane
(perpendicular to `forward`, at `focus_distance`) the pinhole ray
would have passed through. Rays for in-focus geometry converge
regardless of where on the lens they start; rays for out-of-focus
geometry spread, producing blur. `aperture_radius = 0` reduces to the
pinhole exactly — and `camera_ray` keeps that as an explicit branch
so it's *bit-identical*, not just numerically close (see entry 29).

### Phase 1 — Thin-lens camera

Done; see "Recent work history" entry 29. Summary: `Camera` gained
`aperture_radius` + `focus_distance`, `Camera::with_dof` constructs a
DOF camera (focus on the look-at point), `camera_ray` gained a lens
sample parameter and a pinhole fast-path branch, `render::sampler`
gained `halton_lens` / `concentric_disk` /
`cranley_patterson_lens_offset`, and `pixel_color` threads a
per-sample lens point through — but only when the aperture is
nonzero. SDL: a positional `(camera-dof ...)` constructor. Existing
scenes render byte-identically (aperture defaults to 0.0).

### Phase 2 — Ergonomics and adaptive interplay

Deferred so Phase 1 stayed focused on a working core. Candidates,
none of them required for the feature to function:

- **Adaptive-sampling interaction.** DOF is the first feature to
  introduce pixel variance that *isn't* a geometric edge — the
  adaptive-oversampling plan anticipated this ("DOF gets adaptive
  sample distribution for free"). Phase 2 verifies that with the
  existing `render-samples.png` heatmap: out-of-focus regions
  *should* pull more samples automatically. If a smoothly-blurred
  area instead under-samples (low local variance but still visibly
  noisy) or over-samples badly, that's a real tuning question —
  possibly `min_samples` guidance, possibly a metric tweak. Can't be
  assessed until Phase 1 is renderable, which is why it's its own
  checkpoint.
- **Focus and aperture ergonomics.** An explicit focus distance
  independent of the look-at point (focus nearer/farther than the
  subject); aperture expressed as an f-number rather than a raw
  world-unit radius; possibly an autofocus-on-a-named-object helper.
  Which of these earn their keep is easier to judge with Phase 1 in
  hand. A new positional constructor is the likely shape for any of
  them — the user has flagged possible SDL geometry-format changes
  that could affect a map-keyed camera later, so positional stays
  the convention for now.
- **A DOF-specific diagnostic** if one turns out to be warranted.

### Verification

Phase 1: render `scenes/depth_of_field_test.lisp` — the middle green
sphere (on the focus plane) should be sharp, the red and blue
spheres (nearer and farther) visibly blurred, the blur growing with
the `0.3` aperture radius. Existing scenes must render
byte-identically (they do — aperture defaults to 0.0 and
`camera_ray`'s pinhole branch is bit-identical), which the
byte-pinned tests in `tests/sdl_suite.rs` pin down. The lens sampler
functions have unit tests in `src/render/sampler.rs` (concentric map
stays in the unit disk, landmarks map correctly, lens CP rotation is
decorrelated from the sub-pixel rotation).

## Light types: spotlights and area lights — implementation plan

Adds non-point light types to the renderer. Spotlights first
(directional cone with smooth falloff), then area lights (extended
emitters that produce soft shadows). The work splits into six
phases, three for spotlights and three for area lights, with the
last in each group an optional polish step that can be picked up
later. Each non-polish phase is sized to land in one session and
leave the codebase shippable.

### Design

**Light shape.** The Future-directions bullet anticipated turning
`Light` into an enum "following the same pattern as `Shape`." The
recommended concrete shape is a *hybrid*: keep `Light` as a struct
with the fields every light has in common (`location`, `color`,
`intensity`) and add a `kind: LightKind` field that carries the
variant-specific data. Reasons: those three fields are genuinely
shared (read directly in `shade_pixel` as `l.color`, `l.intensity`,
and in `collect_lights` as `l.location`), so a pure enum would
force either accessor methods or `match` arms at every field
read. The hybrid keeps mechanical churn low while still getting
variant dispatch where it actually matters — `collect_lights`
(different fields to transform per variant) and the shadow-ray
helper (different sampling and falloff per variant). This decision
is flagged below.

**Where variant dispatch lives.** Two places: `collect_lights`
already walks the scene graph extracting world-space lights — it
needs to transform extra per-variant fields (a spotlight's
direction, an area light's extent axes) under the accumulated
affine. And the shadow-ray helper currently named `light_vector`
needs to apply per-variant attenuation (spotlight cone falloff,
area light sample-point selection) on top of the transmittance
walk it already does. `shade_pixel` stays light-type-agnostic — it
already just consumes `(Vector, f64)` and does Lambert + Phong
with the returned direction; we route all the type-specific work
through the scalar.

**Sampling strategy for area lights.** Area lights produce soft
shadows by taking *multiple* shadow rays to different points on
the emitter's surface and averaging. There are two ways to
distribute those rays:

- Fixed N shadow samples per area light per shaded point.
  Predictable, but multiplies shadow-ray cost by N for every
  pixel regardless of whether that pixel is in penumbra or not.
- One shadow sample per *pixel sample*, jittered using the
  existing Halton + Cranley-Patterson sampler in
  `render::sampler` with a dedicated base pair (e.g. bases
  11, 13 — distinct from sub-pixel 2,3 and lens 5,7). The
  adaptive oversampler then drives extra samples into penumbra
  pixels naturally, because that's where per-pixel variance is
  high. Fully shadowed and fully lit pixels terminate at
  `min_samples` and pay almost nothing.

The second is the codebase-aligned choice. Both the adaptive
oversampling plan and the DOF plan explicitly anticipate it
("area lights get adaptive sample distribution for free" is the
same observation as "DOF gets adaptive sample distribution for
free"). The cost is one structural change: thread a per-sample
2D light coordinate down from `pixel_color` through `ray_color` /
`shade_pixel` to the light-sampling helper. Point lights and
spotlights ignore the coordinate; only the area variant consumes
it.

### Phase 1 — `LightKind` refactor (no behavior change)

Done; see "Recent work history." Summary: `Light` gained a
`kind: LightKind` field, `LightKind` is a one-arm enum (`Point`),
constructors set `kind: LightKind::Point`. `light_vector` is now a
thin dispatcher matching on `light.kind` that delegates to
`light_vector_point` (the pre-Phase-1 body, unchanged); Phase 2's
`light_vector_spot` slots in as a sibling. `Shape::collect_lights`
propagates `kind` through to the world-space `Light` it builds.
`shade_pixel` is unchanged — it stays light-type-agnostic.
Every existing scene renders byte-identically; the byte-pinned
tests in `tests/sdl_suite.rs` pass without modification.

### Phase 2 — Spotlight (`Spot` variant)

Done; see "Recent work history." Summary: `LightKind` gained a
`Spot { direction, inner_angle, outer_angle }` arm; `Light::spot`
is the `const fn` constructor. The shadow-ray helper
`light_vector_spot` applies an early `smoothstep(cos(outer),
cos(inner), cos_theta)` cone-falloff check before delegating to
`light_vector_point` for the transmittance walk, so shaded points
outside the outer cone skip the walk entirely.
`Shape::collect_lights` transforms `direction` by the linear part
of the accumulated affine and renormalizes when collecting a
`Spot`-kinded light. SDL: positional `(light-spot location
direction color intensity inner-angle outer-angle)`; the binding
normalizes the direction and rejects `inner > outer` at the
boundary. New `scenes/spotlight_test.lisp` + smoke test, plus
spotlight cases added to `tests/sdl/bindings_lights.lisp`. Point
lights flow through the `Point` arm unchanged, so existing
byte-pinned tests pass without modification.

### Phase 3 — Spotlight ergonomics (optional, deferred)

Polish items that can be picked up if and when they earn their
keep:

- An aim-at-target constructor: `(light-spot-aimed location
  target color intensity inner-angle outer-angle)` computes
  `direction = normalize(target - location)`. Usually more
  natural than supplying a direction vector directly. Likely
  shipped as the *primary* constructor in `_common.lisp` with
  the raw-direction form available for explicit cases.
- Distance attenuation. No light currently has distance falloff,
  and adding it for spotlights only would be inconsistent. If
  it lands, it lands on the base `Light` struct (controlled by
  an explicit field rather than tied to a variant). Out of
  scope for Phase 2 either way.
- A diagnostic visualization mode that draws cone outlines for
  spotlights — useful while authoring scenes.

None of these are required for the feature to function. Phase 3
exists as a placeholder so the work has a clear deferred bucket.

### Phase 4 — Area light: type + geometry (hard-shadow baseline)

Done; see "Recent work history." Summary: `LightKind` gained a
`Area { axis, radius }` arm; `Light::area` is the `const fn`
constructor. The shadow-ray helper `light_vector_area` computes a
Lambertian cosine attenuation `dot(axis, normalize(point -
light.location))` against the disk's normal — returns `None` for
points in the back hemisphere (skip the light entirely) and
otherwise delegates to `light_vector_point` for the transmittance
walk, folding the cosine factor into the returned scalar. Phase 4
samples the disk *center* only, so shadows are hard; Phase 5 will
replace the center-only sampling with per-pixel-sample jittered
points on the disk for soft shadows. `Shape::collect_lights`
transforms `axis` by the linear part of the accumulated affine and
renormalizes; `radius` is left alone (Phase 6 polish). SDL:
positional `(light-area location axis radius color intensity)`;
the binding normalizes the axis and rejects zero-length axis and
non-positive radius at the boundary. New
`scenes/area_light_test.lisp` + smoke test, plus area-light cases
added to `tests/sdl/bindings_lights.lisp`. Point and spot lights
flow through their respective `LightKind` arms unchanged, so
existing byte-pinned tests pass without modification.

### Phase 5 — Area light: soft shadows

Done; see "Recent work history." Summary: `render::sampler`
gained `halton_area` (bases 11, 13) and
`cranley_patterson_area_offset` (seed 2, distinct from pixel's
0 and lens's 1). `pixel_color` checks once per pixel whether any
of the scene's effective lights is `LightKind::Area`; if so, it
computes the area-CP offset once and generates a per-pixel-
sample `light_coord` (the area Halton point shifted by the CP
offset) — same hoisted-flag pattern as the DOF lens
coordinate, so point/spot-only scenes pay nothing for the
feature. `ray_color` and `shade_pixel` gained a `light_coord:
(f64, f64)` parameter threaded by value (kept separate from
`Depth` per the plan's recommendation); `light_vector`
dispatches it into `light_vector_area`, where it maps through
`concentric_disk` to a unit-disk point and gets oriented into
world space by a new `disk_basis(axis)` helper to produce the
shadow-ray origin (replacing `light.location`). The
transmittance walk itself was extracted from
`light_vector_point` into a `shadow_ray_walk(origin, target,
scene)` helper that area lights now share — point lights still
go through it with `origin = light.location`, producing
bit-identical output to the pre-refactor code. Recursive rays
(reflection, transmission) reuse the pixel sample's same
`light_coord` rather than re-deriving one per bounce, mirroring
the DOF "reflection rays don't get fresh aperture jitter" call.
New `scenes/soft_shadow_test.lisp` + `soft_shadow_test_scene_loads`
smoke test; `area_light_test` from Phase 4 transitions from a
hard shadow at disk center to a soft shadow sampled across the
disk (intended behavior change; the smoke test only verifies
loading, so nothing breaks). Existing byte-pinned tests pass
without modification.

### Phase 6 — Area light polish (optional, deferred)

Polish items, sized like Phase 3:

- Quad area lights. Add `LightKind::Area { shape: AreaShape,
  axis, ... }` (or a separate variant) carrying the second
  basis vector for a parallelogram emitter. Sample map is the
  trivial unit-square map rather than `concentric_disk`. Most
  of the renderer work is reusable from the disk case.
- Sphere area lights — bulb-shaped emitters. The sampling
  geometry is different enough to be a real chunk of work
  (uniform spherical-cap sampling); worth its own phase if it
  lands.
- Importance sampling of the area light (sample weighted by
  the solid angle the light subtends from the shaded point
  rather than uniformly over the emitter's surface). Reduces
  variance in penumbra at no per-sample cost. Worth measuring
  against the unbiased uniform sample before committing.
- Light-visible-as-geometry: make the area light's disk show
  up in primary rays as a glowing surface rather than being
  invisible like other lights. Currently `Shape::Light` is
  `None` for all ray types; an area light is the first kind
  where visibility makes physical sense (you can *see* the
  bulb of a desk lamp). The cleanest path is probably an
  optional emissive surface on the disk, not a Light variant
  change.
- Guidance for `min_samples` when an area light is in the
  scene. The default `min_samples = 4` may under-resolve
  penumbra under some authoring choices; a per-scene
  recommendation or auto-bump may earn its keep.

### Decisions still open

- **Hybrid struct + `LightKind` vs. pure enum.** The
  Future-directions bullet's "enum like `Shape`" framing was
  written before the field-access patterns were fully
  internalized. The hybrid is recommended above; flag this for
  review since it is a deliberate departure from the original
  framing.
- **Spotlight inner/outer angles vs. cosines.** Store as
  angles (radians) for debuggability; cosines are a
  micro-optimization worth measuring before committing to. If
  the constructor precomputes and caches the cosines, the
  per-shade cost is the same either way.
- **Spotlight aim-at-target shape.** Whether the primary
  constructor is direction-based (Phase 2 default) or
  target-based (Phase 3 polish). Either order is workable;
  the recommendation is direction-based first for symmetry
  with how `(camera-looking-at ...)` exposes both a location
  and a target.
- **Area-light geometry: disk-first vs. quad-first.** Disk
  recommended for the reasons listed under Phase 4. Easy to
  reverse if a use case shows up that wants a rectangular
  emitter (a softbox, a glowing window) before the disk
  feels limiting.
- **Light-sample coordinate threading: alongside `Depth` vs.
  combined into a `RayContext` blob.** Recommend separate;
  flagged for revisit if a third per-ray piece of state
  shows up.
- **What to name the shadow-ray helper.** It currently lives
  as `light_vector` and returns a `(Vector, f64)`. With area
  lights it becomes a *sample* (the f64 may fold in cone
  falloff and disk-cosine attenuation, not just transmittance).
  A rename to `light_sample` would communicate the broader
  responsibility; left to taste.

### Verification

Phase 1: every byte-pinned test in `tests/sdl_suite.rs` passes
unchanged. The whole point of Phase 1 is "no behavior change,
shape only" — anything else means the refactor isn't right.

Phase 2: render `scenes/spotlight_test.lisp` and confirm a cone
of light hits the floor with a visibly smooth edge between
inner and outer angles, geometry outside the cone is unlit by
the spotlight (but still receives ambient + any other lights),
and rotating the scene under a `(rotate-y ...)` rotates the
spotlight's cone direction with the geometry (because
`collect_lights` transforms `direction`). Point-light scenes
unaffected.

Phase 4: render `scenes/area_light_test.lisp` and confirm the
disk light illuminates the half-space its front face points
into and is dark on the back side. Shadows are *hard* at this
phase — a marker that Phase 5 still has work to do.

Phase 5: re-render `scenes/area_light_test.lisp` and confirm
shadows are now visibly soft (a penumbra band around the hard
core). Render `scenes/soft_shadow_test.lisp` and confirm the
characteristic gradient. Most diagnostically, render the
`render-samples.png` heatmap: the penumbra region should be
distinctly brighter than the fully-lit and fully-shadowed
regions around it, confirming the adaptive sampler is driving
extra samples exactly where soft-shadow variance is highest.
Point-light and spotlight scenes from earlier phases are
byte-identical: those variants ignore the new sample coordinate.

### Out of scope

- Directional lights (the sun: parallel rays from infinity).
  Mentioned in the original Future-directions bullet. A
  natural `LightKind::Directional { direction }` variant
  would land in roughly the same shape as `Spot` minus the
  cone math; deferred because spot + area covers the
  artistic surface area that motivated this work.
- Volumetric / participating-media lighting (god rays through
  fog). Substantially more invasive — different rendering
  equation, different integrator. Out of scope.
- Light-temperature / blackbody-based color authoring. The
  existing per-light `color` channel already exposes any tint;
  a `kelvin` constructor would be ergonomics, no new variant.

## CSG: implementation plan

Adds constructive solid geometry — `difference` and `intersection` of
solid shapes — so scenes can cut and combine primitives. This is the
first step of the POV-Ray port (see `docs/povray_gap_analysis.md`):
the Texaco star, which is a cylinder with five wedges and a "T" cut out
of it, and the Texaco bowl, a sphere minus a slightly smaller sphere
minus a cylinder, both need it, as do most of the other POV projects.
The work splits into three phases, plus a deferred bucket. Phase 1 adds
the query CSG needs without changing any output. Phase 2 adds the CSG
node and its SDL surface. Phase 3 is the Texaco port, which serves as
the end-to-end test.

### The model: span lists

`hit_test` answers "where does this ray first hit the surface?". CSG
needs more: at each point along the ray, *is the ray inside the
solid?* The standard answer is to describe each solid, for a given
ray, as a sorted list of **spans** — parameter intervals `[t_in, t_out]`
where the ray is inside the solid — and to combine span lists with
interval set operations:

- **Union** (a `Group`): merge the lists, coalescing overlaps.
- **Intersection**: keep only the ranges covered by both lists.
- **Difference** `A − B`: keep the ranges of A not covered by B. Where
  a B span cuts into an A span, the new boundary is a B surface *seen
  from inside B*, so its normal is **negated**. That flip is the
  classic CSG bug to watch for. It's why the bowl's inner wall lights
  correctly.

Each span endpoint carries what `hit_test` would have returned at that
point: `t`, the outward normal, and the surface (`Option<Surface>`, so
the `Surfaced` "innermost wins" rule keeps working). The hit point
isn't stored; it's recomputed from the ray and `t` when a hit is
returned, exactly as `Transformed::hit_test` already does.

Spans cover the **whole line**, negative `t` included. A span that
starts behind the ray origin is how "the ray starts inside this solid"
is represented, and a correct inside/outside state at `t > EPSILON`
depends on it. Spans that end at or before 0 can be dropped, because
they can't affect anything in front of the ray. That also means the
existing `AABB::intersects` test (which already keeps boxes containing
the origin) is a valid early-out for spans too.

Convex primitives — sphere, cuboid, cylinder, cone — produce at most
one span per ray. A plane acts as a **half-space**: the solid side is
the side opposite `normal`, and the span is `(-∞, t]` or `[t, +∞)`
(or the whole line or nothing, for a ray parallel to it). Infinite
endpoints never become hits: only finite `t > EPSILON` endpoints do.
The future torus (xmastree) is the first non-convex solid and can
produce two spans, which is why the representation is a list and not a
single interval.

Triangles and meshes aren't solids (a triangle has no inside), so they
aren't allowed as CSG operands. That's enforced when the scene is
built, not silently mis-rendered.

### Phase 1 — Span query (no behavior change)

Done; see "Recent work history" entry 39. Summary: `SpanEnd` / `Span`
types, `Shape::spans` for every variant (convex primitives give one
span, `Plane` is a half-space with infinite endpoints, `Group` is a
union, `Transform`/`Surfaced`/`Bounded` pass through, and `Triangle`
and `Light` give nothing), the free functions `span_union`,
`span_intersection` and `span_difference` (normals of subtracted
boundaries flipped), and `first_span_hit` for Phase 2's `hit_test`.
Unit tests in `shapes.rs`, including a check against `hit_test` over
thousands of rays. Output is byte-identical because nothing calls
the new code yet.

A side effect worth writing down: `Sphere` and `Cuboid` currently
treat a ray that starts inside them as a miss, so a transmitted ray
through a `glassy` sphere never sees the back face. Spans have both
faces, so Phase 2's CSG node gets this right automatically. Changing
the primitives' own `hit_test` would alter existing renders, so that
stays out of this plan (see Phase 4).

### Phase 2 — The `Csg` node and SDL bindings

Done; see "Recent work history" entry 40. Summary:

- `Shape::Csg(Box<Csg { op, a, b }>)` computes its spans from its
  operands' spans and uses `first_span_hit` as its `hit_test`.
- `bounds`, `validate_surfaces`, `collect_lights` and the SDL
  `Display` arm are updated.
- `Shape::is_solid` gates the `difference` / `intersection`
  constructors.
- SDL `(difference a b c …)` = `a − (b ∪ c ∪ …)` and a left-folding
  `(intersection …)`, both rejecting triangles and meshes with a
  positioned error.
- Unit tests, a binding test script, a rejection test, and the
  `csg_test` scene.

The open decisions below were settled as recommended: binary `Csg`
with n-ary difference expanded in the binding, `Vec<Span>` storage,
and outward-of-solid normals on exit hits.

### Phase 3 — Texaco port (acceptance test)

Done; see "Recent work history" entry 41. Summary:

- `scenes/_pov.lisp` holds the porting helpers and conventions.
- `scenes/pov_compass.lisp` confirms POV coordinates and rotations
  carry over unchanged.
- `scenes/texaco.lisp` is a structural port of the logo whose
  geometry matches `texaco.gif`.
- `scenes/texaco_frames.lisp` renders the 24-frame animation through
  `sdl_run`.

Surface tuning is deliberately left for later.

### Phase 4 — Follow-ons

Done except as noted; see "Recent work history" entry 42.

- **Torus spans.** Done, with the torus primitive itself: the first
  solid with more than one span per ray.
- **Performance.** Done. CSG span lists come from a per-thread buffer
  pool, and groups and tori normalize their spans in place. Compound
  CSG operands are wrapped in `Bounded` automatically. Texaco renders
  about 30% faster, byte-identically.
- **`merge`.** Done: `CsgOp::Merge` and `(merge a b …)`.
- **`inverse`.** Skipped: no ported scene uses it (a keyword scan of
  every POV file found none), which was this item's condition.
- **Back faces of transparent primitives.** Not done; waiting on a
  decision. A prototype (`Sphere` / `Cuboid` `hit_test` falling back to
  the exit crossing when the entry is behind the ray) was rendered
  against `transparency_test` and reverted. The results:
  - Glass spheres look denser, because the back face also blends in.
  - Shadows darken, because the shadow walk now crosses two surfaces.
  - A dark crescent appears where the far wall is seen from inside with
    its outward normal, facing away from the light.
  It probably wants to land together with normals that face the ray for
  shading.

### Verification

- **Phase 1:** the new unit tests pass, and every byte-pinned test in
  `tests/sdl_suite.rs` passes unchanged, because nothing renders
  differently.
- **Phase 2:** render `scenes/csg_test.lisp`:
  - Cut faces are lit on the correct side (the flipped-normal check).
  - Shadows follow the cut shapes, not the uncut primitives.
  - The bowl's inner wall is visible and lit.
  - Reflections show the cut geometry.
  - The `glassy` operand's back face is visible through it.
  - Existing scenes stay byte-identical.
- **Phase 3:** `scenes/texaco.lisp` matches `texaco.gif` in shape and
  composition (color and finish differences are expected), and the
  compass has red +x to the right, green +y up, and blue +z away from
  the camera.

### Decisions still open

- **Span storage.** A `Vec<Span>` per call is simplest and fine for
  Texaco. Revisit only under Phase 4 performance work.
- **Where n-ary difference is expanded.** The plan puts it in the
  binding (wrapping the extra operands in a group). A Rust n-ary
  `Csg` would save a node but widen the enum; binary is recommended.
- **Exit-hit normal orientation.** Keep outward-of-solid, matching the
  primitives, unless the transparency work wants to revisit it for
  every primitive at once.

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

**CSG.** Planned — see "CSG: implementation plan" above. First
step of the POV-Ray port (`docs/povray_gap_analysis.md`).

**More primitives.** Sphere, plane, cuboid, triangle, cylinder, cone and
torus are all in. Each new primitive is a struct + `Hittable` impl + a
new `Shape` variant + `From` impl + match arms (`hit_test` dispatch,
`bounds()`, `spans()` for CSG, `is_solid()`, `validate_surfaces`,
`collect_lights` leaf-noop, and the SDL `value.rs` Display arm + a
`bindings.rs` constructor).

**More mesh formats.** PLY would be a clean addition (fits academic
test models like the Stanford bunny); the loader interface is already
shaped right — add a `load_ply` to `mesh.rs` that returns `Shape` the
same way `load_obj` does.

**Shadow-ray traversal optimization.** Since Phase 2 of the
transparency work, `light_vector` walks the shadow ray with a
repeated-nearest-hit loop, accumulating transmittance through
transparent occluders. For the common all-opaque case this is still
"find one occluder and stop" (the first opaque hit zeroes
transmittance and returns immediately), but each step calls the full
`hit_test`, which computes a hit point, normal, and surface it
doesn't need — a shadow ray only needs *whether* something opaque is
in the way (or, for a transparent occluder, just its `transparency`).
A dedicated traversal that returns the minimal information — a
boolean for opaque-occluder-found, or just the transparency of the
next occluder — would speed shadow tests up, especially in scenes
with many lights or many objects. The early-exit is already
"transmittance hit 0"; this is about making each step cheaper, not
changing the loop shape. Cleanly self-contained.

**Light types beyond point.** Spotlights and area lights are the
next planned chunk of work — see the dedicated "Light types:
spotlights and area lights — implementation plan" section above
for the design and phasing. Directional lights (the sun: parallel
rays from infinity) are explicitly out of scope of that plan but
would land in roughly the same shape as the spotlight variant
minus the cone math.

**Refraction.** Non-refractive transparency has landed (see
"Transparency / transmission: implementation plan" and "Surface
model"). Refraction proper is substantially more involved — requires
Snell's-law bending of the transmitted ray, Fresnel equations, IOR
per surface, and accounting for the medium the ray is currently
traveling through — but it builds directly on the Phase 1
transmission machinery (the transmitted-ray cast in `shade_pixel`,
the `transmit` recursion budget in `Depth`); refraction is "bend the
transmitted ray and weight reflection vs. transmission by Fresnel"
rather than a from-scratch feature.

**Depth of field.** Phase 1 (the thin-lens camera) has landed — see
"Depth of field: implementation plan" and "The Camera". What's left
is the Phase 2 ergonomics work captured in that plan section
(explicit focus distance, f-number aperture, verifying the
adaptive-sampling interplay).

**Scene definition language.** See the dedicated "Scene definition
language: implementation plan" section above — this is the next major
piece of work, and the design and phasing are captured there rather than
in this list.

**Camera animation.** Now that `default_camera()` is a function returning a
fresh `Camera`, varying its parameters per frame is one new function call.
Render multiple frames, encode as video.

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
