// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! The view transform: how the renderer's linear, unbounded pixel
//! values become display values.
//!
//! Two stages, kept separate (see "View transform (tone mapping):
//! implementation plan" in CLAUDE.md):
//!
//! 1. **View transform** (`ViewTransform::apply`): scene-linear to
//!    display-linear in `[0, 1]`. Exposure (a multiplier of
//!    `2^exposure`), then a `ToneCurve`. An artistic choice, set per
//!    scene.
//! 2. **Encoding** (`ViewTransform::encode`): display-linear to 8-bit
//!    sRGB, the same for every image.
//!
//! The default, `Clip` at exposure 0, is the renderer's original
//! behaviour, byte for byte.
//!
//! The transform also has a wire form (`write_wire` / `read_wire`), so
//! `StreamTarget` can send it with the (still scene-linear) pixels and
//! `rtview_receiver` can apply it at the far end.

use std::io::{self, ErrorKind, Read};

use super::color::LinearColor;

/// How exposed colours are brought into `[0, 1]`.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ToneCurve {
    /// Clamp each channel to `[0, 1]` on its own. The original
    /// behaviour. Bright colours shift hue (an over-bright orange turns
    /// yellow) and flatten to white.
    Clip,
    /// Clamp negatives to 0, then, if the largest channel is over 1,
    /// divide all three by it. Keeps hue and saturation exactly; still
    /// flattens highlights.
    HueClip,
    /// Extended Reinhard on luminance: `L' = L (1 + L / white²) / (1 +
    /// L)`, with the colour scaled by `L' / L`. Never clips luminance
    /// (it reaches 1 exactly at `white`), and keeps hue; `HueClip`
    /// catches a saturated colour whose channels still go over 1.
    /// Flattens contrast in the mid-tones unless exposure is raised.
    Reinhard { white: f64 },
    /// AgX, Blender's default view transform since 4.0, in the
    /// analytic form three.js and Filament use: sRGB to Rec. 2020, the
    /// AgX inset matrix, a log2 encoding over [-12.47, 4.03] stops, a
    /// polynomial fit of the AgX sigmoid, the outset matrix, a 2.2
    /// power back to linear, and Rec. 2020 back to sRGB. Bright colours
    /// fade gradually toward white without skewing hue. Base look only.
    AgX,
}

/// Reinhard's default white point: luminance 4 (two stops over 1) maps
/// to 1.
pub const DEFAULT_REINHARD_WHITE: f64 = 4.0;

// ---------------------------------------------------------------------
// AgX constants.
//
// The inset and outset matrices, the EV range and the sigmoid
// polynomial are from three.js's `AgXToneMapping`
// (src/renderers/shaders/ShaderChunk/tonemapping_pars_fragment.glsl.js,
// MIT licence), which cites Filament's implementation
// (github.com/google/filament/pull/7236, Apache 2.0) and the "minimal
// AgX" write-up (iolite-engine.com/blog_posts/minimal_agx_implementation),
// both derived from Troy Sobotka's AgX and EaryChow's AgX_LUT_Gen. The
// matrices are written here as rows (each row sums to 1, so white
// stays white; `agx_matrices_preserve_white` checks it).
//
// The sRGB <-> Rec. 2020 matrices are computed from the two standards'
// primaries and the D65 white point (three.js rounds them to four
// places).
// ---------------------------------------------------------------------

const SRGB_TO_REC2020: [[f64; 3]; 3] = [
    [0.627403895934699, 0.32928303837788375, 0.04331306568741722],
    [0.06909728935823198, 0.9195403950754586, 0.011362315566309171],
    [0.01639143887515023, 0.08801330787722578, 0.895595253247624],
];

const REC2020_TO_SRGB: [[f64; 3]; 3] = [
    [1.6604910021084343, -0.5876411387885497, -0.07284986331988486],
    [-0.1245504745215906, 1.1328998971259605, -0.00834942260436948],
    [-0.018150763354905224, -0.10057889800800744, 1.1187296613629125],
];

const AGX_INSET: [[f64; 3]; 3] = [
    [0.856627153315983, 0.0951212405381588, 0.0482516061458583],
    [0.137318972929847, 0.761241990602591, 0.101439036467562],
    [0.11189821299995, 0.0767994186031903, 0.811302368396859],
];

const AGX_OUTSET: [[f64; 3]; 3] = [
    [1.1271005818144368, -0.11060664309660323, -0.016493938717834573],
    [-0.1413297634984383, 1.157823702216272, -0.016493938717834257],
    [-0.14132976349843826, -0.11060664309660294, 1.2519364065950405],
];

const AGX_MIN_EV: f64 = -12.47393;
const AGX_MAX_EV: f64 = 4.026069;

fn mat_mul(m: &[[f64; 3]; 3], c: LinearColor) -> LinearColor {
    [
        m[0][0] * c[0] + m[0][1] * c[1] + m[0][2] * c[2],
        m[1][0] * c[0] + m[1][1] * c[1] + m[1][2] * c[2],
        m[2][0] * c[0] + m[2][1] * c[1] + m[2][2] * c[2],
    ]
}

/// The polynomial fit of AgX's default-contrast sigmoid, on `[0, 1]`.
fn agx_contrast(x: f64) -> f64 {
    let x2 = x * x;
    let x4 = x2 * x2;
    15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4 - 6.868 * x2 * x + 0.4298 * x2 + 0.1191 * x
        - 0.00232
}

fn agx(c: LinearColor) -> LinearColor {
    let c = mat_mul(&AGX_INSET, mat_mul(&SRGB_TO_REC2020, c));
    let mut v = [0.0; 3];
    for i in 0..3 {
        // `max` also turns NaN into the floor.
        let e = (c[i].max(1e-10).log2() - AGX_MIN_EV) / (AGX_MAX_EV - AGX_MIN_EV);
        v[i] = agx_contrast(e.clamp(0.0, 1.0));
    }
    let v = mat_mul(&AGX_OUTSET, v);
    let v = [v[0].max(0.0).powf(2.2), v[1].max(0.0).powf(2.2), v[2].max(0.0).powf(2.2)];
    let v = mat_mul(&REC2020_TO_SRGB, v);
    [v[0].clamp(0.0, 1.0), v[1].clamp(0.0, 1.0), v[2].clamp(0.0, 1.0)]
}

/// Rec. 709 / sRGB luminance of a linear colour.
fn luminance(c: LinearColor) -> f64 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn hue_clip(c: LinearColor) -> LinearColor {
    let c = [nonneg(c[0]), nonneg(c[1]), nonneg(c[2])];
    let m = c[0].max(c[1]).max(c[2]);
    if m > 1.0 {
        [c[0] / m, c[1] / m, c[2] / m]
    } else {
        c
    }
}

fn reinhard(c: LinearColor, white: f64) -> LinearColor {
    let c = [nonneg(c[0]), nonneg(c[1]), nonneg(c[2])];
    let l = luminance(c);
    if l <= 0.0 {
        return [0.0, 0.0, 0.0];
    }
    let mapped = l * (1.0 + l / (white * white)) / (1.0 + l);
    let k = mapped / l;
    hue_clip([c[0] * k, c[1] * k, c[2] * k])
}

impl ToneCurve {
    /// Every curve, for listing in messages.
    pub const NAMES: [&'static str; 4] = ["clip", "hue-clip", "reinhard", "agx"];

    /// The curve's name in the SDL (`:clip`) and in `RAYTRACER_CURVE`.
    pub fn name(&self) -> &'static str {
        match self {
            ToneCurve::Clip => "clip",
            ToneCurve::HueClip => "hue-clip",
            ToneCurve::Reinhard { .. } => "reinhard",
            ToneCurve::AgX => "agx",
        }
    }

    /// The curve for a name, with default parameters (Reinhard's white
    /// point is `DEFAULT_REINHARD_WHITE`), or `None` if there's no such
    /// curve.
    pub fn from_name(name: &str) -> Option<ToneCurve> {
        match name {
            "clip" => Some(ToneCurve::Clip),
            "hue-clip" => Some(ToneCurve::HueClip),
            "reinhard" => Some(ToneCurve::Reinhard { white: DEFAULT_REINHARD_WHITE }),
            "agx" => Some(ToneCurve::AgX),
            _ => None,
        }
    }

    /// The curve's id on the wire.
    fn wire_id(&self) -> u32 {
        match self {
            ToneCurve::Clip => 0,
            ToneCurve::HueClip => 1,
            ToneCurve::Reinhard { .. } => 2,
            ToneCurve::AgX => 3,
        }
    }

    /// The curve's parameters on the wire: Reinhard's white point.
    fn wire_params(&self) -> Vec<f32> {
        match self {
            ToneCurve::Reinhard { white } => vec![*white as f32],
            _ => Vec::new(),
        }
    }

    fn from_wire(id: u32, params: &[f32]) -> io::Result<ToneCurve> {
        let curve = match id {
            0 => ToneCurve::Clip,
            1 => ToneCurve::HueClip,
            2 => match params {
                [white] if white.is_finite() && *white > 0.0 => {
                    ToneCurve::Reinhard { white: *white as f64 }
                }
                _ => return Err(invalid(format!("bad reinhard parameters {:?}", params))),
            },
            3 => ToneCurve::AgX,
            other => return Err(invalid(format!("unknown tone curve id {}", other))),
        };
        if params.len() != curve.wire_params().len() {
            return Err(invalid(format!(
                "tone curve {} takes {} parameters, got {}",
                curve.name(),
                curve.wire_params().len(),
                params.len()
            )));
        }
        Ok(curve)
    }

    /// Map an exposed colour into `[0, 1]`.
    pub fn apply(&self, c: LinearColor) -> LinearColor {
        match self {
            ToneCurve::Clip => [clamp01(c[0]), clamp01(c[1]), clamp01(c[2])],
            ToneCurve::HueClip => hue_clip(c),
            ToneCurve::Reinhard { white } => reinhard(c, *white),
            ToneCurve::AgX => agx(c),
        }
    }
}

/// Exposure then a tone curve. See the module docs.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ViewTransform {
    /// In stops: colours are multiplied by `2^exposure` before the
    /// curve. 0 leaves them alone, -1 halves them.
    pub exposure: f64,
    pub curve: ToneCurve,
}

impl Default for ViewTransform {
    fn default() -> Self {
        ViewTransform { exposure: 0.0, curve: ToneCurve::Clip }
    }
}

/// Flags bit in the stream header meaning "a view-transform block
/// follows the header".
pub const WIRE_FLAG_VIEW: u32 = 1;

impl ViewTransform {
    /// Whether this is the default (the original behaviour).
    pub fn is_default(&self) -> bool {
        *self == ViewTransform::default()
    }

    /// The colour after exposure, before the curve: what the clip report
    /// and clip map measure. Exposure 0 returns `c` unchanged.
    pub fn expose(&self, c: LinearColor) -> LinearColor {
        if self.exposure == 0.0 {
            return c;
        }
        let k = self.exposure.exp2();
        [c[0] * k, c[1] * k, c[2] * k]
    }

    /// Stage 1: scene-linear to display-linear in `[0, 1]`.
    pub fn apply(&self, c: LinearColor) -> LinearColor {
        self.curve.apply(self.expose(c))
    }

    /// Both stages: scene-linear to 8-bit sRGB.
    pub fn encode(&self, c: LinearColor) -> [u8; 3] {
        encode_display(self.apply(c))
    }

    /// Stage 2 for a colour that has already been exposed (so the
    /// caller can measure it first): the curve, then sRGB.
    pub fn encode_exposed(&self, exposed: LinearColor) -> [u8; 3] {
        encode_display(self.curve.apply(exposed))
    }

    /// The view-transform block for the stream: curve id (`u32`; 0
    /// clip, 1 hue-clip, 2 reinhard, 3 agx),
    /// exposure (`f32`), parameter count (`u32`), then the parameters
    /// (`f32` each), all little-endian.
    pub fn write_wire(&self, out: &mut Vec<u8>) {
        let params = self.curve.wire_params();
        out.extend_from_slice(&self.curve.wire_id().to_le_bytes());
        out.extend_from_slice(&(self.exposure as f32).to_le_bytes());
        out.extend_from_slice(&(params.len() as u32).to_le_bytes());
        for p in params {
            out.extend_from_slice(&p.to_le_bytes());
        }
    }

    /// Read a block written by `write_wire`.
    pub fn read_wire<R: Read>(r: &mut R) -> io::Result<ViewTransform> {
        let id = read_u32(r)?;
        let exposure = read_f32(r)? as f64;
        let n = read_u32(r)?;
        if n > 16 {
            return Err(invalid(format!("implausible tone curve parameter count {}", n)));
        }
        let mut params = Vec::with_capacity(n as usize);
        for _ in 0..n {
            params.push(read_f32(r)?);
        }
        Ok(ViewTransform { exposure, curve: ToneCurve::from_wire(id, &params)? })
    }
}

/// The sRGB transfer curve (IEC 61966-2-1), clamping to `[0, 1]`.
pub fn linear_to_srgb(x: f64) -> f64 {
    if x < 0.0 {
        0.0
    } else if x < 0.0031308 {
        x * 12.92
    } else if x < 1.0 {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    } else {
        1.0
    }
}

/// Stage 2: display-linear to 8-bit sRGB. `[0, 1]` maps onto 256 equal
/// bins (1.0 lands in the top one).
pub fn encode_display(c: LinearColor) -> [u8; 3] {
    [
        (linear_to_srgb(c[0]) * 256.0) as u8,
        (linear_to_srgb(c[1]) * 256.0) as u8,
        (linear_to_srgb(c[2]) * 256.0) as u8,
    ]
}

/// Clamp to `[0, 1]`, exactly as the original encoder did: NaN falls
/// through both comparisons and comes out as 1.0. (It shouldn't occur;
/// the point is byte-identity.)
fn clamp01(x: f64) -> f64 {
    if x < 0.0 {
        0.0
    } else if x < 1.0 {
        x
    } else {
        1.0
    }
}

fn nonneg(x: f64) -> f64 {
    if x > 0.0 { x } else { 0.0 }
}

fn invalid(msg: String) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, msg)
}

fn read_u32<R: Read>(r: &mut R) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_f32<R: Read>(r: &mut R) -> io::Result<f32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(f32::from_le_bytes(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The encoder as it was before view transforms, kept here as the
    /// reference for byte-identity.
    fn legacy_encode(c: LinearColor) -> [u8; 3] {
        let f = |x: f64| {
            let v = if x < 0.0 {
                0.0
            } else if x < 0.0031308 {
                x * 12.92
            } else if x < 1.0 {
                1.055 * x.powf(1.0 / 2.4) - 0.055
            } else {
                1.0
            };
            (v * 256.0) as u8
        };
        [f(c[0]), f(c[1]), f(c[2])]
    }

    fn sweep() -> Vec<f64> {
        let mut v: Vec<f64> = (0..=3000).map(|i| i as f64 / 1000.0).collect();
        v.extend([-1.0, -0.0, 1e-9, 0.0031307, 0.0031308, 0.999_999_9, 1.0, 1.000_000_1, 1e9, f64::NAN]);
        v
    }

    #[test]
    fn default_clip_matches_the_old_encoder() {
        let view = ViewTransform::default();
        assert!(view.is_default());
        for x in sweep() {
            for c in [[x, 0.5, 0.25], [0.1, x, 2.0], [x, x, x]] {
                assert_eq!(view.encode(c), legacy_encode(c), "at {:?}", c);
            }
        }
    }

    #[test]
    fn exposure_scales_by_powers_of_two() {
        let c = [0.25, 0.5, 3.0];
        let v = |exposure| ViewTransform { exposure, curve: ToneCurve::Clip };
        assert_eq!(v(0.0).expose(c), c);
        assert_eq!(v(1.0).expose(c), [0.5, 1.0, 6.0]);
        assert_eq!(v(-2.0).expose(c), [0.0625, 0.125, 0.75]);
        // Exposure happens before the curve: -2 stops brings 3.0 into
        // range, so nothing clips.
        assert_eq!(v(-2.0).apply(c), [0.0625, 0.125, 0.75]);
        assert_eq!(v(0.0).apply(c), [0.25, 0.5, 1.0]);
        assert!(!v(-2.0).is_default());
    }

    #[test]
    fn clip_clamps_each_channel() {
        assert_eq!(ToneCurve::Clip.apply([1.8, 0.9, 0.3]), [1.0, 0.9, 0.3]);
        assert_eq!(ToneCurve::Clip.apply([-0.5, f64::NAN, 0.5]), [0.0, 1.0, 0.5]);
    }

    #[test]
    fn hue_clip_preserves_ratios() {
        // The over-bright orange that plain clipping turns yellow.
        let out = ToneCurve::HueClip.apply([1.8, 0.9, 0.3]);
        assert_eq!(out[0], 1.0);
        assert!((out[1] - 0.5).abs() < 1e-15 && (out[2] - 1.0 / 6.0).abs() < 1e-15, "{:?}", out);
        for c in [[3.0, 2.0, 1.0], [0.2, 7.0, 0.7], [5.0, 5.0, 5.0], [1.0001, 0.0, 0.5]] {
            let out = ToneCurve::HueClip.apply(c);
            let m = c[0].max(c[1]).max(c[2]);
            for i in 0..3 {
                assert!((out[i] - c[i] / m).abs() < 1e-15, "{:?} -> {:?}", c, out);
                assert!(out[i] <= 1.0);
            }
        }
    }

    #[test]
    fn hue_clip_leaves_in_range_colours_alone() {
        for c in [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [0.3, 0.9, 0.1], [1.0, 0.0, 0.2]] {
            assert_eq!(ToneCurve::HueClip.apply(c), c);
        }
        // Negatives go to 0 first, and don't affect the scaling.
        assert_eq!(ToneCurve::HueClip.apply([-1.0, 2.0, 1.0]), [0.0, 1.0, 0.5]);
        assert_eq!(ToneCurve::HueClip.apply([f64::NAN, 0.5, 0.5]), [0.0, 0.5, 0.5]);
    }

    #[test]
    fn curve_names_round_trip() {
        for name in ToneCurve::NAMES.iter() {
            assert_eq!(ToneCurve::from_name(name).unwrap().name(), *name);
        }
        assert_eq!(
            ToneCurve::from_name("reinhard"),
            Some(ToneCurve::Reinhard { white: DEFAULT_REINHARD_WHITE })
        );
        assert_eq!(ToneCurve::from_name("filmic"), None);
    }

    #[test]
    fn wire_round_trip() {
        for view in [
            ViewTransform::default(),
            ViewTransform { exposure: -1.5, curve: ToneCurve::HueClip },
            ViewTransform { exposure: 2.0, curve: ToneCurve::Clip },
        ] {
            let mut buf = Vec::new();
            view.write_wire(&mut buf);
            assert_eq!(buf.len(), 12);
            let back = ViewTransform::read_wire(&mut &buf[..]).unwrap();
            assert_eq!(back, view);
        }
    }

    #[test]
    fn wire_rejects_bad_blocks() {
        let block = |id: u32, n: u32| {
            let mut b = Vec::new();
            b.extend_from_slice(&id.to_le_bytes());
            b.extend_from_slice(&0f32.to_le_bytes());
            b.extend_from_slice(&n.to_le_bytes());
            for _ in 0..n.min(4) {
                b.extend_from_slice(&1f32.to_le_bytes());
            }
            b
        };
        assert!(ViewTransform::read_wire(&mut &block(99, 0)[..]).is_err());
        assert!(ViewTransform::read_wire(&mut &block(0, 1)[..]).is_err());
        assert!(ViewTransform::read_wire(&mut &block(0, 1000)[..]).is_err());
        assert!(ViewTransform::read_wire(&mut &block(0, 0)[..8]).is_err());
    }

    const ALL_CURVES: [ToneCurve; 4] = [
        ToneCurve::Clip,
        ToneCurve::HueClip,
        ToneCurve::Reinhard { white: DEFAULT_REINHARD_WHITE },
        ToneCurve::AgX,
    ];

    /// Grey levels from far below to far above 1.
    fn grey_ramp() -> Vec<f64> {
        (-60..=40).map(|i| (i as f64 * 0.25).exp2()).collect()
    }

    #[test]
    fn curves_map_black_to_black_and_stay_in_range() {
        for curve in ALL_CURVES.iter() {
            assert_eq!(curve.apply([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0], "{:?}", curve);
            for g in grey_ramp() {
                for c in [[g, g, g], [g, 0.3 * g, 0.05 * g], [0.1 * g, 0.2 * g, g], [-g, g, 0.5 * g]] {
                    let out = curve.apply(c);
                    assert!(out.iter().all(|v| (0.0..=1.0).contains(v)), "{:?}({:?}) = {:?}", curve, c, out);
                }
            }
        }
    }

    #[test]
    fn curves_keep_greys_grey_and_rise_with_brightness() {
        for curve in ALL_CURVES.iter() {
            let mut last = -1.0;
            for g in grey_ramp() {
                let out = curve.apply([g, g, g]);
                assert!(
                    (out[0] - out[1]).abs() < 1e-9 && (out[1] - out[2]).abs() < 1e-9,
                    "{:?} tints grey {}: {:?}",
                    curve,
                    g,
                    out
                );
                assert!(out[0] >= last, "{:?} not monotonic at {}", curve, g);
                last = out[0];
            }
            // A coloured ramp's luminance rises too.
            let mut last = -1.0;
            for g in grey_ramp() {
                let l = luminance(curve.apply([g, 0.4 * g, 0.1 * g]));
                assert!(l >= last - 1e-12, "{:?} luminance falls at {}", curve, g);
                last = l;
            }
        }
    }

    #[test]
    fn reinhard_reaches_white_at_its_white_point() {
        for white in [1.5, 4.0, 10.0] {
            let out = ToneCurve::Reinhard { white }.apply([white, white, white]);
            assert!(out.iter().all(|v| (v - 1.0).abs() < 1e-12), "white {}: {:?}", white, out);
            let below = ToneCurve::Reinhard { white }.apply([0.9 * white; 3]);
            assert!(below[0] < 1.0);
        }
        // Small values pass nearly unchanged (L' ~ L for L << 1).
        let out = ToneCurve::Reinhard { white: 4.0 }.apply([0.01, 0.01, 0.01]);
        assert!((out[0] - 0.01).abs() < 2e-4, "{:?}", out);
        // Hue is kept: channel ratios survive while nothing clips.
        let out = ToneCurve::Reinhard { white: 4.0 }.apply([0.8, 0.4, 0.1]);
        assert!((out[1] / out[0] - 0.5).abs() < 1e-12 && (out[2] / out[0] - 0.125).abs() < 1e-12);
    }

    /// Hue in degrees of a display-linear colour, from its sRGB encoding
    /// (the usual HSV hexagon).
    fn hue_deg(c: LinearColor) -> f64 {
        let [r, g, b] = [linear_to_srgb(c[0]), linear_to_srgb(c[1]), linear_to_srgb(c[2])];
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        if d <= 0.0 {
            return 0.0;
        }
        let h = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        60.0 * h
    }

    fn hue_shift(from: f64, to: f64) -> f64 {
        ((to - from + 180.0).rem_euclid(360.0) - 180.0).abs()
    }

    #[test]
    fn agx_keeps_hue_where_clipping_skews_it() {
        // Warm, wood-like colours: AgX moves their hue a few degrees at
        // most from 1x to 16x, where per-channel clipping swings them by
        // 24-39 degrees (orange toward yellow and back).
        for c in [[1.0, 0.5, 0.1], [0.9, 0.65, 0.3]] {
            let h0 = hue_deg(c);
            for k in [1.0, 4.0, 16.0] {
                let bright = [c[0] * k, c[1] * k, c[2] * k];
                let agx = hue_shift(h0, hue_deg(ToneCurve::AgX.apply(bright)));
                assert!(agx < 5.0, "AgX moved {:?} x{} by {} degrees", c, k, agx);
            }
            let clip = hue_shift(h0, hue_deg(ToneCurve::Clip.apply([c[0] * 4.0, c[1] * 4.0, c[2] * 4.0])));
            assert!(clip > 20.0, "clip only moved {:?} by {}", c, clip);
        }
        // A saturated blue: AgX rotates it moderately as it desaturates
        // toward white (this is the AgX look), far less than clipping,
        // which turns it cyan and then white.
        let blue = [0.1, 0.2, 1.0];
        let h0 = hue_deg(blue);
        let agx = hue_shift(h0, hue_deg(ToneCurve::AgX.apply([1.6, 3.2, 16.0])));
        let clip = hue_shift(h0, hue_deg(ToneCurve::Clip.apply([1.6, 3.2, 16.0])));
        assert!(agx < 20.0 && clip > 100.0, "agx {} clip {}", agx, clip);
    }

    #[test]
    fn agx_matrices_preserve_white() {
        for m in [&SRGB_TO_REC2020, &REC2020_TO_SRGB, &AGX_INSET, &AGX_OUTSET] {
            let w = mat_mul(m, [1.0, 1.0, 1.0]);
            assert!(w.iter().all(|v| (v - 1.0).abs() < 1e-12), "{:?}", w);
        }
        // The two primaries conversions are inverses.
        let c = [0.3, 0.6, 0.9];
        let back = mat_mul(&REC2020_TO_SRGB, mat_mul(&SRGB_TO_REC2020, c));
        assert!((0..3).all(|i| (back[i] - c[i]).abs() < 1e-12), "{:?}", back);
        // Mid grey 0.18 lands near 0.21 display-linear, 1.0 near 0.59,
        // and the curve saturates just short of 1.
        let g = |x: f64| ToneCurve::AgX.apply([x, x, x])[0];
        assert!((g(0.18) - 0.2145).abs() < 1e-3, "{}", g(0.18));
        assert!((g(1.0) - 0.5902).abs() < 1e-3, "{}", g(1.0));
        assert!(g(1.0e6) > 0.99 && g(1.0e6) < 1.0);
    }

    #[test]
    fn reinhard_and_agx_round_trip_on_the_wire() {
        for view in [
            ViewTransform { exposure: 0.5, curve: ToneCurve::Reinhard { white: 6.0 } },
            ViewTransform { exposure: -1.0, curve: ToneCurve::AgX },
        ] {
            let mut buf = Vec::new();
            view.write_wire(&mut buf);
            assert_eq!(ViewTransform::read_wire(&mut &buf[..]).unwrap(), view);
        }
        // Reinhard needs exactly one positive parameter.
        let mut bad = Vec::new();
        bad.extend_from_slice(&2u32.to_le_bytes());
        bad.extend_from_slice(&0f32.to_le_bytes());
        bad.extend_from_slice(&0u32.to_le_bytes());
        assert!(ViewTransform::read_wire(&mut &bad[..]).is_err());
        let mut bad = Vec::new();
        bad.extend_from_slice(&2u32.to_le_bytes());
        bad.extend_from_slice(&0f32.to_le_bytes());
        bad.extend_from_slice(&1u32.to_le_bytes());
        bad.extend_from_slice(&(-1f32).to_le_bytes());
        assert!(ViewTransform::read_wire(&mut &bad[..]).is_err());
    }
}
