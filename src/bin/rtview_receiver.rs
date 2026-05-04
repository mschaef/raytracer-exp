// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Throwaway TCP receiver for the raytracer's `StreamTarget` wire format.
//!
//! Stage one of the rtview integration: lets us validate the wire protocol
//! end-to-end without the Cocoa GUI in play. Listens on
//! `RTVIEW_ADDR` (default `127.0.0.1:9999`), accepts a single connection,
//! reads the header + row stream, applies the same linear → sRGB encode
//! that `PngTarget` uses, and writes the result to `received.png`. After
//! one render it exits.
//!
//! Run alongside the renderer:
//!
//! ```sh
//! cargo run --release --bin rtview_receiver         # terminal A
//! RTVIEW_ADDR=127.0.0.1:9999 cargo run --release    # terminal B
//! ```
//!
//! Then byte-compare `received.png` against `render.png`. Differences
//! larger than ±1 per channel are a wire-protocol bug; ±1 differences are
//! expected (the wire narrows `f64 → f32` while the on-disk path
//! quantizes from `f64` directly, so quantization boundaries can flip).

extern crate image;

use std::env;
use std::io::{ErrorKind, Read};
use std::net::TcpListener;

/// Mirrors `render::color::linear_to_srgb` but takes `f32` because the
/// wire payload is `f32`. Stage one stays self-contained — the renderer
/// crate has no `lib.rs` to import from, so duplicating six lines of
/// transfer function is cheaper than restructuring the package.
fn linear_to_srgb(x: f32) -> f32 {
    if x < 0.0 {
        0.0
    } else if x < 0.003_130_8 {
        x * 12.92
    } else if x < 1.0 {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    } else {
        1.0
    }
}

/// Mirrors `render::color::to_png_color` for `f32` inputs.
fn to_png_color(c: [f32; 3]) -> [u8; 3] {
    [
        (linear_to_srgb(c[0]) * 256.0) as u8,
        (linear_to_srgb(c[1]) * 256.0) as u8,
        (linear_to_srgb(c[2]) * 256.0) as u8,
    ]
}

fn read_u32_le<R: Read>(r: &mut R) -> std::io::Result<u32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn main() -> std::io::Result<()> {
    let addr = env::var("RTVIEW_ADDR").unwrap_or_else(|_| "127.0.0.1:9999".to_string());

    let listener = TcpListener::bind(&addr)?;
    eprintln!("rtview_receiver: listening on {}", addr);

    let (mut conn, peer) = listener.accept()?;
    eprintln!("rtview_receiver: connection from {}", peer);

    // Header: 4 magic + 3 × u32 = 16 bytes.
    let mut magic = [0u8; 4];
    conn.read_exact(&mut magic)?;
    if &magic != b"RTVW" {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            format!("bad magic: expected RTVW, got {:?}", magic),
        ));
    }
    let width = read_u32_le(&mut conn)?;
    let height = read_u32_le(&mut conn)?;
    let flags = read_u32_le(&mut conn)?;
    eprintln!(
        "rtview_receiver: header {}x{}, flags={} (0=linear-f32)",
        width, height, flags
    );
    if flags != 0 {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            format!("unknown flags value {}", flags),
        ));
    }

    let mut img = image::ImageBuffer::<image::Rgb<u8>, Vec<u8>>::new(width, height);

    // Reusable scratch buffer for row payloads. Sized for the largest row
    // we expect; grows on demand if we ever see a wider one.
    let mut payload = Vec::<u8>::with_capacity((width as usize) * 12);

    let mut rows_received: u32 = 0;
    loop {
        // Each row message: y, x, count, then count × 3 × f32. Reading
        // the y first lets us detect end-of-stream (clean EOF) here
        // rather than buried inside a partial-row read.
        let y = match read_u32_le(&mut conn) {
            Ok(v) => v,
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        };
        let x = read_u32_le(&mut conn)?;
        let count = read_u32_le(&mut conn)?;

        let payload_len = (count as usize) * 12;
        if payload.len() < payload_len {
            payload.resize(payload_len, 0);
        }
        conn.read_exact(&mut payload[..payload_len])?;

        // Decode the f32 triple per pixel and write the encoded sRGB
        // value into the image buffer. Single contiguous read above
        // beats per-pixel syscalls by orders of magnitude on a 2K row.
        for i in 0..(count as usize) {
            let off = i * 12;
            // Build the [u8; 4] arrays explicitly. Slice-to-array
            // conversion via `try_into` would need a `TryInto` import
            // under edition 2018; this form is portable and just as
            // readable.
            let r = f32::from_le_bytes([
                payload[off], payload[off + 1], payload[off + 2], payload[off + 3],
            ]);
            let g = f32::from_le_bytes([
                payload[off + 4], payload[off + 5], payload[off + 6], payload[off + 7],
            ]);
            let b = f32::from_le_bytes([
                payload[off + 8], payload[off + 9], payload[off + 10], payload[off + 11],
            ]);
            let p = to_png_color([r, g, b]);
            img.put_pixel(x + i as u32, y, image::Rgb(p));
        }

        rows_received += 1;
        // Light-touch progress display — a row counter rewritten in place
        // via \r, same idiom as the renderer's ProgressTarget. Don't
        // print on every row; at 2K-square scenes that's 8K writes total,
        // and stderr is fast enough that batching per 32 rows keeps the
        // output readable without losing useful feedback.
        if rows_received % 32 == 0 {
            eprint!("\rrtview_receiver: {} rows received ", rows_received);
        }
    }
    eprintln!(
        "\rrtview_receiver: {} rows received, connection closed",
        rows_received
    );

    img.save("received.png")
        .map_err(|e| std::io::Error::new(ErrorKind::Other, e.to_string()))?;
    eprintln!("rtview_receiver: wrote received.png");

    Ok(())
}
