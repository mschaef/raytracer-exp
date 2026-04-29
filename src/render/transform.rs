// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Affine 3D transforms.
//!
//! Represented as a (3x3 linear part, 3-vector translation) pair rather than
//! a 4x4 matrix. This is sufficient for translate/rotate/scale/shear and any
//! composition of them (everything we need for a CSG-style scene graph), and
//! lets us keep the math elementary: composition and inversion both have
//! short closed forms in this representation, so we never have to write a
//! general 4x4 inverse.

use crate::render::geometry::{Point, addp, negp};

/// Row-major 3x3 matrix; `m[i][j]` is row `i`, column `j`. Matrix-vector
/// multiplication treats vectors as columns: `(m * v)[i] = Σ m[i][j] * v[j]`.
pub type Mat3 = [[f64; 3]; 3];

pub const MAT3_IDENTITY: Mat3 = [
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
];

/// An affine transform in 3D: applies the linear part, then translates.
///
/// `transform_point(p) = linear * p + translation`
/// `transform_vector(v) = linear * v`        (vectors don't translate)
#[derive(Copy, Clone)]
pub struct Affine {
    pub linear: Mat3,
    pub translation: Point,
}

pub fn mat3_apply(m: Mat3, v: Point) -> Point {
    [
        m[0][0]*v[0] + m[0][1]*v[1] + m[0][2]*v[2],
        m[1][0]*v[0] + m[1][1]*v[1] + m[1][2]*v[2],
        m[2][0]*v[0] + m[2][1]*v[1] + m[2][2]*v[2],
    ]
}

pub fn mat3_multiply(a: Mat3, b: Mat3) -> Mat3 {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = a[i][0]*b[0][j] + a[i][1]*b[1][j] + a[i][2]*b[2][j];
        }
    }
    r
}

pub fn mat3_transpose(m: Mat3) -> Mat3 {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

/// Closed-form inverse of a 3x3 matrix via the adjugate / determinant.
/// Panics on a singular matrix — for our uses (rotation, scale by nonzero
/// factors, translation, and any composition of those), the linear part
/// is always invertible.
pub fn mat3_inverse(m: Mat3) -> Mat3 {
    let det = m[0][0]*(m[1][1]*m[2][2] - m[1][2]*m[2][1])
            - m[0][1]*(m[1][0]*m[2][2] - m[1][2]*m[2][0])
            + m[0][2]*(m[1][0]*m[2][1] - m[1][1]*m[2][0]);

    if det.abs() < 1e-12 {
        panic!("Cannot invert singular 3x3 matrix (determinant = {})", det);
    }

    let inv_det = 1.0 / det;

    [
        [
            (m[1][1]*m[2][2] - m[1][2]*m[2][1]) * inv_det,
            (m[0][2]*m[2][1] - m[0][1]*m[2][2]) * inv_det,
            (m[0][1]*m[1][2] - m[0][2]*m[1][1]) * inv_det,
        ],
        [
            (m[1][2]*m[2][0] - m[1][0]*m[2][2]) * inv_det,
            (m[0][0]*m[2][2] - m[0][2]*m[2][0]) * inv_det,
            (m[0][2]*m[1][0] - m[0][0]*m[1][2]) * inv_det,
        ],
        [
            (m[1][0]*m[2][1] - m[1][1]*m[2][0]) * inv_det,
            (m[0][1]*m[2][0] - m[0][0]*m[2][1]) * inv_det,
            (m[0][0]*m[1][1] - m[0][1]*m[1][0]) * inv_det,
        ],
    ]
}

impl Affine {
    pub fn identity() -> Self {
        Affine { linear: MAT3_IDENTITY, translation: [0.0, 0.0, 0.0] }
    }

    pub fn translation(d: Point) -> Self {
        Affine { linear: MAT3_IDENTITY, translation: d }
    }

    /// Per-axis scale. `s = [sx, sy, sz]` scales by `sx` along x, etc.
    /// Pass equal components for uniform scale.
    pub fn scale(s: Point) -> Self {
        Affine {
            linear: [
                [s[0], 0.0, 0.0],
                [0.0, s[1], 0.0],
                [0.0, 0.0, s[2]],
            ],
            translation: [0.0, 0.0, 0.0],
        }
    }

    pub fn rotation_x(theta: f64) -> Self {
        let c = theta.cos();
        let s = theta.sin();
        Affine {
            linear: [
                [1.0, 0.0, 0.0],
                [0.0,   c,  -s],
                [0.0,   s,   c],
            ],
            translation: [0.0, 0.0, 0.0],
        }
    }

    pub fn rotation_y(theta: f64) -> Self {
        let c = theta.cos();
        let s = theta.sin();
        Affine {
            linear: [
                [  c, 0.0,   s],
                [0.0, 1.0, 0.0],
                [ -s, 0.0,   c],
            ],
            translation: [0.0, 0.0, 0.0],
        }
    }

    pub fn rotation_z(theta: f64) -> Self {
        let c = theta.cos();
        let s = theta.sin();
        Affine {
            linear: [
                [  c,  -s, 0.0],
                [  s,   c, 0.0],
                [0.0, 0.0, 1.0],
            ],
            translation: [0.0, 0.0, 0.0],
        }
    }

    /// Rotation by `theta` radians around an arbitrary axis. The axis does
    /// not need to be unit-length; it is normalized internally. Implemented
    /// with Rodrigues' rotation formula.
    pub fn rotation_axis(axis: Point, theta: f64) -> Self {
        let len = (axis[0]*axis[0] + axis[1]*axis[1] + axis[2]*axis[2]).sqrt();
        if len < 1e-12 {
            panic!("Cannot rotate around a zero-length axis");
        }
        let x = axis[0] / len;
        let y = axis[1] / len;
        let z = axis[2] / len;
        let c = theta.cos();
        let s = theta.sin();
        let t = 1.0 - c;
        Affine {
            linear: [
                [t*x*x + c,    t*x*y - s*z, t*x*z + s*y],
                [t*x*y + s*z,  t*y*y + c,   t*y*z - s*x],
                [t*x*z - s*y,  t*y*z + s*x, t*z*z + c  ],
            ],
            translation: [0.0, 0.0, 0.0],
        }
    }

    /// Compose two transforms: returns the transform equivalent to applying
    /// `other` first, then `self`. That is, `(self.compose(other))(p) ==
    /// self(other(p))`.
    ///
    /// Derivation: if A = (A_l, a_t) and B = (B_l, b_t), then
    ///   A(B(p)) = A_l * (B_l * p + b_t) + a_t
    ///           = (A_l * B_l) * p + (A_l * b_t + a_t)
    /// so the composed linear is A_l * B_l and the composed translation is
    /// A_l * b_t + a_t.
    pub fn compose(self, other: Affine) -> Affine {
        Affine {
            linear: mat3_multiply(self.linear, other.linear),
            translation: addp(mat3_apply(self.linear, other.translation),
                              self.translation),
        }
    }

    /// Inverse of an affine. Closed form: if A = (L, t), then
    /// A^{-1} = (L^{-1}, -L^{-1} * t).
    pub fn inverse(self) -> Affine {
        let inv_linear = mat3_inverse(self.linear);
        Affine {
            linear: inv_linear,
            translation: negp(mat3_apply(inv_linear, self.translation)),
        }
    }

    pub fn transform_point(&self, p: Point) -> Point {
        addp(mat3_apply(self.linear, p), self.translation)
    }

    pub fn transform_vector(&self, v: Point) -> Point {
        mat3_apply(self.linear, v)
    }
}
