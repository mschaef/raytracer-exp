// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Procedural pigments: a surface colour that varies over space.
//!
//! A `Pigment` is evaluated at a point in *texture space*, the local
//! coordinates of whatever gave the hit its surface (the leaf primitive,
//! or the enclosing `with-surface` wrapper), so a pattern moves, turns
//! and scales with its object. The pigment's own `transform` then maps
//! that point into pattern space (e.g. POV-Ray's `scale 0.05` on a wood
//! pigment makes the rings 20 times finer). The pattern turns the point
//! into a value, the wave shapes the value, and the colour map turns the
//! value into a colour, following POV-Ray's pipeline.

use crate::render::color::LinearColor;
use crate::render::geometry::Point;
use crate::render::noise::{turbulence, Octaves};
use crate::render::transform::Affine;

/// The spatial pattern a pigment follows.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Pattern {
    /// Concentric cylinders around the pattern-space z axis: the value is
    /// the distance from the axis, so with the default triangle wave it
    /// ramps up and back down once per unit. Turbulence wobbles the
    /// rings, as POV-Ray's `wood` does.
    Wood,
    /// A 3D checkerboard of unit cubes: the value is 0 or 1. Turbulence
    /// displaces the point before the cells are chosen.
    Checker,
}

/// How a pattern value in `[0, 1)` is reshaped before the colour-map
/// lookup (POV-Ray's wave types).
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Wave {
    /// Unchanged: a sawtooth from 0 to 1.
    Ramp,
    /// Up to 1 at the half-way point and back down to 0.
    Triangle,
    /// A smooth sine from 0.5 up to 1, down to 0 and back.
    Sine,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Pigment {
    pub pattern: Pattern,
    /// Turbulence amplitude per axis (POV-Ray's `turbulence <x, y, z>`;
    /// a single number is the same on all three). Zero is none.
    pub turbulence: [f64; 3],
    pub octaves: Octaves,
    /// Applied to continuous patterns (wood); ignored for the checker.
    pub wave: Wave,
    /// `(value, colour)` entries in ascending order of value. Values
    /// between entries interpolate; values outside take the nearest
    /// end. Two entries at the same value make a hard edge.
    pub color_map: Vec<(f64, LinearColor)>,
    /// Texture space to pattern space: the inverse of the transforms
    /// written on the pigment.
    pub from_texture: Affine,
}

impl Pigment {
    fn has_turbulence(&self) -> bool {
        self.turbulence.iter().any(|a| *a != 0.0)
    }

    /// The pigment's colour at `p`, a point in texture space.
    pub fn color_at(&self, p: Point) -> LinearColor {
        let q = self.from_texture.transform_point(p);
        let value = match self.pattern {
            Pattern::Wood => {
                // POV-Ray's wood: turbulence displaces x and y through a
                // sine, then the value is the distance from the z axis.
                let (mut x, mut y) = (q[0], q[1]);
                if self.has_turbulence() {
                    let t = turbulence(q, self.octaves);
                    let [tx, ty, _] = self.turbulence;
                    x += ((x + t[0]) * tx * std::f64::consts::TAU).sin();
                    y += ((y + t[1]) * ty * std::f64::consts::TAU).sin();
                }
                apply_wave((x * x + y * y).sqrt(), self.wave)
            }
            Pattern::Checker => {
                let q = if self.has_turbulence() {
                    let t = turbulence(q, self.octaves);
                    let a = self.turbulence;
                    [q[0] + a[0] * t[0], q[1] + a[1] * t[1], q[2] + a[2] * t[2]]
                } else {
                    q
                };
                let cells = q[0].floor() + q[1].floor() + q[2].floor();
                if (cells as i64).rem_euclid(2) == 0 { 0.0 } else { 1.0 }
            }
        };
        color_map_lookup(&self.color_map, value)
    }
}

/// Reduce `v` to `[0, 1)` and apply the wave shape.
fn apply_wave(v: f64, wave: Wave) -> f64 {
    let f = v - v.floor();
    match wave {
        Wave::Ramp => f,
        Wave::Triangle => {
            if f < 0.5 { 2.0 * f } else { 2.0 - 2.0 * f }
        }
        Wave::Sine => 0.5 * (1.0 + (f * std::f64::consts::TAU).sin()),
    }
}

/// The colour for `value` from a map of ascending `(value, colour)`
/// entries.
pub fn color_map_lookup(map: &[(f64, LinearColor)], value: f64) -> LinearColor {
    match map {
        [] => [0.0, 0.0, 0.0],
        [(_, c)] => *c,
        _ => {
            if value <= map[0].0 {
                return map[0].1;
            }
            for w in map.windows(2) {
                let (v0, c0) = w[0];
                let (v1, c1) = w[1];
                if value < v1 {
                    if v1 <= v0 {
                        return c1;
                    }
                    let t = (value - v0) / (v1 - v0);
                    return [
                        c0[0] + t * (c1[0] - c0[0]),
                        c0[1] + t * (c1[1] - c0[1]),
                        c0[2] + t * (c1[2] - c0[2]),
                    ];
                }
            }
            map[map.len() - 1].1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: LinearColor = [0.0, 0.0, 0.0];
    const WHITE: LinearColor = [1.0, 1.0, 1.0];

    fn pigment(pattern: Pattern, map: Vec<(f64, LinearColor)>) -> Pigment {
        Pigment {
            pattern,
            turbulence: [0.0; 3],
            octaves: Octaves::default(),
            wave: Wave::Triangle,
            color_map: map,
            from_texture: Affine::identity(),
        }
    }

    #[test]
    fn color_map_interpolates_and_clamps() {
        let map = vec![(0.2, BLACK), (0.6, WHITE)];
        assert_eq!(color_map_lookup(&map, 0.0), BLACK);
        let mid = color_map_lookup(&map, 0.4);
        assert!(mid.iter().all(|c| (c - 0.5).abs() < 1e-12), "{:?}", mid);
        assert_eq!(color_map_lookup(&map, 0.9), WHITE);
        // A repeated entry is a hard edge (POV's Dark_Wood does this).
        let edge = vec![(0.0, BLACK), (0.5, BLACK), (0.5, WHITE), (1.0, WHITE)];
        assert_eq!(color_map_lookup(&edge, 0.4999), BLACK);
        assert_eq!(color_map_lookup(&edge, 0.5), WHITE);
        assert_eq!(color_map_lookup(&[], 0.5), BLACK);
        assert_eq!(color_map_lookup(&[(0.3, WHITE)], 0.9), WHITE);
    }

    #[test]
    fn waves() {
        assert_eq!(apply_wave(0.25, Wave::Ramp), 0.25);
        assert_eq!(apply_wave(2.25, Wave::Ramp), 0.25);
        assert_eq!(apply_wave(0.25, Wave::Triangle), 0.5);
        assert_eq!(apply_wave(0.75, Wave::Triangle), 0.5);
        assert_eq!(apply_wave(0.5, Wave::Triangle), 1.0);
        assert!((apply_wave(0.25, Wave::Sine) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn wood_rings_are_concentric_about_z() {
        let wood = pigment(Pattern::Wood, vec![(0.0, BLACK), (1.0, WHITE)]);
        // Distance 0.25 from the axis, anywhere along z and around it.
        let c = wood.color_at([0.25, 0.0, 0.0]);
        assert_eq!(c, [0.5, 0.5, 0.5]);
        assert_eq!(wood.color_at([0.0, -0.25, 9.0]), c);
        let s = 0.25 / 2f64.sqrt();
        let d = wood.color_at([s, s, -3.0]);
        assert!((d[0] - 0.5).abs() < 1e-12);
        // Brightest half a unit out, dark again at one unit.
        assert_eq!(wood.color_at([0.5, 0.0, 0.0]), WHITE);
        assert_eq!(wood.color_at([1.0, 0.0, 0.0]), BLACK);
    }

    #[test]
    fn pigment_transform_scales_the_pattern() {
        let mut wood = pigment(Pattern::Wood, vec![(0.0, BLACK), (1.0, WHITE)]);
        // POV's `scale 0.05`: rings every 0.05 units.
        wood.from_texture = Affine::scale([0.05, 0.05, 0.05]).inverse();
        assert_eq!(wood.color_at([0.025, 0.0, 0.0]), WHITE);
        assert_eq!(wood.color_at([0.05, 0.0, 0.0]), BLACK);
    }

    #[test]
    fn checker_alternates() {
        let checker = pigment(Pattern::Checker, vec![(0.0, BLACK), (1.0, WHITE)]);
        assert_eq!(checker.color_at([0.5, 0.5, 0.5]), BLACK);
        assert_eq!(checker.color_at([1.5, 0.5, 0.5]), WHITE);
        assert_eq!(checker.color_at([1.5, 1.5, 0.5]), BLACK);
        assert_eq!(checker.color_at([-0.5, 0.5, 0.5]), WHITE);
    }

    #[test]
    fn turbulence_perturbs_but_stays_in_the_map() {
        let mut wood = pigment(Pattern::Wood, vec![(0.0, BLACK), (1.0, WHITE)]);
        wood.turbulence = [0.3; 3];
        let mut differs = false;
        for i in 0..200 {
            let p = [0.05 * i as f64, 0.37, 0.11 * i as f64];
            let plain = pigment(Pattern::Wood, vec![(0.0, BLACK), (1.0, WHITE)]).color_at(p);
            let c = wood.color_at(p);
            assert!(c[0] >= 0.0 && c[0] <= 1.0);
            differs |= (c[0] - plain[0]).abs() > 1e-6;
        }
        assert!(differs);
    }

    #[test]
    fn wood_turbulence_is_per_axis() {
        // Turbulence only in y leaves points on the x axis' y = 0 line
        // perturbed in y alone; turbulence only in z (which wood ignores)
        // changes nothing.
        let plain = pigment(Pattern::Wood, vec![(0.0, BLACK), (1.0, WHITE)]);
        let mut z_only = plain.clone();
        z_only.turbulence = [0.0, 0.0, 1000.0];
        let mut y_only = plain.clone();
        y_only.turbulence = [0.0, 0.3, 0.0];
        let mut y_differs = false;
        for i in 0..100 {
            let p = [0.013 * i as f64 + 0.2, 0.31, 0.07 * i as f64];
            assert_eq!(z_only.color_at(p), plain.color_at(p));
            y_differs |= y_only.color_at(p) != plain.color_at(p);
        }
        assert!(y_differs);
    }
}
