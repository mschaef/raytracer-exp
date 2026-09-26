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

use raytracer::render::{render, HeatmapTargets, Scene, ViewMode};
use raytracer::render::view::ToneCurve;
use raytracer::render::output::{
    PngTarget,
    StreamTarget,
    ProgressTarget,
    RenderTarget,
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

/// Resolve the diagnostic render-view selector. `RAYTRACER_VIEW` is
/// an opt-in knob for understanding what each shading term
/// contributes to the final image: setting it to `indirect`,
/// `reflection`, `transmission`, `local`, or `full` makes the
/// renderer return only that component at the primary hit. Unknown
/// values print a warning and fall back to `full`, so a typo
/// doesn't silently render the wrong thing. Phase 4 of the
/// path-tracing plan.
fn view_mode() -> ViewMode {
    let s = match env::var("RAYTRACER_VIEW") {
        Ok(s) => s,
        Err(_) => return ViewMode::Full,
    };
    match s.to_lowercase().as_str() {
        "full" => ViewMode::Full,
        "local" | "direct" => ViewMode::Local,
        "indirect" | "gi" => ViewMode::Indirect,
        "reflection" | "reflect" => ViewMode::Reflection,
        "transmission" | "transmit" => ViewMode::Transmission,
        other => {
            eprintln!(
                "warning: RAYTRACER_VIEW={:?} not recognized \
                 (full|local|indirect|reflection|transmission); using full",
                other
            );
            ViewMode::Full
        }
    }
}

/// The output PNG filename for the main pixel target. `render.png`
/// in `Full` mode (the default) so a normal render produces the
/// same file it always has; `render-{mode}.png` in any other view
/// so successive diagnostic renders don't clobber each other.
fn output_filename(mode: ViewMode) -> &'static str {
    match mode {
        ViewMode::Full => "render.png",
        ViewMode::Local => "render-local.png",
        ViewMode::Indirect => "render-indirect.png",
        ViewMode::Reflection => "render-reflection.png",
        ViewMode::Transmission => "render-transmission.png",
    }
}

/// `RAYTRACER_DECOMP` is the sample-source decomposition switch:
/// when set to any truthy value (anything but unset, empty, or
/// `"0"`), main.rs renders the scene at all four non-`Full` view
/// modes after the main render and writes each component to its
/// own PNG. Together with the canonical `render.png` (always
/// produced) this gives five views of the same scene from a single
/// invocation. Phase 4 of the path-tracing plan.
///
/// Cost: 5x the render time of a single `Full` render. Decomp is a
/// diagnostic, run on demand; the every-day path skips it.
fn decomposition_enabled() -> bool {
    match env::var("RAYTRACER_DECOMP") {
        Ok(v) => !v.is_empty() && v != "0" && v.to_lowercase() != "false",
        Err(_) => false,
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
    heatmaps: HeatmapTargets<'_>,
    scene: &Scene, w: u32, h: u32,
) {
    let parallel = is_parallel();

    // Wrap the destination in a ProgressTarget so the user sees live
    // row-completion updates while the render is in flight. The wrapper
    // forwards every submit_row to the underlying target unchanged and
    // emits a closing newline via finish() (called by render() at end).
    let progress = ProgressTarget::new(target, h, &scene.name);

    let start = Instant::now();
    render(scene, w, h, &progress, heatmaps, parallel);
    let duration = start.elapsed();

    println!("Time elapsed in {} is: {:?} (parallel: {})", scene.name, duration, parallel);
}

/// Apply `RAYTRACER_CURVE` (a tone-curve name, e.g. `hue-clip`) and
/// `RAYTRACER_EXPOSURE` (stops, e.g. `-1`) over the scene's own `:view`,
/// for trying a look without editing the scene. Bad values are fatal,
/// since a render with the wrong look is worse than no render.
fn view_transform_overrides(scene: &mut Scene) {
    if let Ok(name) = env::var("RAYTRACER_CURVE") {
        scene.view.curve = ToneCurve::from_name(&name).unwrap_or_else(|| {
            eprintln!(
                "error: RAYTRACER_CURVE={:?} is not a tone curve (expected one of: {})",
                name,
                ToneCurve::NAMES.join(", ")
            );
            process::exit(2);
        });
    }
    if let Ok(text) = env::var("RAYTRACER_WHITE") {
        let white = match text.trim().parse::<f64>() {
            Ok(w) if w.is_finite() && w > 0.0 => w,
            _ => {
                eprintln!("error: RAYTRACER_WHITE={:?} is not a positive number", text);
                process::exit(2);
            }
        };
        match scene.view.curve {
            ToneCurve::Reinhard { .. } => scene.view.curve = ToneCurve::Reinhard { white },
            other => {
                eprintln!(
                    "error: RAYTRACER_WHITE only applies to the reinhard curve (the curve is {})",
                    other.name()
                );
                process::exit(2);
            }
        }
    }
    if let Ok(text) = env::var("RAYTRACER_EXPOSURE") {
        scene.view.exposure = match text.trim().parse::<f64>() {
            Ok(e) if e.is_finite() => e,
            _ => {
                eprintln!("error: RAYTRACER_EXPOSURE={:?} is not a number of stops", text);
                process::exit(2);
            }
        };
    }
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
    eprintln!("  RAYTRACER_VIEW=MODE  Render only one shade_pixel component:");
    eprintln!("                       full (default), local, indirect,");
    eprintln!("                       reflection, transmission. Output goes");
    eprintln!("                       to render-MODE.png for non-full modes.");
    eprintln!("  RAYTRACER_CURVE=NAME Tone curve, overriding the scene's :view");
    eprintln!("                       (clip, hue-clip, reinhard, agx,");
    eprintln!("                       agx-punchy; default reinhard).");
    eprintln!("  RAYTRACER_WHITE=N    Reinhard's white point: the luminance that");
    eprintln!("                       maps to 1 (default 4).");
    eprintln!("  RAYTRACER_EXPOSURE=N Exposure in stops, overriding the scene's");
    eprintln!("                       :view (e.g. -1 halves every value).");
    eprintln!("  RAYTRACER_DECOMP=1   After the main render, also render at");
    eprintln!("                       each non-full view mode, producing");
    eprintln!("                       render-local/indirect/reflection/");
    eprintln!("                       transmission.png. Costs 5x render time.");
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

    let mut scene = load_scene(&script_path);
    // The SDL constructs scenes with `view_mode: ViewMode::Full`;
    // main.rs is the only place that flips it, from the
    // `RAYTRACER_VIEW` env var. Tests evaluate SDL scripts
    // directly (no `RAYTRACER_VIEW` plumbing), so byte-pinned
    // tests always render at `Full` — bit-identical to the
    // pre-Phase-4 renderer.
    scene.view_mode = view_mode();
    view_transform_overrides(&mut scene);
    let (width, height) = image_size();

    // Diagnostic heatmaps: same dimensions as the pixel target.
    //
    // - `time_heatmap` accumulates per-pixel render time in
    //   nanoseconds; saved as `render-heatmap.png`.
    // - `samples_heatmap` accumulates per-pixel adaptive sample
    //   counts; saved as `render-samples.png`. The two views
    //   correlate strongly with each other, but they're not
    //   redundant — time picks up per-sample cost variation (a
    //   ray that hits the teapot's BVH is more expensive than one
    //   that hits a plane, even at the same sample count), while
    //   sample count isolates "where is the adaptive sampler
    //   actually working harder."
    //
    // Both are always built. Switch either to `None` in the
    // `HeatmapTargets` below and `render()` skips its per-pixel
    // bookkeeping for that metric entirely.
    let time_heatmap = PngHeatmapTarget::new(width, height);
    let samples_heatmap = PngHeatmapTarget::new(width, height);
    // Per-pixel average max indirect-bounce depth, scaled by 100
    // (so a 2.4-bounce average lands as 240). Phase 4 of the
    // path-tracing plan — "per-surface convergence visualization"
    // in the plan's language. Saved as `render-depth.png`. For
    // scenes with `indirect_limit == 0` (the default) the metric
    // is identically zero everywhere; the resulting PNG normalizes
    // to all-black, which is the right diagnostic outcome for
    // "no GI happened."
    let depth_heatmap = PngHeatmapTarget::new(width, height);
    // Clip map: each pixel's largest channel, on a fixed scale (black
    // if it's at most 1.0, grey to white for up to three stops over).
    // Saved as `render-clip.png`. Shows where the 8-bit encode clips
    // and shifts colour, and how badly; see "View transform (tone
    // mapping): implementation plan" in CLAUDE.md.
    let clip_heatmap = PngHeatmapTarget::new(width, height);
    let heatmaps = HeatmapTargets {
        time: Some(&time_heatmap),
        samples: Some(&samples_heatmap),
        depth: Some(&depth_heatmap),
        clip: Some(&clip_heatmap),
    };

    // RTVIEW_ADDR=host:port routes pixels to a streaming receiver
    // instead of writing render.png. With the quadrant layout gone
    // there's no more OffsetTarget wrapping — the streaming path
    // and the on-disk path each render straight into a single
    // backing target.
    match env::var("RTVIEW_ADDR") {
        Ok(addr) => {
            let target = StreamTarget::connect(&addr, width, height)
                .expect("rtview receiver not reachable at RTVIEW_ADDR");
            render_into(&target, heatmaps, &scene, width, height);
            println!("{}", target.clip_report());
        }
        Err(_) => {
            let target = PngTarget::new(width, height);
            render_into(&target, heatmaps, &scene, width, height);
            // One line after the timing: how much of the image clipped
            // in the encode (any channel over 1.0), per channel, and
            // the brightest value.
            println!("{}", target.clip_report());
            // `render.png` in `Full` mode (the default — same
            // behavior as every prior phase); `render-{mode}.png`
            // when the user has asked for a diagnostic view via
            // `RAYTRACER_VIEW`. Distinct filenames so successive
            // diagnostic renders don't clobber the canonical
            // `render.png` from a previous `Full` run.
            target.save(output_filename(scene.view_mode)).unwrap();
        }
    }

    // Time heatmap: `HeatmapScale::Log` compresses the bright end so
    // the body of the distribution gets more grayscale gradient —
    // useful when scenes contain a complex mesh alongside cheap
    // primitives, where a handful of expensive pixels otherwise
    // dominate the dynamic range even after the 99th-percentile
    // clamp. Swap to `Linear` to see direct proportional brightness.
    time_heatmap.save("render-heatmap.png", HeatmapScale::Log).unwrap();

    // Sample-count heatmap: `HeatmapScale::Linear`. Sample counts are
    // bounded between `min_samples` and `max_samples` (4..32 with the
    // defaults), so the distribution isn't heavy-tailed and log
    // compression would mislead — proportional brightness is the
    // honest read of "how many samples did this pixel take vs. the
    // typical pixel."
    samples_heatmap.save("render-samples.png", HeatmapScale::Linear).unwrap();

    // Depth heatmap: `HeatmapScale::Linear`. Like the samples
    // heatmap, depth is bounded (between 0 and `100 *
    // indirect_limit`, since the stored value is "average max
    // depth ×100"), so a linear scale is honest. Pixels with no
    // indirect lighting (`indirect_limit == 0` or all paths
    // RR-terminated at the first bounce) read as black; pixels
    // where paths reach the deepest bounces (well-lit open areas
    // with bright surfaces) read brightest. Useful as a "where is
    // path tracing doing more bounce work" view that complements
    // the sample-count heatmap's "where is the adaptive sampler
    // doing more work."
    depth_heatmap.save("render-depth.png", HeatmapScale::Linear).unwrap();

    // Clip map: `HeatmapScale::ClipStops`, a fixed scale rather than
    // the 99th percentile, so clip maps from different renders compare
    // directly and an image with no clipping is all black.
    clip_heatmap.save("render-clip.png", HeatmapScale::ClipStops).unwrap();

    // Sample-source decomposition (Phase 4 of the path-tracing plan).
    // When `RAYTRACER_DECOMP` is set, render the scene at each of the
    // four non-`Full` view modes after the main render, writing one
    // PNG per mode. Together with the canonical `render.png` produced
    // above this gives five views of the same scene — useful for
    // verifying that direct lighting, indirect (GI), reflection, and
    // transmission each behave as expected on their own.
    //
    // Each decomposition render reuses the same scene, just with a
    // different `view_mode`; the renderer at the primary hit returns
    // only the selected component. Recursive rays from inside the
    // shading branches still compute the full radiance at their
    // bounce points, so the indirect / reflection / transmission
    // contributions include everything they bring back from the
    // scene — they're just isolated at the primary level.
    //
    // No diagnostic heatmaps are written per decomp render: they
    // would either collide with each other or require five more
    // filenames, and the canonical heatmaps from the main render
    // are the most diagnostically useful single view anyway.
    // Disabling them via `HeatmapTargets::default()` also skips the
    // per-pixel timing and sample-count bookkeeping inside the
    // renderer, shaving overhead off each decomp pass.
    if decomposition_enabled() {
        // Skip decomposition entirely when streaming — the
        // diagnostic deliverable is PNGs on disk, and serializing
        // five renders to one streaming receiver would be confusing.
        if env::var("RTVIEW_ADDR").is_ok() {
            eprintln!(
                "warning: RAYTRACER_DECOMP ignored when RTVIEW_ADDR is set \
                 (decomposition writes PNGs, not a stream)"
            );
        } else {
            for mode in [
                ViewMode::Local,
                ViewMode::Indirect,
                ViewMode::Reflection,
                ViewMode::Transmission,
            ] {
                // Skip whichever mode the user already requested via
                // RAYTRACER_VIEW — that PNG was just written by the
                // main render, no point overwriting it with a
                // bit-identical render.
                if mode == scene.view_mode {
                    continue;
                }
                scene.view_mode = mode;
                let decomp_target = PngTarget::new(width, height);
                render_into(
                    &decomp_target,
                    HeatmapTargets::default(),
                    &scene,
                    width,
                    height,
                );
                decomp_target.save(output_filename(mode)).unwrap();
            }
        }
    }
}
