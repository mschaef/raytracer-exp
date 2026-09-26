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
use std::sync::Arc;
use std::sync::Mutex;
use std::fmt;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use image::ImageResult;

use super::color::LinearColor;
use super::view::{ViewTransform, WIRE_FLAG_VIEW};

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

    /// Called once by `render()` before any row is submitted, with the
    /// scene's view transform (exposure and tone curve). Targets that
    /// encode for display (`PngTarget`) use it for the rows that
    /// follow; `StreamTarget` sends it to its receiver. Default no-op.
    /// Wrapper targets forward it.
    fn begin(&self, _view: &ViewTransform) {}

    /// Called once by `render()` after every row has been submitted.
    /// Default no-op. Streaming targets override this to send a
    /// "render complete" signal; `ProgressTarget` uses it to emit a
    /// final newline so subsequent stdout/stderr output starts cleanly.
    /// PNG-on-disk targets ignore it because saving is an explicit
    /// user action.
    fn finish(&self) {}
}

/// A running count of the pixels that clip when a target encodes them:
/// those with any channel over 1.0, where the 8-bit encode clamps them
/// and shifts their colour (see "View transform (tone mapping):
/// implementation plan" in CLAUDE.md). `PngTarget` and `StreamTarget`
/// each keep one and `record` every row they receive, so the count
/// covers everything written to that target, compositing included.
///
/// Lock-free: rows arrive concurrently from the render workers.
pub struct ClipStats {
    pixels: AtomicU64,
    clipped: AtomicU64,
    channels: [AtomicU64; 3],
    /// The largest channel value seen, as `f64` bits. Non-negative
    /// `f64`s order the same way as their bit patterns, so
    /// `fetch_max` on the bits keeps the numeric maximum.
    max_bits: AtomicU64,
}

impl ClipStats {
    pub fn new() -> Self {
        ClipStats {
            pixels: AtomicU64::new(0),
            clipped: AtomicU64::new(0),
            channels: [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)],
            max_bits: AtomicU64::new(0f64.to_bits()),
        }
    }

    /// Count a row of pixels. Negative and NaN values count as 0.
    pub fn record(&self, row: &[LinearColor]) {
        let mut clipped = 0;
        let mut channels = [0u64; 3];
        let mut max = 0.0f64;
        for px in row {
            let mut any = false;
            for c in 0..3 {
                let v = px[c];
                if v > 1.0 {
                    channels[c] += 1;
                    any = true;
                }
                if v > max {
                    max = v;
                }
            }
            if any {
                clipped += 1;
            }
        }
        self.pixels.fetch_add(row.len() as u64, Ordering::Relaxed);
        self.clipped.fetch_add(clipped, Ordering::Relaxed);
        for c in 0..3 {
            self.channels[c].fetch_add(channels[c], Ordering::Relaxed);
        }
        self.max_bits.fetch_max(max.to_bits(), Ordering::Relaxed);
    }

    pub fn report(&self) -> ClipReport {
        ClipReport {
            pixels: self.pixels.load(Ordering::Relaxed),
            clipped: self.clipped.load(Ordering::Relaxed),
            channels: [
                self.channels[0].load(Ordering::Relaxed),
                self.channels[1].load(Ordering::Relaxed),
                self.channels[2].load(Ordering::Relaxed),
            ],
            max: f64::from_bits(self.max_bits.load(Ordering::Relaxed)),
        }
    }
}

impl Default for ClipStats {
    fn default() -> Self {
        ClipStats::new()
    }
}

/// A snapshot of `ClipStats`. Pixels written twice (compositing) count
/// twice.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ClipReport {
    /// Pixels recorded.
    pub pixels: u64,
    /// Pixels with at least one channel over 1.
    pub clipped: u64,
    /// Pixels with red, green and blue over 1, respectively.
    pub channels: [u64; 3],
    /// The largest channel value recorded.
    pub max: f64,
}

impl fmt::Display for ClipReport {
    /// One line, e.g. `clipped: 3.2% of pixels (R 3.1%, G 0.4%, B
    /// 0.0%), max 2.71`, or `clipped: none (max 0.93)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.clipped == 0 {
            return write!(f, "clipped: none (max {:.2})", self.max);
        }
        let pct = |n: u64| 100.0 * n as f64 / self.pixels.max(1) as f64;
        write!(
            f,
            "clipped: {:.1}% of pixels (R {:.1}%, G {:.1}%, B {:.1}%), max {:.2}",
            pct(self.clipped),
            pct(self.channels[0]),
            pct(self.channels[1]),
            pct(self.channels[2]),
            self.max
        )
    }
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
    clip: ClipStats,
    /// The view transform for the rows being written: the default
    /// until a `render()` calls `begin`. Each render sets its own, so
    /// renders composited into one target each keep their own look.
    view: Mutex<ViewTransform>,
}

impl PngTarget {
    pub fn new(width: u32, height: u32) -> Self {
        PngTarget {
            buffer: Mutex::new(image::ImageBuffer::new(width, height)),
            clip: ClipStats::new(),
            view: Mutex::new(ViewTransform::default()),
        }
    }

    /// How many of the pixels written so far clipped in the encode.
    pub fn clip_report(&self) -> ClipReport {
        self.clip.report()
    }

    /// Write a single pixel directly. Useful for compositing operations
    /// that don't fit the row-at-a-time pattern (e.g. drawing a crosshair).
    /// Color is in the same linear space as `submit_row`; the view
    /// transform and sRGB encoding happen internally.
    pub fn put_pixel(&self, x: u32, y: u32, color: LinearColor) {
        let view = *self.view.lock().unwrap();
        let exposed = view.expose(color);
        self.clip.record(&[exposed]);
        let encoded = view.encode_exposed(exposed);
        let mut buf = self.buffer.lock().unwrap();
        buf.put_pixel(x, y, image::Rgb(encoded));
    }

    /// Save the accumulated buffer as a PNG. Borrows `self` so the
    /// target remains usable afterward — useful for the SDL where
    /// targets live behind a shared pointer and consumption would
    /// require ownership juggling. The underlying `image::ImageBuffer`
    /// has its own `&self` save, so this is just a thin shim that
    /// holds the mutex for the duration of the I/O.
    pub fn save(&self, path: impl AsRef<Path>) -> ImageResult<()> {
        self.buffer.lock().unwrap().save(path)
    }
}

impl RenderTarget for PngTarget {
    fn begin(&self, view: &ViewTransform) {
        *self.view.lock().unwrap() = *view;
    }

    fn submit_row(&self, x: u32, y: u32, row: &[LinearColor]) {
        // Encode outside the lock so concurrent workers can do the
        // linear → sRGB conversion in parallel and only contend for the
        // pixel-buffer write itself. The temporary Vec is per-call, on
        // the order of a few KB at typical row widths — invisible
        // against ray-tracing cost.
        let view = *self.view.lock().unwrap();
        // The clip report counts values after exposure, before the
        // curve: what the curve has to bring into range.
        let exposed: Vec<LinearColor> = row.iter().map(|c| view.expose(*c)).collect();
        self.clip.record(&exposed);
        let encoded: Vec<[u8; 3]> = exposed.iter().map(|c| view.encode_exposed(*c)).collect();

        let mut buf = self.buffer.lock().unwrap();
        for (i, p) in encoded.iter().enumerate() {
            buf.put_pixel(x + i as u32, y, image::Rgb(*p));
        }
    }
}

/// TCP-streaming target. Opens a connection on construction, sends a
/// header with the image dimensions (and the view transform, when it
/// isn't the default) at `begin`, then writes one length-prefixed
/// message per `submit_row` call. The receiver (e.g. the
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
///   flags   u32      bit 0 (WIRE_FLAG_VIEW): a view-transform block
///                    follows. Other bits are reserved; receivers reject
///                    them.
///
/// View-transform block (only when flags bit 0 is set; see
/// `ViewTransform::write_wire`):
///   curve    u32     0 clip, 1 hue-clip, 2 reinhard (1 param:
///                    white), 3 agx
///   exposure f32     stops
///   nparams  u32
///   params   [f32; nparams]
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
/// to do (sRGB, tone mapping, HDR pass-through). The pixels stay
/// scene-linear and unclamped even when the scene has a view transform:
/// the transform travels in the header and the receiver applies it, so
/// it keeps the full values (for adjusting exposure live, saving HDR, or
/// reporting clipping). The default transform sends flags 0 and no
/// block, the same bytes as before view transforms existed.
///
/// A stream carries one view transform: the header goes out at the
/// first `begin` (or before the first row, with the default, if no
/// `begin` came). A later `begin` with a different transform is ignored
/// with a warning.
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
    inner: Mutex<StreamState>,
    clip: ClipStats,
}

struct StreamState {
    stream: TcpStream,
    width: u32,
    height: u32,
    /// The view transform the header announced, once it's been sent.
    sent: Option<ViewTransform>,
}

impl StreamState {
    /// Send the header for `view` if it hasn't gone yet, and return the
    /// transform the stream carries.
    fn ensure_header(&mut self, view: &ViewTransform) -> ViewTransform {
        if let Some(sent) = self.sent {
            return sent;
        }
        let mut hdr = Vec::with_capacity(32);
        hdr.extend_from_slice(b"RTVW");
        hdr.extend_from_slice(&self.width.to_le_bytes());
        hdr.extend_from_slice(&self.height.to_le_bytes());
        if view.is_default() {
            hdr.extend_from_slice(&0u32.to_le_bytes());
        } else {
            hdr.extend_from_slice(&WIRE_FLAG_VIEW.to_le_bytes());
            view.write_wire(&mut hdr);
        }
        // Errors are dropped, as for rows (see `submit_row`).
        let _ = self.stream.write_all(&hdr);
        self.sent = Some(*view);
        *view
    }
}

impl StreamTarget {
    /// Connect to the receiver at `addr` (e.g. `"127.0.0.1:9999"`).
    /// The header follows at `begin`, once the view transform is known;
    /// subsequent `submit_row` calls stream rows on the same connection.
    /// Returns an `io::Error` if the connection fails — the renderer can
    /// decide how to react (today, `main.rs` aborts with a clear
    /// message).
    pub fn connect(addr: &str, width: u32, height: u32) -> io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        // Disable Nagle: rows are already packed into a single write
        // each, and we'd rather have them on the wire promptly than
        // batched into 40ms windows.
        stream.set_nodelay(true)?;
        Ok(StreamTarget {
            inner: Mutex::new(StreamState { stream, width, height, sent: None }),
            clip: ClipStats::new(),
        })
    }

    /// How many of the pixels sent so far will clip in the receiver's
    /// encode (which clamps each channel, as `PngTarget` does).
    pub fn clip_report(&self) -> ClipReport {
        self.clip.report()
    }
}

impl RenderTarget for StreamTarget {
    fn begin(&self, view: &ViewTransform) {
        let mut state = self.inner.lock().unwrap();
        let sent = state.ensure_header(view);
        if sent != *view {
            eprintln!(
                "warning: stream already carries view transform {:?}; ignoring {:?}",
                sent, view
            );
        }
    }

    fn submit_row(&self, x: u32, y: u32, row: &[LinearColor]) {
        let view = self.inner.lock().unwrap().ensure_header(&ViewTransform::default());
        // 12-byte row header + 12 bytes per pixel (3 × f32). Sized exactly
        // so the Vec allocates once and the write is a single contiguous
        // payload.
        //
        // The clip report counts what the receiver's curve will see
        // (after exposure); the pixels themselves go out unexposed.
        let exposed: Vec<LinearColor> = row.iter().map(|c| view.expose(*c)).collect();
        self.clip.record(&exposed);
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
        let _ = self.inner.lock().unwrap().stream.write_all(&buf);
    }

    fn finish(&self) {
        // Flush isn't strictly necessary (no BufWriter sits in front of
        // the socket) but keeps the door open for later buffering. Drop
        // closes the connection, which the receiver reads as EOF and
        // treats as "render complete".
        let _ = self.inner.lock().unwrap().stream.flush();
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
    fn begin(&self, view: &ViewTransform) {
        self.inner.begin(view);
    }

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
    fn begin(&self, view: &ViewTransform) {
        self.inner.begin(view);
    }

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

/// Owned counterpart to [`OffsetTarget`]. Same semantics — every
/// `submit_row` is forwarded to `inner` with `(dx, dy)` added — but
/// holds an `Arc<dyn RenderTarget>` instead of a borrowed reference,
/// so the wrapper itself can live in a heap-allocated, reference-
/// counted value (e.g. an SDL [`Value::Target`](crate::sdl::value::Value)).
///
/// `OffsetTarget` is preferred where the inner target's lifetime is
/// statically known (the four-quadrant render in `main.rs`); this
/// variant exists for callers that build target trees at runtime.
/// Like `OffsetTarget`, does not propagate `finish()` to the inner
/// target — multiple `ArcOffsetTarget`s commonly share one inner.
pub struct ArcOffsetTarget {
    inner: Arc<dyn RenderTarget>,
    dx: u32,
    dy: u32,
}

impl ArcOffsetTarget {
    pub fn new(inner: Arc<dyn RenderTarget>, dx: u32, dy: u32) -> Self {
        ArcOffsetTarget { inner, dx, dy }
    }
}

impl RenderTarget for ArcOffsetTarget {
    fn begin(&self, view: &ViewTransform) {
        self.inner.begin(view);
    }

    fn submit_row(&self, x: u32, y: u32, row: &[LinearColor]) {
        self.inner.submit_row(x + self.dx, y + self.dy, row);
    }
    // finish() intentionally not propagated — see struct doc.
}

/// Owned counterpart to [`ProgressTarget`]. Same behavior — forwards
/// every `submit_row` to `inner` and prints `\r{label}: n/total rows`
/// progress to stderr — but holds an `Arc<dyn RenderTarget>` so it
/// composes cleanly into runtime-built target trees (see
/// [`ArcOffsetTarget`] for the rationale).
///
/// Propagates `finish()` to the inner target, matching `ProgressTarget`'s
/// rule.
pub struct ArcProgressTarget {
    inner: Arc<dyn RenderTarget>,
    total_rows: u32,
    completed: AtomicU32,
    label: String,
}

impl ArcProgressTarget {
    pub fn new(inner: Arc<dyn RenderTarget>, total_rows: u32, label: String) -> Self {
        ArcProgressTarget {
            inner,
            total_rows,
            completed: AtomicU32::new(0),
            label,
        }
    }
}

impl RenderTarget for ArcProgressTarget {
    fn begin(&self, view: &ViewTransform) {
        self.inner.begin(view);
    }

    fn submit_row(&self, x: u32, y: u32, row: &[LinearColor]) {
        self.inner.submit_row(x, y, row);

        let n = self.completed.fetch_add(1, Ordering::Relaxed) + 1;

        let stderr = std::io::stderr();
        let mut handle = stderr.lock();
        let _ = write!(handle, "\r  {}: {}/{} rows ", self.label, n, self.total_rows);
        let _ = handle.flush();
    }

    fn finish(&self) {
        eprintln!();
        self.inner.finish();
    }
}

/// Where per-pixel diagnostic data goes when a heat-map render is requested.
///
/// Parallel to `RenderTarget` but separate: the renderer can run with no
/// heatmap (zero overhead), with one, or with multiple kinds of diagnostic
/// targets without the pixel target needing to know.
///
/// The carried value is `u32` per pixel — what it *means* is whatever the
/// renderer chose to feed in. Today the renderer can feed two metrics:
/// per-pixel render time in nanoseconds, and per-pixel adaptive-sample
/// count. The target itself doesn't care which: it just stores u32 values
/// and normalizes them at save time. A pixel value that exceeds `u32::MAX`
/// (e.g. a runaway timing) saturates at the cap rather than wrapping —
/// the renderer uses `try_from` for the cast so an outlier still appears
/// as the brightest possible value rather than silently aliasing to a
/// small one.
pub trait HeatmapTarget: Send + Sync {
    /// Hand a finished row of per-pixel metric values to the target.
    /// Coordinates follow the same convention as `RenderTarget::submit_row`.
    /// The values' semantics (ns of render time, samples taken, etc.)
    /// are determined by which `HeatmapTargets` slot the renderer fed
    /// them through, not by the target itself.
    fn submit_metric_row(&self, x: u32, y: u32, metric: &[u32]);

    /// Called once by `render()` after every metric row has been submitted.
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
    /// For the clip map, whose metric is a pixel's largest channel
    /// ×1000. A fixed scale, not the 99th percentile, so that images
    /// compare: black up to 1.0 (not clipped), then grey from 64 just
    /// over 1.0 up to white at 8.0 (three stops over) and beyond.
    ClipStops,
}

/// Heatmap target backed by an in-memory buffer of per-pixel `u32`
/// metric values. 4 bytes per pixel, so a 2048² render uses 16 MB of
/// metric data, comparable to one channel of a rendered PNG. The
/// metric's *meaning* is whatever the renderer fed in — per-pixel
/// timing in nanoseconds, adaptive sample count, etc. — but the
/// storage and the save-time normalization don't depend on it.
///
/// `save(path, scale)` normalizes against the 99th percentile of
/// values (not the absolute max) and writes a single-channel grayscale
/// PNG where black = lowest-value pixel and white = at-or-above the
/// 99th percentile. The percentile clamp keeps a handful of outlier
/// pixels from dominating the dynamic range; the `scale` parameter
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
        // right output in that case. The same guard also covers the
        // tight-distribution case some metrics produce — e.g. a
        // sample-count heatmap where the 99th percentile equals
        // `min_samples`, which is a small positive integer — but
        // there the percentile is already a positive `u32`, so the
        // `.max(1)` is purely belt-and-braces.
        if let HeatmapScale::ClipStops = scale {
            let mut img = image::ImageBuffer::<image::Luma<u8>, Vec<u8>>::new(self.width, self.height);
            for (i, &t) in buffer.iter().enumerate() {
                let x = (i as u32) % self.width;
                let y = (i as u32) / self.width;
                img.put_pixel(x, y, image::Luma([clip_stops_gray(t)]));
            }
            return img.save(path);
        }

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
                    HeatmapScale::ClipStops => unreachable!(),
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

/// The clip map's metric for a pixel: its largest channel ×1000,
/// saturating (so a value of 1.0 is exactly 1000).
pub fn clip_metric(color: &LinearColor) -> u32 {
    let m = color[0].max(color[1]).max(color[2]);
    if m > 0.0 {
        (m * 1000.0).round().min(u32::MAX as f64) as u32
    } else {
        0
    }
}

/// Grey level for a clip-map metric (see `HeatmapScale::ClipStops`).
fn clip_stops_gray(metric: u32) -> u8 {
    if metric <= 1000 {
        return 0;
    }
    let stops = (metric as f64 / 1000.0).log2();
    (64.0 + 191.0 * (stops / 3.0).min(1.0)).round() as u8
}

impl HeatmapTarget for PngHeatmapTarget {
    fn submit_metric_row(&self, x: u32, y: u32, metric: &[u32]) {
        let mut buf = self.buffer.lock().unwrap();
        let row_start = (y as usize) * (self.width as usize) + (x as usize);
        // Single slice copy per row — same cost characteristics as
        // PngTarget's row write, just to a different backing buffer.
        buf[row_start..row_start + metric.len()].copy_from_slice(metric);
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
    fn submit_metric_row(&self, x: u32, y: u32, metric: &[u32]) {
        self.inner.submit_metric_row(x + self.dx, y + self.dy, metric);
    }
    // finish() intentionally not propagated — see struct doc.
}

#[cfg(test)]
mod clip_tests {
    use super::*;

    #[test]
    fn clip_stats_count_channels_and_max() {
        let stats = ClipStats::new();
        stats.record(&[[0.5, 0.5, 0.5], [1.2, 0.9, 0.1], [2.5, 1.5, 0.0]]);
        stats.record(&[[1.0, 1.0, 1.0], [0.0, 0.0, 3.0]]);
        let r = stats.report();
        assert_eq!(r.pixels, 5);
        // Exactly 1.0 doesn't clip.
        assert_eq!(r.clipped, 3);
        assert_eq!(r.channels, [2, 1, 1]);
        assert_eq!(r.max, 3.0);
        assert_eq!(
            r.to_string(),
            "clipped: 60.0% of pixels (R 40.0%, G 20.0%, B 20.0%), max 3.00"
        );
    }

    #[test]
    fn clip_stats_none_and_odd_values() {
        let stats = ClipStats::new();
        assert_eq!(stats.report().to_string(), "clipped: none (max 0.00)");
        stats.record(&[[-1.0, f64::NAN, 0.25]]);
        let r = stats.report();
        assert_eq!((r.pixels, r.clipped, r.max), (1, 0, 0.25));
        assert_eq!(r.to_string(), "clipped: none (max 0.25)");
    }

    #[test]
    fn png_target_counts_rows_and_single_pixels() {
        let t = PngTarget::new(4, 2);
        t.submit_row(0, 0, &[[0.2, 0.2, 0.2], [1.5, 0.2, 0.2], [0.0, 0.0, 0.0], [0.9, 0.9, 1.1]]);
        t.put_pixel(0, 1, [0.0, 4.0, 0.0]);
        let r = t.clip_report();
        assert_eq!((r.pixels, r.clipped, r.channels, r.max), (5, 3, [1, 1, 1], 4.0));
    }

    /// Records the transform each `begin` receives.
    struct Spy(Mutex<Vec<ViewTransform>>);

    impl RenderTarget for Spy {
        fn begin(&self, view: &ViewTransform) {
            self.0.lock().unwrap().push(*view);
        }
        fn submit_row(&self, _x: u32, _y: u32, _row: &[LinearColor]) {}
    }

    #[test]
    fn begin_passes_through_wrappers() {
        use super::super::view::ToneCurve;
        let view = ViewTransform { exposure: -1.0, curve: ToneCurve::HueClip };
        let spy = Spy(Mutex::new(Vec::new()));
        OffsetTarget::new(&spy, 1, 2).begin(&view);
        ProgressTarget::new(&spy, 4, "t").begin(&view);
        let arc: Arc<dyn RenderTarget> = Arc::new(Spy(Mutex::new(Vec::new())));
        ArcOffsetTarget::new(arc.clone(), 0, 0).begin(&view);
        ArcProgressTarget::new(arc, 4, "t".to_string()).begin(&view);
        assert_eq!(*spy.0.lock().unwrap(), vec![view, view]);
    }

    #[test]
    fn png_target_applies_the_view_transform() {
        use super::super::view::ToneCurve;
        let t = PngTarget::new(2, 1);
        // Before any `begin`, the default: clipping each channel turns
        // this over-bright orange yellow.
        t.submit_row(0, 0, &[[1.8, 0.9, 0.3], [0.5, 0.5, 0.5]]);
        let clipped = *t.buffer.lock().unwrap().get_pixel(0, 0);
        // Hue-preserving clip keeps it orange; exposure -1 halves the
        // grey and brings the orange's red to 0.9 (so nothing clips).
        t.begin(&ViewTransform { exposure: 0.0, curve: ToneCurve::HueClip });
        t.submit_row(0, 0, &[[1.8, 0.9, 0.3], [0.5, 0.5, 0.5]]);
        let hue = *t.buffer.lock().unwrap().get_pixel(0, 0);
        assert_eq!(hue.0, super::super::view::encode_display([1.0, 0.5, 0.3 / 1.8]));
        assert!(hue.0[1] < clipped.0[1], "{:?} vs {:?}", hue, clipped);
        t.begin(&ViewTransform { exposure: -1.0, curve: ToneCurve::Clip });
        t.submit_row(0, 0, &[[1.8, 0.9, 0.3], [0.5, 0.5, 0.5]]);
        let px = |x| t.buffer.lock().unwrap().get_pixel(x, 0).0;
        assert_eq!(px(0), super::super::view::encode_display([0.9, 0.45, 0.15]));
        assert_eq!(px(1), super::super::view::encode_display([0.25, 0.25, 0.25]));
        // The clip report counts after exposure: two clipped (the first
        // two rows' orange), not three.
        let r = t.clip_report();
        assert_eq!((r.pixels, r.clipped), (6, 2));
        assert_eq!(r.max, 1.8);
    }

    #[test]
    fn stream_sends_the_view_transform_in_its_header() {
        use super::super::view::ToneCurve;
        use std::io::Read;
        use std::net::TcpListener;

        // Read everything a StreamTarget sends for one begin and one row.
        fn capture(view: Option<ViewTransform>, second: Option<ViewTransform>) -> Vec<u8> {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap().to_string();
            let reader = std::thread::spawn(move || {
                let (mut conn, _) = listener.accept().unwrap();
                let mut bytes = Vec::new();
                conn.read_to_end(&mut bytes).unwrap();
                bytes
            });
            {
                let t = StreamTarget::connect(&addr, 3, 2).unwrap();
                if let Some(v) = view {
                    t.begin(&v);
                }
                if let Some(v) = second {
                    t.begin(&v);
                }
                t.submit_row(0, 1, &[[2.5, 0.5, 0.0]]);
            }
            reader.join().unwrap()
        }
        let u32_at = |b: &[u8], i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let f32_at = |b: &[u8], i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);

        // The default transform: the original 16-byte header, flags 0.
        for view in [Some(ViewTransform::default()), None] {
            let b = capture(view, None);
            assert_eq!(&b[0..4], b"RTVW");
            assert_eq!((u32_at(&b, 4), u32_at(&b, 8), u32_at(&b, 12)), (3, 2, 0));
            assert_eq!(b.len(), 16 + 12 + 12);
            // The row: y, x, count, then the pixel, unclamped.
            assert_eq!((u32_at(&b, 16), u32_at(&b, 20), u32_at(&b, 24)), (1, 0, 1));
            assert_eq!(f32_at(&b, 28), 2.5);
        }

        // Anything else: flags bit 0 and the block, and the pixels still
        // go out scene-linear.
        let view = ViewTransform { exposure: -1.0, curve: ToneCurve::HueClip };
        let other = ViewTransform { exposure: 3.0, curve: ToneCurve::Clip };
        let b = capture(Some(view), Some(other));
        assert_eq!(u32_at(&b, 12), WIRE_FLAG_VIEW);
        assert_eq!(ViewTransform::read_wire(&mut &b[16..28]).unwrap(), view);
        assert_eq!(b.len(), 16 + 12 + 12 + 12);
        assert_eq!(f32_at(&b, 40), 2.5);
    }

    #[test]
    fn clip_map_scale() {
        assert_eq!(clip_metric(&[0.5, 1.0, 0.2]), 1000);
        assert_eq!(clip_metric(&[-1.0, 0.0, 0.0]), 0);
        assert_eq!(clip_stops_gray(0), 0);
        assert_eq!(clip_stops_gray(1000), 0);
        assert_eq!(clip_stops_gray(1001), 64);
        // One stop over is a third of the way from 64 to 255; three
        // stops or more is white.
        assert_eq!(clip_stops_gray(2000), 128);
        assert_eq!(clip_stops_gray(8000), 255);
        assert_eq!(clip_stops_gray(u32::MAX), 255);
    }
}
