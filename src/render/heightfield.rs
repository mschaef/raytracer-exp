// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Height fields: POV-Ray's `height_field { tga "file" ... }`, built as
//! a triangle mesh when the scene is loaded.
//!
//! The field fills the unit square in x and z, with heights in `[0, 1]`
//! in y. Image column `c` of `w` is at `x = c / (w - 1)`; image row `r`
//! of `h` (counting from the top of the picture) is at
//! `z = 1 - r / (h - 1)`, so the picture reads the right way up when
//! seen from above with +z at the top. Each grid cell is two triangles.
//!
//! Heights come from the image as POV-Ray reads them: an 8-bit grey
//! value `v` is `v / 255`, a palette image uses the palette index the
//! same way, and a true-colour image is `(red * 256 + green) / 65535`
//! (POV's 16-bit trick; a grey picture gives `v / 255` either way).
//!
//! The TGA reader is local rather than from the `image` crate: it's a
//! small format, and the reader only has to cover what height fields
//! use (uncompressed and RLE; grey, palette and 24/32-bit colour).

use std::path::Path;

use crate::render::geometry::{crossp, normalizep, subp, Point};
use crate::render::shapes::{Shape, Triangle};
use crate::render::Surface;

/// A grid of heights in `[0, 1]`, row 0 at the top of the picture.
#[derive(Clone, PartialEq, Debug)]
pub struct HeightGrid {
    pub width: usize,
    pub height: usize,
    pub values: Vec<f64>,
}

impl HeightGrid {
    fn at(&self, col: usize, row: usize) -> f64 {
        self.values[row * self.width + col]
    }
}

/// Read a TGA file's pixels as heights. Errors name what's unsupported.
pub fn read_tga_heights(path: &Path) -> Result<HeightGrid, String> {
    let data = std::fs::read(path).map_err(|e| format!("can't read {:?}: {}", path, e))?;
    parse_tga_heights(&data).map_err(|e| format!("{:?}: {}", path, e))
}

/// Parse TGA bytes into heights (see the module comment).
pub fn parse_tga_heights(data: &[u8]) -> Result<HeightGrid, String> {
    if data.len() < 18 {
        return Err("too short for a TGA header".into());
    }
    let id_len = data[0] as usize;
    let cmap_type = data[1];
    let image_type = data[2];
    let cmap_first = u16::from_le_bytes([data[3], data[4]]) as usize;
    let cmap_len = u16::from_le_bytes([data[5], data[6]]) as usize;
    let cmap_bits = data[7] as usize;
    let width = u16::from_le_bytes([data[12], data[13]]) as usize;
    let height = u16::from_le_bytes([data[14], data[15]]) as usize;
    let bits = data[16] as usize;
    let descriptor = data[17];
    let top_first = descriptor & 0x20 != 0;
    let right_to_left = descriptor & 0x10 != 0;

    let (kind, rle) = match image_type {
        1 => (Kind::Palette, false),
        2 => (Kind::Color, false),
        3 => (Kind::Grey, false),
        9 => (Kind::Palette, true),
        10 => (Kind::Color, true),
        11 => (Kind::Grey, true),
        other => return Err(format!("TGA image type {} isn't supported", other)),
    };
    let bytes_per_pixel = match (kind, bits) {
        (Kind::Grey, 8) | (Kind::Palette, 8) => 1,
        (Kind::Color, 24) => 3,
        (Kind::Color, 32) => 4,
        _ => return Err(format!("{} bits per pixel isn't supported for this TGA type", bits)),
    };
    if width < 2 || height < 2 {
        return Err(format!("a height field needs at least 2x2 pixels (got {}x{})", width, height));
    }

    // Skip the id and the colour map: a palette image's height is its
    // index, so the palette entries themselves aren't needed.
    let mut pos = 18 + id_len;
    if cmap_type == 1 {
        pos += cmap_len * ((cmap_bits + 7) / 8);
    }
    let _ = cmap_first;

    let count = width * height;
    let mut pixels: Vec<u8> = Vec::with_capacity(count * bytes_per_pixel);
    if rle {
        while pixels.len() < count * bytes_per_pixel {
            let header = *data.get(pos).ok_or("RLE data ends early")?;
            pos += 1;
            let run = (header & 0x7f) as usize + 1;
            if header & 0x80 != 0 {
                let px = data.get(pos..pos + bytes_per_pixel).ok_or("RLE data ends early")?;
                for _ in 0..run {
                    pixels.extend_from_slice(px);
                }
                pos += bytes_per_pixel;
            } else {
                let n = run * bytes_per_pixel;
                pixels.extend_from_slice(data.get(pos..pos + n).ok_or("RLE data ends early")?);
                pos += n;
            }
        }
        pixels.truncate(count * bytes_per_pixel);
    } else {
        let n = count * bytes_per_pixel;
        pixels.extend_from_slice(data.get(pos..pos + n).ok_or("pixel data ends early")?);
    }

    let mut values = vec![0.0; count];
    for stored_row in 0..height {
        let row = if top_first { stored_row } else { height - 1 - stored_row };
        for stored_col in 0..width {
            let col = if right_to_left { width - 1 - stored_col } else { stored_col };
            let p = &pixels[(stored_row * width + stored_col) * bytes_per_pixel..];
            // TGA stores colour as blue, green, red.
            let v = match kind {
                Kind::Grey | Kind::Palette => p[0] as f64 / 255.0,
                Kind::Color => (p[2] as f64 * 256.0 + p[1] as f64) / 65535.0,
            };
            values[row * width + col] = v;
        }
    }
    Ok(HeightGrid { width, height, values })
}

#[derive(Copy, Clone, PartialEq, Debug)]
enum Kind {
    Grey,
    Palette,
    Color,
}

/// The height field's triangles in the unit square (see the module
/// comment), as a flat list for the caller to put in a BVH.
///
/// Cells whose four corners are all below `water_level` are left out,
/// as POV-Ray leaves out what's under its water level. With `smooth`,
/// each vertex gets a normal from the heights around it (central
/// differences), so the facets shade as a smooth surface; otherwise
/// each triangle is flat.
pub fn height_field_triangles(grid: &HeightGrid, water_level: f64, smooth: bool, surface: Option<Surface>) -> Vec<Shape> {
    let (w, h) = (grid.width, grid.height);
    let dx = 1.0 / (w - 1) as f64;
    let dz = 1.0 / (h - 1) as f64;
    let point = |c: usize, r: usize| -> Point { [c as f64 * dx, grid.at(c, r), 1.0 - r as f64 * dz] };
    let vertex_normal = |c: usize, r: usize| -> Point {
        let (c0, c1) = (c.saturating_sub(1), (c + 1).min(w - 1));
        let (r0, r1) = (r.saturating_sub(1), (r + 1).min(h - 1));
        // Slopes in x and z (z runs opposite to the rows).
        let sx = (grid.at(c1, r) - grid.at(c0, r)) / ((c1 - c0) as f64 * dx);
        let sz = (grid.at(c, r0) - grid.at(c, r1)) / ((r1 - r0) as f64 * dz);
        normalizep([-sx, 1.0, -sz])
    };

    let mut out = Vec::new();
    for r in 0..h - 1 {
        for c in 0..w - 1 {
            let corners = [(c, r), (c + 1, r), (c, r + 1), (c + 1, r + 1)];
            if corners.iter().all(|&(cc, rr)| grid.at(cc, rr) < water_level) {
                continue;
            }
            // (c, r) is the cell's far-left corner (larger z). Two
            // triangles, each wound so its geometric normal faces up.
            for tri in [[(c, r), (c + 1, r), (c, r + 1)], [(c + 1, r), (c + 1, r + 1), (c, r + 1)]] {
                let v = [point(tri[0].0, tri[0].1), point(tri[1].0, tri[1].1), point(tri[2].0, tri[2].1)];
                // Normalized by hand: on a fine grid the cross product
                // of two edges is far below `normalizep`'s EPSILON.
                let cross = crossp(subp(v[1], v[0]), subp(v[2], v[0]));
                let len = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
                let face = [cross[0] / len, cross[1] / len, cross[2] / len];
                let normals = if smooth {
                    [
                        vertex_normal(tri[0].0, tri[0].1),
                        vertex_normal(tri[1].0, tri[1].1),
                        vertex_normal(tri[2].0, tri[2].1),
                    ]
                } else {
                    [face, face, face]
                };
                out.push(Shape::Triangle(Triangle { vertices: v, normals, surface }));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An uncompressed TGA of the given type and pixel bytes.
    fn tga(image_type: u8, bits: u8, w: u16, h: u16, top_first: bool, pixels: &[u8]) -> Vec<u8> {
        let mut d = vec![0u8; 18];
        d[2] = image_type;
        d[12..14].copy_from_slice(&w.to_le_bytes());
        d[14..16].copy_from_slice(&h.to_le_bytes());
        d[16] = bits;
        d[17] = if top_first { 0x20 } else { 0 };
        d.extend_from_slice(pixels);
        d
    }

    #[test]
    fn reads_grey_colour_and_orientation() {
        // 2x2 grey, stored bottom row first: rows come out top first.
        let g = parse_tga_heights(&tga(3, 8, 2, 2, false, &[0, 51, 102, 255])).unwrap();
        assert_eq!(g.values, vec![102.0 / 255.0, 1.0, 0.0, 51.0 / 255.0]);
        // 24-bit: (red * 256 + green) / 65535, bytes stored B, G, R.
        let c = parse_tga_heights(&tga(2, 24, 2, 2, true, &[0, 0, 0, 9, 1, 2, 0, 255, 255, 0, 128, 128])).unwrap();
        assert_eq!(c.values[0], 0.0);
        assert_eq!(c.values[1], (2.0 * 256.0 + 1.0) / 65535.0);
        assert_eq!(c.values[2], 1.0);
        assert!((c.values[3] - 128.0 / 255.0).abs() < 1e-12, "grey reads as v / 255");
    }

    #[test]
    fn reads_rle() {
        // Type 11 (RLE grey): a run of three 7s, then one literal 200.
        let d = tga(11, 8, 2, 2, true, &[0x82, 7, 0x00, 200]);
        let g = parse_tga_heights(&d).unwrap();
        assert_eq!(g.values, vec![7.0 / 255.0, 7.0 / 255.0, 7.0 / 255.0, 200.0 / 255.0]);
    }

    #[test]
    fn rejects_what_it_cannot_read() {
        assert!(parse_tga_heights(&[0u8; 5]).is_err());
        assert!(parse_tga_heights(&tga(2, 16, 2, 2, true, &[0; 8])).is_err());
        assert!(parse_tga_heights(&tga(3, 8, 2, 2, true, &[0; 3])).is_err(), "short data");
    }

    #[test]
    fn mesh_spans_the_unit_square_and_faces_up() {
        let grid = HeightGrid { width: 3, height: 2, values: vec![0.0, 0.5, 1.0, 0.2, 0.2, 0.2] };
        let tris = height_field_triangles(&grid, 0.0, false, None);
        assert_eq!(tris.len(), 4);
        for t in &tris {
            if let Shape::Triangle(t) = t {
                assert!(t.normals[0][1] > 0.0, "faces up: {:?}", t.normals);
                for v in &t.vertices {
                    assert!((0.0..=1.0).contains(&v[0]) && (0.0..=1.0).contains(&v[2]));
                }
            }
        }
        // Row 0 (top of the picture) is at z = 1: the height-1 corner.
        let top_right = tris.iter().any(|t| match t {
            Shape::Triangle(t) => t.vertices.iter().any(|v| *v == [1.0, 1.0, 1.0]),
            _ => false,
        });
        assert!(top_right);
        // Smooth normals face up too.
        for t in height_field_triangles(&grid, 0.0, true, None) {
            if let Shape::Triangle(t) = t {
                assert!(t.normals.iter().all(|n| n[1] > 0.0));
            }
        }
    }

    #[test]
    fn water_level_drops_cells_entirely_below_it() {
        let grid = HeightGrid { width: 3, height: 2, values: vec![0.0, 0.1, 0.9, 0.0, 0.1, 0.9] };
        // The left cell's corners are all under 0.2; the right one isn't.
        assert_eq!(height_field_triangles(&grid, 0.2, false, None).len(), 2);
        assert_eq!(height_field_triangles(&grid, 0.0, false, None).len(), 4);
    }
}
