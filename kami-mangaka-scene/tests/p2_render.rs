//! P2 render smoke tests — sky + character silhouettes + outline → PNG.
//!
//! P12 unhid these tests after fixing two macOS-specific bugs:
//!   • `request_adapter().await` was deadlocking on the Metal/pollster path;
//!     `kami-render::OffscreenContext::for_offscreen` now uses sync
//!     `enumerate_adapters` on macOS to sidestep the await.
//!   • `PendingPng::read()` was calling `map_async` AFTER the device poll,
//!     so the callback never fired. The renderer now `queue_map`s the
//!     callbacks first, single `device.poll(Wait)`, then `finish_read`.
//!
//! Outputs land under `$CARGO_TARGET_TMPDIR/mangaka_p2_smoke_*.png` for
//! visual inspection (open the path printed by `cargo test -- --nocapture`).

#![cfg(not(target_family = "wasm"))]

use std::fs;
use std::path::PathBuf;

use kami_mangaka_scene::camera::{CameraSpec, ShotGrammar};
use kami_mangaka_scene::render::{RenderOpts, RenderPasses};
use kami_mangaka_scene::renderer::MangakaRenderer;
use kami_mangaka_scene::scene::{EnvironmentSpec, MangakaScene};

#[test]
fn renders_sky_only_when_no_characters() {
    let Some(r) = try_renderer() else {
        eprintln!("[p2] no GPU adapter; skipping");
        return;
    };
    let scene = MangakaScene::new();
    let opts = RenderOpts {
        width: 256,
        height: 256,
        passes: RenderPasses::BASE,
        seed: 0,
    };
    let res = r.render(&scene, None, opts).expect("render");
    assert!(res.base_png.len() > 100, "base_png too small");
    assert_png_signature(&res.base_png);
    write_artifact("mangaka_p2_smoke_sky_only.png", &res.base_png);
}

#[test]
fn renders_base_plus_outline() {
    let Some(r) = try_renderer() else {
        eprintln!("[p2] no GPU adapter; skipping");
        return;
    };
    let mut scene = MangakaScene::new();
    scene.set_background(EnvironmentSpec {
        biome: "Plains".into(),
        weather: None,
        seed: 0,
        ground_size_m: 32.0,
        layout_anchors: vec![],
    });
    scene.set_camera(CameraSpec {
        shot: ShotGrammar::MediumShot,
        ..CameraSpec::default()
    });

    let opts = RenderOpts {
        width: 320,
        height: 480,
        passes: RenderPasses::BASE | RenderPasses::OUTLINE,
        seed: 0,
    };
    let res = r.render(&scene, None, opts).expect("render");
    assert!(res.base_png.len() > 100);
    assert!(res.outline_png.is_some(), "outline pass produced no PNG");
    let out = res.outline_png.as_ref().unwrap();
    assert!(out.len() > 100);
    assert_png_signature(&res.base_png);
    assert_png_signature(out);
    write_artifact("mangaka_p2_smoke_base.png", &res.base_png);
    write_artifact("mangaka_p2_smoke_outline.png", out);
}

#[test]
fn render_multi_emits_one_png_per_angle() {
    if MangakaRenderer::new().is_err() {
        eprintln!("[p2] no GPU adapter; skipping");
        return;
    }
    let scene = MangakaScene::new();
    let cams: Vec<CameraSpec> = [ShotGrammar::FullShot, ShotGrammar::Closeup, ShotGrammar::Dutch]
        .iter()
        .map(|s| CameraSpec { shot: *s, ..CameraSpec::default() })
        .collect();
    let opts = RenderOpts {
        width: 128,
        height: 128,
        passes: RenderPasses::BASE,
        seed: 0,
    };
    let results = scene.render_multi(&cams, opts).expect("render_multi");
    assert_eq!(results.len(), 3);
    for r in &results {
        assert!(r.base_png.len() > 100);
        assert_png_signature(&r.base_png);
    }
}

// ── helpers ───────────────────────────────────────────────────────────────

fn try_renderer() -> Option<MangakaRenderer> {
    MangakaRenderer::new().ok()
}

fn assert_png_signature(bytes: &[u8]) {
    let sig = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    assert!(bytes.len() >= sig.len(), "PNG bytes too short");
    assert_eq!(&bytes[..sig.len()], &sig, "missing PNG signature");
}

fn write_artifact(name: &str, bytes: &[u8]) {
    let mut path = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    path.push(name);
    fs::write(&path, bytes).ok();
}
