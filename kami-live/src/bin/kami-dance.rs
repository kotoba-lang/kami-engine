//! kami-dance — headless runner for a `:dance/*` EDN scene.
//!
//! Loads a dance `scene.edn`, lints it, and runs the show for N frames with no
//! GPU — proving the whole clj/edn data path runs as a real program (not just a
//! unit test). Deterministic, so it doubles as a `bb`-style verify / golden gen.
//!
//! ```text
//! kami-dance <scene.edn> [--frames N] [--fps F] [--emit-ir] [--lint-only]
//! ```
//!
//! Exit codes: 0 ok · 1 usage / IO / parse error · 2 the scene has lint *errors*.

use std::process::exit;

use kami_live::lint::{lint_scene, Severity};
use kami_live::scene::{run_headless, DanceScene};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut path: Option<String> = None;
    let mut frames: u32 = 1800;
    let mut fps: f32 = 60.0;
    let mut emit_ir = false;
    let mut lint_only = false;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--frames" => frames = it.next().and_then(|s| s.parse().ok()).unwrap_or(frames),
            "--fps" => fps = it.next().and_then(|s| s.parse().ok()).unwrap_or(fps),
            "--emit-ir" => emit_ir = true,
            "--lint-only" => lint_only = true,
            "-h" | "--help" => {
                usage();
                exit(0);
            }
            s if !s.starts_with('-') && path.is_none() => path = Some(s.to_string()),
            other => {
                eprintln!("kami-dance: unknown argument `{other}`");
                usage();
                exit(1);
            }
        }
    }

    let Some(path) = path else {
        usage();
        exit(1);
    };

    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("kami-dance: cannot read {path}: {e}");
            exit(1);
        }
    };

    // ── lint ────────────────────────────────────────────────────────────────
    let lints = lint_scene(&src);
    let mut errors = 0;
    for l in &lints {
        if l.severity == Severity::Error {
            errors += 1;
        }
        eprintln!("[{}] {} — {}", l.severity, l.path, l.message);
    }
    if lints.is_empty() {
        eprintln!("lint: clean ✓");
    }
    if errors > 0 {
        eprintln!("kami-dance: {errors} lint error(s); not running");
        exit(2);
    }
    if lint_only {
        exit(0);
    }

    // ── run ───────────────────────────────────────────────────────────────--
    let Some(mut scene) = DanceScene::from_edn(&src) else {
        eprintln!("kami-dance: scene is not an EDN map");
        exit(1);
    };
    eprintln!(
        "running \"{}\": {} tracks, avatar \"{}\", {} clip(s) — {frames} frames @ {fps} fps",
        scene.title,
        scene.show.setlist().tracks.len(),
        scene.avatar.vrm,
        scene.clip_names().len(),
    );

    let report = run_headless(&mut scene, frames, fps);

    eprintln!(
        "done: {} frames, beat {} (bar {}), {} reactions fired",
        report.frames, report.final_beat, report.final_bar, report.total_actions
    );
    eprintln!(
        "  avatar: {} VRM mesh(es) on the data path; live2d: {}",
        report.mesh_count,
        if report.live2d_params > 0 {
            format!("{} Cubism params driven", report.live2d_params)
        } else {
            "none".into()
        }
    );
    for (fx, n) in &report.fx_counts {
        eprintln!("  fx :{fx} ×{n}");
    }

    if emit_ir {
        // the final frame's render-IR — pipe to kami-webgpu-rs or inspect.
        println!("{}", kotoba_edn::to_string_pretty(&report.final_render_ir));
    }
}

fn usage() {
    eprintln!("usage: kami-dance <scene.edn> [--frames N] [--fps F] [--emit-ir] [--lint-only]");
}
