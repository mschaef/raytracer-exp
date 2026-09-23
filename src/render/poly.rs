// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Real roots of low-degree polynomials, for ray-surface intersection.
//!
//! The torus is the first primitive whose intersection is a quartic
//! rather than a quadratic. The closed-form cubic (Cardano, with the
//! trigonometric form for three real roots) and quartic (Ferrari, via a
//! resolvent cubic) solutions follow the classic structure of Jochen
//! Schwarze's "Cubic and Quartic Roots" (Graphics Gems I). Closed-form
//! quartic roots lose precision in double arithmetic, so `solve_quartic`
//! finishes each root with a few Newton steps on the original
//! polynomial; callers should also keep the polynomial well conditioned
//! (see `Torus::local_roots`, which normalizes the ray direction and
//! re-origins the ray near the torus).

/// Coefficients whose magnitude is below this are treated as zero when
/// deciding which case of a closed-form solution applies.
const ZERO: f64 = 1e-12;

/// Real roots of `x² + b x + c = 0`, ascending. A double root is
/// reported once.
pub fn solve_quadratic(b: f64, c: f64) -> Vec<f64> {
    let disc = b * b - 4.0 * c;
    if disc < -ZERO {
        return vec![];
    }
    if disc <= ZERO {
        return vec![-b / 2.0];
    }
    // The numerically stable form: compute the larger-magnitude root
    // directly and get the other from the product of the roots (c),
    // avoiding the cancellation in `-b + sqrt(disc)` when b > 0.
    let s = disc.sqrt();
    let q = if b >= 0.0 { -0.5 * (b + s) } else { -0.5 * (b - s) };
    let (r0, r1) = if q.abs() < ZERO { (0.0, -b) } else { (q, c / q) };
    if r0 <= r1 { vec![r0, r1] } else { vec![r1, r0] }
}

/// Real roots of `x³ + a x² + b x + c = 0`, ascending. There is always
/// at least one. Repeated roots are reported once.
pub fn solve_cubic(a: f64, b: f64, c: f64) -> Vec<f64> {
    // Substitute x = y - a/3 to get the depressed cubic y³ + p y + q = 0.
    let shift = a / 3.0;
    let p = b - a * a / 3.0;
    let q = 2.0 * a * a * a / 27.0 - a * b / 3.0 + c;

    let half_q = q / 2.0;
    let third_p = p / 3.0;
    let disc = half_q * half_q + third_p * third_p * third_p;

    let mut roots = if disc.abs() < ZERO {
        if half_q.abs() < ZERO {
            // Triple root.
            vec![0.0]
        } else {
            // One single and one double root.
            let u = (-half_q).cbrt();
            vec![2.0 * u, -u]
        }
    } else if disc < 0.0 {
        // Three distinct real roots: trigonometric form.
        let phi = (-half_q / (-third_p * third_p * third_p).sqrt()).clamp(-1.0, 1.0).acos() / 3.0;
        let t = 2.0 * (-third_p).sqrt();
        vec![
            t * phi.cos(),
            -t * (phi + std::f64::consts::PI / 3.0).cos(),
            -t * (phi - std::f64::consts::PI / 3.0).cos(),
        ]
    } else {
        // One real root.
        let s = disc.sqrt();
        vec![(s - half_q).cbrt() - (s + half_q).cbrt()]
    };

    for r in &mut roots {
        *r -= shift;
    }
    roots.sort_by(|x, y| x.total_cmp(y));
    roots
}

/// Evaluate `x⁴ + a x³ + b x² + c x + d` and its derivative at `x`.
fn quartic_and_slope(a: f64, b: f64, c: f64, d: f64, x: f64) -> (f64, f64) {
    let f = (((x + a) * x + b) * x + c) * x + d;
    let df = ((4.0 * x + 3.0 * a) * x + 2.0 * b) * x + c;
    (f, df)
}

/// Real roots of `x⁴ + a x³ + b x² + c x + d = 0`, ascending. Roots are
/// polished with Newton's method on the original polynomial. A double
/// root (a tangent ray) may be reported once or twice; callers that
/// care about inside/outside should test between roots rather than
/// trusting the count.
pub fn solve_quartic(a: f64, b: f64, c: f64, d: f64) -> Vec<f64> {
    // Substitute x = y - a/4 to get y⁴ + p y² + q y + r = 0.
    let shift = a / 4.0;
    let a2 = a * a;
    let p = b - 3.0 * a2 / 8.0;
    let q = c - a * b / 2.0 + a2 * a / 8.0;
    let r = d - a * c / 4.0 + a2 * b / 16.0 - 3.0 * a2 * a2 / 256.0;

    let mut roots: Vec<f64> = Vec::with_capacity(4);
    if r.abs() < ZERO {
        // y (y³ + p y + q) = 0.
        roots.push(0.0);
        roots.extend(solve_cubic(0.0, p, q));
    } else if q.abs() < ZERO {
        // Biquadratic: z² + p z + r = 0 with z = y².
        for z in solve_quadratic(p, r) {
            if z > ZERO {
                let s = z.sqrt();
                roots.push(-s);
                roots.push(s);
            } else if z.abs() <= ZERO {
                roots.push(0.0);
            }
        }
    } else {
        // Ferrari: pick a real root z of the resolvent cubic, which
        // splits the quartic into two quadratics. The largest root is
        // the best conditioned choice.
        let resolvent = solve_cubic(-p / 2.0, -r, r * p / 2.0 - q * q / 8.0);
        let z = *resolvent.last().unwrap();
        let mut u = z * z - r;
        let mut v = 2.0 * z - p;
        if u.abs() < ZERO {
            u = 0.0;
        } else if u > 0.0 {
            u = u.sqrt();
        } else {
            u = f64::NAN;
        }
        if v.abs() < ZERO {
            v = 0.0;
        } else if v > 0.0 {
            v = v.sqrt();
        } else {
            v = f64::NAN;
        }
        if u.is_finite() && v.is_finite() {
            let sv = if q < 0.0 { -v } else { v };
            roots.extend(solve_quadratic(sv, z - u));
            roots.extend(solve_quadratic(-sv, z + u));
        }
    }

    for x in &mut roots {
        *x -= shift;
        // Newton polishing on the original polynomial. Stop early when
        // the slope vanishes (a double root), where Newton can't help.
        for _ in 0..4 {
            let (f, df) = quartic_and_slope(a, b, c, d, *x);
            if df.abs() < ZERO {
                break;
            }
            let step = f / df;
            *x -= step;
            if step.abs() < 1e-15 * (1.0 + x.abs()) {
                break;
            }
        }
    }
    roots.sort_by(|x, y| x.total_cmp(y));
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_roots(got: &[f64], want: &[f64]) {
        assert_eq!(got.len(), want.len(), "got {:?}, want {:?}", got, want);
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 1e-9, "got {:?}, want {:?}", got, want);
        }
    }

    /// Monic quartic coefficients (a, b, c, d) with the given four roots.
    fn quartic_from_roots(r: [f64; 4]) -> (f64, f64, f64, f64) {
        let [r0, r1, r2, r3] = r;
        let a = -(r0 + r1 + r2 + r3);
        let b = r0 * r1 + r0 * r2 + r0 * r3 + r1 * r2 + r1 * r3 + r2 * r3;
        let c = -(r0 * r1 * r2 + r0 * r1 * r3 + r0 * r2 * r3 + r1 * r2 * r3);
        let d = r0 * r1 * r2 * r3;
        (a, b, c, d)
    }

    #[test]
    fn quadratic() {
        assert_roots(&solve_quadratic(-3.0, 2.0), &[1.0, 2.0]);
        assert_roots(&solve_quadratic(2.0, 1.0), &[-1.0]);
        assert_roots(&solve_quadratic(0.0, 1.0), &[]);
        // Large b: the stable form keeps the small root accurate.
        let roots = solve_quadratic(-1e8, 1.0);
        assert!((roots[0] - 1e-8).abs() < 1e-20);
    }

    #[test]
    fn cubic() {
        // (x-1)(x-2)(x-3) = x³ - 6x² + 11x - 6
        assert_roots(&solve_cubic(-6.0, 11.0, -6.0), &[1.0, 2.0, 3.0]);
        // (x-1)(x²+1) = x³ - x² + x - 1: one real root.
        assert_roots(&solve_cubic(-1.0, 1.0, -1.0), &[1.0]);
        // (x-2)²(x+1) = x³ - 3x² + 4: a double root.
        assert_roots(&solve_cubic(-3.0, 0.0, 4.0), &[-1.0, 2.0]);
        // (x-1)³: a triple root.
        assert_roots(&solve_cubic(-3.0, 3.0, -1.0), &[1.0]);
    }

    #[test]
    fn quartic_with_four_real_roots() {
        for roots in [[1.0, 2.0, 3.0, 4.0], [-2.5, -1.5, 1.5, 2.5], [-3.0, 0.0, 0.5, 7.0], [0.1, 0.2, 5.0, 5.3]] {
            let (a, b, c, d) = quartic_from_roots(roots);
            assert_roots(&solve_quartic(a, b, c, d), &roots);
        }
    }

    #[test]
    fn quartic_with_complex_roots() {
        // (x-1)(x-3)(x²+1): two real roots.
        // (x²-4x+3)(x²+1) = x⁴ - 4x³ + 4x² - 4x + 3
        assert_roots(&solve_quartic(-4.0, 4.0, -4.0, 3.0), &[1.0, 3.0]);
        // (x²+1)(x²+4): no real roots.
        assert_roots(&solve_quartic(0.0, 5.0, 0.0, 4.0), &[]);
    }

    #[test]
    fn quartic_special_cases() {
        // Biquadratic: (x²-1)(x²-4).
        assert_roots(&solve_quartic(0.0, -5.0, 0.0, 4.0), &[-2.0, -1.0, 1.0, 2.0]);
        // A zero root: x (x-1)(x-2)(x-3).
        let (a, b, c, d) = quartic_from_roots([0.0, 1.0, 2.0, 3.0]);
        assert_roots(&solve_quartic(a, b, c, d), &[0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn quartic_roots_are_roots_across_many_cases() {
        // Deterministic spread of root sets, including near-coincident
        // pairs like a ray grazing a torus tube.
        let mut seed = 0x1234_5678_u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 11) as f64 / (1u64 << 53) as f64) * 20.0 - 10.0
        };
        for _ in 0..500 {
            let mut roots = [next(), next(), next(), next()];
            roots.sort_by(|x, y| x.total_cmp(y));
            if roots.windows(2).any(|w| w[1] - w[0] < 1e-3) {
                continue;
            }
            let (a, b, c, d) = quartic_from_roots(roots);
            let got = solve_quartic(a, b, c, d);
            assert_eq!(got.len(), 4, "roots {:?} gave {:?}", roots, got);
            for (g, w) in got.iter().zip(&roots) {
                assert!((g - w).abs() < 1e-6, "roots {:?} gave {:?}", roots, got);
            }
        }
    }
}
