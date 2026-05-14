// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Sub-pixel and lens sample positioning.
//!
//! The Halton low-discrepancy sequence drives `pixel_color`'s
//! sub-pixel offsets (and, for depth-of-field cameras, the aperture
//! sample). Sample positions are indexed by sample number, so the
//! adaptive-termination loop in `pixel_color` can extend the sample
//! count for any individual pixel without quality cliffs — sample
//! `i` always has a well-defined position no matter how many samples
//! a pixel ultimately takes. Phase 2 of the adaptive-oversampling
//! plan made this load-bearing: pixels in flat regions terminate at
//! `min_samples`, pixels on edges sample further, and the Halton
//! sequence stays well-distributed for all of them.
//!
//! The pieces, all stateless:
//!
//! * `halton_pair(i)` — radical-inverse base 2 for the x coordinate,
//!   base 3 for y. Returns a point in `[0, 1)²`. `i = 0` returns the
//!   pixel corner `(0, 0)`; callers should start at `i = 1` so the
//!   first sample lands inside the pixel. This is the sub-pixel
//!   (anti-aliasing) sample.
//!
//! * `halton_lens(i)` — the same idea on bases 5 and 7, used for the
//!   depth-of-field aperture sample. A separate pair of bases keeps
//!   the lens coordinate from being correlated with the sub-pixel
//!   coordinate of the same sample index. Higher bases have slightly
//!   worse distribution than (2, 3), but 5 and 7 are still fine for
//!   the modest per-pixel sample counts here.
//!
//! * `concentric_disk(u, v)` — maps a `[0, 1)²` point (e.g. a
//!   rotated `halton_lens` value) onto the unit disk via the
//!   Shirley–Chiu concentric mapping. Used to turn the lens Halton
//!   sample into an aperture offset. Concentric mapping preserves
//!   area and adjacency far better than the naive
//!   `r = √u, θ = 2πv` polar map, which matters for clean bokeh.
//!
//! * `cranley_patterson_offset(x, y)` — a per-pixel `(ox, oy)`
//!   rotation in `[0, 1)²` derived from the pixel coordinates. The
//!   actual sample position is `(halton_pair(i) + (ox, oy)) mod 1`.
//!   Without it, every pixel would sample at the exact same set of
//!   sub-pixel positions, and any residual sampling artifact would
//!   appear as a tiled pattern; with it, the artifact looks like
//!   noise instead. The hash is a splitmix64-style mixer; nothing
//!   here is cryptographic — the only property we need is that
//!   different `(x, y)` produce well-spread offsets.
//!
//! * `cranley_patterson_lens_offset(x, y)` — the same rotation for
//!   the lens sample, with a distinct hash seed so the lens
//!   rotation is decorrelated from the sub-pixel rotation. Both go
//!   through the same `cp_hash` helper; the seed is the only
//!   difference.

/// Radical inverse in `base`, evaluated at index `i`. Returns a value
/// in `[0, 1)`. For `i = 0` returns `0.0`. Standard low-discrepancy
/// construction; see e.g. PBRT, "Sampling and Reconstruction".
///
/// Costs a handful of integer divisions and multiplies per call; in
/// practice the loop runs `log_base(i)` times, so for the sample
/// counts this codebase uses (single-digit to mid-double-digit
/// sample counts per pixel) it's a few iterations per call.
fn radical_inverse(base: u32, mut i: u32) -> f64 {
    let inv_b = 1.0 / base as f64;
    let mut result = 0.0;
    let mut f = inv_b;
    while i > 0 {
        result += (i % base) as f64 * f;
        i /= base;
        f *= inv_b;
    }
    result
}

/// The pair `(H_2(i), H_3(i))` — point `i` of the 2D Halton sequence
/// with bases 2 and 3. Returned values are in `[0, 1)²`.
///
/// `i = 0` returns `(0, 0)`, which sits at the pixel corner rather
/// than inside it; including the corner would bias the average toward
/// the upper-left of the pixel without per-pixel rotation. Callers
/// should pass `i >= 1`.
pub fn halton_pair(i: u32) -> (f64, f64) {
    (radical_inverse(2, i), radical_inverse(3, i))
}

/// The pair `(H_5(i), H_7(i))` — point `i` of the 2D Halton sequence
/// with bases 5 and 7, used for the depth-of-field aperture sample.
/// Returned values are in `[0, 1)²`.
///
/// Distinct bases from `halton_pair` so that, for a given sample
/// index `i`, the lens coordinate and the sub-pixel coordinate are
/// drawn from different sequences and aren't correlated. As with
/// `halton_pair`, `i = 0` returns `(0, 0)` (a corner); callers pass
/// `i >= 1`.
pub fn halton_lens(i: u32) -> (f64, f64) {
    (radical_inverse(5, i), radical_inverse(7, i))
}

/// Shirley–Chiu concentric mapping from the unit square to the unit
/// disk. Takes `(u, v)` in `[0, 1)²` (typically a Cranley-Patterson-
/// rotated `halton_lens` value) and returns a point `(x, y)` with
/// `x² + y² <= 1` — an offset on the camera's aperture disk.
///
/// The mapping divides the square into four triangular wedges and
/// maps each to a quarter of the disk, so equal-area regions of the
/// square map to equal-area regions of the disk and neighbouring
/// square points stay neighbouring on the disk. That low distortion
/// is what keeps depth-of-field blur disks (bokeh) smooth instead of
/// clumped — the naive `r = √u, θ = 2πv` polar map oversamples the
/// rim relative to the centre and shows it.
///
/// The exact centre `(0.5, 0.5)` maps to the origin; the special-case
/// guard avoids a `0/0` in the `b / a` ratio there.
pub fn concentric_disk(u: f64, v: f64) -> (f64, f64) {
    // Remap [0, 1)² to [-1, 1]².
    let a = 2.0 * u - 1.0;
    let b = 2.0 * v - 1.0;

    // Degenerate centre point: no well-defined angle, and the ratio
    // below would be 0/0. Map it straight to the disk centre.
    if a == 0.0 && b == 0.0 {
        return (0.0, 0.0);
    }

    use std::f64::consts::FRAC_PI_4;

    // Pick the wedge by whichever of |a|, |b| dominates: `r` is the
    // dominant coordinate (so |r| is the Chebyshev distance, which
    // becomes the disk radius), and `theta` sweeps ±45° within the
    // wedge.
    let (r, theta) = if a * a > b * b {
        (a, FRAC_PI_4 * (b / a))
    } else {
        (b, FRAC_PI_4 * 2.0 - FRAC_PI_4 * (a / b))
    };

    (r * theta.cos(), r * theta.sin())
}

/// Core hash behind the Cranley-Patterson rotations: maps a pixel
/// coordinate plus a `seed` to an `(ox, oy)` offset in `[0, 1)²`.
///
/// The `seed` lets two independent rotations (sub-pixel and lens)
/// share this hash without colliding — different seeds give
/// uncorrelated offsets for the same pixel. `seed = 0` is folded in
/// as `+ 0` (`0u64.wrapping_mul(_) == 0`), so the seed-0 result is
/// bit-for-bit what the original seedless `cranley_patterson_offset`
/// produced — important, because the byte-pinned tests in
/// `tests/sdl_suite.rs` render through this exact sub-pixel offset.
///
/// The hash is a Weyl-style accumulation followed by a splitmix64
/// finalizer. The constants are odd 64-bit values, chosen for good
/// avalanche behavior; nothing about the choice depends on the pixel
/// dimensions, so axis-aligned arrays of any size remain
/// well-decorrelated.
fn cp_hash(x: u32, y: u32, seed: u64) -> (f64, f64) {
    // Weyl-style accumulation: each coordinate gets multiplied by a
    // different large odd constant and the results are summed. Odd
    // multipliers ensure that no input bits are lost to a power-of-two
    // factor, which keeps small (x, y) pairs (the top-left corner of
    // the image) from mapping to small or correlated hashes. The seed
    // joins the accumulation the same way; with `seed = 0` this term
    // is exactly 0 and the hash is unchanged from the seedless form.
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h = h.wrapping_add((y as u64).wrapping_mul(0xBB67_AE85_84CA_A73B));
    h = h.wrapping_add(seed.wrapping_mul(0xD1B5_4A32_D192_ED03));

    // splitmix64 finalizer: three rounds of "xor with right-shift,
    // multiply by an odd constant." Standard avalanche pattern; the
    // constants are the published splitmix64 ones.
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;

    // Split the 64-bit hash into two 32-bit halves and map each to
    // [0, 1) via `n / 2^32`. The two halves of a well-mixed hash are
    // statistically independent, so the resulting `(ox, oy)` are
    // uncorrelated.
    let lo = (h as u32) as f64;
    let hi = ((h >> 32) as u32) as f64;
    let denom = (1u64 << 32) as f64;
    (lo / denom, hi / denom)
}

/// A deterministic per-pixel `(ox, oy)` offset in `[0, 1)²` derived
/// from the pixel coordinates. Adding this to a `halton_pair` point
/// and taking the fractional part rotates the sequence by a
/// different vector for each pixel — Cranley-Patterson rotation —
/// decorrelating neighbors so that any structural sampling artifact
/// dissolves into per-pixel noise.
pub fn cranley_patterson_offset(x: u32, y: u32) -> (f64, f64) {
    cp_hash(x, y, 0)
}

/// The Cranley-Patterson rotation for the depth-of-field lens sample:
/// a per-pixel `(ox, oy)` offset in `[0, 1)²` applied to a
/// `halton_lens` point before it's mapped onto the aperture disk.
///
/// Uses a distinct hash seed from `cranley_patterson_offset`, so a
/// pixel's lens rotation is uncorrelated with its sub-pixel rotation
/// — the two sampled dimensions stay independent even though both
/// are keyed off the same `(x, y)`.
pub fn cranley_patterson_lens_offset(x: u32, y: u32) -> (f64, f64) {
    cp_hash(x, y, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `radical_inverse(b, 0) == 0.0` for any base.
    #[test]
    fn radical_inverse_zero() {
        assert_eq!(radical_inverse(2, 0), 0.0);
        assert_eq!(radical_inverse(3, 0), 0.0);
        assert_eq!(radical_inverse(5, 0), 0.0);
    }

    /// First few base-2 radical inverses, known-good values.
    ///
    /// H_2(1) = 0.5, H_2(2) = 0.25, H_2(3) = 0.75, H_2(4) = 0.125,
    /// H_2(5) = 0.625, H_2(6) = 0.375, H_2(7) = 0.875.
    #[test]
    fn radical_inverse_base_2() {
        let expected = [0.0, 0.5, 0.25, 0.75, 0.125, 0.625, 0.375, 0.875];
        for (i, e) in expected.iter().enumerate() {
            assert!(
                (radical_inverse(2, i as u32) - e).abs() < 1e-15,
                "H_2({}) = {}, expected {}",
                i,
                radical_inverse(2, i as u32),
                e
            );
        }
    }

    /// First few base-3 radical inverses.
    ///
    /// H_3(1) = 1/3, H_3(2) = 2/3, H_3(3) = 1/9, H_3(4) = 4/9.
    #[test]
    fn radical_inverse_base_3() {
        let expected = [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0 / 9.0, 4.0 / 9.0];
        for (i, e) in expected.iter().enumerate() {
            assert!(
                (radical_inverse(3, i as u32) - e).abs() < 1e-15,
                "H_3({}) = {}, expected {}",
                i,
                radical_inverse(3, i as u32),
                e
            );
        }
    }

    /// All sampler outputs are in `[0, 1)²`. Sanity check across a
    /// modest range; if any value escapes the range we'd push the
    /// sample outside its pixel and produce wrong geometry.
    #[test]
    fn outputs_in_unit_square() {
        for i in 0..1024 {
            let (a, b) = halton_pair(i);
            assert!(a >= 0.0 && a < 1.0, "halton x out of range at i={}: {}", i, a);
            assert!(b >= 0.0 && b < 1.0, "halton y out of range at i={}: {}", i, b);
        }
        for x in 0..64 {
            for y in 0..64 {
                let (a, b) = cranley_patterson_offset(x, y);
                assert!(
                    a >= 0.0 && a < 1.0,
                    "CP x out of range at ({}, {}): {}",
                    x,
                    y,
                    a
                );
                assert!(
                    b >= 0.0 && b < 1.0,
                    "CP y out of range at ({}, {}): {}",
                    x,
                    y,
                    b
                );
            }
        }
    }

    /// Different pixel coordinates produce different rotations. If
    /// the hash were degenerate (e.g. xor-only with no multiplier),
    /// neighboring pixels could collide; this guards against the
    /// trivial form of that bug. We don't claim collision-freeness
    /// over the whole image — just that two adjacent pixels and two
    /// far-apart pixels produce visibly different offsets.
    #[test]
    fn cp_offset_decorrelates_pixels() {
        let a = cranley_patterson_offset(0, 0);
        let b = cranley_patterson_offset(0, 1);
        let c = cranley_patterson_offset(1, 0);
        let d = cranley_patterson_offset(100, 100);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
        assert_ne!(b, c);
    }

    /// First few base-5 / base-7 radical inverses, the bases
    /// `halton_lens` uses. H_5(1) = 1/5, H_5(2) = 2/5; H_7(1) = 1/7.
    #[test]
    fn halton_lens_known_values() {
        let (x1, y1) = halton_lens(1);
        assert!((x1 - 0.2).abs() < 1e-15, "H_5(1) = {}", x1);
        assert!((y1 - 1.0 / 7.0).abs() < 1e-15, "H_7(1) = {}", y1);
        let (x2, _) = halton_lens(2);
        assert!((x2 - 0.4).abs() < 1e-15, "H_5(2) = {}", x2);
    }

    /// `halton_lens` stays in `[0, 1)²` across a modest index range —
    /// same contract as `halton_pair`, since the lens point is fed
    /// through the same Cranley-Patterson rotation before use.
    #[test]
    fn halton_lens_in_unit_square() {
        for i in 0..1024 {
            let (a, b) = halton_lens(i);
            assert!(a >= 0.0 && a < 1.0, "lens x out of range at i={}: {}", i, a);
            assert!(b >= 0.0 && b < 1.0, "lens y out of range at i={}: {}", i, b);
        }
    }

    /// The concentric map never escapes the unit disk: every
    /// `[0, 1)²` input maps to a point with `x² + y² <= 1`. If it
    /// did escape, depth-of-field rays would aim from outside the
    /// intended aperture.
    #[test]
    fn concentric_disk_within_unit_disk() {
        for ui in 0..64 {
            for vi in 0..64 {
                let u = ui as f64 / 64.0;
                let v = vi as f64 / 64.0;
                let (x, y) = concentric_disk(u, v);
                let r2 = x * x + y * y;
                // Small slack for floating-point error at the rim.
                assert!(
                    r2 <= 1.0 + 1e-9,
                    "concentric_disk({}, {}) = ({}, {}), r² = {}",
                    u, v, x, y, r2
                );
            }
        }
    }

    /// The centre of the square maps to the centre of the disk, and
    /// the four square corners map to the rim (radius ≈ 1).
    #[test]
    fn concentric_disk_landmarks() {
        let (cx, cy) = concentric_disk(0.5, 0.5);
        assert!(
            cx.abs() < 1e-15 && cy.abs() < 1e-15,
            "centre maps to ({}, {})",
            cx, cy
        );
        for &(u, v) in &[(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            let (x, y) = concentric_disk(u, v);
            let r = (x * x + y * y).sqrt();
            assert!(
                (r - 1.0).abs() < 1e-12,
                "corner ({}, {}) maps to radius {}",
                u, v, r
            );
        }
    }

    /// The lens Cranley-Patterson rotation is in range and is
    /// decorrelated from the sub-pixel rotation: for the same pixel,
    /// the seeded lens offset must differ from the seedless sub-pixel
    /// offset. (If the seed weren't actually folded into the hash,
    /// these would be identical and the two sampled dimensions would
    /// march in lockstep.)
    #[test]
    fn lens_cp_offset_distinct_from_pixel() {
        for x in 0..64 {
            for y in 0..64 {
                let (a, b) = cranley_patterson_lens_offset(x, y);
                assert!(a >= 0.0 && a < 1.0, "lens CP x out of range: {}", a);
                assert!(b >= 0.0 && b < 1.0, "lens CP y out of range: {}", b);
                assert_ne!(
                    cranley_patterson_lens_offset(x, y),
                    cranley_patterson_offset(x, y),
                    "lens and pixel CP offsets collided at ({}, {})",
                    x, y
                );
            }
        }
    }
}
