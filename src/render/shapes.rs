// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

use crate::render::{
    Light,
    Point,
    Surface,
    Hittable,
    Vector,
    RayHit,
    addp,
    subp,
    dotp,
    crossp,
    lenp,
    negp,
    scalep,
    ray_location,
    normalizep,
    EPSILON,
};

use crate::render::transform::{
    Affine,
    Mat3,
    mat3_apply,
    mat3_transpose,
};

#[derive(Clone, PartialEq, Debug)]
pub struct Sphere {
    pub center: Point,
    pub r: f64,
    pub surface: Surface,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Plane {
    pub normal: Point,
    pub p0: Point,
    pub surface: Surface,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Cuboid {
    pub center: Point,
    pub size: Point,
    pub surface: Surface,
}

/// One triangle of a mesh. Carries three vertices and three per-vertex
/// normals (which may all be the same in the flat-shading case if the
/// source mesh didn't supply per-vertex normals). Surface is per-triangle
/// here; mesh loaders typically clone one surface across every triangle
/// in a mesh, but the representation supports per-triangle materials too.
#[derive(Clone, PartialEq, Debug)]
pub struct Triangle {
    pub vertices: [Point; 3],
    pub normals: [Point; 3],
    pub surface: Surface,
}

/// A finite, closed, solid cylinder, parameterized by the centers of its two
/// end caps and a radius. The axis is the segment from `p0` to `p1`; the
/// curved side surface is the set of points at distance `r` from that
/// segment, and the two end caps are flat disks of radius `r` centered at
/// `p0` and `p1`. Rays are tested against all three surfaces; the nearest
/// qualifying hit wins.
///
/// A note for future-you on transforms: a uniformly-scaled cylinder is still
/// a cylinder, but a non-uniformly-scaled cylinder is an *elliptical*
/// cylinder, which this primitive can't represent. Wrap cylinders in
/// `Transform` (rotate, translate, uniform scale) and the math stays correct;
/// don't try to bake a non-uniform scale into `r` or `(p1 - p0)`.
#[derive(Clone, PartialEq, Debug)]
pub struct Cylinder {
    pub p0: Point,
    pub p1: Point,
    pub r: f64,
    pub surface: Surface,
}

/// A finite, closed, solid cone, parameterized the same way as `Cylinder` —
/// two end centers and a radius — but with one end collapsed to a point.
/// `p0` is the **base** center: the flat circular cap of radius `r`. `p1`
/// is the **apex**: a single point, no cap. The axis is the segment from
/// `p0` to `p1`; the curved lateral surface is the set of points whose
/// angle off the axis (measured from the apex) equals the cone's
/// half-angle `atan(r / |p1 - p0|)`. Rays are tested against the lateral
/// surface and the single base cap; the nearer qualifying hit wins.
///
/// Unlike `Cylinder`, the two ends are **not** interchangeable — `p0`
/// carries the radius, `p1` is the point. Swapping them turns the cone
/// inside out.
///
/// The same transform caveat as `Cylinder` applies: a uniformly-scaled
/// cone is still a cone, but a non-uniformly-scaled cone is an elliptical
/// cone this primitive can't represent. Wrap cones in `Transform`
/// (rotate, translate, uniform scale); don't bake a non-uniform scale
/// into `r` or `(p1 - p0)`.
#[derive(Clone, PartialEq, Debug)]
pub struct Cone {
    pub p0: Point,
    pub p1: Point,
    pub r: f64,
    pub surface: Surface,
}

/// Axis-aligned bounding box. Used as the acceleration primitive for the
/// `Bounded` variant: a ray that misses the AABB doesn't need to recurse
/// into the wrapped subtree at all.
///
/// Geometrically identical to `Cuboid` — same min/max corners — but kept
/// as a separate type because its job is different. `AABB::intersects`
/// returns just a bool (no surface, no normal, no hit point), which is
/// meaningfully cheaper than `Cuboid::hit_test`. The `to_cuboid` method
/// converts an AABB into a renderable `Cuboid` for visualization, which
/// is useful for diagnosing bounds.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct AABB {
    pub min: Point,
    pub max: Point,
}

impl AABB {
    pub fn new(min: Point, max: Point) -> Self {
        AABB { min, max }
    }

    /// Smallest AABB that encloses both `self` and `other`. Used to
    /// compute group bounds by accumulating across children.
    pub fn union(&self, other: &AABB) -> AABB {
        AABB {
            min: [
                self.min[0].min(other.min[0]),
                self.min[1].min(other.min[1]),
                self.min[2].min(other.min[2]),
            ],
            max: [
                self.max[0].max(other.max[0]),
                self.max[1].max(other.max[1]),
                self.max[2].max(other.max[2]),
            ],
        }
    }

    /// Boolean ray-AABB test using the slab method. Returns true if the
    /// ray either originates inside the box or hits it in front of the
    /// origin; false if the ray misses the box or the box is entirely
    /// behind the ray. No hit-distance, normal, or surface — the only
    /// question we need to answer here is "should the renderer recurse
    /// into this subtree?"
    pub fn intersects(&self, ray: &Vector) -> bool {
        let mut t_enter = f64::NEG_INFINITY;
        let mut t_exit = f64::INFINITY;

        for i in 0..3 {
            let origin = ray.start[i];
            let dir = ray.delta[i];

            if dir.abs() < EPSILON {
                // Ray parallel to this slab pair: miss if origin is
                // outside the slab, otherwise this axis doesn't constrain
                // the t range.
                if origin < self.min[i] || origin > self.max[i] {
                    return false;
                }
                continue;
            }

            let inv = 1.0 / dir;
            let mut t1 = (self.min[i] - origin) * inv;
            let mut t2 = (self.max[i] - origin) * inv;

            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }

            if t1 > t_enter {
                t_enter = t1;
            }
            if t2 < t_exit {
                t_exit = t2;
            }

            if t_enter > t_exit {
                return false;
            }
        }

        // Box is in front of the ray, or contains the ray's origin.
        // (t_exit < 0 means box is entirely behind the ray.)
        t_exit > 0.0
    }

    /// Convert the AABB into a renderable `Cuboid` carrying the supplied
    /// surface. Bridge for visualization — lets you compute a bound,
    /// wrap one copy in `Bounded(...)` for acceleration, and place
    /// another copy in the scene as a visible box for diagnosis.
    pub fn to_cuboid(&self, surface: Surface) -> Cuboid {
        let center = [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ];
        let size = [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ];
        Cuboid { center, size, surface }
    }
}

/// Closed enumeration of all shape primitives the renderer knows how to
/// hit-test. Stored inline in `Scene::objects` (no boxing, no vtable).
///
/// The `From` impls below let Rust callers write `Sphere { ... }` and
/// have it auto-promoted to the right variant. The SDL bindings build
/// `Shape::Sphere(...)` etc. directly rather than going through `From`,
/// but the impls remain a sensible Rust-side API for any future
/// non-SDL callers.
///
/// `Group` lets a list of children be treated as a single shape, which
/// is what makes hierarchical scene composition possible. A group's hit
/// is the nearest hit among its children.
///
/// `Transform` wraps a child in an affine transformation. Hit-testing
/// inverse-transforms the ray into the child's local space, runs the
/// child's hit test there, and lifts the resulting hit point and normal
/// back into world space. The `Box` is required because `Shape` is now
/// recursive through this variant.
///
/// `Bounded` wraps a child in an axis-aligned bounding box for hit-test
/// acceleration. The ray is first tested against the AABB; if it misses,
/// the entire wrapped subtree is skipped without recursion. This is the
/// primitive the BVH builder will compose; for now scenes use it directly
/// (e.g. wrap a loaded mesh in `bounded(...)`). The `Box` keeps `Shape`
/// finite-sized.
///
/// `Light` is a point light source positioned in the scene graph. Like
/// every other Shape variant, lights inherit the affine transforms of
/// enclosing `Shape::Transform` nodes — that is the whole point of this
/// variant: scripts can write `(translate [5 0 0] (light-white [0 0 0]))`
/// and have the light end up at world-space `[5 0 0]`, the same way
/// geometry does. Lights are *invisible* to every kind of ray
/// (`Hittable::hit_test` returns `None`): they don't appear in renders,
/// they don't occlude shadow rays, they don't reflect. The renderer
/// extracts them up-front via `Shape::collect_lights` so the shading
/// path iterates a flat world-space list per pixel — no tree walk in
/// the shading hot path.
#[derive(Clone, PartialEq, Debug)]
pub enum Shape {
    Sphere(Sphere),
    Plane(Plane),
    Cuboid(Cuboid),
    Triangle(Triangle),
    Cylinder(Cylinder),
    Cone(Cone),
    Group(Vec<Shape>),
    Transform(Box<Transformed>),
    Bounded(Box<Bounded>),
    Light(Light),
}

/// Storage for a `Shape::Bounded` node. Holds the bounding AABB and the
/// child subtree it accelerates. Construction via `bounded(...)` (which
/// auto-computes the bound from the child) or `bounded_with(bounds, child)`
/// (which uses a caller-supplied bound — handy when the same bound is
/// being used for both acceleration and visualization).
#[derive(Clone, PartialEq, Debug)]
pub struct Bounded {
    pub bounds: AABB,
    pub child: Shape,
}

/// Storage for a `Shape::Transform` node. Cached at construction so that
/// hit-testing only does the cheap part (matrix-vector multiplies) per ray.
///
/// - `forward` is the local-to-world affine: applied to the eight corners
///   of the child's local-space AABB to compute world-space bounds for
///   `Shape::bounds()`. Not used in `hit_test`.
/// - `inverse` is the world-to-local affine: applied to the ray on the way
///   in, so the child sees a ray in its own coordinate system.
/// - `normal_xform` is the inverse-transpose of the *forward* linear part
///   (equivalently, the transpose of `inverse.linear`). It transforms the
///   child's local-space normal back to world space. Using the
///   inverse-transpose rather than the forward matrix is what keeps normals
///   correct under non-uniform scale.
#[derive(Clone, PartialEq, Debug)]
pub struct Transformed {
    pub forward: Affine,
    pub inverse: Affine,
    pub normal_xform: Mat3,
    pub child: Shape,
}

impl From<Sphere> for Shape {
    fn from(s: Sphere) -> Self { Shape::Sphere(s) }
}

impl From<Plane> for Shape {
    fn from(p: Plane) -> Self { Shape::Plane(p) }
}

impl From<Cuboid> for Shape {
    fn from(c: Cuboid) -> Self { Shape::Cuboid(c) }
}

impl From<Triangle> for Shape {
    fn from(t: Triangle) -> Self { Shape::Triangle(t) }
}

impl From<Cylinder> for Shape {
    fn from(c: Cylinder) -> Self { Shape::Cylinder(c) }
}

impl From<Cone> for Shape {
    fn from(c: Cone) -> Self { Shape::Cone(c) }
}

impl From<Light> for Shape {
    fn from(l: Light) -> Self { Shape::Light(l) }
}

impl Hittable for Shape {
    fn hit_test(&self, ray: &Vector) -> Option<RayHit> {
        match self {
            Shape::Sphere(s)        => s.hit_test(ray),
            Shape::Plane(p)         => p.hit_test(ray),
            Shape::Cuboid(c)        => c.hit_test(ray),
            Shape::Triangle(t)      => t.hit_test(ray),
            Shape::Cylinder(c)      => c.hit_test(ray),
            Shape::Cone(c)          => c.hit_test(ray),
            Shape::Group(children)  => nearest_hit(ray, children),
            Shape::Transform(t)     => t.hit_test(ray),
            Shape::Bounded(b)       => b.hit_test(ray),
            // Lights are invisible to every ray (primary, shadow,
            // reflection). The renderer reaches them through
            // `Shape::collect_lights` at render entry, not through
            // hit-testing. Returning `None` here is what keeps them
            // out of the rendered image and what stops them from
            // self-shadowing the geometry they live near.
            Shape::Light(_)         => None,
        }
    }
}

impl Bounded {
    fn hit_test(&self, ray: &Vector) -> Option<RayHit> {
        // Skip the entire wrapped subtree if the ray misses our box.
        // This is the whole point of the BVH primitive: one cheap AABB
        // test eliminates an arbitrarily large amount of recursive work.
        if !self.bounds.intersects(ray) {
            None
        } else {
            self.child.hit_test(ray)
        }
    }
}

impl Shape {
    /// Return the smallest axis-aligned bounding box that encloses this
    /// shape. `None` indicates the shape is unbounded — only `Plane` is
    /// genuinely infinite, but a `Group` containing a Plane (or other
    /// unbounded shape) is also unbounded, and `Transform` returns
    /// `None` for now since computing its world-space bounds requires
    /// the forward affine which we don't currently store (see phase 3).
    ///
    /// Used by `bounded(...)` to auto-compute the bound for an arbitrary
    /// child shape. Also useful directly for visualization: get the
    /// bound, convert it to a `Cuboid` via `AABB::to_cuboid`, and place
    /// it in the scene.
    pub fn bounds(&self) -> Option<AABB> {
        match self {
            Shape::Sphere(s) => Some(AABB::new(
                [s.center[0] - s.r, s.center[1] - s.r, s.center[2] - s.r],
                [s.center[0] + s.r, s.center[1] + s.r, s.center[2] + s.r],
            )),
            Shape::Plane(_) => None,
            Shape::Cuboid(c) => {
                let h = [c.size[0] * 0.5, c.size[1] * 0.5, c.size[2] * 0.5];
                Some(AABB::new(
                    [c.center[0] - h[0], c.center[1] - h[1], c.center[2] - h[2]],
                    [c.center[0] + h[0], c.center[1] + h[1], c.center[2] + h[2]],
                ))
            }
            Shape::Triangle(t) => {
                let v0 = t.vertices[0];
                let v1 = t.vertices[1];
                let v2 = t.vertices[2];
                Some(AABB::new(
                    [
                        v0[0].min(v1[0]).min(v2[0]),
                        v0[1].min(v1[1]).min(v2[1]),
                        v0[2].min(v1[2]).min(v2[2]),
                    ],
                    [
                        v0[0].max(v1[0]).max(v2[0]),
                        v0[1].max(v1[1]).max(v2[1]),
                        v0[2].max(v1[2]).max(v2[2]),
                    ],
                ))
            }
            Shape::Cylinder(c) => {
                // Tight world-space AABB. Per axis i, the radial extent
                // contributed by the round body is r * sqrt(1 - axis_unit[i]²)
                // — i.e. zero on the axis the cylinder is aligned with, and
                // exactly r perpendicular to it. The clamp to 0 absorbs the
                // small negative values that fall out of floating-point
                // rounding when axis_unit isn't *quite* unit length.
                let axis = subp(c.p1, c.p0);
                let axis_len = lenp(axis);
                if axis_len < EPSILON {
                    // Degenerate cylinder (p0 == p1). Construction should
                    // reject this, but bounds() must remain total — fall
                    // back to a sphere-of-radius-r bound at p0.
                    Some(AABB::new(
                        [c.p0[0] - c.r, c.p0[1] - c.r, c.p0[2] - c.r],
                        [c.p0[0] + c.r, c.p0[1] + c.r, c.p0[2] + c.r],
                    ))
                } else {
                    let axis_unit = [
                        axis[0] / axis_len,
                        axis[1] / axis_len,
                        axis[2] / axis_len,
                    ];
                    let radial = [
                        c.r * (1.0 - axis_unit[0] * axis_unit[0]).max(0.0).sqrt(),
                        c.r * (1.0 - axis_unit[1] * axis_unit[1]).max(0.0).sqrt(),
                        c.r * (1.0 - axis_unit[2] * axis_unit[2]).max(0.0).sqrt(),
                    ];
                    Some(AABB::new(
                        [
                            c.p0[0].min(c.p1[0]) - radial[0],
                            c.p0[1].min(c.p1[1]) - radial[1],
                            c.p0[2].min(c.p1[2]) - radial[2],
                        ],
                        [
                            c.p0[0].max(c.p1[0]) + radial[0],
                            c.p0[1].max(c.p1[1]) + radial[1],
                            c.p0[2].max(c.p1[2]) + radial[2],
                        ],
                    ))
                }
            }
            Shape::Cone(c) => {
                // The cone is enclosed by the union of its base disk
                // (radius r, centered at p0) and its apex point p1. The
                // base disk's per-axis extent uses the same
                // r * sqrt(1 - axis_unit[i]²) trick as the cylinder; the
                // apex contributes no radial extent at all. The clamp to
                // 0 absorbs floating-point noise when axis_unit isn't
                // quite unit length.
                let axis = subp(c.p1, c.p0);
                let axis_len = lenp(axis);
                if axis_len < EPSILON {
                    // Degenerate cone (p0 == p1). Construction should
                    // reject this, but bounds() must remain total — fall
                    // back to a sphere-of-radius-r bound at p0.
                    Some(AABB::new(
                        [c.p0[0] - c.r, c.p0[1] - c.r, c.p0[2] - c.r],
                        [c.p0[0] + c.r, c.p0[1] + c.r, c.p0[2] + c.r],
                    ))
                } else {
                    let axis_unit = [
                        axis[0] / axis_len,
                        axis[1] / axis_len,
                        axis[2] / axis_len,
                    ];
                    let radial = [
                        c.r * (1.0 - axis_unit[0] * axis_unit[0]).max(0.0).sqrt(),
                        c.r * (1.0 - axis_unit[1] * axis_unit[1]).max(0.0).sqrt(),
                        c.r * (1.0 - axis_unit[2] * axis_unit[2]).max(0.0).sqrt(),
                    ];
                    Some(AABB::new(
                        [
                            (c.p0[0] - radial[0]).min(c.p1[0]),
                            (c.p0[1] - radial[1]).min(c.p1[1]),
                            (c.p0[2] - radial[2]).min(c.p1[2]),
                        ],
                        [
                            (c.p0[0] + radial[0]).max(c.p1[0]),
                            (c.p0[1] + radial[1]).max(c.p1[1]),
                            (c.p0[2] + radial[2]).max(c.p1[2]),
                        ],
                    ))
                }
            }
            Shape::Group(items) => {
                // Union of children's bounds. Any unbounded child makes
                // the whole group unbounded — that's the right
                // semantics, since you genuinely can't put a finite box
                // around a group containing an infinite plane.
                let mut acc: Option<AABB> = None;
                for item in items {
                    let b = item.bounds()?;
                    acc = Some(match acc {
                        None    => b,
                        Some(a) => a.union(&b),
                    });
                }
                acc
            }
            Shape::Transform(t) => {
                // Transform the eight corners of the child's local-space
                // AABB into world space and take the AABB enclosing the
                // resulting points. This gives a conservative bound — it
                // is not the *tightest* possible world-space AABB for
                // the transformed geometry (a 45°-rotated unit cube has
                // a diagonal world-space extent that the corner method
                // captures correctly, but rotated spheres would get a
                // looser bound than necessary), but it is always
                // sufficient: the actual geometry never escapes the
                // returned AABB.
                let local = t.child.bounds()?;
                let corners = [
                    [local.min[0], local.min[1], local.min[2]],
                    [local.max[0], local.min[1], local.min[2]],
                    [local.min[0], local.max[1], local.min[2]],
                    [local.max[0], local.max[1], local.min[2]],
                    [local.min[0], local.min[1], local.max[2]],
                    [local.max[0], local.min[1], local.max[2]],
                    [local.min[0], local.max[1], local.max[2]],
                    [local.max[0], local.max[1], local.max[2]],
                ];
                let p0 = t.forward.transform_point(corners[0]);
                let mut min = p0;
                let mut max = p0;
                for corner in &corners[1..] {
                    let p = t.forward.transform_point(*corner);
                    for i in 0..3 {
                        if p[i] < min[i] { min[i] = p[i]; }
                        if p[i] > max[i] { max[i] = p[i]; }
                    }
                }
                Some(AABB::new(min, max))
            }
            Shape::Bounded(b) => Some(b.bounds),
            Shape::Light(l) => {
                // A point light has zero extent — its bound is the
                // degenerate AABB containing only its location. We
                // return `Some` rather than `None` so a `Group`
                // containing a light stays bounded, and so `bounded(...)`
                // works without special-casing lights. Light hit-testing
                // is always a no-op, so even if a `Bounded` wrapper
                // skips the light due to a ray-AABB miss the user-
                // visible result is unchanged.
                Some(AABB::new(l.location, l.location))
            }
        }
    }

    /// Walk the scene tree and push every contained light's world-space
    /// `Light` value into `out`. `world_from_local` accumulates the
    /// affine transforms of enclosing `Shape::Transform` nodes; pass
    /// `Affine::identity()` at the top level. Each leaf light pushes a
    /// new `Light` whose `location` has been transformed by the
    /// accumulated affine — color and intensity are unchanged because
    /// affines don't carry photometric meaning.
    ///
    /// `Bounded` wrappers are descended into *unconditionally*: the
    /// collection pass runs once at render entry, not per-ray, so the
    /// AABB early-out provides no speedup and would silently hide a
    /// light that happened to fall outside its enclosing box (which
    /// is a degenerate-bounds edge case but not worth defending
    /// against by having different traversal rules for collection and
    /// hit-testing).
    ///
    /// `world_from_local` is passed by value because `Affine` is
    /// `Copy` (96 bytes) and `Affine::compose` consumes its `self`;
    /// borrowing buys nothing here and just complicates the recursion.
    pub fn collect_lights(&self, world_from_local: Affine, out: &mut Vec<Light>) {
        match self {
            Shape::Light(l) => {
                out.push(Light {
                    location: world_from_local.transform_point(l.location),
                    color: l.color,
                    intensity: l.intensity,
                });
            }
            Shape::Group(children) => {
                for child in children {
                    child.collect_lights(world_from_local, out);
                }
            }
            Shape::Transform(t) => {
                // The child sees the affine that maps its local space
                // to world space. If we already have a world-from-parent
                // affine and the child is wrapped in a parent-from-child
                // affine (`t.forward`), the composed map is
                // `world_from_local ∘ t.forward` — same convention as
                // `Affine::compose`.
                t.child.collect_lights(world_from_local.compose(t.forward), out);
            }
            Shape::Bounded(b) => {
                b.child.collect_lights(world_from_local, out);
            }
            // Leaf geometry contains no lights.
            Shape::Sphere(_)
            | Shape::Plane(_)
            | Shape::Cuboid(_)
            | Shape::Triangle(_)
            | Shape::Cylinder(_)
            | Shape::Cone(_) => {}
        }
    }
}

impl Transformed {
    fn hit_test(&self, ray: &Vector) -> Option<RayHit> {
        // Inverse-transform the ray into the child's local space. Note
        // that we deliberately do NOT renormalize `local_ray.delta`: under
        // non-uniform scale its magnitude changes, but if we leave it
        // alone, the parametric `t` along the local ray equals the `t`
        // along the world ray. That preserves distance comparisons across
        // the whole scene tree without conversions, and lets us recover
        // the world-space hit point directly from the original ray.
        let local_ray = Vector {
            start: self.inverse.transform_point(ray.start),
            delta: self.inverse.transform_vector(ray.delta),
        };

        self.child.hit_test(&local_ray).map(| hit | {
            // Same `t` parameterizes both the world ray and the local ray,
            // so the world-space hit point falls out without a forward
            // matrix multiply.
            let world_hit_point = ray_location(ray, hit.distance);

            // Normals transform by the inverse-transpose of the linear
            // part. Renormalize because non-uniform scale can change the
            // magnitude.
            let world_normal = normalizep(mat3_apply(self.normal_xform, hit.normal));

            RayHit {
                distance: hit.distance,
                hit_point: world_hit_point,
                normal: world_normal,
                surface: hit.surface,
            }
        })
    }
}

impl Hittable for Triangle {
    fn hit_test(&self, ray: &Vector) -> Option<RayHit> {
        // Möller–Trumbore ray-triangle intersection.
        // Returns the parametric t plus barycentric coordinates (u, v) of
        // the hit. The third barycentric (w = 1 - u - v) corresponds to
        // vertex 0; u corresponds to vertex 1; v corresponds to vertex 2.
        // Per-vertex normals are interpolated by these weights to give
        // smooth shading; for flat-shaded triangles all three vertex
        // normals are equal so the interpolation is a no-op.

        let v0 = self.vertices[0];
        let v1 = self.vertices[1];
        let v2 = self.vertices[2];

        let edge1 = subp(v1, v0);
        let edge2 = subp(v2, v0);

        let h = crossp(ray.delta, edge2);
        let a = dotp(edge1, h);

        // Ray (nearly) parallel to triangle plane.
        if a.abs() < EPSILON {
            return None;
        }

        let f = 1.0 / a;
        let s = subp(ray.start, v0);
        let u = f * dotp(s, h);

        if u < 0.0 || u > 1.0 {
            return None;
        }

        let q = crossp(s, edge1);
        let v = f * dotp(ray.delta, q);

        if v < 0.0 || u + v > 1.0 {
            return None;
        }

        let t = f * dotp(edge2, q);

        // Hit must be in front of the ray origin (and not coincident with
        // it, for self-intersection avoidance on shadow / reflection rays).
        if t <= EPSILON {
            return None;
        }

        // Interpolate per-vertex normals using barycentric weights.
        let w = 1.0 - u - v;
        let n0 = self.normals[0];
        let n1 = self.normals[1];
        let n2 = self.normals[2];
        let normal = normalizep([
            w * n0[0] + u * n1[0] + v * n2[0],
            w * n0[1] + u * n1[1] + v * n2[1],
            w * n0[2] + u * n1[2] + v * n2[2],
        ]);

        Some(RayHit {
            distance: t,
            hit_point: ray_location(ray, t),
            normal,
            surface: self.surface,
        })
    }
}

/// Returns the closest hit (smallest positive `distance`) among `objects`,
/// or `None` if none of them were hit. Shared between the scene-level
/// traversal in `render` and the recursive case for `Shape::Group`.
pub fn nearest_hit(ray: &Vector, objects: &[Shape]) -> Option<RayHit> {
    objects
        .iter()
        .fold(None, | last_hit, obj | {
            let hit = obj.hit_test(ray);

            if hit > last_hit {
                hit
            } else {
                last_hit
            }
        })
}

/// Wrap a list of child shapes as a single `Shape::Group`. Cheap convenience
/// constructor — equivalent to writing `Shape::Group(children)` directly,
/// but reads more naturally inside scene definitions.
pub fn group(children: Vec<Shape>) -> Shape {
    Shape::Group(children)
}

/// Wrap a child in a `Shape::Bounded` node, auto-computing the bounding
/// box from the child itself. Panics if the child is unbounded (e.g. a
/// `Plane`, or a `Group` containing one) — bounding an infinite shape is
/// a programming error, not a recoverable condition.
///
/// For acceleration, use this on any subtree that has well-defined finite
/// bounds and may be missed by many rays. The classic case is a loaded
/// mesh: `bounded(load_obj("teapot.obj", surface))` gives a single AABB
/// test that skips all of the teapot's triangles for any ray that misses
/// the box. (A multi-level BVH from `bvh(...)` will further accelerate
/// rays that *do* hit the box; that's phase 2.)
pub fn bounded(child: impl Into<Shape>) -> Shape {
    let child = child.into();
    let bounds = child.bounds()
        .expect("bounded() requires a shape with finite bounds (no Plane, no untransformed-bound Transform)");
    Shape::Bounded(Box::new(Bounded { bounds, child }))
}

/// Wrap a child in a `Shape::Bounded` node with a caller-supplied bound.
/// Useful when the same bound is being used for both acceleration and
/// visualization: compute the bound once via `child.bounds()`, pass it
/// here for the wrapped subtree, and pass it to `AABB::to_cuboid` for a
/// renderable Cuboid that shows where the box is.
///
/// The supplied bound is trusted — it should genuinely contain the
/// child's geometry. A bound that's too small will cause valid hits to
/// be missed (rays that should have hit the geometry get rejected at the
/// box test). A bound that's too large just costs a small amount of
/// performance and is otherwise harmless.
pub fn bounded_with(bounds: AABB, child: impl Into<Shape>) -> Shape {
    Shape::Bounded(Box::new(Bounded { bounds, child: child.into() }))
}

/// Wrap a child in a `Shape::Transform` node carrying an arbitrary affine.
/// This is the lowest-level transform constructor; the per-axis helpers
/// below (`translate`, `scale`, `rotate_x`, …) are thin wrappers around it.
///
/// `forward` is the local-to-world transform. The inverse and the normal
/// transform matrix are computed once here and cached on the node, so
/// per-ray work is just a few mat-vec multiplies.
///
/// The `child` argument is any type that can be converted into `Shape`
/// (i.e. `Sphere`, `Plane`, `Cuboid`, or `Shape` itself). The `From`
/// impls take care of promoting a leaf primitive into the right
/// `Shape` variant automatically, so callers can write
/// `translate([1,0,0], Sphere { ... })` without an explicit wrap.
pub fn transform(forward: Affine, child: impl Into<Shape>) -> Shape {
    let inverse = forward.inverse();
    // normal_xform = (forward.linear)^{-T} = transpose(inverse.linear)
    let normal_xform = mat3_transpose(inverse.linear);
    Shape::Transform(Box::new(Transformed {
        forward,
        inverse,
        normal_xform,
        child: child.into(),
    }))
}

pub fn translate(d: Point, child: impl Into<Shape>) -> Shape {
    transform(Affine::translation(d), child)
}

/// Per-axis scale. For uniform scale, pass equal components, e.g.
/// `scale([2.0, 2.0, 2.0], child)`.
pub fn scale(s: Point, child: impl Into<Shape>) -> Shape {
    transform(Affine::scale(s), child)
}

pub fn rotate_x(theta: f64, child: impl Into<Shape>) -> Shape {
    transform(Affine::rotation_x(theta), child)
}

pub fn rotate_y(theta: f64, child: impl Into<Shape>) -> Shape {
    transform(Affine::rotation_y(theta), child)
}

pub fn rotate_z(theta: f64, child: impl Into<Shape>) -> Shape {
    transform(Affine::rotation_z(theta), child)
}

/// Rotation by `theta` radians around an arbitrary axis (which need not
/// be unit-length).
pub fn rotate_axis(axis: Point, theta: f64, child: impl Into<Shape>) -> Shape {
    transform(Affine::rotation_axis(axis, theta), child)
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

            // Reject hits behind or coincident with the ray origin. Without
            // this, a ray starting on (reflection / shadow) or inside a
            // sphere returns a negative-`t` "hit" that beats every legitimate
            // forward hit in `nearest_hit`'s distance comparison. The other
            // primitives (Plane, Cuboid, Triangle) already do this — Sphere
            // was the outlier.
            if t <= EPSILON {
                return None;
            }

            let hit_point = ray_location(ray, t);

            Some(RayHit {
                distance: t,
                hit_point,
                normal: normalizep(subp(hit_point, self.center)),
                surface: self.surface
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
                let hit_point = ray_location(ray, t);

                Some(RayHit {
                    distance: t,
                    hit_point,
                    normal: self.normal,
                    surface: self.surface
                })
            }
        }
    }
}

impl Hittable for Cuboid {
    fn hit_test(&self, ray: &Vector) -> Option<RayHit> {
        // Slab method for axis-aligned box intersection.
        //
        // For each axis, treat the box as the intersection of two parallel
        // planes ("slabs") and compute the parametric t values where the ray
        // enters and exits that slab. The overall entry t is the largest of
        // the three per-axis entry values; the overall exit t is the smallest
        // of the per-axis exit values. If entry > exit, the ray misses.
        //
        // The face that was hit is the one whose entry t was the maximum,
        // which directly gives us the surface normal.

        let half = [
            self.size[0] * 0.5,
            self.size[1] * 0.5,
            self.size[2] * 0.5,
        ];
        let min = [
            self.center[0] - half[0],
            self.center[1] - half[1],
            self.center[2] - half[2],
        ];
        let max = [
            self.center[0] + half[0],
            self.center[1] + half[1],
            self.center[2] + half[2],
        ];

        let mut t_enter = f64::NEG_INFINITY;
        let mut t_exit = f64::INFINITY;
        let mut enter_axis: usize = 0;
        let mut enter_sign: f64 = 0.0;

        for i in 0..3 {
            let origin = ray.start[i];
            let dir = ray.delta[i];

            if dir.abs() < EPSILON {
                // Ray is parallel to this pair of slabs: miss if origin is
                // outside the slab, otherwise this axis doesn't constrain t.
                if origin < min[i] || origin > max[i] {
                    return None;
                }
                continue;
            }

            let inv = 1.0 / dir;
            let mut t1 = (min[i] - origin) * inv;
            let mut t2 = (max[i] - origin) * inv;

            // After ordering t1 <= t2, the entering face's outward normal
            // points along -axis if the ray was moving in +axis (dir > 0)
            // and along +axis if the ray was moving in -axis (dir < 0).
            let mut sign = -1.0;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                sign = 1.0;
            }

            if t1 > t_enter {
                t_enter = t1;
                enter_axis = i;
                enter_sign = sign;
            }
            if t2 < t_exit {
                t_exit = t2;
            }

            if t_enter > t_exit {
                return None;
            }
        }

        // Box is entirely behind the ray, or ray origin is on/inside the box.
        // Treat origin-inside as a miss to avoid self-intersection on
        // reflection and shadow rays starting at the surface.
        if t_enter <= EPSILON {
            return None;
        }

        let hit_point = ray_location(ray, t_enter);
        let mut normal: Point = [0.0, 0.0, 0.0];
        normal[enter_axis] = enter_sign;

        Some(RayHit {
            distance: t_enter,
            hit_point,
            normal,
            surface: self.surface,
        })
    }
}

impl Hittable for Cylinder {
    fn hit_test(&self, ray: &Vector) -> Option<RayHit> {
        // Closed-cylinder ray intersection. Three sub-tests run against
        // three surfaces — the curved side and the two flat end caps —
        // and the nearest qualifying hit wins.
        //
        // Side: decompose `ray.delta` and `(ray.start - p0)` into axis-
        // parallel and axis-perpendicular components, solve the resulting
        // 2D ray-circle equation, then verify the hit's projection along
        // the axis falls between the caps (s ∈ [0, axis_len]).
        //
        // Caps: each cap is a disk — a ray-plane intersection followed by
        // a radial-distance check. Cap at p0 has outward normal -axis_unit;
        // cap at p1 has outward normal +axis_unit. Caps are tested against
        // the running best-`t` so cap hits beyond the current best are
        // rejected without doing the radial check.

        let axis = subp(self.p1, self.p0);
        let axis_len = lenp(axis);
        // Defensive: a degenerate (zero-length-axis) cylinder is a
        // construction error, but the renderer shouldn't divide-by-zero
        // if one slips through.
        if axis_len < EPSILON {
            return None;
        }
        let axis_unit = scalep(axis, 1.0 / axis_len);

        let delta = subp(ray.start, self.p0);
        let d_dot_a = dotp(ray.delta, axis_unit);
        let delta_dot_a = dotp(delta, axis_unit);

        // Best hit found so far. Tracks `(t, normal)`; the hit point is
        // recovered from t at the end via `ray_location`.
        let mut best: Option<(f64, Point)> = None;

        // --- Side surface ----------------------------------------------
        let d_perp = subp(ray.delta, scalep(axis_unit, d_dot_a));
        let delta_perp = subp(delta, scalep(axis_unit, delta_dot_a));

        let a = dotp(d_perp, d_perp);
        // a < EPSILON means the ray is parallel to the axis — it can hit
        // caps but not the side surface, so we just skip the side test.
        if a >= EPSILON {
            let b = 2.0 * dotp(delta_perp, d_perp);
            let c = dotp(delta_perp, delta_perp) - self.r * self.r;
            let discriminant = b * b - 4.0 * a * c;

            if discriminant >= 0.0 {
                let sqrt_disc = discriminant.sqrt();
                // a > 0 here, so t_near < t_far.
                let t_near = (-b - sqrt_disc) / (2.0 * a);
                let t_far  = (-b + sqrt_disc) / (2.0 * a);

                // Smallest t > EPSILON whose hit point falls between the
                // cap planes. `s = (P - p0) · axis_unit` is computed as
                // `delta_dot_a + t * d_dot_a` to skip computing P.
                let pick = |t: f64| -> Option<f64> {
                    if t <= EPSILON {
                        return None;
                    }
                    let s = delta_dot_a + t * d_dot_a;
                    if s < 0.0 || s > axis_len {
                        None
                    } else {
                        Some(s)
                    }
                };

                let side_hit = pick(t_near)
                    .map(|s| (t_near, s))
                    .or_else(|| pick(t_far).map(|s| (t_far, s)));

                if let Some((t, s)) = side_hit {
                    let hit_point = ray_location(ray, t);
                    // (P - (p0 + s * axis_unit)) has magnitude r in exact
                    // arithmetic — the divide would give a unit normal,
                    // but we renormalize anyway because floating-point
                    // error can leave it slightly off and the rest of the
                    // shading pipeline expects unit normals.
                    let center_on_axis = addp(self.p0, scalep(axis_unit, s));
                    let normal = normalizep(subp(hit_point, center_on_axis));
                    best = Some((t, normal));
                }
            }
        }

        // --- End caps --------------------------------------------------
        // Each cap is a flat disk: ray-plane intersection, then check that
        // the hit point is within radius r of the cap center. The loop
        // checks against the running best-t first to skip the radial test
        // for cap hits we already have a closer hit than.
        //
        // Cap normals point outward — `-axis_unit` at p0, `+axis_unit` at
        // p1 — so a hit with the cap normal coming out of the cap plane
        // toward the camera shades correctly without further work.
        let r_sq = self.r * self.r;
        for (cap_center, cap_normal) in [
            (self.p0, negp(axis_unit)),
            (self.p1, axis_unit),
        ] {
            let denom = dotp(cap_normal, ray.delta);
            if denom.abs() < EPSILON {
                // Ray parallel to cap plane — no intersection.
                continue;
            }
            let t = dotp(subp(cap_center, ray.start), cap_normal) / denom;
            if t <= EPSILON {
                continue;
            }
            if let Some((t_best, _)) = best {
                if t >= t_best {
                    continue;
                }
            }
            let hit_point = ray_location(ray, t);
            let radial = subp(hit_point, cap_center);
            // Squared-distance comparison — saves a sqrt vs. computing the
            // actual distance.
            if dotp(radial, radial) <= r_sq {
                best = Some((t, cap_normal));
            }
        }

        best.map(|(t, normal)| RayHit {
            distance: t,
            hit_point: ray_location(ray, t),
            normal,
            surface: self.surface,
        })
    }
}

impl Hittable for Cone {
    fn hit_test(&self, ray: &Vector) -> Option<RayHit> {
        // Closed-cone ray intersection. Two surfaces — the curved lateral
        // surface and the single flat base cap at `p0` — are tested, and
        // the nearer qualifying hit wins. (The apex end has no cap; it's
        // a point.)
        //
        // Lateral surface: the cone is the locus where the angle between
        // `(P - apex)` and the axis equals the half-angle θ, i.e.
        // `((P - apex)·axis_unit)² = cos²θ · |P - apex|²`. Substituting the
        // ray gives a quadratic in t. Two checks trim spurious roots:
        // `s = (P - apex)·axis_unit` must lie in `[0, axis_len]` — the
        // upper bound clips at the base plane, and the *lower* bound
        // (s ≥ 0) is load-bearing: it discards the infinite double cone's
        // second nappe behind the apex, which the cos²θ equation also
        // admits.
        //
        // Base cap: a flat disk — ray-plane intersection at `p0` followed
        // by a radial-distance check, tested against the running best-t.

        // axis_unit points apex → base.
        let axis = subp(self.p0, self.p1);
        let axis_len = lenp(axis);
        // Defensive: a degenerate cone (zero-length axis, or a radius
        // collapsed to a line) is a construction error, but the renderer
        // shouldn't divide-by-zero if one slips through.
        if axis_len < EPSILON || self.r < EPSILON {
            return None;
        }
        let axis_unit = scalep(axis, 1.0 / axis_len);
        let apex = self.p1;

        // cos²θ where θ is the half-angle: tan θ = r / axis_len, so
        // cos²θ = axis_len² / (axis_len² + r²).
        let axis_len_sq = axis_len * axis_len;
        let cos2 = axis_len_sq / (axis_len_sq + self.r * self.r);

        let co = subp(ray.start, apex);
        let dv = dotp(ray.delta, axis_unit);
        let cv = dotp(co, axis_unit);
        let dd = dotp(ray.delta, ray.delta);
        let dc = dotp(ray.delta, co);
        let cc = dotp(co, co);

        let a = dv * dv - cos2 * dd;
        let b = 2.0 * (dv * cv - cos2 * dc);
        let c = cv * cv - cos2 * cc;

        // Best hit so far: (t, normal). Hit point recovered from t at the
        // end via `ray_location`.
        let mut best: Option<(f64, Point)> = None;

        // --- Lateral surface -------------------------------------------
        // Collect the candidate roots of `a t² + b t + c = 0`. `a` can be
        // positive, negative, or ~0 (the ray running parallel to a cone
        // generator line), so we don't assume a root ordering — we gather
        // every real root and let the s-range check and the min-t pick
        // sort them out.
        let mut roots: [Option<f64>; 2] = [None, None];
        if a.abs() < EPSILON {
            // Degenerate quadratic — the ray is parallel to a generator
            // line of the cone. Falls back to the linear equation
            // `b t + c = 0`.
            if b.abs() >= EPSILON {
                roots[0] = Some(-c / b);
            }
        } else {
            let disc = b * b - 4.0 * a * c;
            if disc >= 0.0 {
                let sqrt_disc = disc.sqrt();
                roots[0] = Some((-b - sqrt_disc) / (2.0 * a));
                roots[1] = Some((-b + sqrt_disc) / (2.0 * a));
            }
        }

        for root in roots {
            let t = match root {
                Some(t) => t,
                None => continue,
            };
            if t <= EPSILON {
                continue;
            }
            // s = (P - apex)·axis_unit, computed without forming P.
            let s = cv + t * dv;
            if s < 0.0 || s > axis_len {
                continue;
            }
            // Closer than what we already have?
            if let Some((t_best, _)) = best {
                if t >= t_best {
                    continue;
                }
            }
            let hit_point = ray_location(ray, t);
            let apex_to_p = subp(hit_point, apex);
            let perp = subp(apex_to_p, scalep(axis_unit, s));
            let perp_len = lenp(perp);
            if perp_len < EPSILON {
                // Hit landed on the apex tip — the normal is ill-defined
                // there. Vanishingly rare; skip rather than emit a
                // garbage normal.
                continue;
            }
            let perp_unit = scalep(perp, 1.0 / perp_len);
            // The lateral normal points outward radially *and* tilts
            // toward the apex by the half-angle: slope = r / axis_len,
            // and `-slope * axis_unit` points base → apex.
            let slope = self.r / axis_len;
            let normal = normalizep(subp(perp_unit, scalep(axis_unit, slope)));
            best = Some((t, normal));
        }

        // --- Base cap --------------------------------------------------
        // A flat disk at `p0` with outward normal `+axis_unit` (axis_unit
        // points apex → base, so it points out of the cone at the base).
        let cap_normal = axis_unit;
        let denom = dotp(cap_normal, ray.delta);
        if denom.abs() >= EPSILON {
            let t = dotp(subp(self.p0, ray.start), cap_normal) / denom;
            let closer = match best {
                Some((t_best, _)) => t > EPSILON && t < t_best,
                None => t > EPSILON,
            };
            if closer {
                let hit_point = ray_location(ray, t);
                let radial = subp(hit_point, self.p0);
                // Squared-distance comparison — saves a sqrt.
                if dotp(radial, radial) <= self.r * self.r {
                    best = Some((t, cap_normal));
                }
            }
        }

        best.map(|(t, normal)| RayHit {
            distance: t,
            hit_point: ray_location(ray, t),
            normal,
            surface: self.surface,
        })
    }
}
