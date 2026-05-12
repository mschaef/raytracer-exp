// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::Instant;

use raytracer::render::{render, Scene};
use raytracer::render::output::{
    PngTarget,
    StreamTarget,
    ProgressTarget,
    RenderTarget,
    HeatmapTarget,
    HeatmapScale,
    PngHeatmapTarget,
};
use raytracer::sdl;
use raytracer::sdl::value::Value;

/// Default output dimensions when no `SIZE` env var is set. Square,
/// matches what the pre-stage-3 quadrant layout rendered per scene.
/// Pick a smaller value via `SIZE=512x512` for quick iteration; the
/// teapot at 1024² is slow without a real BVH.
const DEFAULT_SIZE: (u32, u32) = (1024, 1024);

fn is_parallel() -> bool {
    match env::var("PARALLEL") {
        Ok(val) => val.to_lowercase() == "y",
        Err(_) => true
    }
}

/// Resolve the output image dimensions. `SIZE=N` is shorthand for `NxN`;
/// `SIZE=WxH` sets width and height separately. Anything malformed
/// exits with a usage message — bad numeric input is the kind of thing
/// you want to learn about up front, not on every pixel.
fn image_size() -> (u32, u32) {
    let s = match env::var("SIZE") {
        Ok(s) => s,
        Err(_) => return DEFAULT_SIZE,
    };

    let parse_dim = |text: &str, label: &str| -> u32 {
        text.parse::<u32>().unwrap_or_else(|_| {
            eprintln!(
                "error: SIZE {} {:?} is not a non-negative integer",
                label, text,
            );
            process::exit(1);
        })
    };

    match s.split_once('x') {
        Some((w, h)) => (parse_dim(w, "width"), parse_dim(h, "height")),
        None => {
            let n = parse_dim(&s, "dimension");
            (n, n)
        }
    }
}

/// Derive the canonical scene binding from the script's filename.
/// `cuboid_test.lisp` → `cuboid-test-scene`: take the file stem,
/// translate `_` to `-`, append `-scene`. Matches the convention
/// every script in `scenes/` already follows.
fn binding_name_for(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| {
            eprintln!("error: could not extract a file stem from {}", path.display());
            process::exit(1);
        });
    format!("{}-scene", stem.replace('_', "-"))
}

/// Read an SDL scene script from `path` (absolute or relative to CWD)
/// and extract the canonical `<filename>-scene` binding from it. The
/// path passed to `eval_source` is canonicalized so the SDL's
/// `CurrentDirGuard` anchors `(load ...)` calls against the script's
/// real directory, not the CWD.
fn load_scene(path: &Path) -> Scene {
    let source = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error: could not read {}: {}", path.display(), e);
        process::exit(1);
    });

    // The script has loaded — file definitely exists, so canonicalize
    // can resolve. Fall back to the supplied path if canonicalize
    // somehow fails (e.g. inaccessible symlink target): the read
    // already succeeded, so the eval might still work, with the only
    // cost being broken `(load ...)` resolution.
    let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    let env = sdl::default_env();
    sdl::eval_source(&source, &abs_path.to_string_lossy(), &env);

    let binding = binding_name_for(path);
    let value = env.borrow().lookup(&binding).unwrap_or_else(|| {
        eprintln!(
            "error: {} did not define `{}` (derived from filename)",
            path.display(),
            binding
        );
        process::exit(1);
    });

    match value {
        Value::Scene(rc) => (*rc).clone(),
        other => {
            eprintln!(
                "error: {} :: {} expected a scene, got {} ({})",
                path.display(),
                binding,
                other,
                other.type_name()
            );
            process::exit(1);
        }
    }
}

fn render_into<T: RenderTarget + ?Sized>(
    target: &T,
    heatmap: Option<&dyn HeatmapTarget>,
    scene: &Scene, w: u32, h: u32,
) {
    let parallel = is_parallel();

    // Wrap the destination in a ProgressTarget so the user sees live
    // row-completion updates while the render is in flight. The wrapper
    // forwards every submit_row to the underlying target unchanged and
    // emits a closing newline via finish() (called by render() at end).
    let progress = ProgressTarget::new(target, h, &scene.name);

    let start = Instant::now();
    render(scene, w, h, &progress, heatmap, parallel);
    let duration = start.elapsed();

    println!("Time elapsed in {} is: {:?} (parallel: {})", scene.name, duration, parallel);
}

fn usage_and_exit() -> ! {
    let argv0 = env::args().next().unwrap_or_else(|| "raytracer".to_string());
    eprintln!("usage: {} <path-to-scene.lisp>", argv0);
    eprintln!();
    eprintln!("Renders a single SDL scene to render.png in the current");
    eprintln!("directory. The scene script's filename determines the");
    eprintln!("binding to look up: cuboid_test.lisp expects to define");
    eprintln!("a `cuboid-test-scene` value (file stem with hyphens, plus");
    eprintln!("the `-scene` suffix). Every script in scenes/ follows");
    eprintln!("this convention.");
    eprintln!();
    eprintln!("Environment variables:");
    eprintln!("  SIZE=N or SIZE=WxH   Output image dimensions (default {}x{}).",
        DEFAULT_SIZE.0, DEFAULT_SIZE.1);
    eprintln!("  PARALLEL=n           Disable Rayon parallelism (default on).");
    eprintln!("  RTVIEW_ADDR=host:port  Stream pixels to a live receiver");
    eprintln!("                       instead of writing render.png.");
    process::exit(2);
}

fn main() {
    // Single positional argument: the SDL file to load. Paths are
    // taken as-is — absolute paths used directly, relative paths
    // resolved against CWD. The eventual move toward script-driven
    // rendering (multi-scene compositing, animation) is expected to
    // happen by having the script itself call `(render ...)` and
    // `(save-png ...)` — at that point `main.rs` reduces to "evaluate
    // this script in the SDL," and the convention-based binding lookup
    // here goes away.
    let args: Vec<String> = env::args().collect();
    let script_path: PathBuf = match args.len() {
        2 => PathBuf::from(&args[1]),
        _ => usage_and_exit(),
    };

    let scene = load_scene(&script_path);
    let (width, height) = image_size();

    // Parallel heatmap target: same dimensions as the pixel target,
    // accumulating per-pixel render times in nanoseconds. Saved as a
    // separate single-channel PNG (`render-heatmap.png`) at the end.
    // The renderer always populates this; switching to `None` in the
    // `render_into` calls below would skip it.
    let heatmap = PngHeatmapTarget::new(width, height);

    // RTVIEW_ADDR=host:port routes pixels to a streaming receiver
    // instead of writing render.png. With the quadrant layout gone
    // there's no more OffsetTarget wrapping — the streaming path
    // and the on-disk path each render straight into a single
    // backing target.
    match env::var("RTVIEW_ADDR") {
        Ok(addr) => {
            let target = StreamTarget::connect(&addr, width, height)
                .expect("rtview receiver not reachable at RTVIEW_ADDR");
            render_into(&target, Some(&heatmap), &scene, width, height);
        }
        Err(_) => {
            let target = PngTarget::new(width, height);
            render_into(&target, Some(&heatmap), &scene, width, height);
            target.save("render.png").unwrap();
        }
    }

    // `HeatmapScale::Log` compresses the bright end so the body of the
    // distribution gets more grayscale gradient — useful when scenes
    // contain a complex mesh alongside cheap primitives. Swap to
    // `HeatmapScale::Linear` to see direct proportional brightness.
    heatmap.save("render-heatmap.png", HeatmapScale::Log).unwrap();
}
