// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Normal perturbation: POV-Ray's `normal { bumps | wrinkles ... }`.
//!
//! A `NormalPattern` tilts the shading normal by a vector that varies
//! over space, so a smooth surface shades as if it were rough, without
//! changing its geometry. Like a pigment it's evaluated at the hit's
//! texture point, mapped into pattern space by its own transform and
//! optionally displaced by turbulence. The tilt is added to the normal
//! and the sum renormalized, as POV-Ray does.

use crate::render::geometry::Point;
use crate::render::noise::{turbulence, vector_noise, Octaves};
use crate::render::transform::Affine;

/// Which pattern tilts the normal.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Bump {
    /// Smooth noise: the tilt is `amount` times the vector noise at the
    /// point (POV-Ray's `bumps`, from `DNoise`).
    Bumps,
    /// Crinkled noise: `amount` times the sum over nine octaves of the
    /// absolute vector noise, each octave at twice the frequency and
    /// half the weight (POV-Ray's `wrinkles`).
    Wrinkles,
}

#[derive(Clone, PartialEq, Debug)]
pub struct NormalPattern {
    pub bump: Bump,
    /// How strongly the normal tilts (POV's bump amount).
    pub amount: f64,
    /// Turbulence amplitude per axis, displacing the point before the
    /// pattern is evaluated. Zero is none.
    pub turbulence: [f64; 3],
    pub octaves: Octaves,
    /// Texture space to pattern space: the inverse of the transforms
    /// written on the normal.
    pub from_texture: Affine,
}

impl NormalPattern {
    /// The vector to add to the normal at `p`, a point in texture space.
    pub fn tilt_at(&self, p: Point) -> Point {
        let mut q = self.from_texture.transform_point(p);
        if self.turbulence.iter().any(|a| *a != 0.0) {
            let t = turbulence(q, self.octaves);
            for i in 0..3 {
                q[i] += self.turbulence[i] * t[i];
            }
        }
        let v = match self.bump {
            Bump::Bumps => vector_noise(q),
            Bump::Wrinkles => {
                let mut sum = vector_noise(q);
                let (mut freq, mut weight) = (2.0, 0.5);
                for _ in 1..10 {
                    let n = vector_noise([q[0] * freq, q[1] * freq, q[2] * freq]);
                    for i in 0..3 {
                        sum[i] += weight * n[i].abs();
                    }
                    freq *= 2.0;
                    weight *= 0.5;
                }
                sum
            }
        };
        [self.amount * v[0], self.amount * v[1], self.amount * v[2]]
    }

    /// `normal` (unit) tilted by the pattern at `p`, renormalized. Falls
    /// back to `normal` in the vanishing case where the tilt cancels it.
    pub fn perturb(&self, normal: Point, p: Point) -> Point {
        let t = self.tilt_at(p);
        let n = [normal[0] + t[0], normal[1] + t[1], normal[2] + t[2]];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 1e-9 { [n[0] / len, n[1] / len, n[2] / len] } else { normal }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(bump: Bump, amount: f64) -> NormalPattern {
        NormalPattern {
            bump,
            amount,
            turbulence: [0.0; 3],
            octaves: Octaves::default(),
            from_texture: Affine::identity(),
        }
    }

    #[test]
    fn perturbed_normals_are_unit_and_vary() {
        let n = [0.0, 1.0, 0.0];
        for bump in [Bump::Bumps, Bump::Wrinkles] {
            let p = pattern(bump, 0.5);
            let a = p.perturb(n, [0.3, 0.0, 0.7]);
            let b = p.perturb(n, [1.9, 0.0, 2.4]);
            for v in [a, b] {
                let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                assert!((len - 1.0).abs() < 1e-12);
                assert!(v[1] > 0.5, "stays near the surface normal: {:?}", v);
            }
            assert_ne!(a, b, "{:?}", bump);
        }
    }

    #[test]
    fn zero_amount_leaves_the_normal_alone() {
        let n = [0.0, 0.6, 0.8];
        assert_eq!(pattern(Bump::Bumps, 0.0).perturb(n, [0.3, 0.2, 0.1]), n);
    }

    #[test]
    fn a_larger_amount_tilts_further() {
        let n = [0.0, 1.0, 0.0];
        let mut small = 0.0;
        let mut large = 0.0;
        for k in 0..200 {
            let p = [k as f64 * 0.137, 0.0, k as f64 * 0.311];
            small += 1.0 - pattern(Bump::Bumps, 0.1).perturb(n, p)[1];
            large += 1.0 - pattern(Bump::Bumps, 0.7).perturb(n, p)[1];
        }
        assert!(large > 5.0 * small && small > 0.0, "{} {}", small, large);
    }
}
