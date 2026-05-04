// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Library crate for the raytracer project.
//!
//! Currently exposes only the SDL (scene definition language) module.
//! The renderer itself lives in `main.rs`'s private module tree for
//! now; once Phase 2 of the SDL plan begins we'll move `render` and
//! `scenes` here so the SDL bindings can reference them directly.

pub mod sdl;
