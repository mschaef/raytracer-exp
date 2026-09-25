// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Solid noise for procedural pigments.
//!
//! `noise` is Ken Perlin's "improved noise" (2002): a smooth,
//! deterministic function of 3D position with values in roughly
//! `[-1, 1]` and features about one unit across. It's zero at every
//! integer lattice point. `vector_noise` is three decorrelated copies of
//! it, for displacing points, and `turbulence` sums octaves of it the
//! way POV-Ray's `DTurbulence` does (each octave `lambda` times the
//! frequency and `omega` times the amplitude of the last). The values
//! aren't POV-Ray's own noise tables, so patterns look alike rather than
//! identical.

use crate::render::geometry::Point;

/// Perlin's reference permutation, repeated so indexing can overflow
/// 255 without wrapping.
const PERM: [u8; 256] = [
    151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225,
    140, 36, 103, 30, 69, 142, 8, 99, 37, 240, 21, 10, 23, 190, 6, 148,
    247, 120, 234, 75, 0, 26, 197, 62, 94, 252, 219, 203, 117, 35, 11, 32,
    57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125, 136, 171, 168, 68, 175,
    74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229, 122,
    60, 211, 133, 230, 220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54,
    65, 25, 63, 161, 1, 216, 80, 73, 209, 76, 132, 187, 208, 89, 18, 169,
    200, 196, 135, 130, 116, 188, 159, 86, 164, 100, 109, 198, 173, 186, 3, 64,
    52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118, 126, 255, 82, 85, 212,
    207, 206, 59, 227, 47, 16, 58, 17, 182, 189, 28, 42, 223, 183, 170, 213,
    119, 248, 152, 2, 44, 154, 163, 70, 221, 153, 101, 155, 167, 43, 172, 9,
    129, 22, 39, 253, 19, 98, 108, 110, 79, 113, 224, 232, 178, 185, 112, 104,
    218, 246, 97, 228, 251, 34, 242, 193, 238, 210, 144, 12, 191, 179, 162, 241,
    81, 51, 145, 235, 249, 14, 239, 107, 49, 192, 214, 31, 181, 199, 106, 157,
    184, 84, 204, 176, 115, 121, 50, 45, 127, 4, 150, 254, 138, 236, 205, 93,
    222, 114, 67, 29, 24, 72, 243, 141, 128, 195, 78, 66, 215, 61, 156, 180,
];

fn perm(i: usize) -> usize {
    PERM[i & 255] as usize
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

/// Dot product of the offset `(x, y, z)` with one of twelve gradient
/// directions chosen by the hash.
fn grad(hash: usize, x: f64, y: f64, z: f64) -> f64 {
    let h = hash & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 {
        y
    } else if h == 12 || h == 14 {
        x
    } else {
        z
    };
    (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
}

/// Improved Perlin noise at `p`: smooth, roughly in `[-1, 1]`, zero at
/// integer lattice points.
pub fn noise(p: Point) -> f64 {
    let (fx, fy, fz) = (p[0].floor(), p[1].floor(), p[2].floor());
    let (x, y, z) = (p[0] - fx, p[1] - fy, p[2] - fz);
    // Lattice cell, wrapped to the permutation's period of 256.
    let xi = (fx as i64).rem_euclid(256) as usize;
    let yi = (fy as i64).rem_euclid(256) as usize;
    let zi = (fz as i64).rem_euclid(256) as usize;
    let (u, v, w) = (fade(x), fade(y), fade(z));

    let a = perm(xi) + yi;
    let aa = perm(a) + zi;
    let ab = perm(a + 1) + zi;
    let b = perm(xi + 1) + yi;
    let ba = perm(b) + zi;
    let bb = perm(b + 1) + zi;

    lerp(
        w,
        lerp(
            v,
            lerp(u, grad(perm(aa), x, y, z), grad(perm(ba), x - 1.0, y, z)),
            lerp(u, grad(perm(ab), x, y - 1.0, z), grad(perm(bb), x - 1.0, y - 1.0, z)),
        ),
        lerp(
            v,
            lerp(u, grad(perm(aa + 1), x, y, z - 1.0), grad(perm(ba + 1), x - 1.0, y, z - 1.0)),
            lerp(u, grad(perm(ab + 1), x, y - 1.0, z - 1.0), grad(perm(bb + 1), x - 1.0, y - 1.0, z - 1.0)),
        ),
    )
}

/// Three decorrelated noise values at `p`, one per axis: the same noise
/// sampled at three widely separated, non-lattice offsets, and halved so
/// each component is roughly in `[-0.5, 0.5]`. That's the range of
/// POV-Ray's `DNoise`, so a `turbulence` amount copied from a POV scene
/// disturbs a pattern about as much as it did there.
pub fn vector_noise(p: Point) -> Point {
    [
        0.5 * noise(p),
        0.5 * noise([p[0] + 31.416, p[1] + 47.853, p[2] + 12.793]),
        0.5 * noise([p[0] - 72.117, p[1] + 19.411, p[2] - 53.931]),
    ]
}

/// Octave settings for `turbulence`, with POV-Ray's defaults.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Octaves {
    pub octaves: u32,
    pub omega: f64,
    pub lambda: f64,
}

impl Default for Octaves {
    fn default() -> Self {
        Octaves { octaves: 6, omega: 0.5, lambda: 2.0 }
    }
}

/// Vector turbulence at `p` (POV-Ray's `DTurbulence`): the sum over
/// octaves of `vector_noise`, each octave at `lambda` times the previous
/// frequency and `omega` times the previous amplitude.
pub fn turbulence(p: Point, o: Octaves) -> Point {
    let mut sum = vector_noise(p);
    let mut freq = o.lambda;
    let mut amp = o.omega;
    for _ in 1..o.octaves.max(1) {
        let v = vector_noise([p[0] * freq, p[1] * freq, p[2] * freq]);
        for i in 0..3 {
            sum[i] += amp * v[i];
        }
        freq *= o.lambda;
        amp *= o.omega;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_zero_on_the_lattice_and_deterministic() {
        for p in [[0.0, 0.0, 0.0], [3.0, -7.0, 12.0], [255.0, 256.0, -1.0]] {
            assert_eq!(noise(p), 0.0);
        }
        let p = [1.37, -2.91, 0.44];
        assert_eq!(noise(p), noise(p));
        assert!(noise(p) != 0.0);
    }

    #[test]
    fn noise_is_bounded_and_varied() {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        let mut sum = 0.0;
        let n = 20000;
        for i in 0..n {
            let t = i as f64;
            let v = noise([t * 0.137, t * 0.071 - 3.0, t * 0.029 + 5.0]);
            min = min.min(v);
            max = max.max(v);
            sum += v;
        }
        assert!(min >= -1.1 && max <= 1.1, "range [{}, {}]", min, max);
        assert!(min < -0.4 && max > 0.4, "too flat: [{}, {}]", min, max);
        assert!((sum / n as f64).abs() < 0.05, "mean {}", sum / n as f64);
    }

    #[test]
    fn noise_is_continuous() {
        // Small steps give small changes everywhere, including across
        // lattice cell boundaries.
        let mut p = [-2.0, 0.3, 0.7];
        let step = 0.001;
        let mut last = noise(p);
        for _ in 0..4000 {
            p[0] += step;
            let v = noise(p);
            assert!((v - last).abs() < 0.01, "jump at {:?}", p);
            last = v;
        }
    }

    #[test]
    fn vector_noise_components_differ() {
        let v = vector_noise([0.3, 0.6, 0.9]);
        assert!(v[0] != v[1] && v[1] != v[2]);
    }

    #[test]
    fn turbulence_octaves() {
        let p = [0.43, 1.21, -0.77];
        // One octave is just vector noise.
        let one = turbulence(p, Octaves { octaves: 1, ..Octaves::default() });
        assert_eq!(one, vector_noise(p));
        // More octaves add detail but stay the same order of magnitude.
        let six = turbulence(p, Octaves::default());
        assert!(six != one);
        assert!(six.iter().all(|c| c.abs() < 2.5));
    }
}
