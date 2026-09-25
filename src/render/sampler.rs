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
//! * `halton_lens(i)` — Halton on bases 5 and 7, *scrambled* with
//!   Faure's digit permutations, used for the depth-of-field aperture
//!   sample. A separate pair of bases keeps the lens coordinate from
//!   being correlated with the sub-pixel coordinate of the same sample
//!   index. Unscrambled, the first four points were `(i/5, i/7)`, a
//!   line across the aperture; the scrambling breaks that up.
//!
//! * `area_sample(i)` — the area-light sample (a point on a disk or
//!   quad emitter). This one is the R2 sequence rather than Halton:
//!   Halton on bases 11 and 13, used before, puts its first ten or so
//!   points along the square's diagonal (`(i/11, i/13)`), so a pixel
//!   that stopped after a few samples had sampled a line across the
//!   light rather than its area. R2 spreads even its first four
//!   points over all four quadrants.
//!
//! * `halton_indirect(i)` — bases 17 and 19, Faure-scrambled like
//!   the lens sample, used for the cosine-weighted hemisphere sample
//!   at a diffuse hit. A fourth distinct stream, so the
//!   indirect-bounce direction is uncorrelated with the other three
//!   sampled dimensions (sub-pixel, lens, area-light) at the same
//!   sample index.
//!
//! * `concentric_disk(u, v)` — maps a `[0, 1)²` point (e.g. a
//!   rotated `halton_lens` value or `area_sample` value) onto the
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

/// `radical_inverse` with each base-`base` digit `d` replaced by
/// `perm[d]` before it's placed after the point. `perm` must be a
/// permutation of `0..base` that keeps `0` fixed, so the implicit
/// trailing zeros stay zero.
fn scrambled_radical_inverse(base: u32, perm: &[u32], mut i: u32) -> f64 {
    debug_assert!(perm.len() == base as usize && perm[0] == 0);
    let inv_b = 1.0 / base as f64;
    let mut result = 0.0;
    let mut f = inv_b;
    while i > 0 {
        result += perm[(i % base) as usize] as f64 * f;
        i /= base;
        f *= inv_b;
    }
    result
}

/// Faure's digit permutations for bases 5 and 7.
const FAURE_5: [u32; 5] = [0, 3, 2, 1, 4];
const FAURE_7: [u32; 7] = [0, 2, 5, 3, 1, 4, 6];

/// Point `i` of the 2D Halton sequence on bases 5 and 7, scrambled
/// with Faure's permutations, used for the depth-of-field aperture
/// sample. Returned values are in `[0, 1)²`.
///
/// Distinct bases from `halton_pair` so that, for a given sample
/// index `i`, the lens coordinate and the sub-pixel coordinate are
/// drawn from different sequences and aren't correlated. Together
/// they're still the 4D Halton sequence, which is low-discrepancy in
/// all four dimensions at once.
///
/// Why scrambled: for `i` below 5, the plain radical inverses are
/// exactly `(i/5, i/7)`, a line across the aperture, so a pixel that
/// stopped after 4 samples had sampled a line across the lens.
/// Permuting the digits keeps every stratification property of the
/// sequence (each digit position still takes every value equally
/// often) but moves those early points off the line. On the depth of
/// field test scene at its own settings, the error against a
/// 1024-sample reference fell by about a fifth for the same number of
/// samples.
///
/// Not R2, which `area_sample` uses: R2 is spread slightly better in
/// 2D, but using it for both would tie each pixel's lens point to its
/// light point (they'd differ by a fixed offset), so a scene with
/// both depth of field and an area light would never explore the
/// combinations and wouldn't converge to the right image.
///
/// As with `halton_pair`, `i = 0` returns `(0, 0)` (a corner);
/// callers pass `i >= 1`.
pub fn halton_lens(i: u32) -> (f64, f64) {
    (
        scrambled_radical_inverse(5, &FAURE_5, i),
        scrambled_radical_inverse(7, &FAURE_7, i),
    )
}

/// Point `i` of the R2 sequence (Martin Roberts, 2018), used for the
/// area-light sample. Returned values are in `[0, 1)²`.
///
/// R2 is the 2D generalization of the golden-ratio sequence: point
/// `i` is `(0.5 + i / g, 0.5 + i / g²) mod 1`, where `g` is the
/// plastic number (the real root of `x³ = x + 1`). Every prefix of
/// it is evenly spread, however short, which is what an adaptively
/// terminated pixel needs: a pixel that stops at 4 samples has
/// sampled one point in each quadrant of the light.
///
/// It replaced a Halton pair on bases 11 and 13. For `i` below 11 that
/// pair is exactly `(i/11, i/13)`, a line along the diagonal, so short
/// runs sampled a line across the light. On the soft-shadow test
/// scene, at the scene's own sample settings, R2 cut the error against
/// a 1024-sample reference by about two thirds for about 5% more
/// samples.
///
/// Its lattice structure is unrelated to Halton's, so the area
/// coordinate stays independent of the sub-pixel and lens coordinates
/// of the same sample index. Callers pass `i >= 1`, like the Halton
/// helpers.
pub fn area_sample(i: u32) -> (f64, f64) {
    // The plastic number and its square.
    const G: f64 = 1.324_717_957_244_746;
    const A1: f64 = 1.0 / G;
    const A2: f64 = 1.0 / (G * G);
    let n = i as f64;
    ((0.5 + n * A1).fract(), (0.5 + n * A2).fract())
}

/// Faure's digit permutations for bases 17 and 19.
const FAURE_17: [u32; 17] = [0, 9, 4, 13, 2, 11, 6, 15, 8, 1, 10, 5, 14, 3, 12, 7, 16];
const FAURE_19: [u32; 19] = [0, 11, 4, 15, 8, 2, 13, 6, 17, 9, 1, 12, 5, 16, 10, 3, 14, 7, 18];

/// Point `i` of the 2D Halton sequence on bases 17 and 19, scrambled
/// with Faure's permutations, used for the path-tracing
/// indirect-bounce sample. Returned values are in `[0, 1)²`.
///
/// Distinct bases from `halton_pair` (2, 3) and `halton_lens` (5, 7),
/// and unrelated to `area_sample` (R2), so the indirect-bounce
/// coordinate is independent of the other three sampled dimensions
/// at the same sample index.
///
/// Scrambled for the same reason as `halton_lens`, and it matters more
/// here. Unscrambled, the first 16 points are `(i/17, i/19)`, a line
/// across the square, so every pixel of a GI scene that took 16 or
/// fewer samples bounced its indirect rays along a line of directions.
/// Measured against high-sample references at 4–16 samples, the
/// scrambling cut the error by about 40% on `gi_test` and 25% on
/// `cornell_box`. At those scenes' own settings (hundreds of samples)
/// the gain is small.
///
/// As with the other Halton helpers, `i = 0` returns `(0, 0)`;
/// callers pass `i >= 1`.
pub fn halton_indirect(i: u32) -> (f64, f64) {
    (
        scrambled_radical_inverse(17, &FAURE_17, i),
        scrambled_radical_inverse(19, &FAURE_19, i),
    )
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
/// `area_sample` point before it's mapped onto the emitter.
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

    /// First few scrambled base-5 / base-7 radical inverses. Digit 1
    /// maps to 3 in base 5 and to 2 in base 7, so point 1 is
    /// (3/5, 2/7); digit 2 maps to 2 and to 5, so point 2 is
    /// (2/5, 5/7). Index 6 is `11` in base 5, giving 3/5 + 3/25.
    #[test]
    fn halton_lens_known_values() {
        let (x1, y1) = halton_lens(1);
        assert!((x1 - 0.6).abs() < 1e-15, "lens(1).x = {}", x1);
        assert!((y1 - 2.0 / 7.0).abs() < 1e-15, "lens(1).y = {}", y1);
        let (x2, y2) = halton_lens(2);
        assert!((x2 - 0.4).abs() < 1e-15, "lens(2).x = {}", x2);
        assert!((y2 - 5.0 / 7.0).abs() < 1e-15, "lens(2).y = {}", y2);
        let (x6, _) = halton_lens(6);
        assert!((x6 - (0.6 + 3.0 / 25.0)).abs() < 1e-15, "lens(6).x = {}", x6);
    }

    /// The first four lens points aren't on a line (the unscrambled
    /// pair's were: `(i/5, i/7)`), and like any Halton prefix of
    /// length `b`, the first five points take five different values
    /// of the base-5 coordinate's leading digit.
    #[test]
    fn halton_lens_prefix_is_not_a_line() {
        let p: Vec<(f64, f64)> = (1..=4).map(halton_lens).collect();
        let area = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
            ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)).abs() / 2.0
        };
        assert!(area(p[0], p[1], p[2]) > 0.01, "points 1-3 collinear: {:?}", p);
        assert!(area(p[0], p[1], p[3]) > 0.01, "points 1, 2, 4 collinear: {:?}", p);
        let mut xs: Vec<u32> = (1..=5).map(|i| (halton_lens(i).0 * 5.0 + 1e-9).floor() as u32).collect();
        xs.sort();
        assert_eq!(xs, vec![0, 1, 2, 3, 4]);
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

    /// R2's first point: `0.5 + 1/g` and `0.5 + 1/g²`, mod 1.
    #[test]
    fn area_sample_known_values() {
        let (x1, y1) = area_sample(1);
        assert!((x1 - 0.254_877_666_246_692_7).abs() < 1e-12, "R2(1).x = {}", x1);
        assert!((y1 - 0.069_840_290_998_053_3).abs() < 1e-12, "R2(1).y = {}", y1);
    }

    /// Short prefixes are spread out: the first four points land in
    /// four different quadrants, and each quadrant gets between two
    /// and six of the first sixteen. The Halton (11, 13) pair this
    /// replaced fails both, since its first ten points all lie on the
    /// diagonal.
    #[test]
    fn area_sample_prefixes_are_spread() {
        let quadrant = |(u, v): (f64, f64)| (u >= 0.5) as usize * 2 + (v >= 0.5) as usize;
        let mut seen = [false; 4];
        for i in 1..=4 {
            seen[quadrant(area_sample(i))] = true;
        }
        assert!(seen.iter().all(|s| *s), "first four points: {:?}", seen);
        let mut counts = [0; 4];
        for i in 1..=16 {
            counts[quadrant(area_sample(i))] += 1;
        }
        assert!(counts.iter().all(|c| (2..=6).contains(c)), "first sixteen: {:?}", counts);
    }

    /// `area_sample` stays in `[0, 1)²` across a modest index range —
    /// same contract as `halton_pair` / `halton_lens`, since the
    /// area sample feeds the same Cranley-Patterson + concentric-
    /// disk pipeline before reaching the emitter.
    #[test]
    fn area_sample_in_unit_square() {
        for i in 0..1024 {
            let (a, b) = area_sample(i);
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

    /// First scrambled base-17 / base-19 radical inverses: digit 1
    /// maps to 9 and to 11, digit 2 to 4 and to 4.
    #[test]
    fn halton_indirect_known_values() {
        let (x1, y1) = halton_indirect(1);
        assert!((x1 - 9.0 / 17.0).abs() < 1e-15, "indirect(1).x = {}", x1);
        assert!((y1 - 11.0 / 19.0).abs() < 1e-15, "indirect(1).y = {}", y1);
        let (x2, y2) = halton_indirect(2);
        assert!((x2 - 4.0 / 17.0).abs() < 1e-15, "indirect(2).x = {}", x2);
        assert!((y2 - 4.0 / 19.0).abs() < 1e-15, "indirect(2).y = {}", y2);
    }

    /// Faure's construction: the base-2 permutation is the identity; an
    /// even base `b` doubles the permutation for `b/2` and appends the
    /// same doubled values plus one; an odd base `b` takes the
    /// permutation for `b-1`, bumps every value at or above the middle
    /// `c = (b-1)/2` and inserts `c` in the middle.
    fn faure_permutation(b: u32) -> Vec<u32> {
        if b == 2 {
            vec![0, 1]
        } else if b % 2 == 0 {
            let half = faure_permutation(b / 2);
            half.iter().map(|x| 2 * x).chain(half.iter().map(|x| 2 * x + 1)).collect()
        } else {
            let c = (b - 1) / 2;
            let mut p: Vec<u32> = faure_permutation(b - 1)
                .into_iter()
                .map(|x| if x >= c { x + 1 } else { x })
                .collect();
            p.insert(c as usize, c);
            p
        }
    }

    /// The hard-coded tables are Faure's permutations.
    #[test]
    fn faure_tables_match_the_construction() {
        assert_eq!(FAURE_5.to_vec(), faure_permutation(5));
        assert_eq!(FAURE_7.to_vec(), faure_permutation(7));
        assert_eq!(FAURE_17.to_vec(), faure_permutation(17));
        assert_eq!(FAURE_19.to_vec(), faure_permutation(19));
    }

    /// The unscrambled pair's first sixteen points lie on a line
    /// (`(i/17, i/19)`, correlation 1). The scrambled ones are
    /// essentially uncorrelated, and every quadrant gets between two
    /// and six of them.
    #[test]
    fn halton_indirect_prefix_is_spread() {
        let p: Vec<(f64, f64)> = (1..=16).map(halton_indirect).collect();
        let n = p.len() as f64;
        let (mx, my) = (p.iter().map(|q| q.0).sum::<f64>() / n, p.iter().map(|q| q.1).sum::<f64>() / n);
        let cov: f64 = p.iter().map(|q| (q.0 - mx) * (q.1 - my)).sum();
        let vx: f64 = p.iter().map(|q| (q.0 - mx).powi(2)).sum();
        let vy: f64 = p.iter().map(|q| (q.1 - my).powi(2)).sum();
        let r = cov / (vx * vy).sqrt();
        assert!(r.abs() < 0.3, "first sixteen points correlated: r = {}", r);
        let quadrant = |(u, v): (f64, f64)| (u >= 0.5) as usize * 2 + (v >= 0.5) as usize;
        let mut counts = [0; 4];
        for i in 1..=16 {
            counts[quadrant(halton_indirect(i))] += 1;
        }
        assert!(counts.iter().all(|c| (2..=6).contains(c)), "first sixteen: {:?}", counts);
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
