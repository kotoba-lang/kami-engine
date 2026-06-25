//! Render the reference dance scene to a PNG sequence with the native executor.
//! `cargo run -p kami-live --example dance_png --target aarch64-apple-darwin`
//!
//! Each frame's `:dance/*` EDN → render-IR → `kami_webgpu_rs::render` (offscreen,
//! PBR + shadows, no window). The performer is a lit placeholder cuboid bobbing
//! the choreography, the crowd are lit boxes, the camera tracks the dancer — the
//! data path made visible. The skinned VRM mesh replaces the box on the GPU VRM
//! surface (run_embed_vrm, ADR-0031) once that pipeline consumes `:meshes`.

use kami_live::scene::DanceScene;

const SCENE: &str = include_str!("../../kami-clj-play3d/games/dance/scene.edn");

fn main() {
    let mut scene = DanceScene::from_edn(SCENE).expect("reference scene loads");
    scene.show.start();
    let (w, h) = (640u32, 400u32);
    let fps = 30.0;
    let mut saved = 0;
    for step in 0..300 {
        let f = scene.frame(1.0 / fps);
        if step % 30 == 0 {
            let edn = f.render_ir_edn();
            let (g, insts) = kami_webgpu_rs::parse_ir(&edn);
            let px = kami_webgpu_rs::render(&g, &insts, w, h);
            let name = format!("dance_{saved:02}.png");
            image::save_buffer(&name, &px, w, h, image::ExtendedColorType::Rgba8).unwrap();
            println!("wrote {name} — {} instances @ beat {}", insts.len(), scene.show.grid().phase().beat);
            saved += 1;
        }
    }
    println!("done: {saved} frames of the dance rendered");
}
