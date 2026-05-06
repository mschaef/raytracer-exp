// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! SDL render-target wrapper.
//!
//! The host's `RenderTarget` ecosystem is built around lifetimes:
//! `OffsetTarget<'a, T>` and `ProgressTarget<'a, T>` borrow their
//! inner target. That fits stack-allocated composition (the
//! four-quadrant render in `main.rs`) but doesn't fit the SDL, where
//! values are heap-allocated and reference-counted with no useful
//! 'static lifetime to point at.
//!
//! [`SdlTarget`] is the bridge. It owns an `Arc<dyn RenderTarget>` —
//! Arc rather than Rc so the trait-object's `Send + Sync` auto-traits
//! propagate cleanly to the renderer's parallel workers — and uses the
//! Arc-based [`ArcOffsetTarget`](crate::render::output::ArcOffsetTarget)
//! / [`ArcProgressTarget`](crate::render::output::ArcProgressTarget)
//! variants for composition.
//!
//! The struct also keeps an optional `Arc<PngTarget>` alongside the
//! abstract target, set only when the value was constructed via
//! `(png-target ...)`. That extra handle is what lets `(save-png ...)`
//! reach into the concrete target without a runtime downcast. Wrapping
//! a `png-target` in `(offset-target ...)` or `(progress-target ...)`
//! propagates the handle, so `save-png` keeps working through wrappers.

use std::path::Path;
use std::sync::Arc;

use crate::render::output::{
    ArcOffsetTarget, ArcProgressTarget, PngTarget, RenderTarget,
};

/// SDL-side wrapper around a render target.
///
/// Held inside [`Value::Target`](crate::sdl::value::Value) as
/// `Rc<SdlTarget>` so script-side aliasing is cheap.
pub struct SdlTarget {
    /// The renderer-facing handle. Always populated.
    target: Arc<dyn RenderTarget>,

    /// Concrete PNG handle, if this value was constructed via
    /// `png-target` (or wraps one). Lets `save-png` save without a
    /// runtime downcast.
    png: Option<Arc<PngTarget>>,
}

impl SdlTarget {
    /// `(png-target width height)` — a fresh in-memory PNG buffer.
    pub fn png(width: u32, height: u32) -> Self {
        let png = Arc::new(PngTarget::new(width, height));
        // Coerce Arc<PngTarget> to Arc<dyn RenderTarget>. The clone
        // bumps the refcount so the same buffer is reachable through
        // both handles — submit_row goes through `target`, save goes
        // through `png`.
        let target: Arc<dyn RenderTarget> = png.clone();
        SdlTarget {
            target,
            png: Some(png),
        }
    }

    /// `(offset-target inner dx dy)` — wraps `inner` so its rows land
    /// at `(dx, dy)` instead of `(0, 0)`. Save-ability is inherited
    /// from `inner`: wrapping a png-target keeps `save-png` working.
    pub fn offset(inner: &SdlTarget, dx: u32, dy: u32) -> Self {
        let wrapped: Arc<dyn RenderTarget> =
            Arc::new(ArcOffsetTarget::new(inner.target.clone(), dx, dy));
        SdlTarget {
            target: wrapped,
            png: inner.png.clone(),
        }
    }

    /// `(progress-target inner total-rows label)` — wraps `inner`
    /// with row-completion progress reporting on stderr.
    pub fn progress(inner: &SdlTarget, total_rows: u32, label: String) -> Self {
        let wrapped: Arc<dyn RenderTarget> = Arc::new(ArcProgressTarget::new(
            inner.target.clone(),
            total_rows,
            label,
        ));
        SdlTarget {
            target: wrapped,
            png: inner.png.clone(),
        }
    }

    /// Borrow the renderer-facing handle. The renderer receives this
    /// as `&dyn RenderTarget` and dispatches `submit_row` calls
    /// through the trait object.
    pub fn as_render_target(&self) -> &dyn RenderTarget {
        &*self.target
    }

    /// Save the underlying PNG buffer to `path`. Returns `Err` if this
    /// target was not constructed via (or wrapped around) a
    /// `png-target` — every other variant has no PNG buffer to save.
    /// The `save` itself takes `&PngTarget`, so the buffer remains
    /// usable for further rendering or a second save.
    pub fn save_png(&self, path: &Path) -> Result<(), String> {
        match &self.png {
            Some(p) => p.save(path).map_err(|e| e.to_string()),
            None => Err(
                "save-png expected a png-target (or a wrapper around one)".to_string(),
            ),
        }
    }

    /// Whether this target supports `save-png`. Useful for the
    /// `target?` predicate's narrower siblings if we ever add them.
    pub fn is_png(&self) -> bool {
        self.png.is_some()
    }
}
