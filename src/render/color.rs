// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

pub type LinearColor = [f64; 3];

pub fn scale_linear_color(color: &LinearColor, s: f64) -> LinearColor {
    [
        color[0] * s,
        color[1] * s,
        color[2] * s
    ]
}

pub fn add_linear_color(colora: &LinearColor, colorb: &LinearColor) -> LinearColor {
    [
        colora[0] + colorb[0],
        colora[1] + colorb[1],
        colora[2] + colorb[2],
    ]
}

/// Component-wise color multiplication. Used for modulating one color
/// by another — e.g. tinting a surface's diffuse color by a light's
/// emitted color. Distinct from `scale_linear_color`, which multiplies
/// by a scalar.
pub fn multiply_linear_color(colora: &LinearColor, colorb: &LinearColor) -> LinearColor {
    [
        colora[0] * colorb[0],
        colora[1] * colorb[1],
        colora[2] * colorb[2],
    ]
}

// Encoding to 8-bit sRGB lives in `render::view` (`ViewTransform`,
// `encode_display`), with the exposure and tone curve that come first.
