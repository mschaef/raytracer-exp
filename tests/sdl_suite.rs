// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Integration test harness for the SDL.
//!
//! Each test in this file evaluates one `tests/sdl/<name>.lisp`
//! script in a fresh interpreter. An SDL panic (e.g. from a failed
//! `assert`) bubbles out as a Rust test failure naming the offending
//! script.
//!
//! Run all scripts:
//!
//! ```sh
//! cargo test --test sdl_suite
//! ```
//!
//! Run a single script (Rust filters tests by name substring):
//!
//! ```sh
//! cargo test --test sdl_suite -- closures
//! ```
//!
//! ## Adding a new test script
//!
//! 1. Drop a `tests/sdl/<name>.lisp` file. Use `_` rather than `-`
//!    in the filename so it's a valid Rust identifier.
//! 2. Add a line `sdl_test!(<name>);` below.
//!
//! The macro line is one-time boilerplate per file; in exchange,
//! each script becomes its own `#[test]`, so cargo reports a real
//! test count and you can run scripts individually by name.
//!
//! The `all_scripts_have_a_test` test below is a guard: it walks the
//! directory and panics if any `.lisp` file is missing its
//! `sdl_test!` declaration, so a forgotten line shows up as a test
//! failure rather than silently skipping the script.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use raytracer::sdl;

/// The list of declared script tests. Kept manually in sync with
/// `tests/sdl/*.lisp`. The `all_scripts_have_a_test` test below
/// catches drift.
const DECLARED: &[&str] = &[
    "arithmetic",
    "closures",
    "comparison",
    "control_flow",
    "def_let",
    "destructuring",
    "fn_form",
    "literals",
    "logic",
    "map_ops",
    "predicates",
    "quote",
    "recur",
    "strings",
    "vec_ops",
];

fn run_script(name: &str) {
    let path = sdl_dir().join(format!("{}.lisp", name));
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("could not read {}: {}", path.display(), e));
    sdl::read_and_eval(&source, &format!("{}.lisp", name));
}

fn sdl_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("sdl")
}

macro_rules! sdl_test {
    ($name:ident) => {
        #[test]
        fn $name() {
            run_script(stringify!($name));
        }
    };
}

sdl_test!(arithmetic);
sdl_test!(closures);
sdl_test!(comparison);
sdl_test!(control_flow);
sdl_test!(def_let);
sdl_test!(destructuring);
sdl_test!(fn_form);
sdl_test!(literals);
sdl_test!(logic);
sdl_test!(map_ops);
sdl_test!(predicates);
sdl_test!(quote);
sdl_test!(recur);
sdl_test!(strings);
sdl_test!(vec_ops);

/// Guard test: every `.lisp` file in `tests/sdl/` must have a
/// corresponding `sdl_test!` declaration above. Catches "added a
/// file but forgot the test line" oversights.
#[test]
fn all_scripts_have_a_test() {
    let declared: HashSet<&str> = DECLARED.iter().copied().collect();
    let mut on_disk: HashSet<String> = HashSet::new();
    let entries = fs::read_dir(sdl_dir())
        .unwrap_or_else(|e| panic!("could not read tests/sdl/: {}", e));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("lisp") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                on_disk.insert(stem.to_string());
            }
        }
    }
    let on_disk_refs: HashSet<&str> = on_disk.iter().map(|s| s.as_str()).collect();

    let undeclared: Vec<&&str> = on_disk_refs.difference(&declared).collect();
    let missing_files: Vec<&&str> = declared.difference(&on_disk_refs).collect();

    if !undeclared.is_empty() || !missing_files.is_empty() {
        let mut msg = String::new();
        if !undeclared.is_empty() {
            let mut names: Vec<&&&str> = undeclared.iter().collect();
            names.sort();
            msg.push_str(&format!(
                "\n  scripts on disk with no sdl_test! line: {:?}",
                names
            ));
        }
        if !missing_files.is_empty() {
            let mut names: Vec<&&&str> = missing_files.iter().collect();
            names.sort();
            msg.push_str(&format!(
                "\n  declared sdl_test!s with no script file:  {:?}",
                names
            ));
        }
        panic!("test declarations out of sync with tests/sdl/:{}", msg);
    }
}
