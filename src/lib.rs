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
//! Exposes the renderer (`render`) and the scene definition language
//! (`sdl`). The binary crate (`main.rs`) loads scenes from
//! `scenes/*.lisp` via the SDL — Phase 8 deleted the older
//! `src/scenes.rs` module of hand-written Rust scenes once every
//! scene had a parallel `.lisp` definition. `tests/sdl_suite.rs`
//! consumes the SDL via `raytracer::sdl`.

pub mod render;
pub mod sdl;
