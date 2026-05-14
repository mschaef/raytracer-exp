// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Host bindings.
//!
//! Wires the ray tracer's `Surface`, `Light`, `Camera`, `Shape`,
//! `Scene`, and `RenderTarget` types up as native SDL functions, so
//! scripts can build and render scenes without touching Rust.
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
//! Phase 2 added scene-construction bindings; Phase 3 added the
//! render-dispatch bindings (`png-target`, `offset-target`,
//! `progress-target`, `render`, `save-png`) that drive an end-to-end
//! render to disk.
//!
//! Numbers passed to host fields are accepted as either int or float
//! and coerced to f64 — scripts can write `(sphere {:r 1 :center [0 0 0] ...})`
//! without sprinkling `1.0`s through the source.

use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use crate::render::color::LinearColor;
use crate::render::geometry::Point;
use crate::render::mesh::load_obj;
use crate::render::render;
use crate::render::shapes::{
    bounded, bounded_with, group, rotate_axis, rotate_x, rotate_y, rotate_z,
    scale, transform, translate, AABB, Cuboid, Cylinder, Plane, Shape, Sphere,
    Triangle,
};
use crate::render::transform::Affine;
use crate::render::{Camera, HeatmapTargets, Light, Scene, Surface};

use crate::sdl::env::EnvRef;
use crate::sdl::error::Position;
use crate::sdl::target::SdlTarget;
use crate::sdl::value::{Function, FunctionKind, NativeFn, Value};
use crate::sdl_panic;

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

/// Install every host binding into `env`. Called by `default_env`
/// after the language built-ins are installed so a script can use
/// both. Covers the Phase 2 scene-construction surface and the
/// Phase 3 render-dispatch surface.
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

    // Mesh loading (Phase 7).
    define_native(env, "load-obj", builtin_load_obj);

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

    // Render dispatch (Phase 3).
    define_native(env, "png-target", builtin_png_target);
    define_native(env, "offset-target", builtin_offset_target);
    define_native(env, "progress-target", builtin_progress_target);
    define_native(env, "render", builtin_render);
    define_native(env, "save-png", builtin_save_png);

    // Type predicates for the new variants.
    define_native(env, "surface?", builtin_surface_q);
    define_native(env, "camera?", builtin_camera_q);
    define_native(env, "affine?", builtin_affine_q);
    define_native(env, "aabb?", builtin_aabb_q);
    define_native(env, "light?", builtin_light_q);
    define_native(env, "shape?", builtin_shape_q);
    define_native(env, "scene?", builtin_scene_q);
    define_native(env, "target?", builtin_target_q);
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
        // Lights are also acceptable wherever a shape is expected.
        // The `Shape::Light` variant is what makes "lights live in
        // the scene graph" work, and the SDL light constructors
        // return `Value::Light` (a separate variant from
        // `Value::Shape` so `light?` stays distinct from `shape?`).
        // Auto-wrapping here lets the same `(light-white ...)` /
        // `(light-point ...)` value flow through `(translate ...)`,
        // `(group [...])`, and `:objects` without any explicit
        // "convert to shape" step. The light value's `Rc` stays
        // cheap to clone in the script; the deep-clone via
        // `(**l).clone()` is the same boundary crossing as for
        // `Value::Shape`.
        Value::Light(l) => Shape::Light((**l).clone()),
        other => sdl_panic!(
            pos.clone(),
            "{} expected a shape, got {} ({})",
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

/// Pull the (Rc-shared) `SdlTarget` out of a `Value::Target`. Returns
/// the shared `Rc` so callers can re-share without cloning the
/// underlying target.
fn require_target(v: &Value, ctx: &str, pos: &Position) -> Rc<SdlTarget> {
    match v {
        Value::Target(t) => t.clone(),
        other => sdl_panic!(
            pos.clone(),
            "{} expected a target, got {} ({})",
            ctx,
            other,
            other.type_name()
        ),
    }
}

fn require_scene_value(v: &Value, ctx: &str, pos: &Position) -> Rc<Scene> {
    match v {
        Value::Scene(s) => s.clone(),
        other => sdl_panic!(
            pos.clone(),
            "{} expected a scene, got {} ({})",
            ctx,
            other,
            other.type_name()
        ),
    }
}

/// Coerce an int or float that's required to be non-negative into
/// `u32`. Used for image dimensions and offset components.
fn require_u32(v: &Value, ctx: &str, pos: &Position) -> u32 {
    let n = require_int(v, ctx, pos);
    if n < 0 {
        sdl_panic!(pos.clone(), "{} expected a non-negative integer (got {})", ctx, n);
    }
    if n > u32::MAX as i64 {
        sdl_panic!(pos.clone(), "{} integer out of u32 range (got {})", ctx, n);
    }
    n as u32
}

// ---------------------------------------------------------------------------
// Surface
// ---------------------------------------------------------------------------

/// `(surface {:color [r g b] :ambient n :specular n :light n :checked b
///            :reflection n :transparency n})`
///
/// All keys except `:color` have defaults. The defaults match an
/// uninteresting matte surface so that omitting a key gives a
/// predictable result (no specular highlight, full diffuse, no
/// reflection, no checkering, fully opaque).
///
/// `:transparency` is the Phase 1 transmission coefficient in
/// `[0.0, 1.0]` — `0.0` (the default) is fully opaque, `1.0` is
/// fully see-through. Omitting it reproduces every pre-transparency
/// scene exactly.
fn builtin_surface(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "surface", pos);
    let map = require_map(&args[0], "surface", pos);

    let color: LinearColor = require_key_point(&map, "color", "surface", pos);
    let ambient = maybe_key_number(&map, "ambient", "surface", pos).unwrap_or(0.0);
    let specular = maybe_key_number(&map, "specular", "surface", pos).unwrap_or(0.0);
    let light = maybe_key_number(&map, "light", "surface", pos).unwrap_or(1.0);
    let checked = maybe_key_bool(&map, "checked", "surface", pos).unwrap_or(false);
    let reflection = maybe_key_number(&map, "reflection", "surface", pos).unwrap_or(0.0);
    let transparency = maybe_key_number(&map, "transparency", "surface", pos).unwrap_or(0.0);

    Value::Surface(Surface {
        color,
        ambient,
        specular,
        light,
        checked,
        reflection,
        transparency,
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

/// `(load-obj <path-string> <surface>)` — load a Wavefront OBJ file
/// from disk and return it as a `Shape::Group` of triangles, all
/// sharing the supplied surface.
///
/// Path resolution mirrors the `(load ...)` special form: relative
/// paths join the loading file's directory via `CURRENT_DIR`, with
/// absolute paths used as-is. This matters because scenes/ files want
/// to reference `../models/foo.obj` symbolically rather than
/// depending on what CWD the renderer was invoked from. (The Rust
/// `mesh::load_obj` itself takes `impl AsRef<Path>` and does no
/// resolution; we do all the resolution at the binding boundary.)
///
/// Positional rather than map-keyed because the only two arguments
/// (where, what surface) are obvious from order. Returns `Value::Shape`
/// wrapping the `Shape::Group` so the result composes with the rest of
/// the shape constructors — `(bounded (translate ... (load-obj ...)))`
/// is the typical idiom.
fn builtin_load_obj(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "load-obj", pos);
    let path_str = require_string(&args[0], "load-obj path", pos);
    let surface = match &args[1] {
        Value::Surface(s) => *s,
        other => sdl_panic!(
            pos.clone(),
            "load-obj surface expected a surface, got {} ({})",
            other,
            other.type_name()
        ),
    };

    // Same resolution rule as eval_load: absolute paths used as-is,
    // relative paths anchor against CURRENT_DIR (the loading file's
    // directory). When CURRENT_DIR is unset (e.g. inline source), the
    // path falls through unchanged and resolves against the process
    // CWD — the user-facing failure mode there is "file not found",
    // same as the Rust call would have produced.
    let path = Path::new(&path_str);
    let resolved: std::path::PathBuf = if path.is_absolute() {
        path.to_path_buf()
    } else {
        crate::sdl::CURRENT_DIR.with(|c| {
            c.borrow()
                .clone()
                .map(|d| d.join(path))
                .unwrap_or_else(|| path.to_path_buf())
        })
    };

    Value::Shape(Rc::new(load_obj(&resolved, surface)))
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

/// `(scene {:name "..." :camera C :background [r g b] :objects [...]
///          :reflect-limit n :transmit-limit n :min-samples n
///          :max-samples m :variance-threshold t})`
///
/// After the stage-2 collapse the scene is a single top-level
/// `Shape`. `:objects` is exposed at the SDL surface as a list for
/// ergonomics — scripts continue to write a flat vector of geometry
/// and lights — but the constructor wraps it in `Shape::Group(...)`
/// and stores it as `Scene::root`. Lights, geometry, and any
/// composite shape can all appear in `:objects`; bare
/// `(light-white ...)` values are auto-wrapped as `Shape::Light` via
/// `require_shape_value` exactly as in stage 1.
///
/// `:min-samples` / `:max-samples` / `:variance-threshold` configure
/// the adaptive-oversampling loop in `pixel_color` (Phase 2 of the
/// adaptive-oversampling plan). Defaults: `min-samples = 4`,
/// `max-samples = 32`, `variance-threshold = 0.005` (linear color).
/// Setting `min-samples = max-samples` reproduces the previous
/// fixed-count behavior, which byte-pinned tests rely on for
/// determinism.
///
/// Removed keys: `:lights` (stage 2 of the lights-as-shapes
/// migration — move lights into `:objects`) and `:oversample`
/// (Phase 2 of adaptive oversampling — replaced by the three keys
/// above). Scripts still carrying either get an explicit migration
/// error rather than a silent ignore, so a missed migration shows
/// up loudly the first time someone tries to load the scene.
fn builtin_scene(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "scene", pos);
    let map = require_map(&args[0], "scene", pos);

    if map.contains_key("lights") {
        sdl_panic!(
            pos.clone(),
            "scene: the :lights key was removed in stage 2 of the lights-as-shapes \
             migration. Move every light into :objects — bare (light-white ...) / \
             (light-point ...) values are auto-wrapped as shapes by the scene \
             constructor (see lights_in_objects.lisp for the migration pattern)."
        );
    }

    if map.contains_key("oversample") {
        sdl_panic!(
            pos.clone(),
            "scene: the :oversample key was removed in Phase 2 of the \
             adaptive-oversampling plan. Per-pixel sample count is now adaptive — \
             replace :oversample with some combination of :min-samples (default 4), \
             :max-samples (default 32), and :variance-threshold (default 0.005). \
             A drop-in for `:oversample 2` is omitting all three (defaults match) \
             or setting :min-samples 4 :max-samples 4 if you specifically want the \
             old fixed-count behavior."
        );
    }

    let name = require_key_string(&map, "name", "scene", pos);
    let camera = require_key_camera(&map, "camera", "scene", pos);
    let background = maybe_key_point(&map, "background", "scene", pos)
        .unwrap_or([0.0, 0.0, 0.0]);

    let objects_v = require_key(&map, "objects", "scene", pos);
    let objects_items = require_vec(objects_v, "scene :objects", pos);
    let objects: Vec<Shape> = objects_items
        .iter()
        .map(|v| require_shape_value(v, "scene :objects element", pos))
        .collect();

    // Wrap the object list in a Shape::Group as the scene root.
    // `Shape::Group::hit_test` is identical to the previous
    // `nearest_hit(ray, &scene.objects)` traversal: same fold, same
    // children, no overhead. `collect_lights` walks straight through
    // the Group into each child.
    let root = Shape::Group(objects);

    // Defaulted to match the pre-SDL constants. Scripts that care can
    // override.
    let reflect_limit = maybe_key_int(&map, "reflect-limit", "scene", pos)
        .unwrap_or(2) as u32;

    // Transmission recursion cap for transparent surfaces. Default 8 —
    // generous enough for a ray through several stacked transparent
    // surfaces, since transmission depth is naturally larger than
    // reflection depth (see Scene::transmit_limit). Scenes with no
    // transparent surfaces never spawn a transmitted ray, so the
    // value is irrelevant to them.
    let transmit_limit = maybe_key_int(&map, "transmit-limit", "scene", pos)
        .unwrap_or(8) as u32;

    // Adaptive-sampling defaults: 4 samples on flat surfaces (which
    // matches the previous fixed `oversample = 2` cost exactly), up
    // to 32 in noisy regions, with a per-channel min/max spread of
    // 0.005 linear-color units as the early-termination threshold.
    let min_samples = maybe_key_int(&map, "min-samples", "scene", pos)
        .unwrap_or(4) as u32;
    let max_samples = maybe_key_int(&map, "max-samples", "scene", pos)
        .unwrap_or(32) as u32;
    let variance_threshold = maybe_key_number(&map, "variance-threshold", "scene", pos)
        .unwrap_or(0.005);

    Value::Scene(Rc::new(Scene {
        name,
        camera,
        root,
        background,
        reflect_limit,
        transmit_limit,
        min_samples,
        max_samples,
        variance_threshold,
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
fn builtin_target_q(args: &[Value], pos: &Position) -> Value {
    unary_predicate(args, "target?", pos, |v| matches!(v, Value::Target(_)))
}

// ---------------------------------------------------------------------------
// Phase 3 — render dispatch
// ---------------------------------------------------------------------------
//
// Targets are stateful and reference-counted: `(png-target ...)` produces
// a value that can be shared across `(render ...)` calls and `(save-png ...)`
// without ownership juggling. The wrappers `(offset-target ...)` and
// `(progress-target ...)` produce new target values that internally hold
// an Arc to the wrapped target — so wrapping doesn't move the inner
// target out of the script's environment, and the script can still refer
// to the original.
//
// `(render ...)` is positional rather than map-keyed even though it has
// four parameters: `scene`, `target`, `width`, `height`. The order
// reads naturally ("render this scene to that target at WxH") and the
// shape is closed enough not to need optional fields.

/// `(png-target width height)` — a fresh in-memory PNG buffer.
fn builtin_png_target(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "png-target", pos);
    let w = require_u32(&args[0], "png-target width", pos);
    let h = require_u32(&args[1], "png-target height", pos);
    Value::Target(Rc::new(SdlTarget::png(w, h)))
}

/// `(offset-target inner dx dy)` — wrap `inner` so its rows land at
/// `(dx, dy)` instead of `(0, 0)`. Save-ability passes through:
/// wrapping a png-target keeps `save-png` working on the wrapper.
fn builtin_offset_target(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 3, "offset-target", pos);
    let inner = require_target(&args[0], "offset-target inner", pos);
    let dx = require_u32(&args[1], "offset-target dx", pos);
    let dy = require_u32(&args[2], "offset-target dy", pos);
    Value::Target(Rc::new(SdlTarget::offset(&inner, dx, dy)))
}

/// `(progress-target inner total-rows label)` — wrap `inner` with
/// row-completion progress reporting on stderr.
fn builtin_progress_target(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 3, "progress-target", pos);
    let inner = require_target(&args[0], "progress-target inner", pos);
    let total = require_u32(&args[1], "progress-target total-rows", pos);
    let label = require_string(&args[2], "progress-target label", pos);
    Value::Target(Rc::new(SdlTarget::progress(&inner, total, label)))
}

/// `(render scene target width height)` — drive an end-to-end render
/// of `scene` into `target` at `width × height` pixels. Always uses
/// the parallel renderer; the host's `PARALLEL=n` toggle is a
/// `main`-level concern that the SDL doesn't surface yet. The
/// per-pixel heatmap is also off — Phase 6+ will add it back.
///
/// Returns the target so calls can chain via `->`-style threading
/// (Phase 4) or be used inline.
fn builtin_render(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 4, "render", pos);
    let scene = require_scene_value(&args[0], "render scene", pos);
    let target = require_target(&args[1], "render target", pos);
    let width = require_u32(&args[2], "render width", pos);
    let height = require_u32(&args[3], "render height", pos);

    render(&scene, width, height, target.as_render_target(),
           HeatmapTargets::default(), true);

    // Hand the target back so chained pipelines work without holding
    // a separate binding. The Rc is cheap to clone.
    Value::Target(target)
}

/// `(save-png target path)` — save a png-target's accumulated buffer
/// to disk. Panics if `target` was not constructed from (or wrapped
/// around) a png-target. `target` remains usable after save —
/// further rendering or a second save are both fine.
fn builtin_save_png(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "save-png", pos);
    let target = require_target(&args[0], "save-png target", pos);
    let path_str = require_string(&args[1], "save-png path", pos);
    match target.save_png(Path::new(&path_str)) {
        Ok(()) => Value::Nil,
        Err(e) => sdl_panic!(pos.clone(), "save-png failed: {}", e),
    }
}
