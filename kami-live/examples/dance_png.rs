//! Render the reference dance scene to a PNG sequence with the native executor.
//! `cargo run -p kami-live --example dance_png --target aarch64-apple-darwin`
//!
//! Advances into the **shuffle** track (visible side-step) and renders a close
//! sequence over one bar, so the performer slides left↔right — the choreography
//! made visible. Each frame: `:dance/*` EDN → render-IR → `kami_webgpu_rs::render`
//! (offscreen PBR + shadows). The performer is a lit placeholder cuboid; the
//! skinned VRM mesh replaces it once the GPU executor consumes `:meshes`.

use kami_live::scene::DanceScene;

const SCENE: &str = include_str!("../../kami-clj-play3d/games/dance/scene.edn");

fn main() {
    let mut scene = DanceScene::from_edn(SCENE).expect("reference scene loads");
    scene.show.start();
    let (w, h) = (640u32, 400u32);
    let dt = 1.0 / 60.0;

    // Advance ~31s to the Verse (shuffle): the performer side-steps ±0.4 units.
    for _ in 0..(31.0 / dt) as i32 {
        scene.frame(dt);
    }
    // Render 12 frames across ~1.4s (one 128-bpm bar) of the side-step.
    for i in 0..12 {
        for _ in 0..7 {
            scene.frame(dt);
        }
        let f = scene.frame(dt);
        let (g, insts) = kami_webgpu_rs::parse_ir(&f.render_ir_edn());
        let px = kami_webgpu_rs::render(&g, &insts, w, h);
        let name = format!("dance_{i:02}.png");
        image::save_buffer(&name, &px, w, h, image::ExtendedColorType::Rgba8).unwrap();
        let ph = scene.show.grid().phase();
        println!("wrote {name} — beat {} bar-frac {:.2}", ph.beat, ph.bar_frac);
    }
    println!("done — the performer slides left↔right across the bar");
}
