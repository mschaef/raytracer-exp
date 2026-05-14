// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Sub-pixel sample positioning.
//!
//! The Halton-(2, 3) low-discrepancy sequence drives `pixel_color`'s
//! sub-pixel offsets. Sample positions are indexed by sample number,
//! so the adaptive-termination loop in `pixel_color` can extend the
//! sample count for any individual pixel without quality cliffs —
//! sample `i` always has a well-defined position no matter how many
//! samples a pixel ultimately takes. Phase 2 of the
//! adaptive-oversampling plan made this load-bearing: pixels in flat
//! regions terminate at `min_samples`, pixels on edges sample further,
//! and the Halton sequence stays well-distributed for all of them.
//!
//! Two pieces, both stateless:
//!
//! * `halton_pair(i)` — radical-inverse base 2 for the x coordinate,
//!   base 3 for y. Returns a point in `[0, 1)²`. `i = 0` returns the
//!   pixel corner `(0, 0)`; callers should start at `i = 1` so the
//!   first sample lands inside the pixel.
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

/// A deterministic per-pixel `(ox, oy)` offset in `[0, 1)²` derived
/// from the pixel coordinates. Adding this to a Halton point and
/// taking the fractional part rotates the sequence by a different
/// vector for each pixel — Cranley-Patterson rotation —
/// decorrelating neighbors so that any structural sampling artifact
/// dissolves into per-pixel noise.
///
/// The hash is a Weyl-style accumulation followed by a splitmix64
/// finalizer. The constants are odd 64-bit values, chosen for good
/// avalanche behavior; nothing about the choice depends on the pixel
/// dimensions, so axis-aligned arrays of any size remain
/// well-decorrelated.
pub fn cranley_patterson_offset(x: u32, y: u32) -> (f64, f64) {
    // Weyl-style accumulation: each coordinate gets multiplied by a
    // different large odd constant and the results are summed. Odd
    // multipliers ensure that no input bits are lost to a power-of-two
    // factor, which keeps small (x, y) pairs (the top-left corner of
    // the image) from mapping to small or correlated hashes.
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h = h.wrapping_add((y as u64).wrapping_mul(0xBB67_AE85_84CA_A73B));

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
}
