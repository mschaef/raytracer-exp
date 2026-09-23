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
    LightKind,
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

/// A primitive's `surface` is `Option<Surface>` rather than `Surface`
/// directly: a leaf can be constructed without a surface, in which case
/// the deepest enclosing `Shape::Surfaced` wrapper supplies one at
/// hit-time. An explicit `Some(_)` on the leaf wins over any enclosing
/// wrapper ("innermost wins"). A leaf with `None` and *no* enclosing
/// wrapper is a scene-construction error caught by
/// `Shape::validate_surfaces` — render-time fallback in `shade_pixel`
/// is a defensive safety net, not the primary check.
#[derive(Clone, PartialEq, Debug)]
pub struct Sphere {
    pub center: Point,
    pub r: f64,
    pub surface: Option<Surface>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Plane {
    pub normal: Point,
    pub p0: Point,
    pub surface: Option<Surface>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Cuboid {
    pub center: Point,
    pub size: Point,
    pub surface: Option<Surface>,
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
    pub surface: Option<Surface>,
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
    pub surface: Option<Surface>,
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
    pub surface: Option<Surface>,
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

    /// The overlap of `self` and `other`. If they don't overlap, the
    /// result is a degenerate (zero-volume) box at the corner where
    /// they come closest rather than an inverted one, so it stays a
    /// valid, if useless, bound.
    pub fn intersection(&self, other: &AABB) -> AABB {
        let mut min = [0.0; 3];
        let mut max = [0.0; 3];
        for i in 0..3 {
            min[i] = self.min[i].max(other.min[i]);
            max[i] = self.max[i].min(other.max[i]).max(min[i]);
        }
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
        Cuboid { center, size, surface: Some(surface) }
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
///
/// `Surfaced` decorates a subtree with a *default surface*. Every
/// leaf primitive carries `surface: Option<Surface>`, and a leaf with
/// `None` inherits its surface from the deepest enclosing `Surfaced`
/// wrapper. Concretely: `Surfaced::hit_test` calls its child's
/// `hit_test`, and if the returned `RayHit` has `surface: None` it
/// fills in `Some(self.surface)` before returning; a `Some(_)` from
/// the child passes through untouched. That's "innermost wins" — a
/// child that explicitly set its surface beats any enclosing default,
/// and the *innermost* enclosing `Surfaced` is the first to see a
/// `None` (and the first to fill it). Construction-time validation
/// (`Shape::validate_surfaces`) rejects scenes containing a leaf
/// with neither an explicit surface nor an enclosing `Surfaced`, so
/// the renderer can assume a `Some(surface)` reaches `shade_pixel`.
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
    Surfaced(Box<SurfacedShape>),
    Csg(Box<Csg>),
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

/// Storage for a `Shape::Surfaced` node. Holds the default surface this
/// wrapper supplies and the child subtree it decorates. See the
/// `Shape::Surfaced` doc comment for the inheritance semantics; the
/// implementation is in `SurfacedShape::hit_test` below.
#[derive(Clone, PartialEq, Debug)]
pub struct SurfacedShape {
    pub surface: Surface,
    pub child: Shape,
}

/// Which CSG operation a `Shape::Csg` node performs. There's no
/// `Union`: a union of solids is just a `Group`, which already
/// behaves as one both for `hit_test` and for `spans`.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum CsgOp {
    /// Points inside `a` and not inside `b`.
    Difference,
    /// Points inside both `a` and `b`.
    Intersection,
}

/// Storage for a `Shape::Csg` node: a binary CSG operation on two solid
/// operands. Both operands are solids (`Shape::is_solid`), which the
/// `difference` / `intersection` constructors enforce. The node answers
/// `hit_test` by combining its operands' span lists (see "CSG span
/// query" below) and returning the first boundary in front of the ray.
/// It also answers `spans`, so CSG nodes nest.
#[derive(Clone, PartialEq, Debug)]
pub struct Csg {
    pub op: CsgOp,
    pub a: Shape,
    pub b: Shape,
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
            Shape::Surfaced(s)      => s.hit_test(ray),
            Shape::Csg(c)           => c.hit_test(ray),
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

impl SurfacedShape {
    /// Fill in the child's surface if the leaf didn't carry one.
    /// "Innermost wins": a `Some(_)` from the child means either an
    /// explicit leaf surface or a deeper `Surfaced` wrapper has
    /// already supplied one, and we leave it alone; a `None` means
    /// the leaf is asking for a default, and we provide ours. The
    /// rest of the `RayHit` (distance, hit point, normal) is
    /// untouched — `Surfaced` affects shading only.
    fn hit_test(&self, ray: &Vector) -> Option<RayHit> {
        self.child.hit_test(ray).map(|hit| {
            if hit.surface.is_some() {
                hit
            } else {
                RayHit {
                    surface: Some(self.surface),
                    ..hit
                }
            }
        })
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
            Shape::Surfaced(s) => s.child.bounds(),
            Shape::Csg(c) => match c.op {
                // `a − b` never extends beyond `a`.
                CsgOp::Difference => c.a.bounds(),
                // `a ∩ b` lies inside both bounds, so their overlap
                // bounds it. An unbounded operand (a half-space plane)
                // doesn't constrain it.
                CsgOp::Intersection => match (c.a.bounds(), c.b.bounds()) {
                    (Some(a), Some(b)) => Some(a.intersection(&b)),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                },
            },
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
                // Photometric fields (color, intensity) carry through
                // unchanged because affines don't carry photometric
                // meaning. `location` always transforms as a point.
                // Variant-specific geometric fields transform per
                // `LightKind` arm: a spotlight's `direction` is a
                // vector (linear part only — translations don't apply),
                // renormalized because non-uniform scale can change
                // its magnitude even when the source was unit-length.
                // Phase 4 will add an `Area { axis, .. }` arm here
                // that transforms `axis` the same way.
                let kind = match l.kind {
                    LightKind::Point => LightKind::Point,
                    LightKind::Spot { direction, inner_angle, outer_angle } => {
                        LightKind::Spot {
                            direction: normalizep(
                                world_from_local.transform_vector(direction),
                            ),
                            inner_angle,
                            outer_angle,
                        }
                    }
                    LightKind::Area { axis, radius } => {
                        // `axis` is a vector and transforms by the
                        // linear part only; renormalize because
                        // non-uniform scale can change its magnitude.
                        // `radius` stays as authored — uniform-scale-
                        // aware radius scaling is a Phase 6 nicety
                        // (same posture as Cylinder/Cone radii).
                        LightKind::Area {
                            axis: normalizep(world_from_local.transform_vector(axis)),
                            radius,
                        }
                    }
                };
                out.push(Light {
                    location: world_from_local.transform_point(l.location),
                    color: l.color,
                    intensity: l.intensity,
                    kind,
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
            Shape::Surfaced(s) => {
                // Surface decoration doesn't affect light geometry,
                // so just descend into the child. Any lights inside
                // a `Surfaced` wrapper are collected as if the
                // wrapper weren't there — which is the right thing,
                // since lights are invisible to hit-testing and so
                // can't pick up the wrapper's surface anyway.
                s.child.collect_lights(world_from_local, out);
            }
            Shape::Csg(c) => {
                // A light inside CSG geometry is just positioned there;
                // the CSG operation doesn't affect it.
                c.a.collect_lights(world_from_local, out);
                c.b.collect_lights(world_from_local, out);
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

    /// Walk the scene tree and verify that every leaf primitive
    /// either carries an explicit surface (`surface: Some(_)`) or
    /// sits under an enclosing `Shape::Surfaced` ancestor. Returns
    /// `Err(message)` describing the first violation; returns `Ok(())`
    /// if the whole tree is well-formed.
    ///
    /// `has_surfaced_ancestor` is the recursion-threaded "is there a
    /// `Shape::Surfaced` ancestor above me?" flag. Top-level callers
    /// pass `false`. Descending into a `Surfaced` arm sets it to
    /// `true` for the subtree; every other variant passes it through
    /// unchanged.
    ///
    /// The renderer's `shade_pixel` carries a defensive hot-pink
    /// fallback for the case where this check is bypassed (direct
    /// Rust scene construction that skips Scene-level validation),
    /// but the construction-time check is the authoritative gate —
    /// a `None` surface that survives until `shade_pixel` is a bug,
    /// not an authoring choice.
    pub fn validate_surfaces(&self, has_surfaced_ancestor: bool) -> Result<(), String> {
        match self {
            Shape::Sphere(s) => check_leaf_surface(&s.surface, has_surfaced_ancestor, "sphere"),
            Shape::Plane(p) => check_leaf_surface(&p.surface, has_surfaced_ancestor, "plane"),
            Shape::Cuboid(c) => check_leaf_surface(&c.surface, has_surfaced_ancestor, "cuboid"),
            Shape::Triangle(t) => check_leaf_surface(&t.surface, has_surfaced_ancestor, "triangle"),
            Shape::Cylinder(c) => check_leaf_surface(&c.surface, has_surfaced_ancestor, "cylinder"),
            Shape::Cone(c) => check_leaf_surface(&c.surface, has_surfaced_ancestor, "cone"),
            Shape::Group(children) => {
                for child in children {
                    child.validate_surfaces(has_surfaced_ancestor)?;
                }
                Ok(())
            }
            Shape::Transform(t) => t.child.validate_surfaces(has_surfaced_ancestor),
            Shape::Bounded(b) => b.child.validate_surfaces(has_surfaced_ancestor),
            Shape::Surfaced(s) => s.child.validate_surfaces(true),
            Shape::Csg(c) => {
                c.a.validate_surfaces(has_surfaced_ancestor)?;
                c.b.validate_surfaces(has_surfaced_ancestor)
            }
            // Lights don't have surfaces (and aren't hit-tested), so
            // they're trivially valid regardless of ancestry.
            Shape::Light(_) => Ok(()),
        }
    }
}

/// Validate one leaf primitive's surface field against the
/// surrounding context. A leaf with an explicit surface is always
/// fine; a leaf with `None` is fine if and only if an enclosing
/// `Shape::Surfaced` will supply one at hit time.
fn check_leaf_surface(
    surface: &Option<Surface>,
    has_surfaced_ancestor: bool,
    kind: &str,
) -> Result<(), String> {
    if surface.is_some() || has_surfaced_ancestor {
        Ok(())
    } else {
        Err(format!(
            "{} leaf has no surface and is not inside a Surfaced wrapper",
            kind,
        ))
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
/// mesh: `bounded(load_obj("teapot.obj", Some(surface)))` gives a single
/// AABB test that skips all of the teapot's triangles for any ray that
/// misses the box. (A multi-level BVH from `bvh(...)` will further
/// accelerate rays that *do* hit the box; that's phase 2.)
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

/// Wrap a child in a `Shape::Surfaced` node carrying a default
/// `surface`. Every leaf in `child` whose own `surface` is `None`
/// will be shaded with this default; leaves that carry an explicit
/// `Some(_)` keep their own surface. Inheritance is "innermost wins" —
/// a nested `surfaced(inner, ...)` inside `surfaced(outer, ...)`
/// shades unsurfaced leaves with `inner`, because the inner wrapper
/// is the first to see (and fill) a `None`.
///
/// Leaves with `None` and no enclosing `Surfaced` ancestor are a
/// scene-construction error caught by `Shape::validate_surfaces` at
/// scene-build time; the renderer itself has a defensive hot-pink
/// fallback in `shade_pixel`, but that is the safety net, not the
/// primary check.
pub fn surfaced(surface: Surface, child: impl Into<Shape>) -> Shape {
    Shape::Surfaced(Box::new(SurfacedShape { surface, child: child.into() }))
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
/// `a − b`: the points inside `a` and not inside `b`. Faces cut by `b`
/// show `b`'s surface if it has one, otherwise an enclosing
/// `Surfaced` wrapper's (the same "innermost wins" rule as any leaf).
///
/// Panics if either operand isn't a solid (see `Shape::is_solid`);
/// the SDL binding checks first so scripts get a positioned error.
pub fn difference(a: impl Into<Shape>, b: impl Into<Shape>) -> Shape {
    csg(CsgOp::Difference, a.into(), b.into())
}

/// `a ∩ b`: the points inside both `a` and `b`. Panics if either
/// operand isn't a solid (see `Shape::is_solid`).
pub fn intersection(a: impl Into<Shape>, b: impl Into<Shape>) -> Shape {
    csg(CsgOp::Intersection, a.into(), b.into())
}

fn csg(op: CsgOp, a: Shape, b: Shape) -> Shape {
    assert!(a.is_solid() && b.is_solid(),
            "CSG operands must be solids (triangles and meshes have no inside)");
    Shape::Csg(Box::new(Csg { op, a, b }))
}

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

// ---------------------------------------------------------------------
// CSG span query
// ---------------------------------------------------------------------
//
// Phase 1 of the "CSG: implementation plan" in CLAUDE.md. `hit_test`
// answers "where does this ray first hit the surface?"; CSG needs to
// know, at every point along the ray, whether the ray is *inside* the
// solid. Each solid describes that as a sorted list of `Span`s — the
// parameter intervals `[enter.t, exit.t]` where the ray is inside it —
// and CSG operators combine span lists with interval set operations.
//
// Spans cover the whole line, negative `t` included: a span that starts
// behind the ray origin is how "the ray starts inside this solid" is
// represented. Endpoints can be infinite (a `Plane` is a half-space);
// infinite endpoints never become hits.
//
// The renderer reaches this code only through `Shape::Csg` nodes, so
// scenes without CSG render exactly as they did before it existed.

/// One boundary crossing of a solid along a ray. Carries what
/// `hit_test` would return at that point: the ray parameter `t`, the
/// solid's *outward* normal there, and the surface (`Option` so the
/// `Surfaced` "innermost wins" rule keeps working). The hit point
/// isn't stored; it's recomputed from the ray and `t` when a hit is
/// returned, the same way `Transformed::hit_test` does it.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct SpanEnd {
    pub t: f64,
    pub normal: Point,
    pub surface: Option<Surface>,
}

/// One interval `[enter.t, exit.t]` along a ray where the ray is inside
/// a solid. `enter.t < exit.t` always holds; either end may be infinite.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Span {
    pub enter: SpanEnd,
    pub exit: SpanEnd,
}

impl SpanEnd {
    /// The same crossing seen from the other side of the surface. Used
    /// by `span_difference`: where a subtracted solid B carves into A,
    /// the new boundary is B's surface viewed from inside B, so its
    /// outward normal (as a boundary of `A − B`) points the other way.
    fn flipped(self) -> SpanEnd {
        SpanEnd { normal: negp(self.normal), ..self }
    }

    fn fill_surface(&mut self, surface: Surface) {
        if self.surface.is_none() {
            self.surface = Some(surface);
        }
    }
}

/// Build the single span of a *convex* solid from its candidate surface
/// crossings (in any order): the ray is inside between the smallest and
/// largest crossing. Fewer than two distinct crossings — a miss, or a
/// ray grazing an edge — gives no span.
fn convex_span(candidates: &[(f64, Point)], surface: Option<Surface>) -> Option<Span> {
    let mut lo: Option<(f64, Point)> = None;
    let mut hi: Option<(f64, Point)> = None;
    for &(t, n) in candidates {
        if lo.map_or(true, |(lt, _)| t < lt) {
            lo = Some((t, n));
        }
        if hi.map_or(true, |(ht, _)| t > ht) {
            hi = Some((t, n));
        }
    }
    match (lo, hi) {
        (Some((t0, n0)), Some((t1, n1))) if t0 < t1 => Some(Span {
            enter: SpanEnd { t: t0, normal: n0, surface },
            exit: SpanEnd { t: t1, normal: n1, surface },
        }),
        _ => None,
    }
}

impl Shape {
    /// Whether this shape encloses a volume, and so can be a CSG
    /// operand. Every primitive is a solid except `Triangle`, which is
    /// a surface with no inside; that includes meshes from `load_obj`,
    /// which are groups of triangles. `Plane` counts as a solid: a
    /// half-space. A `Group` is solid if all its children are.
    /// `Light`s don't enclose anything but don't break solidity either,
    /// so a light can sit inside a CSG operand.
    pub fn is_solid(&self) -> bool {
        match self {
            Shape::Triangle(_) => false,
            Shape::Group(children) => children.iter().all(Shape::is_solid),
            Shape::Transform(t) => t.child.is_solid(),
            Shape::Bounded(b) => b.child.is_solid(),
            Shape::Surfaced(s) => s.child.is_solid(),
            Shape::Sphere(_)
            | Shape::Plane(_)
            | Shape::Cuboid(_)
            | Shape::Cylinder(_)
            | Shape::Cone(_)
            | Shape::Csg(_)
            | Shape::Light(_) => true,
        }
    }

    /// Append this shape's spans along `ray` to `out`: sorted by `t`,
    /// non-overlapping, covering the whole line (negative `t`
    /// included). See the section comment above.
    ///
    /// `Triangle`s aren't solids and contribute nothing; the CSG
    /// constructors refuse them as operands (see `is_solid`), so a
    /// triangle never reaches this from a `Csg` node. `Light`s
    /// contribute nothing.
    pub fn spans(&self, ray: &Vector, out: &mut Vec<Span>) {
        match self {
            Shape::Sphere(s)   => out.extend(s.span(ray)),
            Shape::Plane(p)    => out.extend(p.span(ray)),
            Shape::Cuboid(c)   => out.extend(c.span(ray)),
            Shape::Cylinder(c) => out.extend(c.span(ray)),
            Shape::Cone(c)     => out.extend(c.span(ray)),
            // Not a solid: a triangle has no inside.
            Shape::Triangle(_) => {}
            Shape::Light(_)    => {}
            Shape::Group(children) => {
                // A group is the union of its children. Each child's
                // list is already sorted and non-overlapping, but lists
                // from different children can overlap, so normalize.
                let mut all = Vec::new();
                for child in children {
                    child.spans(ray, &mut all);
                }
                out.extend(span_union_of(all));
            }
            Shape::Transform(t) => {
                // Same ray transform as `Transformed::hit_test`,
                // deliberately *not* renormalizing the local direction,
                // so `t` is identical in local and world space and the
                // span endpoints need no conversion. Only the normals
                // go back through the inverse-transpose.
                let local_ray = Vector {
                    start: t.inverse.transform_point(ray.start),
                    delta: t.inverse.transform_vector(ray.delta),
                };
                let first = out.len();
                t.child.spans(&local_ray, out);
                for span in &mut out[first..] {
                    span.enter.normal = normalizep(mat3_apply(t.normal_xform, span.enter.normal));
                    span.exit.normal = normalizep(mat3_apply(t.normal_xform, span.exit.normal));
                }
            }
            Shape::Bounded(b) => {
                // `AABB::intersects` keeps any box the ray is in front
                // of *or inside*, and only rejects boxes entirely
                // behind the origin. Spans behind the origin can't
                // affect anything at `t > 0`, so this early-out is as
                // valid for spans as it is for `hit_test`.
                if b.bounds.intersects(ray) {
                    b.child.spans(ray, out);
                }
            }
            Shape::Csg(c) => out.extend(c.spans(ray)),
            Shape::Surfaced(s) => {
                let first = out.len();
                s.child.spans(ray, out);
                for span in &mut out[first..] {
                    span.enter.fill_surface(s.surface);
                    span.exit.fill_surface(s.surface);
                }
            }
        }
    }
}

impl Csg {
    /// This node's spans along `ray`: the operands' span lists
    /// combined by the node's set operation.
    fn spans(&self, ray: &Vector) -> Vec<Span> {
        let mut a = Vec::new();
        self.a.spans(ray, &mut a);
        // Both operations are empty wherever `a` is, so a ray that
        // misses `a` needn't evaluate `b` at all.
        if a.is_empty() {
            return a;
        }
        let mut b = Vec::new();
        self.b.spans(ray, &mut b);
        match self.op {
            CsgOp::Difference => span_difference(&a, &b),
            CsgOp::Intersection => span_intersection(&a, &b),
        }
    }

    fn hit_test(&self, ray: &Vector) -> Option<RayHit> {
        first_span_hit(&self.spans(ray), ray)
    }
}

/// The hit a ray sees on a solid described by `spans`: the first finite
/// endpoint with `t > EPSILON`. Usually that's an `enter`; it's an
/// `exit` when the ray starts inside the solid, and the normal is then
/// the solid's outward normal, facing away from the ray (the same thing
/// the primitives' `hit_test` would report for a back face).
///
/// `Csg::hit_test` is this applied to the node's spans.
pub fn first_span_hit(spans: &[Span], ray: &Vector) -> Option<RayHit> {
    for span in spans {
        for end in [span.enter, span.exit] {
            if end.t > EPSILON && end.t.is_finite() {
                return Some(RayHit {
                    distance: end.t,
                    hit_point: ray_location(ray, end.t),
                    normal: end.normal,
                    surface: end.surface,
                });
            }
        }
    }
    None
}

/// Union of an arbitrary collection of spans (any order, possibly
/// overlapping) as a sorted, non-overlapping list. Touching spans
/// coalesce, so the shared face between two abutting solids disappears.
fn span_union_of(mut spans: Vec<Span>) -> Vec<Span> {
    spans.sort_by(|a, b| a.enter.t.total_cmp(&b.enter.t));
    let mut result: Vec<Span> = Vec::with_capacity(spans.len());
    for span in spans {
        match result.last_mut() {
            Some(last) if span.enter.t <= last.exit.t => {
                if span.exit.t > last.exit.t {
                    last.exit = span.exit;
                }
            }
            _ => result.push(span),
        }
    }
    result
}

/// Union of two sorted, non-overlapping span lists.
pub fn span_union(a: &[Span], b: &[Span]) -> Vec<Span> {
    span_union_of(a.iter().chain(b.iter()).copied().collect())
}

/// Intersection of two sorted, non-overlapping span lists: the ranges
/// inside both. Each result endpoint is the crossing that bounds the
/// overlap (the later of the two enters, the earlier of the two exits),
/// so it carries the right solid's normal and surface.
pub fn span_intersection(a: &[Span], b: &[Span]) -> Vec<Span> {
    let mut result = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let enter = if a[i].enter.t >= b[j].enter.t { a[i].enter } else { b[j].enter };
        let exit = if a[i].exit.t <= b[j].exit.t { a[i].exit } else { b[j].exit };
        if enter.t < exit.t {
            result.push(Span { enter, exit });
        }
        // Advance whichever span ends first; the other may still
        // overlap the next span of the list that advanced.
        if a[i].exit.t <= b[j].exit.t {
            i += 1;
        } else {
            j += 1;
        }
    }
    result
}

/// Difference of two sorted, non-overlapping span lists: the ranges
/// inside `a` but not inside `b`. Boundaries contributed by `b` are
/// flipped (see `SpanEnd::flipped`) and keep `b`'s surface.
pub fn span_difference(a: &[Span], b: &[Span]) -> Vec<Span> {
    let mut result = Vec::new();
    // `j` skips `b` spans that end before the current `a` span starts.
    // It never skips past a `b` span that might still overlap a later
    // `a` span, since both lists are sorted.
    let mut j = 0;
    for span in a {
        while j < b.len() && b[j].exit.t <= span.enter.t {
            j += 1;
        }
        let mut enter = span.enter;
        let mut k = j;
        while k < b.len() && b[k].enter.t < span.exit.t {
            let cut = b[k];
            if cut.enter.t > enter.t {
                result.push(Span { enter, exit: cut.enter.flipped() });
            }
            if cut.exit.t > enter.t {
                enter = cut.exit.flipped();
            }
            k += 1;
        }
        if enter.t < span.exit.t {
            result.push(Span { enter, exit: span.exit });
        }
    }
    result
}

impl Sphere {
    fn span(&self, ray: &Vector) -> Option<Span> {
        // Same quadratic as `hit_test`, keeping both roots and not
        // rejecting negative `t`. A tangent ray (zero discriminant)
        // touches the sphere at one point and has no inside.
        let oc = subp(ray.start, self.center);
        let a = dotp(ray.delta, ray.delta);
        let b = 2.0 * dotp(oc, ray.delta);
        let c = dotp(oc, oc) - self.r * self.r;
        let discriminant = b * b - 4.0 * a * c;
        if discriminant <= 0.0 {
            return None;
        }
        let sqrt_disc = discriminant.sqrt();
        let t0 = (-b - sqrt_disc) / (2.0 * a);
        let t1 = (-b + sqrt_disc) / (2.0 * a);
        let normal_at = |t| normalizep(subp(ray_location(ray, t), self.center));
        Some(Span {
            enter: SpanEnd { t: t0, normal: normal_at(t0), surface: self.surface },
            exit: SpanEnd { t: t1, normal: normal_at(t1), surface: self.surface },
        })
    }
}

impl Plane {
    fn span(&self, ray: &Vector) -> Option<Span> {
        // A plane is a half-space: the solid side is the one opposite
        // `normal`, i.e. the points where `(P - p0)·normal <= 0`.
        let side = |t: f64| SpanEnd { t, normal: self.normal, surface: self.surface };
        let denom = dotp(self.normal, ray.delta);
        if denom.abs() < EPSILON {
            // Parallel to the plane: the ray is inside for its whole
            // length or not at all.
            let inside = dotp(subp(ray.start, self.p0), self.normal) <= 0.0;
            return inside.then(|| Span {
                enter: side(f64::NEG_INFINITY),
                exit: side(f64::INFINITY),
            });
        }
        let t = dotp(subp(self.p0, ray.start), self.normal) / denom;
        if denom < 0.0 {
            // Moving against the normal: enters the solid side at `t`.
            Some(Span { enter: side(t), exit: side(f64::INFINITY) })
        } else {
            Some(Span { enter: side(f64::NEG_INFINITY), exit: side(t) })
        }
    }
}

impl Cuboid {
    fn span(&self, ray: &Vector) -> Option<Span> {
        // Slab method, as in `hit_test`, but tracking the exit face as
        // well as the entry face and without rejecting negative `t`.
        let mut t_enter = f64::NEG_INFINITY;
        let mut t_exit = f64::INFINITY;
        let mut enter_normal: Point = [0.0, 0.0, 0.0];
        let mut exit_normal: Point = [0.0, 0.0, 0.0];

        for i in 0..3 {
            let half = self.size[i] * 0.5;
            let min = self.center[i] - half;
            let max = self.center[i] + half;
            let origin = ray.start[i];
            let dir = ray.delta[i];

            if dir.abs() < EPSILON {
                if origin < min || origin > max {
                    return None;
                }
                continue;
            }

            let inv = 1.0 / dir;
            let mut t1 = (min - origin) * inv;
            let mut t2 = (max - origin) * inv;
            // Moving in +axis: enter through the min face (normal
            // -axis), exit through the max face (+axis). Moving in
            // -axis, the other way round.
            let mut sign = -1.0;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                sign = 1.0;
            }
            if t1 > t_enter {
                t_enter = t1;
                enter_normal = [0.0, 0.0, 0.0];
                enter_normal[i] = sign;
            }
            if t2 < t_exit {
                t_exit = t2;
                exit_normal = [0.0, 0.0, 0.0];
                exit_normal[i] = -sign;
            }
        }

        // At least one axis constrains `t` (the direction is nonzero),
        // so both ends are finite here.
        (t_enter < t_exit).then_some(Span {
            enter: SpanEnd { t: t_enter, normal: enter_normal, surface: self.surface },
            exit: SpanEnd { t: t_exit, normal: exit_normal, surface: self.surface },
        })
    }
}

impl Cylinder {
    fn span(&self, ray: &Vector) -> Option<Span> {
        // The same side and cap tests as `hit_test`, keeping every
        // valid crossing instead of the nearest one in front. The
        // cylinder is convex, so the span runs from the smallest
        // crossing to the largest.
        let axis = subp(self.p1, self.p0);
        let axis_len = lenp(axis);
        if axis_len < EPSILON {
            return None;
        }
        let axis_unit = scalep(axis, 1.0 / axis_len);

        let delta = subp(ray.start, self.p0);
        let d_dot_a = dotp(ray.delta, axis_unit);
        let delta_dot_a = dotp(delta, axis_unit);

        let mut candidates: Vec<(f64, Point)> = Vec::with_capacity(4);

        // Side surface.
        let d_perp = subp(ray.delta, scalep(axis_unit, d_dot_a));
        let delta_perp = subp(delta, scalep(axis_unit, delta_dot_a));
        let a = dotp(d_perp, d_perp);
        if a >= EPSILON {
            let b = 2.0 * dotp(delta_perp, d_perp);
            let c = dotp(delta_perp, delta_perp) - self.r * self.r;
            let discriminant = b * b - 4.0 * a * c;
            if discriminant > 0.0 {
                let sqrt_disc = discriminant.sqrt();
                for t in [(-b - sqrt_disc) / (2.0 * a), (-b + sqrt_disc) / (2.0 * a)] {
                    let s = delta_dot_a + t * d_dot_a;
                    if (0.0..=axis_len).contains(&s) {
                        let center_on_axis = addp(self.p0, scalep(axis_unit, s));
                        let normal = normalizep(subp(ray_location(ray, t), center_on_axis));
                        candidates.push((t, normal));
                    }
                }
            }
        }

        // End caps.
        let r_sq = self.r * self.r;
        for (cap_center, cap_normal) in [(self.p0, negp(axis_unit)), (self.p1, axis_unit)] {
            let denom = dotp(cap_normal, ray.delta);
            if denom.abs() < EPSILON {
                continue;
            }
            let t = dotp(subp(cap_center, ray.start), cap_normal) / denom;
            let radial = subp(ray_location(ray, t), cap_center);
            if dotp(radial, radial) <= r_sq {
                candidates.push((t, cap_normal));
            }
        }

        convex_span(&candidates, self.surface)
    }
}

impl Cone {
    fn span(&self, ray: &Vector) -> Option<Span> {
        // The same lateral and base-cap tests as `hit_test`, keeping
        // every valid crossing. A closed cone is convex, so the span
        // runs from the smallest crossing to the largest.
        let axis = subp(self.p0, self.p1);
        let axis_len = lenp(axis);
        if axis_len < EPSILON || self.r < EPSILON {
            return None;
        }
        let axis_unit = scalep(axis, 1.0 / axis_len);
        let apex = self.p1;

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

        let mut roots: [Option<f64>; 2] = [None, None];
        if a.abs() < EPSILON {
            if b.abs() >= EPSILON {
                roots[0] = Some(-c / b);
            }
        } else {
            // `>= 0`, not `> 0`: a ray straight down the axis passes
            // through the apex, where the root is double. A tangent
            // graze also gives a double root, but with no other
            // crossing `convex_span` discards it.
            let disc = b * b - 4.0 * a * c;
            if disc >= 0.0 {
                let sqrt_disc = disc.sqrt();
                roots[0] = Some((-b - sqrt_disc) / (2.0 * a));
                roots[1] = Some((-b + sqrt_disc) / (2.0 * a));
            }
        }

        let mut candidates: Vec<(f64, Point)> = Vec::with_capacity(3);
        let slope = self.r / axis_len;
        for t in roots.iter().flatten().copied() {
            // `s >= 0` discards the double cone's second nappe behind
            // the apex; `s <= axis_len` clips at the base plane.
            let s = cv + t * dv;
            if !(0.0..=axis_len).contains(&s) {
                continue;
            }
            let apex_to_p = subp(ray_location(ray, t), apex);
            let perp = subp(apex_to_p, scalep(axis_unit, s));
            let perp_len = lenp(perp);
            // At the apex tip the lateral normal is undefined.
            // `hit_test` skips such a hit; here dropping it could leave
            // the span without an endpoint, so point the normal out
            // through the apex instead.
            let normal = if perp_len < EPSILON {
                negp(axis_unit)
            } else {
                normalizep(subp(scalep(perp, 1.0 / perp_len), scalep(axis_unit, slope)))
            };
            candidates.push((t, normal));
        }

        // Base cap at `p0`, outward normal `+axis_unit`.
        let denom = dotp(axis_unit, ray.delta);
        if denom.abs() >= EPSILON {
            let t = dotp(subp(self.p0, ray.start), axis_unit) / denom;
            let radial = subp(ray_location(ray, t), self.p0);
            if dotp(radial, radial) <= self.r * self.r {
                candidates.push((t, axis_unit));
            }
        }

        convex_span(&candidates, self.surface)
    }
}

#[cfg(test)]
mod span_tests {
    use super::*;

    const TOL: f64 = 1e-9;

    fn ray(start: Point, delta: Point) -> Vector {
        Vector { start, delta }
    }

    fn surface(r: f64) -> Surface {
        Surface {
            color: [r, 0.0, 0.0],
            ambient: 0.2,
            specular: 0.5,
            light: 0.6,
            checked: false,
            reflection: 0.0,
            transparency: 0.0,
            metallic: false,
        }
    }

    fn spans_of(shape: &Shape, r: &Vector) -> Vec<Span> {
        let mut out = Vec::new();
        shape.spans(r, &mut out);
        out
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < TOL
    }

    fn close_p(a: Point, b: Point) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-7)
    }

    fn sphere(center: Point, r: f64) -> Shape {
        Shape::Sphere(Sphere { center, r, surface: None })
    }

    fn cuboid(center: Point, size: Point) -> Shape {
        Shape::Cuboid(Cuboid { center, size, surface: None })
    }

    fn cylinder(p0: Point, p1: Point, r: f64) -> Shape {
        Shape::Cylinder(Cylinder { p0, p1, r, surface: None })
    }

    fn cone(p0: Point, p1: Point, r: f64) -> Shape {
        Shape::Cone(Cone { p0, p1, r, surface: None })
    }

    fn assert_single_span(spans: &[Span], t0: f64, n0: Point, t1: f64, n1: Point) {
        assert_eq!(spans.len(), 1, "expected one span, got {:?}", spans);
        let s = spans[0];
        assert!(close(s.enter.t, t0), "enter t {} != {}", s.enter.t, t0);
        assert!(close(s.exit.t, t1), "exit t {} != {}", s.exit.t, t1);
        assert!(close_p(s.enter.normal, n0), "enter normal {:?} != {:?}", s.enter.normal, n0);
        assert!(close_p(s.exit.normal, n1), "exit normal {:?} != {:?}", s.exit.normal, n1);
    }

    // --- Per-primitive spans ----------------------------------------

    #[test]
    fn sphere_through_center() {
        let s = sphere([0.0, 0.0, 0.0], 1.0);
        let spans = spans_of(&s, &ray([0.0, 0.0, -5.0], [0.0, 0.0, 1.0]));
        assert_single_span(&spans, 4.0, [0.0, 0.0, -1.0], 6.0, [0.0, 0.0, 1.0]);
    }

    #[test]
    fn sphere_from_inside_and_miss() {
        let s = sphere([0.0, 0.0, 0.0], 1.0);
        let spans = spans_of(&s, &ray([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
        assert_single_span(&spans, -1.0, [-1.0, 0.0, 0.0], 1.0, [1.0, 0.0, 0.0]);
        assert!(spans_of(&s, &ray([0.0, 5.0, -5.0], [0.0, 0.0, 1.0])).is_empty());
    }

    #[test]
    fn cuboid_through_center_and_from_inside() {
        let c = cuboid([0.0, 0.0, 0.0], [2.0, 4.0, 6.0]);
        let spans = spans_of(&c, &ray([-5.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
        assert_single_span(&spans, 4.0, [-1.0, 0.0, 0.0], 6.0, [1.0, 0.0, 0.0]);
        let spans = spans_of(&c, &ray([0.0, 0.0, 0.0], [0.0, -1.0, 0.0]));
        assert_single_span(&spans, -2.0, [0.0, 1.0, 0.0], 2.0, [0.0, -1.0, 0.0]);
        assert!(spans_of(&c, &ray([5.0, 0.0, 0.0], [0.0, 1.0, 0.0])).is_empty());
    }

    #[test]
    fn cylinder_across_and_along_axis() {
        let c = cylinder([0.0, 0.0, -1.0], [0.0, 0.0, 1.0], 0.5);
        // Across: in and out through the curved side.
        let spans = spans_of(&c, &ray([-5.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
        assert_single_span(&spans, 4.5, [-1.0, 0.0, 0.0], 5.5, [1.0, 0.0, 0.0]);
        // Along the axis: in and out through the caps.
        let spans = spans_of(&c, &ray([0.0, 0.0, -5.0], [0.0, 0.0, 1.0]));
        assert_single_span(&spans, 4.0, [0.0, 0.0, -1.0], 6.0, [0.0, 0.0, 1.0]);
        // In through a cap, out through the side.
        let spans = spans_of(&c, &ray([0.0, 0.0, -2.0], [0.25, 0.0, 1.0]));
        assert_eq!(spans.len(), 1);
        assert!(close_p(spans[0].enter.normal, [0.0, 0.0, -1.0]));
        assert!(close_p(spans[0].exit.normal, [1.0, 0.0, 0.0]));
    }

    #[test]
    fn cone_across_base_and_lateral() {
        // Base at z=0 (radius 1), apex at z=1. 45° half-angle.
        let c = cone([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        // Up the axis: in through the base, out at the apex.
        let spans = spans_of(&c, &ray([0.0, 0.0, -5.0], [0.0, 0.0, 1.0]));
        assert_eq!(spans.len(), 1);
        assert!(close(spans[0].enter.t, 5.0) && close(spans[0].exit.t, 6.0));
        assert!(close_p(spans[0].enter.normal, [0.0, 0.0, -1.0]));
        // Across at z=0.5, where the radius is 0.5: through the
        // lateral surface both ways, normals tilted toward the apex.
        let spans = spans_of(&c, &ray([-5.0, 0.0, 0.5], [1.0, 0.0, 0.0]));
        let h = std::f64::consts::FRAC_1_SQRT_2;
        assert_single_span(&spans, 4.5, [-h, 0.0, h], 5.5, [h, 0.0, h]);
        // Above the apex: the second nappe of the double cone must not
        // show up.
        assert!(spans_of(&c, &ray([-5.0, 0.0, 1.5], [1.0, 0.0, 0.0])).is_empty());
    }

    #[test]
    fn plane_is_a_half_space() {
        // Solid below z=0 (opposite the +z normal).
        let p = Shape::Plane(Plane { normal: [0.0, 0.0, 1.0], p0: [0.0, 0.0, 0.0], surface: None });
        let down = spans_of(&p, &ray([0.0, 0.0, 5.0], [0.0, 0.0, -1.0]));
        assert_eq!(down.len(), 1);
        assert!(close(down[0].enter.t, 5.0) && down[0].exit.t == f64::INFINITY);
        let up = spans_of(&p, &ray([0.0, 0.0, 5.0], [0.0, 0.0, 1.0]));
        assert_eq!(up.len(), 1);
        assert!(up[0].enter.t == f64::NEG_INFINITY && close(up[0].exit.t, -5.0));
        let inside = spans_of(&p, &ray([0.0, 0.0, -1.0], [1.0, 0.0, 0.0]));
        assert_eq!(inside.len(), 1);
        assert!(inside[0].enter.t == f64::NEG_INFINITY && inside[0].exit.t == f64::INFINITY);
        assert!(spans_of(&p, &ray([0.0, 0.0, 1.0], [1.0, 0.0, 0.0])).is_empty());
    }

    #[test]
    fn non_solids_have_no_spans() {
        let r = ray([0.0, 0.0, -5.0], [0.0, 0.0, 1.0]);
        let tri = Shape::Triangle(Triangle {
            vertices: [[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]],
            normals: [[0.0, 0.0, -1.0]; 3],
            surface: None,
        });
        assert!(spans_of(&tri, &r).is_empty());
        assert!(spans_of(&Shape::Light(Light::white([0.0, 0.0, 0.0])), &r).is_empty());
    }

    // --- Wrappers ----------------------------------------------------

    #[test]
    fn transform_preserves_t_and_maps_normals() {
        // A unit sphere stretched 2x along x: along x the span is
        // [8, 12] in world `t`, same `t` as the local ray.
        let s = scale([2.0, 1.0, 1.0], sphere([0.0, 0.0, 0.0], 1.0));
        let spans = spans_of(&s, &ray([-10.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
        assert_single_span(&spans, 8.0, [-1.0, 0.0, 0.0], 12.0, [1.0, 0.0, 0.0]);
        // Rotated cuboid: a 2x2x2 box rotated 90° about z still has
        // face normals along ±x for a ray along x.
        let c = rotate_z(std::f64::consts::FRAC_PI_2, cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]));
        let spans = spans_of(&c, &ray([-5.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
        assert_single_span(&spans, 4.0, [-1.0, 0.0, 0.0], 6.0, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn surfaced_fills_only_missing_surfaces() {
        let outer = surface(0.1);
        let inner = surface(0.9);
        let g = surfaced(outer, group(vec![
            sphere([0.0, 0.0, 0.0], 1.0),
            Shape::Sphere(Sphere { center: [5.0, 0.0, 0.0], r: 1.0, surface: Some(inner) }),
        ]));
        let spans = spans_of(&g, &ray([-5.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].enter.surface, Some(outer));
        assert_eq!(spans[0].exit.surface, Some(outer));
        assert_eq!(spans[1].enter.surface, Some(inner));
        assert_eq!(spans[1].exit.surface, Some(inner));
    }

    #[test]
    fn group_is_a_union() {
        let r = ray([-5.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        // Overlapping spheres coalesce into one span.
        let g = group(vec![sphere([0.5, 0.0, 0.0], 1.0), sphere([-0.5, 0.0, 0.0], 1.0)]);
        assert_single_span(&spans_of(&g, &r), 3.5, [-1.0, 0.0, 0.0], 6.5, [1.0, 0.0, 0.0]);
        // Disjoint spheres give two spans, sorted, whatever the child order.
        let g = group(vec![sphere([3.0, 0.0, 0.0], 1.0), sphere([-3.0, 0.0, 0.0], 1.0)]);
        let spans = spans_of(&g, &r);
        assert_eq!(spans.len(), 2);
        assert!(close(spans[0].enter.t, 1.0) && close(spans[1].enter.t, 7.0));
    }

    #[test]
    fn bounded_matches_its_child() {
        let child = sphere([0.0, 0.0, 0.0], 1.0);
        let b = bounded(child.clone());
        for r in [
            ray([-5.0, 0.2, 0.1], [1.0, 0.0, 0.0]),
            ray([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]),   // origin inside
            ray([-5.0, 5.0, 0.0], [1.0, 0.0, 0.0]),  // miss
        ] {
            assert_eq!(spans_of(&b, &r), spans_of(&child, &r));
        }
    }

    // --- Consistency with hit_test -----------------------------------

    /// Deterministic pseudo-random numbers in [0, 1) (splitmix64).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
            ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
        }
        fn in_range(&mut self, lo: f64, hi: f64) -> f64 {
            lo + (hi - lo) * self.next()
        }
    }

    /// For rays starting well outside `shape` and aimed near it, the
    /// first span endpoint in front of the ray must be an `enter`
    /// that agrees with `hit_test`'s distance and normal, and
    /// `first_span_hit` must return the same hit.
    fn check_consistency(name: &str, shape: &Shape) {
        let mut rng = Rng(0x5eed);
        let mut hits = 0;
        for _ in 0..2000 {
            // Origin on a sphere of radius 20 around the origin.
            let dir = normalizep([rng.in_range(-1.0, 1.0), rng.in_range(-1.0, 1.0), rng.in_range(-1.0, 1.0)]);
            let start = scalep(dir, 20.0);
            let target = [rng.in_range(-2.0, 2.0), rng.in_range(-2.0, 2.0), rng.in_range(-2.0, 2.0)];
            let r = ray(start, normalizep(subp(target, start)));

            let hit = shape.hit_test(&r);
            let spans = spans_of(shape, &r);
            let span_hit = first_span_hit(&spans, &r);

            match (&hit, &span_hit) {
                (None, None) => {}
                (Some(h), Some(s)) => {
                    hits += 1;
                    assert!((h.distance - s.distance).abs() < 1e-7,
                            "{}: hit_test t {} vs span t {}", name, h.distance, s.distance);
                    assert!(close_p(h.normal, s.normal),
                            "{}: hit_test normal {:?} vs span normal {:?}", name, h.normal, s.normal);
                    let front = spans.iter().find(|sp| sp.exit.t > EPSILON).unwrap();
                    assert!(front.enter.t > EPSILON, "{}: origin should be outside", name);
                }
                _ => panic!("{}: hit_test {:?} vs spans {:?} disagree", name, hit.map(|h| h.distance), spans),
            }
        }
        assert!(hits > 200, "{}: too few hits ({}) to be a meaningful check", name, hits);
    }

    fn solids() -> Vec<(&'static str, Shape)> {
        vec![
            ("sphere", sphere([0.3, -0.2, 0.1], 1.5)),
            ("cuboid", cuboid([0.2, 0.1, -0.3], [2.0, 1.0, 3.0])),
            ("cylinder", cylinder([-1.0, -0.5, 0.0], [1.0, 1.0, 0.5], 0.8)),
            ("cone", cone([0.0, -1.0, 0.0], [0.5, 1.5, 0.3], 1.2)),
        ]
    }

    #[test]
    fn spans_agree_with_hit_test() {
        for (name, shape) in solids() {
            check_consistency(name, &shape);
        }
    }

    #[test]
    fn spans_agree_with_hit_test_under_transforms() {
        for (name, shape) in solids() {
            let t = translate([0.3, -0.4, 0.2],
                              rotate_axis([1.0, 2.0, 0.5], 0.7,
                                          scale([1.5, 0.6, 1.1], shape)));
            check_consistency(name, &t);
        }
    }

    // --- Set operations ----------------------------------------------

    /// A span with a marker normal: `+x` at the enter end, `+y` at
    /// the exit end, scaled by `tag` so results can be traced back to
    /// the list they came from.
    fn mk(t0: f64, t1: f64, tag: f64) -> Span {
        Span {
            enter: SpanEnd { t: t0, normal: [tag, 0.0, 0.0], surface: None },
            exit: SpanEnd { t: t1, normal: [0.0, tag, 0.0], surface: None },
        }
    }

    fn ts(spans: &[Span]) -> Vec<(f64, f64)> {
        spans.iter().map(|s| (s.enter.t, s.exit.t)).collect()
    }

    #[test]
    fn union_coalesces_overlapping_and_touching_spans() {
        let a = [mk(0.0, 2.0, 1.0), mk(5.0, 6.0, 1.0)];
        let b = [mk(1.0, 3.0, 2.0), mk(6.0, 7.0, 2.0), mk(9.0, 10.0, 2.0)];
        let u = span_union(&a, &b);
        assert_eq!(ts(&u), vec![(0.0, 3.0), (5.0, 7.0), (9.0, 10.0)]);
        // The coalesced span keeps the outermost endpoints' normals.
        assert_eq!(u[0].enter.normal, [1.0, 0.0, 0.0]);
        assert_eq!(u[0].exit.normal, [0.0, 2.0, 0.0]);
    }

    #[test]
    fn intersection_keeps_overlaps_with_bounding_crossings() {
        let a = [mk(0.0, 4.0, 1.0), mk(6.0, 10.0, 1.0)];
        let b = [mk(2.0, 7.0, 2.0), mk(8.0, 9.0, 2.0)];
        let i = span_intersection(&a, &b);
        assert_eq!(ts(&i), vec![(2.0, 4.0), (6.0, 7.0), (8.0, 9.0)]);
        assert_eq!(i[0].enter.normal, [2.0, 0.0, 0.0]); // entered via b
        assert_eq!(i[0].exit.normal, [0.0, 1.0, 0.0]);  // left via a
        // Touching spans don't intersect.
        assert!(span_intersection(&[mk(0.0, 1.0, 1.0)], &[mk(1.0, 2.0, 2.0)]).is_empty());
    }

    #[test]
    fn difference_carves_and_flips_cut_normals() {
        let a = [mk(0.0, 10.0, 1.0)];
        // A hole in the middle splits the span; both new boundaries are
        // b's crossings with flipped normals.
        let d = span_difference(&a, &[mk(3.0, 5.0, 2.0)]);
        assert_eq!(ts(&d), vec![(0.0, 3.0), (5.0, 10.0)]);
        assert_eq!(d[0].exit.normal, [-2.0, 0.0, 0.0]);
        assert_eq!(d[1].enter.normal, [0.0, -2.0, 0.0]);
        assert_eq!(d[1].exit.normal, [0.0, 1.0, 0.0]);
        // b covering the start trims it.
        assert_eq!(ts(&span_difference(&a, &[mk(-1.0, 4.0, 2.0)])), vec![(4.0, 10.0)]);
        // b covering the end trims it.
        assert_eq!(ts(&span_difference(&a, &[mk(8.0, 12.0, 2.0)])), vec![(0.0, 8.0)]);
        // b covering everything removes it.
        assert!(span_difference(&a, &[mk(-1.0, 11.0, 2.0)]).is_empty());
        // Several holes, and a b span straddling two a spans.
        let a2 = [mk(0.0, 4.0, 1.0), mk(6.0, 10.0, 1.0)];
        let b2 = [mk(1.0, 2.0, 2.0), mk(3.0, 7.0, 2.0), mk(8.0, 9.0, 2.0)];
        assert_eq!(ts(&span_difference(&a2, &b2)),
                   vec![(0.0, 1.0), (2.0, 3.0), (7.0, 8.0), (9.0, 10.0)]);
        // Nothing to subtract.
        assert_eq!(ts(&span_difference(&a2, &[])), ts(&a2));
    }

    #[test]
    fn first_span_hit_handles_inside_and_behind() {
        let r = ray([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        // Entirely behind the origin: no hit.
        assert!(first_span_hit(&[mk(-5.0, -1.0, 1.0)], &r).is_none());
        // Origin inside: the exit is the hit.
        let h = first_span_hit(&[mk(-1.0, 3.0, 1.0)], &r).unwrap();
        assert_eq!(h.distance, 3.0);
        assert_eq!(h.normal, [0.0, 1.0, 0.0]);
        // Infinite ends are never hits.
        let inf = [mk(f64::NEG_INFINITY, f64::INFINITY, 1.0)];
        assert!(first_span_hit(&inf, &r).is_none());
    }

    // --- The Csg node -------------------------------------------------

    fn plane(normal: Point, p0: Point) -> Shape {
        Shape::Plane(Plane { normal, p0, surface: None })
    }

    /// A unit bowl open toward -z: sphere minus a 0.9 sphere minus the
    /// half-space z <= 0.
    fn bowl() -> Shape {
        difference(difference(sphere([0.0, 0.0, 0.0], 1.0), sphere([0.0, 0.0, 0.0], 0.9)),
                   plane([0.0, 0.0, 1.0], [0.0, 0.0, 0.0]))
    }

    #[test]
    fn csg_hit_test_sees_the_bowl_interior() {
        let b = bowl();
        // Straight in from the open side: the first surface is the
        // inner wall, facing back toward the camera.
        let hit = b.hit_test(&ray([0.0, 0.0, -5.0], [0.0, 0.0, 1.0])).unwrap();
        assert!(close(hit.distance, 5.9));
        assert!(close_p(hit.normal, [0.0, 0.0, -1.0]));
        // From behind: the outer wall.
        let hit = b.hit_test(&ray([0.0, 0.0, 5.0], [0.0, 0.0, -1.0])).unwrap();
        assert!(close(hit.distance, 4.0));
        assert!(close_p(hit.normal, [0.0, 0.0, 1.0]));
        // In front of the rim plane there's nothing left to hit.
        assert!(b.hit_test(&ray([-5.0, 0.0, -0.5], [1.0, 0.0, 0.0])).is_none());
        // The rim itself: a ray along z just inside the outer radius
        // enters through the flat cut face, whose normal is the
        // flipped plane normal (pointing -z, out of the bowl).
        let hit = b.hit_test(&ray([0.95, 0.0, -5.0], [0.0, 0.0, 1.0])).unwrap();
        assert!(close(hit.distance, 5.0));
        assert!(close_p(hit.normal, [0.0, 0.0, -1.0]));
    }

    #[test]
    fn csg_difference_with_several_cutters_and_nesting() {
        // A 2x2x2 cube with a hole bored through along x (a cylinder)
        // and a notch cut from the top (a box), subtracted together
        // as one group, the way the n-ary SDL form builds it.
        let holed = difference(
            cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]),
            group(vec![cylinder([-2.0, 0.0, 0.0], [2.0, 0.0, 0.0], 0.3),
                       cuboid([0.0, 1.0, 0.0], [0.5, 1.0, 4.0])]),
        );
        // Down the bore: nothing to hit.
        assert!(holed.hit_test(&ray([-5.0, 0.0, 0.0], [1.0, 0.0, 0.0])).is_none());
        // Beside the bore: the cube's own face.
        let hit = holed.hit_test(&ray([-5.0, 0.6, 0.0], [1.0, 0.0, 0.0])).unwrap();
        assert!(close(hit.distance, 4.0));
        assert!(close_p(hit.normal, [-1.0, 0.0, 0.0]));
        // Down into the notch: the notch floor at y = 0.5, facing up.
        let hit = holed.hit_test(&ray([0.0, 5.0, 0.5], [0.0, -1.0, 0.0])).unwrap();
        assert!(close(hit.distance, 4.5));
        assert!(close_p(hit.normal, [0.0, 1.0, 0.0]));
        // Nesting: a Csg node is itself a valid operand.
        let nested = difference(holed, sphere([1.0, 1.0, 1.0], 0.5));
        assert!(nested.is_solid());
        let hit = nested.hit_test(&ray([5.0, 0.8, 0.8], [-1.0, 0.0, 0.0])).unwrap();
        // The sphere scoops the corner out: the ray first meets the
        // sphere's surface at x = 1 - sqrt(0.25 - 0.08), and the normal
        // there is the sphere's, flipped to point out of the scoop.
        let dx = 0.17_f64.sqrt();
        assert!(close(hit.distance, 4.0 + dx));
        assert!(close_p(hit.normal, [dx / 0.5, 0.4, 0.4]));
    }

    #[test]
    fn csg_intersection_and_bounds() {
        // A sphere clipped to a cube: flat faces where the cube is
        // inside the sphere, round elsewhere.
        let lens = intersection(sphere([0.0, 0.0, 0.0], 1.0), cuboid([0.0, 0.0, 0.0], [1.0, 1.0, 3.0]));
        let hit = lens.hit_test(&ray([-5.0, 0.0, 0.0], [1.0, 0.0, 0.0])).unwrap();
        assert!(close(hit.distance, 4.5) && close_p(hit.normal, [-1.0, 0.0, 0.0]));
        let hit = lens.hit_test(&ray([0.0, 0.0, -5.0], [0.0, 0.0, 1.0])).unwrap();
        assert!(close(hit.distance, 4.0) && close_p(hit.normal, [0.0, 0.0, -1.0]));
        let b = lens.bounds().unwrap();
        assert_eq!(b.min, [-0.5, -0.5, -1.0]);
        assert_eq!(b.max, [0.5, 0.5, 1.0]);
        // Difference is bounded by its first operand; an intersection
        // with a half-space is bounded by the bounded operand.
        assert_eq!(bowl().bounds(), sphere([0.0, 0.0, 0.0], 1.0).bounds());
        let half = intersection(sphere([0.0, 0.0, 0.0], 1.0), plane([0.0, 0.0, 1.0], [0.0, 0.0, 0.0]));
        assert_eq!(half.bounds(), sphere([0.0, 0.0, 0.0], 1.0).bounds());
    }

    #[test]
    fn csg_agrees_with_itself_under_transforms() {
        // The Transform wrapper must give the same hits whether it sits
        // above the Csg node or the node's operands are each wrapped.
        let above = rotate_y(0.4, scale([1.5, 0.7, 1.2], bowl()));
        let wrap = |s: Shape| rotate_y(0.4, scale([1.5, 0.7, 1.2], s));
        let below = difference(
            difference(wrap(sphere([0.0, 0.0, 0.0], 1.0)), wrap(sphere([0.0, 0.0, 0.0], 0.9))),
            wrap(plane([0.0, 0.0, 1.0], [0.0, 0.0, 0.0])),
        );
        let mut rng = Rng(0xb0);
        let mut hits = 0;
        for _ in 0..1000 {
            let dir = normalizep([rng.in_range(-1.0, 1.0), rng.in_range(-1.0, 1.0), rng.in_range(-1.0, 1.0)]);
            let start = scalep(dir, 10.0);
            let target = [rng.in_range(-1.0, 1.0), rng.in_range(-1.0, 1.0), rng.in_range(-1.0, 1.0)];
            let r = ray(start, normalizep(subp(target, start)));
            match (above.hit_test(&r), below.hit_test(&r)) {
                (None, None) => {}
                (Some(x), Some(y)) => {
                    hits += 1;
                    assert!((x.distance - y.distance).abs() < 1e-7);
                    assert!(close_p(x.normal, y.normal));
                }
                (x, y) => panic!("disagree: {:?} vs {:?}", x.map(|h| h.distance), y.map(|h| h.distance)),
            }
        }
        assert!(hits > 100);
    }

    #[test]
    fn csg_cut_faces_take_the_cutters_surface() {
        let body = surface(0.1);
        let cutter = surface(0.9);
        let s = surfaced(body, difference(
            cuboid([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]),
            Shape::Sphere(Sphere { center: [-1.0, 0.0, 0.0], r: 0.5, surface: Some(cutter) }),
        ));
        // Into the dimple: the cutter's surface.
        let hit = s.hit_test(&ray([-5.0, 0.0, 0.0], [1.0, 0.0, 0.0])).unwrap();
        assert_eq!(hit.surface, Some(cutter));
        // Beside it: the body's inherited surface.
        let hit = s.hit_test(&ray([-5.0, 0.8, 0.0], [1.0, 0.0, 0.0])).unwrap();
        assert_eq!(hit.surface, Some(body));
    }

    #[test]
    fn solidity() {
        let tri = Shape::Triangle(Triangle {
            vertices: [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: [[0.0, 0.0, 1.0]; 3],
            surface: None,
        });
        assert!(!tri.is_solid());
        assert!(!group(vec![sphere([0.0, 0.0, 0.0], 1.0), tri.clone()]).is_solid());
        assert!(!translate([1.0, 0.0, 0.0], group(vec![tri])).is_solid());
        assert!(group(vec![sphere([0.0, 0.0, 0.0], 1.0), Shape::Light(Light::white([0.0; 3]))]).is_solid());
        assert!(bowl().is_solid());
        assert!(plane([0.0, 0.0, 1.0], [0.0; 3]).is_solid());
    }

    #[test]
    #[should_panic(expected = "CSG operands must be solids")]
    fn csg_rejects_non_solid_operands() {
        let tri = Shape::Triangle(Triangle {
            vertices: [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: [[0.0, 0.0, 1.0]; 3],
            surface: None,
        });
        difference(sphere([0.0, 0.0, 0.0], 1.0), tri);
    }

    #[test]
    fn csg_of_real_shapes_through_span_ops() {
        // End-to-end preview of Phase 2: a bowl, i.e. a unit sphere
        // minus a smaller concentric sphere minus the half-space
        // z <= 0 (a plane with normal +z).
        let r = ray([0.0, 0.0, -5.0], [0.0, 0.0, 1.0]);
        let outer = spans_of(&sphere([0.0, 0.0, 0.0], 1.0), &r);
        let inner = spans_of(&sphere([0.0, 0.0, 0.0], 0.9), &r);
        let front = spans_of(&Shape::Plane(Plane { normal: [0.0, 0.0, 1.0], p0: [0.0, 0.0, 0.0], surface: None }), &r);
        let shell = span_difference(&outer, &inner);
        let bowl = span_difference(&shell, &front);
        // Only the back wall of the shell remains: z from 0.9 to 1.0.
        assert_eq!(bowl.len(), 1);
        assert!(close(bowl[0].enter.t, 5.9) && close(bowl[0].exit.t, 6.0));
        // Seen from the camera, the first surface is the *inside* of
        // the inner sphere, whose normal must face back toward -z.
        let hit = first_span_hit(&bowl, &r).unwrap();
        assert!(close_p(hit.normal, [0.0, 0.0, -1.0]));
    }
}
