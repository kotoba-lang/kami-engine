//! Host preview of the web Model-B GPU path's **exact pixels** — the same render
//! `WebDanceGpu::frame` rasterises and blits in the browser, saved to a PNG so the
//! dance image is verifiable headlessly (the in-browser wgpu *blit* needs a real
//! GPU; the CPU rasterisation here does not).
//!
//! `cargo run -p kami-web-modelb --example render_png --target aarch64-apple-darwin`
//! → `web_modelb.png`.

use kami_webgpu_rs::{parse_ir, render};

fn main() {
    let scene_edn = include_str!("../../kami-clj-play3d/games/dance/scene.edn");
    let mut show =
        kami_live::scene::DanceScene::from_edn(scene_edn).expect("dance scene.edn parses");

    let (w, h) = (420u32, 480u32);
    // advance into the show (lights up, crowd placed), then grab a frame.
    let mut f = show.frame(0.016);
    for _ in 0..60 {
        f = show.frame(0.016);
    }

    // the exact render WebDanceGpu::frame does: parse the render-IR, frame the
    // performer, rasterise the instances on the CPU.
    let (mut g, insts) = parse_ir(&f.render_ir_edn());
    g.eye = Some([0.0, 1.5, 4.2]);
    g.target = Some([0.0, 0.95, 0.0]);
    let px = render(&g, &insts, w, h);

    image::save_buffer("web_modelb.png", &px, w, h, image::ExtendedColorType::Rgba8).unwrap();
    println!("wrote web_modelb.png — the pixels WebDanceGpu blits in the browser ({} instances)", insts.len());
}
