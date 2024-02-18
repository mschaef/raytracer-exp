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

fn linear_to_srgb(x: f64) -> f64 {
    if x < 0.0 {
        0.0
    } else if x < 0.0031308	{
        x * 12.92
    } else if x < 1.0 {
        1.055 * x.powf(1.0/2.4) - 0.055
    } else {
        1.0
    }
}

pub fn to_png_color(color: &LinearColor) -> [u8; 3] {
    [
        (linear_to_srgb(color[0]) * 256.0) as u8,
        (linear_to_srgb(color[1]) * 256.0) as u8,
        (linear_to_srgb(color[2]) * 256.0) as u8
    ]
}
