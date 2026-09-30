// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! `sdl-run`: ad-hoc evaluator for SDL scripts.
//!
//! Usage:
//!
//! ```sh
//! cargo run --bin sdl_run -- path/to/script.lisp
//! ```
//!
//! Reads the file, evaluates every top-level form, and prints the
//! value of the final form. A script error prints its position-tagged
//! message and exits with status 1.

use std::env;
use std::fs;
use std::process;

use raytracer::sdl::{self, Value};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: {} <script.lisp>", args[0]);
        process::exit(2);
    }
    let path = &args[1];
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading {}: {}", path, e);
            process::exit(1);
        }
    };
    match sdl::catch_errors(|| sdl::read_and_eval(&source, path)) {
        Ok(Value::Nil) => {} // Don't print nil; it's noise for scripts run for side-effects.
        Ok(v) => println!("{}", v),
        Err(message) => {
            eprintln!("{}", message);
            process::exit(1);
        }
    }
}
