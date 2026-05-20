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
//! * `halton_area(i)` — same idea again on bases 11 and 13, used for
//!   the area-light disk sample in Phase 5 of the "Light types"
//!   plan. A third distinct pair of bases keeps the area-light
//!   coordinate independent of both the sub-pixel and lens
//!   coordinates of the same sample index.
//!
//! * `halton_indirect(i)` — bases 17 and 19, used for the
//!   cosine-weighted hemisphere sample at a diffuse hit in Phase 1
//!   of the path-tracing plan. A fourth distinct pair of bases so
//!   the indirect-bounce direction is uncorrelated with all three
//!   other sampled dimensions (sub-pixel, lens, area-light) at the
//!   same sample index.
//!
//! * `concentric_disk(u, v)` — maps a `[0, 1)²` point (e.g. a
//!   rotated `halton_lens` value or `halton_area` value) onto the
//!   unit disk via the Shirley–Chiu concentric mapping. Used to turn
//!   the lens Halton sample into an aperture offset and the area
//!   Halton sample into a point on a disk emitter. Concentric
//!   mapping preserves area and adjacency far better than the naive
//!   `r = √u, θ = 2πv` polar map, which matters for clean bokeh and
//!   well-distributed soft-shadow samples.
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
//!   rotation is decorrelated from the sub-pixel rotation.
//!
//! * `cranley_patterson_area_offset(x, y)` — the rotation for the
//!   area-light disk sample, with its own seed so the area
//!   coordinate is decorrelated from both the sub-pixel and lens
//!   coordinates. All three go through the same `cp_hash` helper;
//!   the seed is the only difference between them.
//!
//! * `cranley_patterson_indirect_offset(x, y)` — the rotation for
//!   the path-tracing indirect-bounce sample (seed 3), keeping
//!   the indirect direction independent of all three other CP
//!   rotations.
//!
//! * `cosine_hemisphere_sample(u, v)` — turns a `[0, 1)²` point
//!   (typically a rotated `halton_indirect` value) into a
//!   cosine-weighted direction in the local upper hemisphere
//!   (z ≥ 0). Used by the indirect branch in `shade_pixel` to pick
//!   the next bounce direction; cosine-weighted because the
//!   Lambertian shading cancels the cosine factor in the rendering
//!   equation, leaving the unbiased estimator just `incoming *
//!   surface.color * surface.light` with no extra weighting.
//!
//! * `hemisphere_basis(normal)` — builds an orthonormal basis
//!   `(u, v)` perpendicular to a unit-length surface normal, so a
//!   local hemisphere sample can be oriented into world space.
//!   The third basis vector is `normal` itself.

use crate::render::geometry::{crossp, normalizep, Point};

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

/// The pair `(H_11(i), H_13(i))` — point `i` of the 2D Halton
/// sequence with bases 11 and 13, used for the area-light disk
/// sample. Returned values are in `[0, 1)²`.
///
/// Distinct bases from `halton_pair` (2, 3) and `halton_lens`
/// (5, 7) so the area coordinate is independent of both the
/// sub-pixel and lens coordinates of the same sample index — the
/// three sampled dimensions march independently. Bases 11 and 13
/// give slightly worse 2D distribution than the lower pairs, but
/// they're more than adequate for the per-pixel sample counts
/// here and the alternative (sharing a base, accepting
/// correlation) is worse. As with the other Halton helpers,
/// `i = 0` returns `(0, 0)`; callers pass `i >= 1`.
pub fn halton_area(i: u32) -> (f64, f64) {
    (radical_inverse(11, i), radical_inverse(13, i))
}

/// The pair `(H_17(i), H_19(i))` — point `i` of the 2D Halton
/// sequence with bases 17 and 19, used for the path-tracing
/// indirect-bounce sample in Phase 1 of the path-tracing plan.
/// Returned values are in `[0, 1)²`.
///
/// Distinct bases from `halton_pair` (2, 3), `halton_lens` (5, 7),
/// and `halton_area` (11, 13) so the indirect-bounce coordinate is
/// independent of the other three sampled dimensions at the same
/// sample index — four uncorrelated 2D streams driven by one
/// per-pixel sample counter. Bases 17 and 19 have somewhat coarser
/// distribution than the lower pairs but stay well-behaved for the
/// per-pixel sample counts the adaptive oversampler reaches in
/// practice. As with the other Halton helpers, `i = 0` returns
/// `(0, 0)`; callers pass `i >= 1`.
pub fn halton_indirect(i: u32) -> (f64, f64) {
    (radical_inverse(17, i), radical_inverse(19, i))
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

/// The Cranley-Patterson rotation for the area-light disk sample:
/// a per-pixel `(ox, oy)` offset in `[0, 1)²` applied to a
/// `halton_area` point before it's mapped onto the emitter disk.
///
/// Uses a distinct hash seed (`2`) from both
/// `cranley_patterson_offset` (`0`) and
/// `cranley_patterson_lens_offset` (`1`), so a pixel's area
/// rotation is uncorrelated with both its sub-pixel and lens
/// rotations — the three sampled dimensions stay independent.
pub fn cranley_patterson_area_offset(x: u32, y: u32) -> (f64, f64) {
    cp_hash(x, y, 2)
}

/// The Cranley-Patterson rotation for the path-tracing
/// indirect-bounce sample: a per-pixel `(ox, oy)` offset in
/// `[0, 1)²` applied to a `halton_indirect` point before it's
/// mapped to a hemisphere direction by `cosine_hemisphere_sample`.
///
/// Uses a distinct hash seed (`3`) from `cranley_patterson_offset`
/// (`0`), `cranley_patterson_lens_offset` (`1`), and
/// `cranley_patterson_area_offset` (`2`), so a pixel's indirect
/// rotation is uncorrelated with all three of its other rotations.
/// All four CP rotations go through the same `cp_hash` helper; the
/// seed is the only difference.
pub fn cranley_patterson_indirect_offset(x: u32, y: u32) -> (f64, f64) {
    cp_hash(x, y, 3)
}

/// Cosine-weighted sample of the local upper hemisphere (z ≥ 0),
/// returned as a unit vector `(x, y, z)`. Takes a `[0, 1)²` value
/// `(u, v)` — typically a Cranley-Patterson-rotated
/// `halton_indirect` point — and returns a direction whose
/// probability density is `cos(theta) / π`, where `theta` is the
/// angle from the +z axis.
///
/// Implemented via Malley's method: map `(u, v)` through
/// `concentric_disk` to a uniform unit-disk point `(dx, dy)`, then
/// lift to the hemisphere by setting `z = sqrt(1 - dx² - dy²)`.
/// Projecting a uniform disk sample onto the hemisphere produces
/// the cosine-weighted distribution for free, which is exactly the
/// distribution the rendering equation's Lambertian term wants —
/// the cosine factor cancels with the PDF and the indirect
/// contribution estimator collapses to `incoming * surface.color
/// * surface.light` with no extra weighting.
///
/// The returned vector is in the *local* hemisphere frame (z is
/// "up"); callers compose it with `hemisphere_basis(normal)` to
/// rotate the sample into world space.
///
/// A `(0.5, 0.5)` input lands at the pole `(0, 0, 1)`; the four
/// corners of the unit square land on the equator (`z ≈ 0`) at
/// the four cardinal directions.
pub fn cosine_hemisphere_sample(u: f64, v: f64) -> (f64, f64, f64) {
    let (dx, dy) = concentric_disk(u, v);
    // The disk is at most unit radius, so `1 - dx² - dy²` is in
    // `[0, 1]`. The `max(0, …)` guard handles the rim where
    // floating-point error might push the value slightly negative
    // (`concentric_disk_within_unit_disk` allows 1e-9 slack).
    let z = (1.0 - dx * dx - dy * dy).max(0.0).sqrt();
    (dx, dy, z)
}

/// A 1D Russian-roulette sample in `[0, 1)`, derived from the per-
/// pixel-sample 2D `indirect_coord` and the current indirect-bounce
/// `depth`. Used by the indirect branch in `shade_pixel` to decide
/// whether a path survives or terminates at this bounce.
///
/// The codebase's other per-pixel-sample coords (`light_coord`,
/// `indirect_coord` itself) are 2D and reused across recursion —
/// per-bounce decorrelation would require threading sampler state
/// through recursion proper. Russian roulette is more variance-
/// sensitive: if the same `u` is reused at every bounce, "lucky"
/// rays (small `u`) terminate immediately and "unlucky" rays (large
/// `u`) survive many bounces — predictable, correlated path
/// lengths. To get a *different* `u` at each depth without adding
/// a new threaded parameter, we offset the pixel-sample's `(u, v)`
/// by `depth * φ` (φ = golden ratio) and take the fractional
/// part. The golden ratio is the canonical low-discrepancy 1D
/// offset: successive multiples are well-distributed modulo 1
/// without periodicity, which is exactly what RR needs.
///
/// The base value mixes both `indirect_coord` components so the
/// RR sample isn't *bit-identical* to the bounce direction's u
/// coordinate at depth 0 (those decisions would still be
/// correlated, but not the same number).
pub fn rr_sample(indirect_coord: (f64, f64), depth: u32) -> f64 {
    // Golden ratio. Constant inlined rather than pulled from a
    // module because there's no `std::f64::consts::GOLDEN_RATIO`
    // (yet) and the value is short.
    const GOLDEN_RATIO: f64 = 1.618_033_988_749_895;
    let (iu, iv) = indirect_coord;
    let base = iu + iv * GOLDEN_RATIO;
    (base + depth as f64 * GOLDEN_RATIO).fract()
}

/// Build an orthonormal basis `(u, v)` perpendicular to a unit-
/// length surface `normal`. Used by the indirect-bounce branch to
/// rotate a local-hemisphere sample (in `(dx, dy, dz)` coordinates,
/// where dz is "up") into world space: the world-space direction
/// is `dx * u + dy * v + dz * normal`.
///
/// Mathematically identical to `disk_basis` in `render.rs` (and
/// the same Gram-Schmidt-via-cross-product trick to dodge the
/// degenerate case when `normal` lines up with a world axis): the
/// world axis least aligned with `normal` is chosen as the hint
/// vector, so the first cross product is always well-conditioned.
/// Kept separate from `disk_basis` because the two callers think
/// in different terms — one orients a *disk* (the third basis
/// vector is the disk's *axis*), the other orients a *hemisphere*
/// (the third basis vector is the surface *normal*) — and the
/// extra helper avoids forcing the path-tracing module to reach
/// into the renderer-internal `disk_basis`.
pub fn hemisphere_basis(normal: Point) -> (Point, Point) {
    let absx = normal[0].abs();
    let absy = normal[1].abs();
    let absz = normal[2].abs();
    let hint: Point = if absx <= absy && absx <= absz {
        [1.0, 0.0, 0.0]
    } else if absy <= absz {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let u = normalizep(crossp(normal, hint));
    let v = crossp(normal, u);
    (u, v)
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

    /// First few base-11 / base-13 radical inverses, the bases
    /// `halton_area` uses. H_11(1) = 1/11; H_13(1) = 1/13;
    /// H_11(2) = 2/11.
    #[test]
    fn halton_area_known_values() {
        let (x1, y1) = halton_area(1);
        assert!((x1 - 1.0 / 11.0).abs() < 1e-15, "H_11(1) = {}", x1);
        assert!((y1 - 1.0 / 13.0).abs() < 1e-15, "H_13(1) = {}", y1);
        let (x2, _) = halton_area(2);
        assert!((x2 - 2.0 / 11.0).abs() < 1e-15, "H_11(2) = {}", x2);
    }

    /// `halton_area` stays in `[0, 1)²` across a modest index range —
    /// same contract as `halton_pair` / `halton_lens`, since the
    /// area sample feeds the same Cranley-Patterson + concentric-
    /// disk pipeline before reaching the emitter.
    #[test]
    fn halton_area_in_unit_square() {
        for i in 0..1024 {
            let (a, b) = halton_area(i);
            assert!(a >= 0.0 && a < 1.0, "area x out of range at i={}: {}", i, a);
            assert!(b >= 0.0 && b < 1.0, "area y out of range at i={}: {}", i, b);
        }
    }

    /// The area-light Cranley-Patterson rotation is in range and is
    /// decorrelated from *both* the sub-pixel rotation and the lens
    /// rotation: for the same pixel, the area offset (seed=2) must
    /// differ from the pixel offset (seed=0) and the lens offset
    /// (seed=1). If any of these collided, two sampled dimensions
    /// would lock together and the soft-shadow / DOF / anti-
    /// aliasing samples would correlate when they shouldn't.
    #[test]
    fn area_cp_offset_distinct_from_pixel_and_lens() {
        for x in 0..64 {
            for y in 0..64 {
                let (a, b) = cranley_patterson_area_offset(x, y);
                assert!(a >= 0.0 && a < 1.0, "area CP x out of range: {}", a);
                assert!(b >= 0.0 && b < 1.0, "area CP y out of range: {}", b);
                assert_ne!(
                    cranley_patterson_area_offset(x, y),
                    cranley_patterson_offset(x, y),
                    "area and pixel CP offsets collided at ({}, {})",
                    x, y
                );
                assert_ne!(
                    cranley_patterson_area_offset(x, y),
                    cranley_patterson_lens_offset(x, y),
                    "area and lens CP offsets collided at ({}, {})",
                    x, y
                );
            }
        }
    }

    /// First few base-17 / base-19 radical inverses, the bases
    /// `halton_indirect` uses. H_17(1) = 1/17; H_19(1) = 1/19;
    /// H_17(2) = 2/17.
    #[test]
    fn halton_indirect_known_values() {
        let (x1, y1) = halton_indirect(1);
        assert!((x1 - 1.0 / 17.0).abs() < 1e-15, "H_17(1) = {}", x1);
        assert!((y1 - 1.0 / 19.0).abs() < 1e-15, "H_19(1) = {}", y1);
        let (x2, _) = halton_indirect(2);
        assert!((x2 - 2.0 / 17.0).abs() < 1e-15, "H_17(2) = {}", x2);
    }

    /// `halton_indirect` stays in `[0, 1)²` across a modest index
    /// range — same contract as the other Halton helpers, since the
    /// indirect sample feeds the same Cranley-Patterson rotation
    /// before reaching the hemisphere mapper.
    #[test]
    fn halton_indirect_in_unit_square() {
        for i in 0..1024 {
            let (a, b) = halton_indirect(i);
            assert!(a >= 0.0 && a < 1.0, "indirect x out of range at i={}: {}", i, a);
            assert!(b >= 0.0 && b < 1.0, "indirect y out of range at i={}: {}", i, b);
        }
    }

    /// The indirect-bounce Cranley-Patterson rotation is in range
    /// and is decorrelated from all three of the other CP rotations:
    /// the indirect offset (seed=3) must differ from the pixel
    /// offset (seed=0), the lens offset (seed=1), and the area
    /// offset (seed=2). Four uncorrelated sampled dimensions
    /// require four distinct rotations.
    #[test]
    fn indirect_cp_offset_distinct_from_others() {
        for x in 0..64 {
            for y in 0..64 {
                let (a, b) = cranley_patterson_indirect_offset(x, y);
                assert!(a >= 0.0 && a < 1.0, "indirect CP x out of range: {}", a);
                assert!(b >= 0.0 && b < 1.0, "indirect CP y out of range: {}", b);
                assert_ne!(
                    cranley_patterson_indirect_offset(x, y),
                    cranley_patterson_offset(x, y),
                    "indirect and pixel CP offsets collided at ({}, {})",
                    x, y
                );
                assert_ne!(
                    cranley_patterson_indirect_offset(x, y),
                    cranley_patterson_lens_offset(x, y),
                    "indirect and lens CP offsets collided at ({}, {})",
                    x, y
                );
                assert_ne!(
                    cranley_patterson_indirect_offset(x, y),
                    cranley_patterson_area_offset(x, y),
                    "indirect and area CP offsets collided at ({}, {})",
                    x, y
                );
            }
        }
    }

    /// `cosine_hemisphere_sample` returns a unit vector in the
    /// upper hemisphere (z ≥ 0) for every `[0, 1)²` input. If the
    /// hemisphere lift ever produced a negative z, the indirect
    /// ray would aim into the surface instead of away from it,
    /// which is wrong — and if the length drifted off unit, the
    /// world-space composition via `hemisphere_basis` would give
    /// a non-unit ray direction.
    #[test]
    fn cosine_hemisphere_unit_and_upper_half() {
        for ui in 0..64 {
            for vi in 0..64 {
                let u = ui as f64 / 64.0;
                let v = vi as f64 / 64.0;
                let (x, y, z) = cosine_hemisphere_sample(u, v);
                assert!(
                    z >= 0.0,
                    "cosine_hemisphere_sample({}, {}) = ({}, {}, {}), z < 0",
                    u, v, x, y, z
                );
                let len2 = x * x + y * y + z * z;
                // The sample lies on the hemisphere; small slack
                // for floating-point error.
                assert!(
                    (len2 - 1.0).abs() < 1e-9,
                    "cosine_hemisphere_sample({}, {}) = ({}, {}, {}), |.|² = {}",
                    u, v, x, y, z, len2
                );
            }
        }
    }

    /// `(0.5, 0.5)` — the centre of the unit square — corresponds
    /// to the disk centre under `concentric_disk`, which lifts to
    /// the hemisphere pole `(0, 0, 1)`. Pole-direction samples are
    /// the most-probable outcome under cosine weighting, so this
    /// is a useful landmark check.
    #[test]
    fn cosine_hemisphere_centre_at_pole() {
        let (x, y, z) = cosine_hemisphere_sample(0.5, 0.5);
        assert!(x.abs() < 1e-15, "centre x = {}", x);
        assert!(y.abs() < 1e-15, "centre y = {}", y);
        assert!((z - 1.0).abs() < 1e-15, "centre z = {}", z);
    }

    /// `hemisphere_basis(normal)` returns an orthonormal basis
    /// perpendicular to `normal`: each basis vector is unit-length
    /// and the three vectors are mutually orthogonal. Verified for
    /// several normals including the world axes (the cases the
    /// "least-aligned hint" trick was added to handle).
    #[test]
    fn hemisphere_basis_is_orthonormal() {
        use crate::render::geometry::{dotp, lenp};
        let normals = [
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0 / 3f64.sqrt(), 1.0 / 3f64.sqrt(), 1.0 / 3f64.sqrt()],
            [0.6, 0.8, 0.0],
        ];
        for &n in &normals {
            let (u, v) = hemisphere_basis(n);
            assert!((lenp(u) - 1.0).abs() < 1e-12, "u not unit: {:?}, len={}", u, lenp(u));
            assert!((lenp(v) - 1.0).abs() < 1e-12, "v not unit: {:?}, len={}", v, lenp(v));
            assert!(dotp(u, v).abs() < 1e-12, "u·v not zero: {:?} · {:?} = {}", u, v, dotp(u, v));
            assert!(dotp(u, n).abs() < 1e-12, "u·n not zero for n={:?}", n);
            assert!(dotp(v, n).abs() < 1e-12, "v·n not zero for n={:?}", n);
        }
    }

    /// `rr_sample` always returns a value in `[0, 1)` for any
    /// `indirect_coord` in `[0, 1)²` and any depth. If a value
    /// escaped the range, the Russian-roulette survival test in
    /// `shade_pixel` would behave nonsensically (a negative `u` is
    /// always less than `p`, surviving guaranteed; a `u >= 1` is
    /// always greater, terminating guaranteed).
    #[test]
    fn rr_sample_in_unit_interval() {
        for ui in 0..16 {
            for vi in 0..16 {
                let u = ui as f64 / 16.0;
                let v = vi as f64 / 16.0;
                for depth in 0..32 {
                    let s = rr_sample((u, v), depth);
                    assert!(
                        s >= 0.0 && s < 1.0,
                        "rr_sample(({}, {}), {}) = {} out of [0, 1)",
                        u, v, depth, s
                    );
                }
            }
        }
    }

    /// `rr_sample` decorrelates per bounce: at the same pixel
    /// sample (`indirect_coord` fixed), consecutive depths return
    /// distinct values. If the golden-ratio depth offset were
    /// degenerate (e.g. an integer φ, or 0), the same `u` would
    /// recur every bounce and paths would terminate-or-survive in
    /// lockstep — the variance behavior Russian roulette
    /// specifically tries to avoid.
    #[test]
    fn rr_sample_distinct_across_depths() {
        for ui in 0..16 {
            for vi in 0..16 {
                let u = ui as f64 / 16.0;
                let v = vi as f64 / 16.0;
                // Take rr samples for depths 0..8 and check they're
                // all distinct. Equality (within f64 tolerance) at
                // any pair would indicate a degenerate offset.
                let samples: Vec<f64> = (0..8u32)
                    .map(|d| rr_sample((u, v), d))
                    .collect();
                for i in 0..samples.len() {
                    for j in (i + 1)..samples.len() {
                        assert!(
                            (samples[i] - samples[j]).abs() > 1e-12,
                            "rr_sample(({}, {}), depths {} and {}) collide: {} == {}",
                            u, v, i, j, samples[i], samples[j]
                        );
                    }
                }
            }
        }
    }

    /// `rr_sample` decorrelates across pixel samples too: at the
    /// same depth, different `indirect_coord` values give
    /// different results. The base value `iu + iv * φ` is a
    /// well-known irrational-ratio mixer; this test guards against
    /// a future refactor that accidentally degenerates the base
    /// (e.g. dropping `iv`).
    #[test]
    fn rr_sample_distinct_across_pixel_samples() {
        // Two adjacent pixel-sample coordinates at depth 0.
        let a = rr_sample((0.1, 0.2), 0);
        let b = rr_sample((0.1, 0.3), 0);
        let c = rr_sample((0.2, 0.2), 0);
        assert!((a - b).abs() > 1e-6, "rr_sample varying iv collides: {} == {}", a, b);
        assert!((a - c).abs() > 1e-6, "rr_sample varying iu collides: {} == {}", a, c);
    }
}
