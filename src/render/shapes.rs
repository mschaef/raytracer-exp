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
    Point,
    Surface,
    Hittable,
    Vector,
    RayHit,
    subp,
    dotp,
    crossp,
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

pub struct Sphere {
    pub center: Point,
    pub r: f64,
    pub surface: Surface,
}

pub struct Plane {
    pub normal: Point,
    pub p0: Point,
    pub surface: Surface,
}

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
pub struct Triangle {
    pub vertices: [Point; 3],
    pub normals: [Point; 3],
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
#[derive(Copy, Clone)]
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
/// The `scene_objects!` macro and the `From` impls below let scene
/// definitions just write `Sphere { ... }` and have it auto-promoted to
/// the right variant.
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
pub enum Shape {
    Sphere(Sphere),
    Plane(Plane),
    Cuboid(Cuboid),
    Triangle(Triangle),
    Group(Vec<Shape>),
    Transform(Box<Transformed>),
    Bounded(Box<Bounded>),
}

/// Storage for a `Shape::Bounded` node. Holds the bounding AABB and the
/// child subtree it accelerates. Construction via `bounded(...)` (which
/// auto-computes the bound from the child) or `bounded_with(bounds, child)`
/// (which uses a caller-supplied bound — handy when the same bound is
/// being used for both acceleration and visualization).
pub struct Bounded {
    pub bounds: AABB,
    pub child: Shape,
}

/// Storage for a `Shape::Transform` node. Cached at construction so that
/// hit-testing only does the cheap part (matrix-vector multiplies) per ray.
///
/// - `inverse` is the world-to-local affine: applied to the ray on the way
///   in, so the child sees a ray in its own coordinate system.
/// - `normal_xform` is the inverse-transpose of the *forward* linear part
///   (equivalently, the transpose of `inverse.linear`). It transforms the
///   child's local-space normal back to world space. Using the
///   inverse-transpose rather than the forward matrix is what keeps normals
///   correct under non-uniform scale.
pub struct Transformed {
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

impl Hittable for Shape {
    fn hit_test(&self, ray: &Vector) -> Option<RayHit> {
        match self {
            Shape::Sphere(s)        => s.hit_test(ray),
            Shape::Plane(p)         => p.hit_test(ray),
            Shape::Cuboid(c)        => c.hit_test(ray),
            Shape::Triangle(t)      => t.hit_test(ray),
            Shape::Group(children)  => nearest_hit(ray, children),
            Shape::Transform(t)     => t.hit_test(ray),
            Shape::Bounded(b)       => b.hit_test(ray),
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
            // Transformed bounds need the forward affine, which
            // `Transformed` doesn't currently cache. Phase 3 will add it.
            // For now, callers can wrap the inner (untransformed) shape
            // in `bounded(...)` and put the Transform on the outside,
            // which works correctly because the Transform's hit_test
            // inverse-transforms the ray before dispatching to the
            // Bounded child.
            Shape::Transform(_) => None,
            Shape::Bounded(b) => Some(b.bounds),
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
