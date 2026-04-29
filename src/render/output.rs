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

use std::path::Path;
use std::sync::Mutex;

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

    /// Optional end-of-render hook. Default no-op. Streaming targets can
    /// override this to send a "render complete" signal; PNG targets
    /// don't need it because saving is an explicit user action.
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
