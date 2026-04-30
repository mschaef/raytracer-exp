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
//! The trait deliberately exposes only what `render()` produces: a starting
//! `(x, y)` coordinate and a contiguous slice of `[u8; 3]` pixels. Order of
//! `submit_row` calls is unspecified — under parallel rendering, rows arrive
//! in whatever order Rayon's worker threads happen to produce them, so any
//! impl must tolerate out-of-order rows.

extern crate image;

use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use image::ImageResult;

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
    fn submit_row(&self, x: u32, y: u32, row: &[[u8; 3]]);

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
    pub fn put_pixel(&self, x: u32, y: u32, color: [u8; 3]) {
        let mut buf = self.buffer.lock().unwrap();
        buf.put_pixel(x, y, image::Rgb(color));
    }

    /// Save the accumulated buffer as a PNG. Consumes the target.
    pub fn save(self, path: impl AsRef<Path>) -> ImageResult<()> {
        self.buffer.into_inner().unwrap().save(path)
    }
}

impl RenderTarget for PngTarget {
    fn submit_row(&self, x: u32, y: u32, row: &[[u8; 3]]) {
        let mut buf = self.buffer.lock().unwrap();
        for (i, p) in row.iter().enumerate() {
            buf.put_pixel(x + i as u32, y, image::Rgb(*p));
        }
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
    fn submit_row(&self, x: u32, y: u32, row: &[[u8; 3]]) {
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
    fn submit_row(&self, x: u32, y: u32, row: &[[u8; 3]]) {
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

/// Heatmap target backed by an in-memory buffer of per-pixel timings.
/// Stores `u32` nanoseconds per pixel — 4 bytes per pixel, so a 2048²
/// render uses 16 MB of timing data, comparable to one channel of a
/// rendered PNG.
///
/// `save(path)` finds the maximum across the whole buffer, normalizes
/// linearly into `[0, 255]`, and writes a single-channel grayscale PNG
/// where black = fastest pixel and white = slowest. Linear normalization
/// is the simplest mapping; for pathological scenes where a few
/// expensive pixels swamp the rest, log-scaling or percentile clamping
/// would compress the bright end. Easy enhancement to add later.
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
    /// the target.
    pub fn save(self, path: impl AsRef<Path>) -> ImageResult<()> {
        let buffer = self.buffer.into_inner().unwrap();

        // Max for normalization. Guard against an all-zero buffer (e.g.
        // a render that never emitted any timings) so we don't divide
        // by zero — every pixel ends up black in that case, which is
        // the right answer.
        let max = *buffer.iter().max().unwrap_or(&0);
        let max = max.max(1);

        let mut img = image::ImageBuffer::<image::Luma<u8>, Vec<u8>>::new(
            self.width, self.height,
        );

        for (i, &t) in buffer.iter().enumerate() {
            // `t * 255 / max` in u64 to avoid u32 overflow on the multiply.
            let v = ((t as u64) * 255 / (max as u64)) as u8;
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
