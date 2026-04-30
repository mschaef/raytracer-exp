// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Triangle mesh loading.
//!
//! Currently supports Wavefront OBJ via the `tobj` crate. Loading
//! produces a `Shape::Group` of `Shape::Triangle`s, so the loaded mesh
//! integrates with the rest of the scene tree without anything special:
//! it can be wrapped in transforms, nested in other groups, and
//! eventually accelerated by a BVH at the `Shape::Group` level.
//!
//! Loading errors panic with an informative message. For the renderer
//! today, scene definition is part of program startup and a missing
//! model file is a fatal config error rather than something to recover
//! from. When we add a parsed scene definition language this will
//! likely move to `Result`-based handling.

use std::path::Path;

use crate::render::Surface;
use crate::render::geometry::{crossp, normalizep, subp};
use crate::render::shapes::{Shape, Triangle};

/// Load a Wavefront OBJ file from disk and return it as a `Shape` (a
/// `Shape::Group` of triangles).
///
/// Every triangle in the resulting group shares the supplied `surface` —
/// per-face materials in the OBJ are intentionally ignored because the
/// raytracer's surface model isn't a faithful mapping of OBJ/MTL
/// conventions. To position or scale the loaded mesh, wrap the return
/// value in `translate(...)`, `scale(...)`, or `rotate_*(...)`.
///
/// Polygons with more than three vertices are fan-triangulated by tobj.
/// If the OBJ has per-vertex normals, smooth shading falls out of
/// barycentric interpolation in the triangle hit test; otherwise we
/// compute geometric face normals here and replicate them across each
/// triangle's three vertex slots, which gives flat shading.
///
/// Panics on I/O error, parse error, or malformed mesh data.
pub fn load_obj(path: impl AsRef<Path>, surface: Surface) -> Shape {
    let path_ref = path.as_ref();

    let load_options = tobj::LoadOptions {
        // Fan-triangulate any non-triangular faces.
        triangulate: true,
        // Force a single index stream for positions, normals, and
        // texcoords so that index `i` refers to the same vertex across
        // all attribute arrays. Simplifies the loader loop substantially.
        single_index: true,
        // Lines and points have no surface to render.
        ignore_lines: true,
        ignore_points: true,
    };

    let (models, _materials) = tobj::load_obj(path_ref, &load_options)
        .unwrap_or_else(|e| panic!("Failed to load OBJ {:?}: {}", path_ref, e));

    let mut triangles: Vec<Shape> = Vec::new();

    for model in &models {
        let mesh = &model.mesh;
        let positions = &mesh.positions;
        let normals = &mesh.normals;
        let indices = &mesh.indices;

        let has_vertex_normals = !normals.is_empty();

        // With `triangulate: true` and `single_index: true`, indices
        // come in groups of three, each group describing one triangle.
        for tri in indices.chunks_exact(3) {
            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;

            let v0 = read_vec3(positions, i0);
            let v1 = read_vec3(positions, i1);
            let v2 = read_vec3(positions, i2);

            let (n0, n1, n2) = if has_vertex_normals {
                (
                    read_vec3(normals, i0),
                    read_vec3(normals, i1),
                    read_vec3(normals, i2),
                )
            } else {
                // Compute the geometric normal from the triangle's two
                // edges. This gives flat shading: same normal across
                // the entire face.
                let edge1 = subp(v1, v0);
                let edge2 = subp(v2, v0);
                let face_normal = normalizep(crossp(edge1, edge2));
                (face_normal, face_normal, face_normal)
            };

            triangles.push(Shape::Triangle(Triangle {
                vertices: [v0, v1, v2],
                normals: [n0, n1, n2],
                surface,
            }));
        }
    }

    Shape::Group(triangles)
}

/// Pull the i-th 3-vector out of a flat `Vec<f32>` produced by tobj
/// (which stores vertex attributes interleaved as `[x0, y0, z0, x1, ...]`).
/// Promotes to `f64` to match the rest of the renderer's geometry types.
fn read_vec3(flat: &[f32], i: usize) -> [f64; 3] {
    [
        flat[3 * i] as f64,
        flat[3 * i + 1] as f64,
        flat[3 * i + 2] as f64,
    ]
}
