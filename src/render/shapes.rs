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
    ray_location,
    normalizep,
    EPSILON,
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
pub enum Shape {
    Sphere(Sphere),
    Plane(Plane),
    Cuboid(Cuboid),
    Group(Vec<Shape>),
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

impl Hittable for Shape {
    fn hit_test(&self, ray: &Vector) -> Option<RayHit> {
        match self {
            Shape::Sphere(s)        => s.hit_test(ray),
            Shape::Plane(p)         => p.hit_test(ray),
            Shape::Cuboid(c)        => c.hit_test(ray),
            Shape::Group(children)  => nearest_hit(ray, children),
        }
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
