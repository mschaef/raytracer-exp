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
use crate::render::geometry::{Point, lenp, normalizep, EPSILON};
use crate::render::mesh::load_obj;
use crate::render::render;
use crate::render::shapes::{
    bounded, bounded_with, bvh, difference, group, intersection, merge, rotate_axis,
    rotate_x, rotate_y, rotate_z, scale, surfaced, transform, translate, AABB,
    Cone, Cuboid, Cylinder, Plane, Shape, Sphere, Torus, Triangle,
};
use crate::render::transform::Affine;
use crate::render::noise::Octaves;
use crate::render::pigment::{Pattern, Pigment, Wave};
use crate::render::{Camera, HeatmapTargets, Light, LightKind, Scene, SpotCone, Surface, ViewMode};

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
    define_native(env, "light-spot", builtin_light_spot);
    define_native(env, "light-area", builtin_light_area);
    define_native(env, "light", builtin_light);

    // The renderer's self-intersection tolerance (`render::geometry::
    // EPSILON`): hits closer than this to a ray's origin are ignored.
    // Scenes that offset nearly coincident surfaces (CSG faces a hair
    // apart) should offset by at least this much, and should use this
    // binding rather than a hard-coded copy.
    env.borrow_mut().define("epsilon", Value::Float(EPSILON));

    // Cameras.
    define_native(env, "camera-looking-at", builtin_camera_looking_at);
    define_native(env, "camera-with-fov", builtin_camera_with_fov);
    define_native(env, "camera-dof", builtin_camera_dof);

    // Leaf shapes.
    define_native(env, "sphere", builtin_sphere);
    define_native(env, "plane", builtin_plane);
    define_native(env, "cuboid", builtin_cuboid);
    define_native(env, "triangle", builtin_triangle);
    define_native(env, "cylinder", builtin_cylinder);
    define_native(env, "cone", builtin_cone);
    define_native(env, "torus", builtin_torus);

    // Mesh loading (Phase 7).
    define_native(env, "load-obj", builtin_load_obj);

    // Composite / transformed shapes.
    define_native(env, "group", builtin_group);
    define_native(env, "bvh", builtin_bvh);
    define_native(env, "transform", builtin_transform);
    define_native(env, "translate", builtin_translate);
    define_native(env, "scale", builtin_scale);
    define_native(env, "rotate-x", builtin_rotate_x);
    define_native(env, "rotate-y", builtin_rotate_y);
    define_native(env, "rotate-z", builtin_rotate_z);
    define_native(env, "rotate-axis", builtin_rotate_axis);
    define_native(env, "bounded", builtin_bounded);
    define_native(env, "bounded-with", builtin_bounded_with);
    define_native(env, "with-surface", builtin_with_surface);

    // Constructive solid geometry.
    define_native(env, "difference", builtin_difference);
    define_native(env, "intersection", builtin_intersection);
    define_native(env, "merge", builtin_merge);

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
    define_native(env, "affine-apply", builtin_affine_apply);
    define_native(env, "affine-apply-vector", builtin_affine_apply_vector);

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

/// Optional surface key: `Some(surface)` if `:surface` is present and
/// is a `Value::Surface`, `None` if the key is absent. A present-but-
/// wrong-type value is still rejected with an explicit error (it's
/// almost always a bug, not an intent to fall back to the wrapper's
/// default). Phase 2 of the surface-decoupling work uses this on
/// every leaf primitive so scripts can leave the surface unspecified
/// and have an enclosing `(with-surface ...)` supply one.
fn maybe_key_surface(
    map: &HashMap<String, Value>,
    key: &str,
    ctx: &str,
    pos: &Position,
) -> Option<Surface> {
    match map.get(key) {
        None => None,
        Some(Value::Surface(s)) => Some(*s),
        Some(other) => sdl_panic!(
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

/// Pull a `Surface` out of a positional argument (counterpart to the
/// map-keyed `maybe_key_surface`). Used by `(with-surface ...)` and
/// the positional `(load-obj path surface)` overload.
fn require_surface(v: &Value, ctx: &str, pos: &Position) -> Surface {
    match v {
        Value::Surface(s) => *s,
        other => sdl_panic!(
            pos.clone(),
            "{} expected a surface, got {} ({})",
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
///            :reflection n :transparency n :metallic b})`
///
/// All keys except `:color` have defaults. The defaults match an
/// uninteresting matte surface so that omitting a key gives a
/// predictable result (no specular highlight, full diffuse, no
/// reflection, no checkering, fully opaque, non-metallic).
///
/// `:transparency` is the Phase 1 transmission coefficient in
/// `[0.0, 1.0]` — `0.0` (the default) is fully opaque, `1.0` is
/// fully see-through. Omitting it reproduces every pre-transparency
/// scene exactly.
///
/// `:metallic` (default `false`) flags a metal surface. When `true`,
/// the renderer tints the mirror reflection and specular highlight by
/// the surface `color` and suppresses the diffuse term; the surface
/// is also forced opaque (`:transparency` is ignored). Omitting it
/// reproduces every pre-metallic scene exactly.
fn builtin_surface(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "surface", pos);
    let map = require_map(&args[0], "surface", pos);

    let pigment = map.get("pigment").map(|v| build_pigment(v, pos));
    // With a pigment, :color is optional: the pigment gives the colour.
    let color: LinearColor = if pigment.is_some() {
        maybe_key_point(&map, "color", "surface", pos).unwrap_or([0.5, 0.5, 0.5])
    } else {
        require_key_point(&map, "color", "surface", pos)
    };
    let ambient = maybe_key_number(&map, "ambient", "surface", pos).unwrap_or(0.0);
    let specular = maybe_key_number(&map, "specular", "surface", pos).unwrap_or(0.0);
    let light = maybe_key_number(&map, "light", "surface", pos).unwrap_or(1.0);
    let checked = maybe_key_bool(&map, "checked", "surface", pos).unwrap_or(false);
    let reflection = maybe_key_number(&map, "reflection", "surface", pos).unwrap_or(0.0);
    let transparency = maybe_key_number(&map, "transparency", "surface", pos).unwrap_or(0.0);
    let metallic = maybe_key_bool(&map, "metallic", "surface", pos).unwrap_or(false);

    Value::Surface(Surface {
        color,
        ambient,
        specular,
        light,
        checked,
        reflection,
        transparency,
        metallic,
        pigment,
    })
}

/// Build a pigment from its SDL map (the `:pigment` key of `surface`):
///
/// - `:pattern` — `:wood` (concentric rings around the z axis) or
///   `:checker` (unit cubes).
/// - `:color-map` — `[[value [r g b]] ...]`, ascending values in
///   `[0, 1]`; repeat a value for a hard edge. For a checker,
///   `:colors [a b]` is the shorthand POV uses.
/// - `:turbulence` (default 0; a number, or `[x y z]` per axis), with `:octaves` (6), `:omega` (0.5) and
///   `:lambda` (2.0), as in POV-Ray.
/// - `:wave` — `:triangle` (the default, and POV's for wood), `:ramp`
///   or `:sine`. Ignored by the checker.
/// - `:transform` — an affine applied to the pattern, like POV's
///   transforms inside a `pigment { }` (e.g. `(affine-scale [0.05 0.05
///   0.05])` for rings 20 times finer).
///
/// The pigment is leaked to get the `'static` reference `Surface`
/// holds (see `Surface::pigment`).
fn build_pigment(v: &Value, pos: &Position) -> &'static Pigment {
    let map = require_map(v, "surface :pigment", pos);
    const KEYS: [&str; 9] = [
        "pattern", "color-map", "colors", "turbulence", "octaves", "omega", "lambda", "wave", "transform",
    ];
    for k in map.keys() {
        if !KEYS.contains(&k.as_str()) {
            sdl_panic!(pos, "pigment: unknown key :{} (expected one of :{})", k, KEYS.join(" :"));
        }
    }
    let keyword = |key: &str| -> Option<String> {
        map.get(key).map(|v| match v {
            Value::Keyword(k) => (**k).clone(),
            other => sdl_panic!(pos, "pigment :{} must be a keyword (got {})", key, other),
        })
    };

    let pattern = match keyword("pattern").as_deref() {
        Some("wood") => Pattern::Wood,
        Some("checker") => Pattern::Checker,
        Some(other) => sdl_panic!(pos, "pigment: unknown :pattern :{} (expected :wood or :checker)", other),
        None => sdl_panic!(pos, "pigment: missing :pattern"),
    };
    let wave = match keyword("wave").as_deref() {
        None | Some("triangle") => Wave::Triangle,
        Some("ramp") => Wave::Ramp,
        Some("sine") => Wave::Sine,
        Some(other) => sdl_panic!(pos, "pigment: unknown :wave :{} (expected :triangle, :ramp or :sine)", other),
    };

    let color_map: Vec<(f64, LinearColor)> = match (map.get("color-map"), map.get("colors")) {
        (Some(_), Some(_)) => sdl_panic!(pos, "pigment: give :color-map or :colors, not both"),
        (Some(cm), None) => {
            let entries = require_vec(cm, "pigment :color-map", pos);
            if entries.is_empty() {
                sdl_panic!(pos, "pigment: :color-map is empty");
            }
            let mut out = Vec::with_capacity(entries.len());
            for e in entries.iter() {
                let pair = require_vec(e, "pigment :color-map entry", pos);
                if pair.len() != 2 {
                    sdl_panic!(pos, "pigment: each :color-map entry is [value [r g b]] (got {})", e);
                }
                let value = require_number(&pair[0], "pigment :color-map value", pos);
                let color = require_point(&pair[1], "pigment :color-map colour", pos);
                if let Some((last, _)) = out.last() {
                    if value < *last {
                        sdl_panic!(pos, "pigment: :color-map values must ascend ({} after {})", value, last);
                    }
                }
                out.push((value, color));
            }
            out
        }
        (None, Some(cs)) => {
            let colors = require_vec(cs, "pigment :colors", pos);
            if colors.len() != 2 {
                sdl_panic!(pos, "pigment: :colors takes two colours (got {})", colors.len());
            }
            vec![
                (0.0, require_point(&colors[0], "pigment :colors", pos)),
                (1.0, require_point(&colors[1], "pigment :colors", pos)),
            ]
        }
        (None, None) => sdl_panic!(pos, "pigment: needs :color-map (or :colors for a checker)"),
    };

    let defaults = Octaves::default();
    let octaves = Octaves {
        octaves: match map.get("octaves") {
            Some(v) => {
                let n = require_int(v, "pigment :octaves", pos);
                if !(1..=10).contains(&n) {
                    sdl_panic!(pos, "pigment: :octaves must be between 1 and 10 (got {})", n);
                }
                n as u32
            }
            None => defaults.octaves,
        },
        omega: maybe_key_number(&map, "omega", "pigment", pos).unwrap_or(defaults.omega),
        lambda: maybe_key_number(&map, "lambda", "pigment", pos).unwrap_or(defaults.lambda),
    };
    let transform = map
        .get("transform")
        .map(|v| require_affine(v, "pigment :transform", pos))
        .unwrap_or_else(Affine::identity);
    // A number, or a vector for per-axis amounts (POV's
    // `turbulence <0.05, 0.08, 1000>`).
    let turbulence = match map.get("turbulence") {
        None => [0.0; 3],
        Some(v @ Value::Vec(_)) => require_point(v, "pigment :turbulence", pos),
        Some(v) => [require_number(v, "pigment :turbulence", pos); 3],
    };

    Box::leak(Box::new(Pigment {
        pattern,
        turbulence,
        octaves,
        wave,
        color_map,
        from_texture: transform.inverse(),
    }))
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

/// `(light-spot [x y z] [dx dy dz] [r g b] intensity inner-angle outer-angle)`.
///
/// `direction` is normalized at the binding boundary so callers can
/// supply any non-zero vector; the renderer assumes a unit-length
/// direction in the cone-falloff math. `inner-angle` and
/// `outer-angle` are half-angles in radians measured from the axis
/// (`(/ pi 6)` ≈ 30° is a typical narrow spotlight; `(/ pi 4)` ≈ 45°
/// is wide). The constraint is `inner-angle ≤ outer-angle`; reversing
/// them collapses the smoothstep band and is rejected here rather
/// than letting the renderer silently produce a hard-edged cone.
fn builtin_light_spot(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 6, "light-spot", pos);
    let location = require_point(&args[0], "light-spot location", pos);
    let direction = require_point(&args[1], "light-spot direction", pos);
    let color = require_point(&args[2], "light-spot color", pos);
    let intensity = require_number(&args[3], "light-spot intensity", pos);
    let inner_angle = require_number(&args[4], "light-spot inner-angle", pos);
    let outer_angle = require_number(&args[5], "light-spot outer-angle", pos);

    if lenp(direction) < EPSILON {
        sdl_panic!(
            pos,
            "light-spot: direction must be a non-zero vector (got [{} {} {}])",
            direction[0],
            direction[1],
            direction[2],
        );
    }
    if inner_angle > outer_angle {
        sdl_panic!(
            pos,
            "light-spot: inner-angle ({}) must be ≤ outer-angle ({})",
            inner_angle,
            outer_angle,
        );
    }

    let dir_unit = normalizep(direction);
    Value::Light(Rc::new(Light::spot(
        location,
        dir_unit,
        color,
        intensity,
        inner_angle,
        outer_angle,
    )))
}

/// `(light-area [x y z] [ax ay az] radius [r g b] intensity)`.
///
/// Disk area light centered at `location` with normal `axis` and the
/// given `radius`. `axis` is normalized at the binding boundary so
/// callers can supply any non-zero vector; the renderer assumes a
/// unit-length axis in the cosine attenuation. `radius` must be
/// strictly positive — Phase 4 doesn't actually consult `radius`
/// (the shadow ray samples the disk center only), but Phase 5's
/// disk sampler does and a non-positive radius is meaningless to it,
/// so we reject at construction rather than wait for Phase 5.
fn builtin_light_area(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 5, "light-area", pos);
    let location = require_point(&args[0], "light-area location", pos);
    let axis = require_point(&args[1], "light-area axis", pos);
    let radius = require_number(&args[2], "light-area radius", pos);
    let color = require_point(&args[3], "light-area color", pos);
    let intensity = require_number(&args[4], "light-area intensity", pos);

    if lenp(axis) < EPSILON {
        sdl_panic!(
            pos,
            "light-area: axis must be a non-zero vector (got [{} {} {}])",
            axis[0],
            axis[1],
            axis[2],
        );
    }
    if radius < EPSILON {
        sdl_panic!(
            pos,
            "light-area: radius must be positive (got {})",
            radius,
        );
    }

    let axis_unit = normalizep(axis);
    Value::Light(Rc::new(Light::area(
        location,
        axis_unit,
        radius,
        color,
        intensity,
    )))
}

/// `(light {:location [..] ...})` — the general light constructor,
/// map-keyed, covering every combination the positional constructors
/// don't. Keys:
///
/// - `:location` (required) — where the light is (the centre, for an
///   area light).
/// - `:color` (default white), `:intensity` (default 1).
/// - `:shadowless` (default false) — cast no shadows (a fill light).
/// - A spot cone: `:direction` (the way it shines) or `:point-at` (a
///   point to aim at), with `:inner-angle` and `:outer-angle` in
///   radians. Full strength inside the inner angle, none beyond the
///   outer.
/// - An area, for soft shadows, either a disk — `:radius`, with
///   `:axis` its normal (defaults to the spot direction) — or a
///   parallelogram — `:area-u` and `:area-v`, its two edge vectors,
///   as in POV-Ray's `area_light <u>, <v>, ...`. A disk emits from its
///   front face with a cosine falloff; a parallelogram emits equally in
///   all directions, as POV's area lights do.
///
/// So `(light {:location L})` is a point light, adding a cone makes a
/// spotlight, adding an area makes an area light, and both together
/// make an area light that is also a spotlight. Direction and axis
/// vectors are normalized; unknown keys are rejected so typos don't
/// pass silently.
fn builtin_light(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "light", pos);
    let map = require_map(&args[0], "light", pos);
    const KEYS: [&str; 12] = [
        "location", "color", "intensity", "shadowless", "direction", "point-at",
        "inner-angle", "outer-angle", "radius", "axis", "area-u", "area-v",
    ];
    for k in map.keys() {
        if !KEYS.contains(&k.as_str()) {
            sdl_panic!(pos, "light: unknown key :{} (expected one of :{})", k, KEYS.join(" :"));
        }
    }

    let location = require_key_point(&map, "location", "light", pos);
    let color = maybe_key_point(&map, "color", "light", pos).unwrap_or([1.0, 1.0, 1.0]);
    let intensity = maybe_key_number(&map, "intensity", "light", pos).unwrap_or(1.0);
    let shadowless = maybe_key_bool(&map, "shadowless", "light", pos).unwrap_or(false);

    let unit = |v: Point, what: &str| -> Point {
        if lenp(v) < EPSILON {
            sdl_panic!(pos, "light: {} must be a non-zero vector (got {:?})", what, v);
        }
        normalizep(v)
    };

    // The spot cone.
    let direction = match (
        maybe_key_point(&map, "direction", "light", pos),
        maybe_key_point(&map, "point-at", "light", pos),
    ) {
        (Some(_), Some(_)) => sdl_panic!(pos, "light: give :direction or :point-at, not both"),
        (Some(d), None) => Some(unit(d, ":direction")),
        (None, Some(target)) => {
            let d = [target[0] - location[0], target[1] - location[1], target[2] - location[2]];
            Some(unit(d, ":point-at minus :location"))
        }
        (None, None) => None,
    };
    let inner = maybe_key_number(&map, "inner-angle", "light", pos);
    let outer = maybe_key_number(&map, "outer-angle", "light", pos);
    let cone = match (direction, inner, outer) {
        (None, None, None) => None,
        (Some(direction), Some(inner_angle), Some(outer_angle)) => {
            if inner_angle > outer_angle {
                sdl_panic!(
                    pos,
                    "light: :inner-angle ({}) must be ≤ :outer-angle ({})",
                    inner_angle,
                    outer_angle
                );
            }
            Some(SpotCone { direction, inner_angle, outer_angle })
        }
        (None, _, _) => sdl_panic!(pos, "light: :inner-angle / :outer-angle need :direction or :point-at"),
        (Some(_), _, _) => sdl_panic!(pos, "light: a spot cone needs both :inner-angle and :outer-angle"),
    };

    // The area.
    let radius = maybe_key_number(&map, "radius", "light", pos);
    let axis = maybe_key_point(&map, "axis", "light", pos);
    let area_u = maybe_key_point(&map, "area-u", "light", pos);
    let area_v = maybe_key_point(&map, "area-v", "light", pos);
    if axis.is_some() && radius.is_none() {
        sdl_panic!(pos, "light: :axis only applies to a disk area light (give :radius)");
    }
    let kind = match (radius, area_u, area_v) {
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
            sdl_panic!(pos, "light: give a disk (:radius) or a parallelogram (:area-u :area-v), not both")
        }
        (Some(r), None, None) => {
            if r < EPSILON {
                sdl_panic!(pos, "light: :radius must be positive (got {})", r);
            }
            let axis = match (axis, cone) {
                (Some(a), _) => unit(a, ":axis"),
                (None, Some(c)) => c.direction,
                (None, None) => sdl_panic!(pos, "light: a disk area light needs :axis (or a spot :direction)"),
            };
            LightKind::Area { axis, radius: r, cone }
        }
        (None, Some(u), Some(v)) => {
            let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            if lenp(n) < EPSILON {
                sdl_panic!(pos, "light: :area-u and :area-v must be non-zero and not parallel");
            }
            LightKind::Quad { u, v, cone }
        }
        (None, Some(_), None) | (None, None, Some(_)) => {
            sdl_panic!(pos, "light: a parallelogram area light needs both :area-u and :area-v")
        }
        (None, None, None) => match cone {
            Some(c) => LightKind::Spot {
                direction: c.direction,
                inner_angle: c.inner_angle,
                outer_angle: c.outer_angle,
            },
            None => LightKind::Point,
        },
    };

    Value::Light(Rc::new(Light { location, color, intensity, kind, shadowless }))
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

/// `(camera-dof location look-at up-hint zoom aperture-radius)` —
/// depth-of-field camera. Same framing as `camera-looking-at`, plus a
/// thin-lens aperture of radius `aperture-radius` (world units). The
/// focus distance is the location→look-at distance, so the look-at
/// point is sharp and nearer/farther geometry blurs. An
/// `aperture-radius` of 0 is exactly `camera-looking-at`.
///
/// Positional rather than map-keyed: it's the same four arguments as
/// `camera-looking-at` with one more on the end, and a separate
/// constructor keeps `camera-looking-at` callers untouched. Focusing
/// at a depth other than the look-at point is a Phase 2 ergonomics
/// item, not exposed here yet.
fn builtin_camera_dof(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 5, "camera-dof", pos);
    let location = require_point(&args[0], "camera-dof location", pos);
    let look_at = require_point(&args[1], "camera-dof look-at", pos);
    let up_hint = require_point(&args[2], "camera-dof up-hint", pos);
    let zoom = require_number(&args[3], "camera-dof zoom", pos);
    let aperture_radius = require_number(&args[4], "camera-dof aperture-radius", pos);
    Value::Camera(Camera::with_dof(location, look_at, up_hint, zoom, aperture_radius))
}

// ---------------------------------------------------------------------------
// Leaf shapes
// ---------------------------------------------------------------------------

fn builtin_sphere(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "sphere", pos);
    let map = require_map(&args[0], "sphere", pos);
    let center = require_key_point(&map, "center", "sphere", pos);
    let r = require_key_number(&map, "r", "sphere", pos);
    // `:surface` is optional. A leaf without an explicit surface
    // inherits one from an enclosing `(with-surface ...)`; if no
    // wrapper supplies one, `Shape::validate_surfaces` rejects the
    // scene at build time. See the Phase 2 surface-decoupling notes
    // on `Shape::Surfaced` in `src/render/shapes.rs`.
    let surface = maybe_key_surface(&map, "surface", "sphere", pos);
    Value::Shape(Rc::new(Shape::Sphere(Sphere { center, r, surface })))
}

fn builtin_plane(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "plane", pos);
    let map = require_map(&args[0], "plane", pos);
    let normal = require_key_point(&map, "normal", "plane", pos);
    let p0 = require_key_point(&map, "p0", "plane", pos);
    let surface = maybe_key_surface(&map, "surface", "plane", pos);
    Value::Shape(Rc::new(Shape::Plane(Plane { normal, p0, surface })))
}

fn builtin_cuboid(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "cuboid", pos);
    let map = require_map(&args[0], "cuboid", pos);
    let center = require_key_point(&map, "center", "cuboid", pos);
    let size = require_key_point(&map, "size", "cuboid", pos);
    let surface = maybe_key_surface(&map, "surface", "cuboid", pos);
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

    let surface = maybe_key_surface(&map, "surface", "triangle", pos);

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
    let surface = maybe_key_surface(&map, "surface", "cylinder", pos);
    Value::Shape(Rc::new(Shape::Cylinder(Cylinder { p0, p1, r, surface })))
}

/// `(cone {:p0 [..] :p1 [..] :r n :surface S})` — a closed solid cone.
/// `:p0` is the base center (radius `:r`), `:p1` is the apex point. The
/// two ends are not interchangeable; see `render::shapes::Cone`.
fn builtin_cone(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "cone", pos);
    let map = require_map(&args[0], "cone", pos);
    let p0 = require_key_point(&map, "p0", "cone", pos);
    let p1 = require_key_point(&map, "p1", "cone", pos);
    let r = require_key_number(&map, "r", "cone", pos);
    let surface = maybe_key_surface(&map, "surface", "cone", pos);
    Value::Shape(Rc::new(Shape::Cone(Cone { p0, p1, r, surface })))
}

/// `(torus {:major R :minor r :center [..] :axis [..] :surface S})` — a
/// solid torus: the points within `:minor` of a circle of radius
/// `:major` around `:center`, in the plane perpendicular to `:axis`.
/// `:center` defaults to the origin and `:axis` to +y, which together
/// are POV-Ray's `torus { R, r }`. `:axis` is normalized here. Requires
/// `0 < minor < major`.
fn builtin_torus(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "torus", pos);
    let map = require_map(&args[0], "torus", pos);
    let major = require_key_number(&map, "major", "torus", pos);
    let minor = require_key_number(&map, "minor", "torus", pos);
    let center = maybe_key_point(&map, "center", "torus", pos).unwrap_or([0.0, 0.0, 0.0]);
    let axis = maybe_key_point(&map, "axis", "torus", pos).unwrap_or([0.0, 1.0, 0.0]);
    let surface = maybe_key_surface(&map, "surface", "torus", pos);
    if lenp(axis) < EPSILON {
        sdl_panic!(pos.clone(), "torus :axis must be non-zero (got {:?})", axis);
    }
    if !(minor > 0.0 && minor < major) {
        sdl_panic!(
            pos.clone(),
            "torus needs 0 < :minor < :major (got :major {} :minor {})",
            major,
            minor
        );
    }
    Value::Shape(Rc::new(Shape::Torus(Torus {
        center,
        axis: normalizep(axis),
        major,
        minor,
        surface,
    })))
}

/// `(load-obj <path-string>)` or `(load-obj <path-string> <surface>)`
/// — load a Wavefront OBJ file from disk and return it as a
/// `Shape::Group` of triangles.
///
/// With a `<surface>` argument, every triangle is stamped with that
/// surface — equivalent to the pre-Phase-2 behavior. Without it,
/// triangles carry no surface and the result must be wrapped in
/// `(with-surface S ...)` (or have some other `Shape::Surfaced`
/// ancestor) before going into a scene. The "wrap the load with a
/// surface" idiom is the Phase-2 default — it makes retexturing a
/// mesh a one-line change rather than re-passing the surface
/// through the loader.
///
/// Path resolution mirrors the `(load ...)` special form: relative
/// paths join the loading file's directory via `CURRENT_DIR`, with
/// absolute paths used as-is. This matters because scenes/ files want
/// to reference `../models/foo.obj` symbolically rather than
/// depending on what CWD the renderer was invoked from. (The Rust
/// `mesh::load_obj` itself takes `impl AsRef<Path>` and does no
/// resolution; we do all the resolution at the binding boundary.)
///
/// Positional rather than map-keyed because the argument order
/// (where, what surface) is obvious. Returns `Value::Shape` wrapping
/// the `Shape::Group` so the result composes with the rest of the
/// shape constructors — `(with-surface gold (bounded (translate
/// ... (load-obj "../models/foo.obj"))))` is the typical Phase-2
/// idiom.
fn builtin_load_obj(args: &[Value], pos: &Position) -> Value {
    // Variadic arity (1 or 2): inline check since `require_arity` is
    // fixed-arity. The 1-arg form produces an unsurfaced mesh that
    // must be wrapped in `(with-surface ...)`; the 2-arg form keeps
    // the pre-Phase-2 calling convention working without change.
    if args.len() != 1 && args.len() != 2 {
        sdl_panic!(
            pos.clone(),
            "load-obj takes 1 or 2 arguments (got {})",
            args.len()
        );
    }
    let path_str = require_string(&args[0], "load-obj path", pos);
    let surface: Option<Surface> = if args.len() == 2 {
        Some(require_surface(&args[1], "load-obj surface", pos))
    } else {
        None
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

/// `(bvh [shape1 shape2 ...])` — like `group`, but organizes the shapes
/// into a bounding-volume hierarchy so a ray only tests the few whose
/// boxes it passes through. Use it for large collections (a mesh's worth
/// of spheres, a forest of ornaments). Nested groups are flattened into
/// the tree; shapes without finite bounds (planes) are kept alongside it.
fn builtin_bvh(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 1, "bvh", pos);
    let items = require_vec(&args[0], "bvh", pos);
    let children: Vec<Shape> = items
        .iter()
        .map(|v| require_shape_value(v, "bvh child", pos))
        .collect();
    Value::Shape(Rc::new(bvh(children)))
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

/// `(with-surface surface shape)` — decorate a shape subtree with a
/// default `surface`. Every leaf in `shape` that was constructed
/// without an explicit `:surface` inherits this one; leaves that
/// carry their own surface keep it ("innermost wins"). Nested
/// `(with-surface inner ... (with-surface outer ...))` lets `outer`
/// fill in unsurfaced leaves below it that the `inner` wrapper
/// didn't already supply.
///
/// Single-child by design — for multiple shapes, wrap them with
/// `(group [...])` first. The extra paren is cheap and keeps the
/// "what's inheriting from what" structure explicit at the call
/// site. Lights placed inside `(with-surface ...)` are unaffected
/// — they don't hit-test and `Shape::Surfaced::hit_test` is the
/// only place the wrapper's surface is consulted.
fn builtin_with_surface(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "with-surface", pos);
    let surface = require_surface(&args[0], "with-surface surface", pos);
    let child = require_shape_value(&args[1], "with-surface child", pos);
    Value::Shape(Rc::new(surfaced(surface, child)))
}

// ---------------------------------------------------------------------------
// Constructive solid geometry
// ---------------------------------------------------------------------------

/// Extract the CSG operands from `args`: at least two, each a solid
/// shape. Triangles and `load-obj` meshes have no inside, so they're
/// rejected here with a positioned error rather than reaching the host
/// constructors' assert.
fn require_csg_operands(args: &[Value], name: &str, pos: &Position) -> Vec<Shape> {
    if args.len() < 2 {
        sdl_panic!(
            pos.clone(),
            "{} takes at least 2 arguments (got {})",
            name,
            args.len()
        );
    }
    args.iter()
        .enumerate()
        .map(|(i, v)| {
            let shape = require_shape_value(v, &format!("{} operand", name), pos);
            if !shape.is_solid() {
                sdl_panic!(
                    pos.clone(),
                    "{} operand {} ({}) is not a solid: triangles and meshes have no inside",
                    name,
                    i + 1,
                    v
                );
            }
            shape
        })
        .collect()
}

/// `(difference a b c ...)` — the points inside `a` and not inside any
/// of `b`, `c`, .... POV-Ray's n-ary form: the operands after the first
/// are combined into one `group` (a union) and subtracted in a single
/// operation. Faces cut by an operand show that operand's surface if it
/// has one, otherwise an enclosing `with-surface`'s.
fn builtin_difference(args: &[Value], pos: &Position) -> Value {
    let mut operands = require_csg_operands(args, "difference", pos);
    let rest = operands.split_off(1);
    let a = operands.pop().unwrap();
    let b = if rest.len() == 1 {
        rest.into_iter().next().unwrap()
    } else {
        group(rest)
    };
    Value::Shape(Rc::new(difference(a, b)))
}

/// `(intersection a b c ...)` — the points inside every operand.
/// Folds left: `(intersection (intersection a b) c)`.
fn builtin_intersection(args: &[Value], pos: &Position) -> Value {
    let operands = require_csg_operands(args, "intersection", pos);
    let mut iter = operands.into_iter();
    let first = iter.next().unwrap();
    let result = iter.fold(first, intersection);
    Value::Shape(Rc::new(result))
}

/// `(merge a b c ...)` — the points inside any operand, as one solid
/// with no internal faces. Unlike `group`, where every operand keeps
/// its whole surface, the parts of each operand's surface that lie
/// inside another operand are removed, which matters for transparent
/// solids (POV-Ray's `merge`). Built like `difference`: the operands
/// after the first are grouped and merged with it in one step.
fn builtin_merge(args: &[Value], pos: &Position) -> Value {
    let mut operands = require_csg_operands(args, "merge", pos);
    let rest = operands.split_off(1);
    let a = operands.pop().unwrap();
    let b = if rest.len() == 1 {
        rest.into_iter().next().unwrap()
    } else {
        group(rest)
    };
    Value::Shape(Rc::new(merge(a, b)))
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

fn point_value(p: Point) -> Value {
    Value::Vec(Rc::new(vec![Value::Float(p[0]), Value::Float(p[1]), Value::Float(p[2])]))
}

/// `(affine-apply a p)` — the point `p` transformed by `a`
/// (translation included). Lets a script compute where something ends
/// up, e.g. placing thousands of beads as plain spheres at computed
/// centres instead of wrapping each one in transform nodes.
fn builtin_affine_apply(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "affine-apply", pos);
    let a = require_affine(&args[0], "affine-apply", pos);
    let p = require_point(&args[1], "affine-apply point", pos);
    point_value(a.transform_point(p))
}

/// `(affine-apply-vector a v)` — the direction `v` transformed by the
/// linear part of `a` only (no translation). Not renormalized.
fn builtin_affine_apply_vector(args: &[Value], pos: &Position) -> Value {
    require_arity(args, 2, "affine-apply-vector", pos);
    let a = require_affine(&args[0], "affine-apply-vector", pos);
    let v = require_point(&args[1], "affine-apply-vector vector", pos);
    point_value(a.transform_vector(v))
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
///          :reflect-limit n :transmit-limit n :indirect-limit n
///          :min-samples n :max-samples m :variance-threshold t})`
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

    // Indirect-bounce (path-tracing) recursion cap. Default 0 means
    // "feature off" — no indirect rays are fired and the renderer's
    // output is byte-identical to the pre-GI renderer. A positive
    // value enables path-traced indirect lighting; Phase 1 of the
    // path-tracing plan only threads the value through (no behavior
    // change), with Phase 2 wiring the indirect branch in
    // `shade_pixel`. Scenes with no diffuse interreflection
    // concerns never need to set this.
    let indirect_limit = maybe_key_int(&map, "indirect-limit", "scene", pos)
        .unwrap_or(0) as u32;

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

    // Validate that every leaf in the scene graph has a surface,
    // either explicitly or via an enclosing `Shape::Surfaced`
    // ancestor. This is the authoritative construction-time check
    // for the surface-decoupling work; `shade_pixel`'s hot-pink
    // fallback is purely a safety net behind it.
    if let Err(msg) = root.validate_surfaces(false) {
        sdl_panic!(pos.clone(), "scene: {}", msg);
    }

    Value::Scene(Rc::new(Scene {
        name,
        camera,
        root,
        background,
        reflect_limit,
        transmit_limit,
        indirect_limit,
        min_samples,
        max_samples,
        variance_threshold,
        // Render-view diagnostic selector. Not exposed in the SDL
        // — the SDL is the canonical "what does this scene look
        // like?" definition; switching views from inside a script
        // would muddy that. main.rs overrides this from the
        // `RAYTRACER_VIEW` environment variable when set; tests
        // and SDL scripts always get the byte-identical `Full`
        // default.
        view_mode: ViewMode::Full,
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
