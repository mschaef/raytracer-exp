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
}

impl ToneCurve {
    /// Every curve, for listing in messages.
    pub const NAMES: [&'static str; 2] = ["clip", "hue-clip"];

    /// The curve's name in the SDL (`:clip`) and in `RAYTRACER_CURVE`.
    pub fn name(&self) -> &'static str {
        match self {
            ToneCurve::Clip => "clip",
            ToneCurve::HueClip => "hue-clip",
        }
    }

    /// The curve for a name, or `None` if there's no such curve.
    pub fn from_name(name: &str) -> Option<ToneCurve> {
        match name {
            "clip" => Some(ToneCurve::Clip),
            "hue-clip" => Some(ToneCurve::HueClip),
            _ => None,
        }
    }

    /// The curve's id on the wire.
    fn wire_id(&self) -> u32 {
        match self {
            ToneCurve::Clip => 0,
            ToneCurve::HueClip => 1,
        }
    }

    /// The curve's parameters on the wire (none yet).
    fn wire_params(&self) -> Vec<f32> {
        Vec::new()
    }

    fn from_wire(id: u32, params: &[f32]) -> io::Result<ToneCurve> {
        let curve = match id {
            0 => ToneCurve::Clip,
            1 => ToneCurve::HueClip,
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
            ToneCurve::HueClip => {
                let c = [nonneg(c[0]), nonneg(c[1]), nonneg(c[2])];
                let m = c[0].max(c[1]).max(c[2]);
                if m > 1.0 {
                    [c[0] / m, c[1] / m, c[2] / m]
                } else {
                    c
                }
            }
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

    /// The view-transform block for the stream: curve id (`u32`),
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
}
