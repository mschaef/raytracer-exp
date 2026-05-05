// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Phase 2 host bindings.
//!
//! Wires the ray tracer's `Surface`, `Light`, `Camera`, `Shape`, and
//! `Scene` types up as native SDL functions, so scripts can build the
//! same scene values that Rust scenes (in `crate::scenes`) build.
//!
//! Multi-field constructors (`surface`, `sphere`, `plane`, `cuboid`,
//! `triangle`, `cylinder`, `scene`) are map-keyed: a single argument
//! that's a map of `:keyword value` pairs. This trades terseness for
//! readability and lets us provide sensible defaults for optional
//! fields. Single-purpose constructors (`light-white`, `light-point`,
//! `camera-looking-at`, `translate`, `scale`, `rotate-x`, …) are
//! positional since they have only a few arguments and the order is
//! intuitive.
//!
//! Numbers passed to host fields are accepted as either int or float
//! and coerced to f64 — scripts can write `(sphere {:r 1 :center [0 0 0] ...})`
//! without sprinkling `1.0`s through the source.

use std::collections::HashMap;
use std::rc::Rc;

use crate::render::color::LinearColor;
use crate::render::geometry::Point;
use crate::render::shapes::{
    bounded, bounded_with, group, rotate_axis, rotate_x, rotate_y, rotate_z,
    scale, transform, translate, AABB, Cuboid, Cylinder, Plane, Shape, Sphere,
    Triangle,
};
use crate::render::transform::Affine;
use crate::render::{Camera, Light, Scene, Surface};

use crate::sdl::env::EnvRef;
use crate::sdl::error::Position;
use crate::sdl::value::{Function, FunctionKind, NativeFn, Value};
use crate::sdl_panic;

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

/// Install every Phase 2 binding into `env`. Called by `default_env`
/// after the language built-ins are installed so a script can use both.
pub fn install(env: &EnvRef) {
    // Surfaces.
    define_native(env, "surface", builtin_surface);

    // Lights.
    define_native(env, "light-white", builtin_light_white);
    define_native(env, "light-point", builtin_light_point);

    // Cameras.
    define_native(env, "camera-looking-at", builtin_camera_looking_at);
    define_native(env, "camera-with-fov", builtin_camera_with_fov);

    // Leaf shapes.
    define_native(env, "sphere", builtin_sphere);
    define_native(env, "plane", builtin_plane);
    define_native(env, "cuboid", builtin_cuboid);
    define_native(env, "triangle", builtin_triangle);
    define_native(env, "cylinder", builtin_cylinder);

    // Composite / transformed shapes.
    define_native(env, "group", builtin_group);
    define_native(env, "transform", builtin_transform);
    define_native(env, "translate", builtin_translate);
    define_native(env, "scale", builtin_scale);
    define_native(env, "rotate-x", builtin_rotate_x);
    define_native(env, "rotate-y", builtin_rotate_y);
    define_native(env, "rotate-z", builtin_rotate_z);
    define_native(env, "rotate-axis", builtin_rotate_axis);
    define_native(env, "bounded", builtin_bounded);
    define_native(env, "bounded-with", builtin_bounded_with);

    // Affine transform constructors. Mirror the methods on
    // `Affine` so `transform` can be used directly from script.
    define_native(env, "affine-identity", builtin_affine_identity);
    define_native(env, "affine-translation", builtin_affine_translation);
    define_native(env, "affine-scale", builtin_affine_scale);
    define_native(env, "affine-rotation-x", builtin_affine_rotation_x);
    define_native(env, "affine-rotation-y", builtin_affine_rotation_y);
    define_native(env, "affine-rotation-z", builtin_affine_rotation_z);
    define_native(env, "affine-rotation-axis", builtin_affine_rotation_axis);
    define_native(env, "affine-compose", builtin_affine_compose);
    define_native(env, "affine-inverse", builtin_affine_inverse);

    // Axis-aligned bounding boxes. Useful with `bounded-with`.
    define_native(env, "aabb", builtin_aabb);

    // Scene aggregation.
    define_native(env, "scene", builtin_scene);

    // Type predicates for the new variants.
    define_native(env, "surface?", builtin_surface_q);
    define_native(env, "camera?", builtin_camera_q);
    define_native(env, "affine?", builtin_affine_q);
    define_native(env, "aabb?", builtin_aabb_q);
    define_native(env, "light?", builtin_light_q);
    define_native(env, "shape?", builtin_shape_q);
    define_native(env, "scene?", builtin_scene_q);
}

fn define_native(env: &EnvRef, name: &'static str, func: NativeFn) {
    let f = Function {
        kind: FunctionKind::Native { name, func },
    };
    env.borrow_mut().define(name, Value::Fn(Rc::new(f)));
}

// ---------------------------------------------------------------------------
// Type-extraction helpers
// ---------------------------------------------------------------------------
//
// Each helper either returns the unwrapped host value or panics with a
// position-tagged SDL error message. The small amount of duplication
// across helpers is the cost of getting clear error messages naming the
// specific binding and field that failed.

fn require_arity(args: &[Value], expected: usize, name: &str, pos: &Position) {
    if args.len() != expected {
        sdl_panic!(
            pos.clone(),
            "{} takes {} argument{} (got {})",
            name,
            expected,
            if expected == 1 { "" } else { "s" },
            args.len()
        );
    }
}

fn require_number(v: &Value, ctx: &str, pos: &Position) -> f64 {
    match v.as_f64() {
        Some(f) => f,
        None => sdl_panic!(
            pos.clone(),
            "{} expected a number, got {} ({})",
            ctx,
            v,
            v.type_name()
        ),
    }
}

fn require_int(v: &Value, ctx: &str, pos: &Position) -> i64 {
    match v {
        Value::Int(i) => *i,
        other => sdl_panic!(
            pos.clone(),
            "{} expected an integer, got {} ({})",
            ctx,
            other,
            other.type_name()
        ),
    }
}

fn require_bool(v: &Value, ctx: &str, pos: &Position) -> bool {
    match v {
        Value::Bool(b) => *b,
        other => sdl_panic!(
            pos.clone(),
            "{} expected a bool, got {} ({})",
            ctx,
            other,
            other.type_name()
        ),
    }
}

fn require_string(v: &Value, ctx: &str, pos: &Position) -> String {
    match v {
        Value::String(s) => (**s).clone(),
        other => sdl_panic!(
            pos.clone(),
            "{} expected a string, got {} ({})",
            ctx,
            other,
            other.type_name()
        ),
    }
}

/// Unwrap a vector value into its (Rc-shared) element list; panics on
/// non-vector. Returns a clone of the shared `Rc` so the caller can
/// iterate without borrowing the source `Value`.
fn require_vec(v: &Value, ctx: &str, pos: &Position) -> Rc<Vec<Value>> {
    match v {
        Value::Vec(items) => items.clone(),
        other => sdl_panic!(
            pos.clone(),
            "{} expected a vector, got {} ({})",
            ctx,
            other,
            other.type_name()
        ),
    }
}

/// Unwrap a 3-element vector of numbers as a `[f64; 3]` Point /
/// LinearColor. Same backing type for both — the helper doesn't care
/// whether you're pulling out a position, a color, or a direction.
fn require_point(v: &Value, ctx: &str, pos: &Position) -> Point {
    let items = require_vec(v, ctx, pos);
    if items.len() != 3 {
        sdl_panic!(
            pos.clone(),
            "{} expected a 3-element vector, got {} elements",
            ctx,
            items.len()
        );
    }
    [
        require_number(&items[0], ctx, pos),
        require_number(&items[1], ctx, pos),
        require_number(&items[2], ctx, pos),
    ]
}

/// Unwrap a map value; panics on non-map. Returns a clone of the
/// shared `Rc` so the caller can iterate without holding a borrow.
fn require_map(v: &Value, ctx: &str, pos: &Position) -> Rc<HashMap<String, Value>> {
    match v {
        Value::Map(m) => m.clone(),
        other => sdl_panic!(
            pos.clone(),
            "{} expected a map, got {} ({})",
            ctx,
            other,
            other.type_name()
        ),
    }
}

/// Look up a required keyword key in `map`. Panics with a clear
/// message if the key is missing — required-vs-optional is the
/// distinction the constructors care about.
fn require_key<'a>(
    map: &'a HashMap<String, Value>,
    key: &str,
    ctx: &str,
    pos: &Position,
) -> &'a Value {
    match map.get(key) {
        Some(v) => v,
        None => sdl_panic!(pos.clone(), "{} requires :{} key", ctx, key),
    }
}

fn require_key_number(
    map: &HashMap<String, Value>,
    key: &str,
    ctx: &str,
    pos: &Position,
) -> f64 {
    require_number(require_key(map, key, ctx, pos), &format!("{} :{}", ctx, key), pos)
}

fn require_key_point(
    map: &HashMap<String, Value>,
    key: &str,
    ctx: &str,
    pos: &Position,
) -> Point {
    require_point(require_key(map, key, ctx, pos), &format!("{} :{}", ctx, key), pos)
}

fn require_key_string(
    map: &HashMap<String, Value>,
    key: &str,
    ctx: &str,
    pos: &Position,
) -> String {
    require_string(require_key(map, key, ctx, pos), &format!("{} :{}", ctx, key), pos)
}

fn require_key_surface(
    map: &HashMap<String, Value>,
    key: &str,
    ctx: &str,
    pos: &Position,
) -> Surface {
    let v = require_key(map, key, ctx, pos);
    match v {
        Value::Surface(s) => *s,
        other => sdl_panic!(
            pos.clone(),
            "{} :{} expected a surface, got {} ({})",
            ctx,
            key,
            other,
            other.type_name()
        ),
    }
}

fn require_key_camera(
    map: &HashMap<String, Value>,
    key: &str,
    ctx: &str,
    pos: &Position,
) -> Camera {
    let v = require_key(map, key, ctx, pos);
    match v {
        Value::Camera(c) => *c,
        other => sdl_panic!(
            pos.clone(),
            "{} :{} expected a camera, got {} ({})",
            ctx,
            key,
            other,
            other.type_name()
        ),
    }
}

/// Optional: returns `Some` if the key is present, `None` otherwise.
/// Lets callers fall back to a documented default rather than failing
/// on any missing-key.
fn maybe_key_number(
    map: &HashMap<String, Value>,
    key: &str,
    ctx: &str,
    pos: &Position,
) -> Option<f64> {
    map.get(key)
        .map(|v| require_number(v, &format!("{} :{}", ctx, key), pos))
}

fn maybe_key_bool(
    map: &HashMap<String, Value>,
    key: &str,
    ctx: &str,
    pos: &Position,
) -> Option<bool> {
    map.get(key)
        .map(|v| require_bool(v, &format!("{} :{}", ctx, key), pos))
}

fn maybe_key_int(
    map: &HashMap<String, Value>,
    key: &str,
    ctx: &str,
    pos: &Position,
) -> Option<i64> {
    map.get(key)
        .map(|v| require_int(v, &format!("{} :{}", ctx, key), pos))
}

fn maybe_key_point(
    map: &HashMap<String, Value>,
    key: &str,
    ctx: &str,
    pos: &Position,
) -> Option<Point> {
    map.get(key)
        .map(|v| require_point(v, &format!("{} :{}", ctx, key), pos))
}

fn require_shape_value(v: &Value, ctx: &str, pos: &Position) -> Shape {
    match v {
        // Clone the inner Shape out of the Rc. Shape implements Clone
        // (deep-clone of the tree), which is what lets us hand an owned
        // value to the constructors that expect `impl Into<Shape>`.
        // Sharing in script is still cheap: the Value::Shape is an
        // Rc<Shape>, so passing it around inside the script doesn't
        // clone; the deep-clone only happens at host-binding boundaries
        // when we hand the shape to a host function that takes ownership.
        Value::Shape(s) => (**s).clone(),
        other => sdl_panic!(
            pos.clone(),
            "{} expected a shape, got {} ({})",
            ctx,
            other,
            other.type_name()
        ),
    }
}

fn require_light_value(v: &Value, ctx: &str, pos: &Position) -> Light {
    match v {
        Value::Light(l) => (**l).clone(),
        other => sdl_panic!(
            pos.clone(),
            "{} expected a light, got {} ({})",
            ctx,
            other,
            other.type_name()
        ),
    }
}

fn require_affine(v: &Value, ctx: &str, pos: &Position) -> Affine {
    match v {
        Value::Affine(a) => *a,
        other => sdl_panic!(
            pos.clone(),
            "{} expected an affine, got {} ({})",
            ctx,
            other,
            other.type_name()
        ),
    }
}

fn require_aabb(v: &Value, ctx: &str, pos: &Position) -> AABB {
    match v {
        Value::Aabb(b) => *b,
        other => sdl_panic!(
            pos.clone(),
            "{} expected an aabb, got {} ({})",
            ctx,
            other,
            other.type_name()
        ),
    }
}

// ---------------------------------------------------------------------------
// Surface
// ---------------------------------------------------------------------------

/// `(surface {:color [r g b] :ambient n :specular n :light n :checked b :reflection n})`
///
/// All keys except `:color` have defaults. The defaults match an
/// uninteresting matte surface so that omitting a key gives a
/// predictable result (no specular highlight, full diffuse, no
/// reflection, no checkering).
fn builtin_surface(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "surface", pos);
    let map = require_map(&args[0], "surface", pos);

    let color: LinearColor = require_key_point(&map, "color", "surface", pos);
    let ambient = maybe_key_number(&map, "ambient", "surface", pos).unwrap_or(0.0);
    let specular = maybe_key_number(&map, "specular", "surface", pos).unwrap_or(0.0);
    let light = maybe_key_number(&map, "light", "surface", pos).unwrap_or(1.0);
    let checked = maybe_key_bool(&map, "checked", "surface", pos).unwrap_or(false);
    let reflection = maybe_key_number(&map, "reflection", "surface", pos).unwrap_or(0.0);

    Value::Surface(Surface {
        color,
        ambient,
        specular,
        light,
        checked,
        reflection,
    })
}

// ---------------------------------------------------------------------------
// Lights
// ---------------------------------------------------------------------------

/// `(light-white [x y z])` — full-intensity white point light.
fn builtin_light_white(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "light-white", pos);
    let location = require_point(&args[0], "light-white location", pos);
    Value::Light(Rc::new(Light::white(location)))
}

/// `(light-point [x y z] [r g b] intensity)`.
fn builtin_light_point(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 3, "light-point", pos);
    let location = require_point(&args[0], "light-point location", pos);
    let color = require_point(&args[1], "light-point color", pos);
    let intensity = require_number(&args[2], "light-point intensity", pos);
    Value::Light(Rc::new(Light::point(location, color, intensity)))
}

// ---------------------------------------------------------------------------
// Cameras
// ---------------------------------------------------------------------------

/// `(camera-looking-at location look-at up-hint zoom)`.
fn builtin_camera_looking_at(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 4, "camera-looking-at", pos);
    let location = require_point(&args[0], "camera-looking-at location", pos);
    let look_at = require_point(&args[1], "camera-looking-at look-at", pos);
    let up_hint = require_point(&args[2], "camera-looking-at up-hint", pos);
    let zoom = require_number(&args[3], "camera-looking-at zoom", pos);
    Value::Camera(Camera::looking_at(location, look_at, up_hint, zoom))
}

/// `(camera-with-fov location look-at up-hint fov-radians)`.
fn builtin_camera_with_fov(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 4, "camera-with-fov", pos);
    let location = require_point(&args[0], "camera-with-fov location", pos);
    let look_at = require_point(&args[1], "camera-with-fov look-at", pos);
    let up_hint = require_point(&args[2], "camera-with-fov up-hint", pos);
    let fov = require_number(&args[3], "camera-with-fov fov-radians", pos);
    Value::Camera(Camera::with_fov(location, look_at, up_hint, fov))
}

// ---------------------------------------------------------------------------
// Leaf shapes
// ---------------------------------------------------------------------------

fn builtin_sphere(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "sphere", pos);
    let map = require_map(&args[0], "sphere", pos);
    let center = require_key_point(&map, "center", "sphere", pos);
    let r = require_key_number(&map, "r", "sphere", pos);
    let surface = require_key_surface(&map, "surface", "sphere", pos);
    Value::Shape(Rc::new(Shape::Sphere(Sphere { center, r, surface })))
}

fn builtin_plane(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "plane", pos);
    let map = require_map(&args[0], "plane", pos);
    let normal = require_key_point(&map, "normal", "plane", pos);
    let p0 = require_key_point(&map, "p0", "plane", pos);
    let surface = require_key_surface(&map, "surface", "plane", pos);
    Value::Shape(Rc::new(Shape::Plane(Plane { normal, p0, surface })))
}

fn builtin_cuboid(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "cuboid", pos);
    let map = require_map(&args[0], "cuboid", pos);
    let center = require_key_point(&map, "center", "cuboid", pos);
    let size = require_key_point(&map, "size", "cuboid", pos);
    let surface = require_key_surface(&map, "surface", "cuboid", pos);
    Value::Shape(Rc::new(Shape::Cuboid(Cuboid { center, size, surface })))
}

/// `(triangle {:vertices [[..] [..] [..]] :normals [[..] [..] [..]] :surface S})`.
///
/// `:normals` is optional — if absent, the geometric face normal is
/// computed and used for all three vertex slots (flat shading). This
/// matches what `mesh::load_obj` does when an OBJ file lacks per-vertex
/// normals.
fn builtin_triangle(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "triangle", pos);
    let map = require_map(&args[0], "triangle", pos);

    let verts = require_vec(require_key(&map, "vertices", "triangle", pos),
        "triangle :vertices", pos);
    if verts.len() != 3 {
        sdl_panic!(pos.clone(), "triangle :vertices must have 3 entries (got {})", verts.len());
    }
    let v0 = require_point(&verts[0], "triangle :vertices[0]", pos);
    let v1 = require_point(&verts[1], "triangle :vertices[1]", pos);
    let v2 = require_point(&verts[2], "triangle :vertices[2]", pos);

    let surface = require_key_surface(&map, "surface", "triangle", pos);

    let normals = if let Some(nv) = map.get("normals") {
        let items = require_vec(nv, "triangle :normals", pos);
        if items.len() != 3 {
            sdl_panic!(pos.clone(),
                "triangle :normals must have 3 entries (got {})", items.len());
        }
        [
            require_point(&items[0], "triangle :normals[0]", pos),
            require_point(&items[1], "triangle :normals[1]", pos),
            require_point(&items[2], "triangle :normals[2]", pos),
        ]
    } else {
        // Geometric face normal as the flat-shading default.
        let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
        let n = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let n = if len > 0.0 {
            [n[0] / len, n[1] / len, n[2] / len]
        } else {
            // Degenerate triangle: defaulting to +z is arbitrary, but
            // the renderer would treat this triangle as missed
            // anyway (Möller–Trumbore early-outs on `a.abs() < EPSILON`).
            [0.0, 0.0, 1.0]
        };
        [n, n, n]
    };

    Value::Shape(Rc::new(Shape::Triangle(Triangle {
        vertices: [v0, v1, v2],
        normals,
        surface,
    })))
}

fn builtin_cylinder(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "cylinder", pos);
    let map = require_map(&args[0], "cylinder", pos);
    let p0 = require_key_point(&map, "p0", "cylinder", pos);
    let p1 = require_key_point(&map, "p1", "cylinder", pos);
    let r = require_key_number(&map, "r", "cylinder", pos);
    let surface = require_key_surface(&map, "surface", "cylinder", pos);
    Value::Shape(Rc::new(Shape::Cylinder(Cylinder { p0, p1, r, surface })))
}

// ---------------------------------------------------------------------------
// Composite / transformed shapes
// ---------------------------------------------------------------------------

/// `(group [shape1 shape2 ...])` — wraps a list of shapes as a
/// hierarchical `Shape::Group`.
fn builtin_group(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "group", pos);
    let items = require_vec(&args[0], "group", pos);
    let children: Vec<Shape> = items
        .iter()
        .map(|v| require_shape_value(v, "group child", pos))
        .collect();
    Value::Shape(Rc::new(group(children)))
}

/// `(transform affine shape)` — wraps `shape` in an arbitrary affine.
/// The escape hatch for transforms not expressible as a single rotate /
/// scale / translate. Use `affine-compose` to build the input.
fn builtin_transform(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "transform", pos);
    let aff = require_affine(&args[0], "transform affine", pos);
    let child = require_shape_value(&args[1], "transform child", pos);
    Value::Shape(Rc::new(transform(aff, child)))
}

fn builtin_translate(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "translate", pos);
    let d = require_point(&args[0], "translate offset", pos);
    let child = require_shape_value(&args[1], "translate child", pos);
    Value::Shape(Rc::new(translate(d, child)))
}

fn builtin_scale(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "scale", pos);
    let s = require_point(&args[0], "scale factors", pos);
    let child = require_shape_value(&args[1], "scale child", pos);
    Value::Shape(Rc::new(scale(s, child)))
}

fn builtin_rotate_x(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "rotate-x", pos);
    let theta = require_number(&args[0], "rotate-x theta", pos);
    let child = require_shape_value(&args[1], "rotate-x child", pos);
    Value::Shape(Rc::new(rotate_x(theta, child)))
}

fn builtin_rotate_y(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "rotate-y", pos);
    let theta = require_number(&args[0], "rotate-y theta", pos);
    let child = require_shape_value(&args[1], "rotate-y child", pos);
    Value::Shape(Rc::new(rotate_y(theta, child)))
}

fn builtin_rotate_z(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "rotate-z", pos);
    let theta = require_number(&args[0], "rotate-z theta", pos);
    let child = require_shape_value(&args[1], "rotate-z child", pos);
    Value::Shape(Rc::new(rotate_z(theta, child)))
}

fn builtin_rotate_axis(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 3, "rotate-axis", pos);
    let axis = require_point(&args[0], "rotate-axis axis", pos);
    let theta = require_number(&args[1], "rotate-axis theta", pos);
    let child = require_shape_value(&args[2], "rotate-axis child", pos);
    Value::Shape(Rc::new(rotate_axis(axis, theta, child)))
}

/// `(bounded shape)` — auto-computes the AABB of `shape` and wraps it.
/// Panics (matching the host `bounded`) if `shape` is unbounded
/// (e.g. a Plane).
fn builtin_bounded(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "bounded", pos);
    let child = require_shape_value(&args[0], "bounded child", pos);
    Value::Shape(Rc::new(bounded(child)))
}

/// `(bounded-with aabb shape)` — wraps `shape` in an explicit AABB.
fn builtin_bounded_with(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "bounded-with", pos);
    let aabb = require_aabb(&args[0], "bounded-with bounds", pos);
    let child = require_shape_value(&args[1], "bounded-with child", pos);
    Value::Shape(Rc::new(bounded_with(aabb, child)))
}

// ---------------------------------------------------------------------------
// Affine constructors
// ---------------------------------------------------------------------------

fn builtin_affine_identity(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 0, "affine-identity", pos);
    Value::Affine(Affine::identity())
}

fn builtin_affine_translation(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "affine-translation", pos);
    let d = require_point(&args[0], "affine-translation offset", pos);
    Value::Affine(Affine::translation(d))
}

fn builtin_affine_scale(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "affine-scale", pos);
    let s = require_point(&args[0], "affine-scale factors", pos);
    Value::Affine(Affine::scale(s))
}

fn builtin_affine_rotation_x(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "affine-rotation-x", pos);
    let theta = require_number(&args[0], "affine-rotation-x theta", pos);
    Value::Affine(Affine::rotation_x(theta))
}

fn builtin_affine_rotation_y(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "affine-rotation-y", pos);
    let theta = require_number(&args[0], "affine-rotation-y theta", pos);
    Value::Affine(Affine::rotation_y(theta))
}

fn builtin_affine_rotation_z(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "affine-rotation-z", pos);
    let theta = require_number(&args[0], "affine-rotation-z theta", pos);
    Value::Affine(Affine::rotation_z(theta))
}

fn builtin_affine_rotation_axis(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "affine-rotation-axis", pos);
    let axis = require_point(&args[0], "affine-rotation-axis axis", pos);
    let theta = require_number(&args[1], "affine-rotation-axis theta", pos);
    Value::Affine(Affine::rotation_axis(axis, theta))
}

/// `(affine-compose a b)` returns `a ∘ b`, i.e. "apply `b` first, then
/// `a`" — same convention as `Affine::compose`.
fn builtin_affine_compose(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "affine-compose", pos);
    let a = require_affine(&args[0], "affine-compose first", pos);
    let b = require_affine(&args[1], "affine-compose second", pos);
    Value::Affine(a.compose(b))
}

fn builtin_affine_inverse(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "affine-inverse", pos);
    let a = require_affine(&args[0], "affine-inverse", pos);
    Value::Affine(a.inverse())
}

// ---------------------------------------------------------------------------
// AABB
// ---------------------------------------------------------------------------

/// `(aabb [min-x min-y min-z] [max-x max-y max-z])`.
fn builtin_aabb(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "aabb", pos);
    let min = require_point(&args[0], "aabb min", pos);
    let max = require_point(&args[1], "aabb max", pos);
    Value::Aabb(AABB::new(min, max))
}

// ---------------------------------------------------------------------------
// Scene
// ---------------------------------------------------------------------------

/// `(scene {:name "..." :camera C :background [r g b] :lights [...] :objects [...] :reflect-limit n :oversample n})`
fn builtin_scene(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "scene", pos);
    let map = require_map(&args[0], "scene", pos);

    let name = require_key_string(&map, "name", "scene", pos);
    let camera = require_key_camera(&map, "camera", "scene", pos);
    let background = maybe_key_point(&map, "background", "scene", pos)
        .unwrap_or([0.0, 0.0, 0.0]);

    let lights_v = require_key(&map, "lights", "scene", pos);
    let lights_items = require_vec(lights_v, "scene :lights", pos);
    let lights: Vec<Light> = lights_items
        .iter()
        .map(|v| require_light_value(v, "scene :lights element", pos))
        .collect();

    let objects_v = require_key(&map, "objects", "scene", pos);
    let objects_items = require_vec(objects_v, "scene :objects", pos);
    let objects: Vec<Shape> = objects_items
        .iter()
        .map(|v| require_shape_value(v, "scene :objects element", pos))
        .collect();

    // Defaulted to match scenes.rs constants. Scripts that care can
    // override.
    let reflect_limit = maybe_key_int(&map, "reflect-limit", "scene", pos)
        .unwrap_or(2) as u32;
    let oversample = maybe_key_int(&map, "oversample", "scene", pos)
        .unwrap_or(2) as u32;

    Value::Scene(Rc::new(Scene {
        name,
        camera,
        lights,
        objects,
        background,
        reflect_limit,
        oversample,
    }))
}

// ---------------------------------------------------------------------------
// Type predicates
// ---------------------------------------------------------------------------

fn unary_predicate(args: &[Value], name: &str, pos: &Position, pred: fn(&Value) -> bool) -> Value {
    require_arity(args, 1, name, pos);
    Value::Bool(pred(&args[0]))
}

fn builtin_surface_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "surface?", pos, |v| matches!(v, Value::Surface(_)))
}
fn builtin_camera_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "camera?", pos, |v| matches!(v, Value::Camera(_)))
}
fn builtin_affine_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "affine?", pos, |v| matches!(v, Value::Affine(_)))
}
fn builtin_aabb_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "aabb?", pos, |v| matches!(v, Value::Aabb(_)))
}
fn builtin_light_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "light?", pos, |v| matches!(v, Value::Light(_)))
}
fn builtin_shape_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "shape?", pos, |v| matches!(v, Value::Shape(_)))
}
fn builtin_scene_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "scene?", pos, |v| matches!(v, Value::Scene(_)))
}
