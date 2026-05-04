// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Pluggable render targets.
//!
//! `render()` doesn't write a file or even allocate an image; it walks the
//! scene one row at a time and pushes finished pixels into a `RenderTarget`.
//! What the target does with those rows is its own business — write into a
//! PNG buffer, forward to a streaming UI, log to stdout, whatever.
//!
//! The trait carries pixels as `LinearColor` (`[f64; 3]`) — the same linear,
//! unbounded representation used throughout the renderer. Display-space
//! encoding (sRGB transfer, quantization to 8-bit, tone mapping, etc.) is
//! the target's responsibility, not the renderer's. `PngTarget` does the
//! linear → sRGB encode in its own `submit_row`; a future EXR or 16-bit
//! target can keep the float values, and a tone-mapping wrapper can do its
//! work *before* any encode is applied (which is the only place it's
//! mathematically correct to do).
//!
//! Values may be at or below 0.0 and at or above 1.0; the renderer itself
//! never produces negatives, but the trait makes no promise either way and
//! downstream targets are expected to handle the full range.
//!
//! The trait deliberately exposes only what `render()` produces: a starting
//! `(x, y)` coordinate and a contiguous slice of pixels. Order of
//! `submit_row` calls is unspecified — under parallel rendering, rows arrive
//! in whatever order Rayon's worker threads happen to produce them, so any
//! impl must tolerate out-of-order rows.

extern crate image;

use std::io::{self, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use image::ImageResult;

use super::color::{LinearColor, to_png_color};

/// Where rendered pixels go.
///
/// Required to be `Send + Sync` because `render()` calls `submit_row` from
/// multiple Rayon worker threads concurrently. Since `submit_row` takes
/// `&self`, impls that need mutable storage must use interior mutability
/// (e.g. `Mutex` for `PngTarget`, `Sender` for a future channel-based
/// streaming target).
pub trait RenderTarget: Send + Sync {
    /// Hand a finished row of pixels to the target. The row covers
    /// pixels `(x, y)` through `(x + row.len() - 1, y)` inclusive. Rows
    /// from different `submit_row` calls do not overlap, but may arrive
    /// in any order.
    ///
    /// Pixel values are linear-space `LinearColor` (`[f64; 3]`), unclamped.
    /// Targets that ultimately render to a display-space format are
    /// responsible for the appropriate encode.
    fn submit_row(&self, x: u32, y: u32, row: &[LinearColor]);

    /// Called once by `render()` after every row has been submitted.
    /// Default no-op. Streaming targets override this to send a
    /// "render complete" signal; `ProgressTarget` uses it to emit a
    /// final newline so subsequent stdout/stderr output starts cleanly.
    /// PNG-on-disk targets ignore it because saving is an explicit
    /// user action.
    fn finish(&self) {}
}

/// PNG-backed target. Holds an in-memory `image::ImageBuffer` behind a
/// `Mutex` for thread-safe row writes; saves to disk on demand via
/// `save(path)`.
///
/// The `Mutex` is the simplest correct choice and contention is bounded:
/// at most `num_threads` workers ever wait, and each lock holds for the
/// time to copy a few KB of pixels — invisible compared to ray tracing
/// cost. If profiling later flags it, options include splitting the
/// buffer into per-row mutexes or using unsafe to give out non-overlapping
/// `&mut` slices, but neither is justified at present.
pub struct PngTarget {
    buffer: Mutex<image::ImageBuffer<image::Rgb<u8>, Vec<u8>>>,
}

impl PngTarget {
    pub fn new(width: u32, height: u32) -> Self {
        PngTarget {
            buffer: Mutex::new(image::ImageBuffer::new(width, height)),
        }
    }

    /// Write a single pixel directly. Useful for compositing operations
    /// that don't fit the row-at-a-time pattern (e.g. drawing a crosshair).
    /// Color is in the same linear space as `submit_row`; sRGB encoding
    /// happens internally.
    pub fn put_pixel(&self, x: u32, y: u32, color: LinearColor) {
        let encoded = to_png_color(&color);
        let mut buf = self.buffer.lock().unwrap();
        buf.put_pixel(x, y, image::Rgb(encoded));
    }

    /// Save the accumulated buffer as a PNG. Consumes the target.
    pub fn save(self, path: impl AsRef<Path>) -> ImageResult<()> {
        self.buffer.into_inner().unwrap().save(path)
    }
}

impl RenderTarget for PngTarget {
    fn submit_row(&self, x: u32, y: u32, row: &[LinearColor]) {
        // Encode outside the lock so concurrent workers can do the
        // linear → sRGB conversion in parallel and only contend for the
        // pixel-buffer write itself. The temporary Vec is per-call, on
        // the order of a few KB at typical row widths — invisible
        // against ray-tracing cost.
        let encoded: Vec<[u8; 3]> = row.iter().map(to_png_color).collect();

        let mut buf = self.buffer.lock().unwrap();
        for (i, p) in encoded.iter().enumerate() {
            buf.put_pixel(x + i as u32, y, image::Rgb(*p));
        }
    }
}

/// TCP-streaming target. Opens a connection on construction, sends a
/// fixed-size header with the image dimensions, then writes one
/// length-prefixed message per `submit_row` call. The receiver (e.g. the
/// `rtview` GUI, or the `rtview_receiver` test binary) reads the header
/// to size its buffer and then consumes row messages until the connection
/// closes.
///
/// Wire format (all integers little-endian):
///
/// ```text
/// Header (16 bytes, sent once):
///   magic   [u8; 4]  "RTVW"
///   width   u32
///   height  u32
///   flags   u32      0 = linear-color f32 payload (only variant today)
///
/// Row message (variable, sent per submit_row):
///   y       u32
///   x       u32
///   count   u32
///   pixels  [f32; count * 3]    R, G, B in linear space
/// ```
///
/// Linear color is sent on the wire — `LinearColor` (`[f64; 3]`) is
/// narrowed to `f32` per channel at the wire boundary. f32 carries ~7
/// decimal digits, far more than any 8-bit display encoding needs, and
/// keeps the bandwidth tractable for typical 2K-square renders. The
/// receiver is responsible for whatever display-space encoding it wants
/// to do (sRGB, tone mapping, HDR pass-through).
///
/// `submit_row` packs y/x/count + payload into a single `Vec<u8>` and
/// issues a single `write_all` under the lock. The pack-then-write idiom
/// keeps the critical section short (no per-pixel syscalls) and pairs
/// well with `TCP_NODELAY` to avoid Nagle-induced stalls on loopback.
///
/// The `Mutex<TcpStream>` is the simplest correct choice — Rayon workers
/// call `submit_row` concurrently and the kernel write needs serialization
/// regardless. If loopback contention ever becomes measurable, a future
/// improvement is an mpsc channel feeding a dedicated writer thread; the
/// trait surface stays the same.
pub struct StreamTarget {
    inner: Mutex<TcpStream>,
}

impl StreamTarget {
    /// Connect to the receiver at `addr` (e.g. `"127.0.0.1:9999"`) and
    /// send the header. Subsequent `submit_row` calls stream rows on the
    /// same connection. Returns an `io::Error` if the connection or the
    /// header write fails — the renderer can decide how to react (today,
    /// `main.rs` aborts with a clear message).
    pub fn connect(addr: &str, width: u32, height: u32) -> io::Result<Self> {
        let mut stream = TcpStream::connect(addr)?;
        // Disable Nagle: rows are already packed into a single write
        // each, and we'd rather have them on the wire promptly than
        // batched into 40ms windows.
        stream.set_nodelay(true)?;

        let mut hdr = Vec::with_capacity(16);
        hdr.extend_from_slice(b"RTVW");
        hdr.extend_from_slice(&width.to_le_bytes());
        hdr.extend_from_slice(&height.to_le_bytes());
        hdr.extend_from_slice(&0u32.to_le_bytes()); // flags: linear f32
        stream.write_all(&hdr)?;

        Ok(StreamTarget { inner: Mutex::new(stream) })
    }
}

impl RenderTarget for StreamTarget {
    fn submit_row(&self, x: u32, y: u32, row: &[LinearColor]) {
        // 12-byte row header + 12 bytes per pixel (3 × f32). Sized exactly
        // so the Vec allocates once and the write is a single contiguous
        // payload.
        let mut buf = Vec::with_capacity(12 + row.len() * 12);
        buf.extend_from_slice(&y.to_le_bytes());
        buf.extend_from_slice(&x.to_le_bytes());
        buf.extend_from_slice(&(row.len() as u32).to_le_bytes());
        for px in row {
            buf.extend_from_slice(&(px[0] as f32).to_le_bytes());
            buf.extend_from_slice(&(px[1] as f32).to_le_bytes());
            buf.extend_from_slice(&(px[2] as f32).to_le_bytes());
        }
        // Errors are silently dropped to match the `RenderTarget` API
        // (which has no `Result`). A dropped connection mid-render means
        // the receiver gets a partial image and the renderer keeps going;
        // for stage one that's acceptable. If we ever want hard failure,
        // it's a single-line API change across all targets.
        let _ = self.inner.lock().unwrap().write_all(&buf);
    }

    fn finish(&self) {
        // Flush isn't strictly necessary (no BufWriter sits in front of
        // the socket) but keeps the door open for later buffering. Drop
        // closes the connection, which the receiver reads as EOF and
        // treats as "render complete".
        let _ = self.inner.lock().unwrap().flush();
    }
}

/// Wraps another `RenderTarget` and adds a `(dx, dy)` offset to every
/// row's coordinates. Lets multiple sub-renders share a single backing
/// target, each writing into its own region.
///
/// Does not propagate `finish()` to the inner target. Several
/// `OffsetTarget`s commonly share one inner; the application is
/// responsible for calling `finish()` on the inner target itself
/// exactly once when all sub-renders are complete.
pub struct OffsetTarget<'a, T: RenderTarget + ?Sized + 'a> {
    inner: &'a T,
    dx: u32,
    dy: u32,
}

impl<'a, T: RenderTarget + ?Sized + 'a> OffsetTarget<'a, T> {
    pub fn new(inner: &'a T, dx: u32, dy: u32) -> Self {
        OffsetTarget { inner, dx, dy }
    }
}

impl<'a, T: RenderTarget + ?Sized + 'a> RenderTarget for OffsetTarget<'a, T> {
    fn submit_row(&self, x: u32, y: u32, row: &[LinearColor]) {
        self.inner.submit_row(x + self.dx, y + self.dy, row);
    }
    // finish() intentionally not propagated — see struct doc.
}

/// Wraps another `RenderTarget` and prints in-place progress to stderr
/// as rows complete. Forwards every `submit_row` to the inner target,
/// then increments a row counter and rewrites the progress line via
/// carriage-return overwrite. `finish()` emits a closing newline so
/// subsequent output starts cleanly.
///
/// The print is serialized via `stderr().lock()` so output from
/// concurrent worker threads doesn't interleave. Lock contention is
/// negligible: even at thousands of rows per render the lock is held
/// for microseconds at a time, and the rendering work that surrounds
/// each call dominates by orders of magnitude.
///
/// `finish()` propagates to the inner target so that wrapping a
/// streaming target with progress reporting still gets the underlying
/// "done" signal delivered. (`OffsetTarget` is the asymmetric case —
/// it does not propagate because multiple offsets typically share one
/// underlying target.)
pub struct ProgressTarget<'a, T: RenderTarget + ?Sized + 'a> {
    inner: &'a T,
    total_rows: u32,
    completed: AtomicU32,
    label: &'a str,
}

impl<'a, T: RenderTarget + ?Sized + 'a> ProgressTarget<'a, T> {
    pub fn new(inner: &'a T, total_rows: u32, label: &'a str) -> Self {
        ProgressTarget {
            inner,
            total_rows,
            completed: AtomicU32::new(0),
            label,
        }
    }
}

impl<'a, T: RenderTarget + ?Sized + 'a> RenderTarget for ProgressTarget<'a, T> {
    fn submit_row(&self, x: u32, y: u32, row: &[LinearColor]) {
        self.inner.submit_row(x, y, row);

        let n = self.completed.fetch_add(1, Ordering::Relaxed) + 1;

        let stderr = std::io::stderr();
        let mut handle = stderr.lock();
        // Trailing space pads over any previous longer line under \r.
        let _ = write!(handle, "\r  {}: {}/{} rows ", self.label, n, self.total_rows);
        let _ = handle.flush();
    }

    fn finish(&self) {
        // Newline so the next println from the application starts on
        // its own line rather than overwriting the progress text.
        eprintln!();
        // Propagate so the inner target's own end-of-render hook fires
        // (e.g. a streaming target sending "Done").
        self.inner.finish();
    }
}

/// Where per-pixel timing data goes when a heat-map render is requested.
///
/// Parallel to `RenderTarget` but separate: the renderer can run with no
/// heatmap (zero overhead), with one, or — eventually — with multiple
/// kinds of diagnostic targets without the pixel target needing to know.
///
/// Times are passed as `u32` nanoseconds. A pixel that takes longer than
/// `u32::MAX` ns (~4.29 s) saturates at `u32::MAX` rather than wrapping;
/// the renderer uses `try_from` for the cast so a runaway pixel still
/// shows up as the brightest possible value rather than silently aliasing
/// to a small one.
pub trait HeatmapTarget: Send + Sync {
    /// Hand a finished row of per-pixel timings to the target. Coordinates
    /// follow the same convention as `RenderTarget::submit_row`.
    fn submit_timing_row(&self, x: u32, y: u32, timings_ns: &[u32]);

    /// Called once by `render()` after every timing row has been submitted.
    /// Default no-op; PNG-on-disk heatmap targets ignore it because saving
    /// is an explicit user action.
    fn finish(&self) {}
}

/// How a `PngHeatmapTarget` maps timing values to grayscale brightness
/// when saving. Both modes apply the same 99th-percentile clamp first
/// (so the brightest 1% of pixels saturate at white in either case);
/// the difference is how values *below* the cutoff are mapped onto
/// `[0, 254]`.
///
/// - `Linear`: brightness scales directly with timing relative to the
///   cutoff. Cheapest mathematically; faithfully represents a roughly
///   uniform distribution.
/// - `Log`: brightness scales as `ln(1 + t) / ln(1 + cutoff)`. The
///   `+1` keeps the formula well-defined at `t = 0`. Compresses the
///   bright end and expands gradient detail in the body of the
///   distribution. Useful when the timing distribution is heavy-tailed
///   even after the percentile clamp — typical of scenes containing
///   complex meshes alongside cheap primitives.
#[derive(Copy, Clone, Debug)]
pub enum HeatmapScale {
    Linear,
    Log,
}

/// Heatmap target backed by an in-memory buffer of per-pixel timings.
/// Stores `u32` nanoseconds per pixel — 4 bytes per pixel, so a 2048²
/// render uses 16 MB of timing data, comparable to one channel of a
/// rendered PNG.
///
/// `save(path, scale)` normalizes against the 99th percentile of
/// timings (not the absolute max) and writes a single-channel grayscale
/// PNG where black = fastest pixel and white = at-or-above the 99th
/// percentile. The percentile clamp keeps a handful of pathologically
/// slow pixels from dominating the dynamic range; the `scale` parameter
/// chooses how the rest of the distribution gets mapped to grayscale.
/// See `HeatmapScale` for the two options.
pub struct PngHeatmapTarget {
    buffer: Mutex<Vec<u32>>,
    width: u32,
    height: u32,
}

impl PngHeatmapTarget {
    pub fn new(width: u32, height: u32) -> Self {
        let n = (width as usize) * (height as usize);
        PngHeatmapTarget {
            buffer: Mutex::new(vec![0u32; n]),
            width,
            height,
        }
    }

    /// Save the accumulated timing buffer as a grayscale PNG. Consumes
    /// the target. `scale` chooses how values below the 99th-percentile
    /// cutoff get mapped to `[0, 254]`; pixels at or above the cutoff
    /// always clamp to 255 (white) regardless of mode.
    pub fn save(self, path: impl AsRef<Path>, scale: HeatmapScale) -> ImageResult<()> {
        let buffer = self.buffer.into_inner().unwrap();

        // Normalize against the 99th percentile rather than the absolute
        // max. A small clone + `select_nth_unstable` pass partitions
        // around the percentile in O(n) average time; the original
        // buffer is left in row-major order so we can write the image
        // in a single pass below.
        //
        // The `.max(1)` guards against a degenerate all-zero render —
        // without it we'd divide by zero. Black-everywhere is the
        // right output in that case.
        let cutoff = if buffer.is_empty() {
            1
        } else {
            let mut sorted = buffer.clone();
            let percentile_idx = (sorted.len() * 99) / 100;
            let (_, p99, _) = sorted.select_nth_unstable(percentile_idx);
            (*p99).max(1)
        };

        // Precompute `ln(1 + cutoff)` once for log scaling.
        let log_cutoff_plus_one = ((cutoff as f64) + 1.0).ln();

        let mut img = image::ImageBuffer::<image::Luma<u8>, Vec<u8>>::new(
            self.width, self.height,
        );

        for (i, &t) in buffer.iter().enumerate() {
            // Pixels at or above the 99th percentile clamp to white in
            // both modes; everything else normalizes against the cutoff
            // according to `scale`.
            let v = if t >= cutoff {
                255
            } else {
                match scale {
                    HeatmapScale::Linear => {
                        // u64 multiply to avoid u32 overflow.
                        ((t as u64) * 255 / (cutoff as u64)) as u8
                    }
                    HeatmapScale::Log => {
                        // ln(1 + t) / ln(1 + cutoff) * 255. The `+ 1`
                        // keeps the numerator finite at t = 0; for
                        // t = cutoff the ratio would equal 1.0, but
                        // values that high already took the clamp
                        // branch above so we never hit that case here.
                        let num = ((t as f64) + 1.0).ln();
                        ((num / log_cutoff_plus_one) * 255.0) as u8
                    }
                }
            };
            let x = (i as u32) % self.width;
            let y = (i as u32) / self.width;
            img.put_pixel(x, y, image::Luma([v]));
        }

        img.save(path)
    }
}

impl HeatmapTarget for PngHeatmapTarget {
    fn submit_timing_row(&self, x: u32, y: u32, timings_ns: &[u32]) {
        let mut buf = self.buffer.lock().unwrap();
        let row_start = (y as usize) * (self.width as usize) + (x as usize);
        // Single slice copy per row — same cost characteristics as
        // PngTarget's row write, just to a different backing buffer.
        buf[row_start..row_start + timings_ns.len()].copy_from_slice(timings_ns);
    }
}

/// Wraps a `HeatmapTarget` and offsets every submitted row's coordinates
/// by `(dx, dy)`. Same role and same convention as `OffsetTarget` (the
/// `RenderTarget` analogue): does not propagate `finish()` to the inner
/// target, since multiple offset wrappers commonly share one inner.
pub struct OffsetHeatmapTarget<'a, H: HeatmapTarget + ?Sized + 'a> {
    inner: &'a H,
    dx: u32,
    dy: u32,
}

impl<'a, H: HeatmapTarget + ?Sized + 'a> OffsetHeatmapTarget<'a, H> {
    pub fn new(inner: &'a H, dx: u32, dy: u32) -> Self {
        OffsetHeatmapTarget { inner, dx, dy }
    }
}

impl<'a, H: HeatmapTarget + ?Sized + 'a> HeatmapTarget for OffsetHeatmapTarget<'a, H> {
    fn submit_timing_row(&self, x: u32, y: u32, timings_ns: &[u32]) {
        self.inner.submit_timing_row(x + self.dx, y + self.dy, timings_ns);
    }
    // finish() intentionally not propagated — see struct doc.
}
