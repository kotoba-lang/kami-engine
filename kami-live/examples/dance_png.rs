//! Render the reference dance scene as a moving HUMANOID + animated GIF.
//! `cargo run -p kami-live --example dance_png --target aarch64-apple-darwin`
//!
//! Builds a stick-figure performer (head / torso / 2 arms / 2 legs as lit boxes)
//! posed each frame from the beat-synced `DancePose` (side-step, vertical bob,
//! arms-up, body yaw), renders it with the native offscreen executor (PBR +
//! shadows) under a close fixed camera, and encodes `dance.gif`. The figure is a
//! placeholder for the skinned VRM mesh, which replaces it once the GPU executor
//! consumes the render-IR `:meshes` (ADR-0044 phase 3 / run_embed_vrm).

use kami_live::scene::DanceScene;
use kami_live::DancePose;
use kami_webgpu_rs::Instance;

const SCENE: &str = include_str!("../../kami-clj-play3d/games/dance/scene.edn");

/// A stick-figure humanoid (6 lit boxes) posed from the dance pose.
fn humanoid(p: &DancePose) -> Vec<Instance> {
    let (s, c) = (p.root_yaw.sin(), p.root_yaw.cos());
    let base = [p.root_translation.x, p.vertical_bob.max(-0.2), p.root_translation.z];
    let arm_lift = p.arms_up * 0.35;
    // (local x, base y, local z), (w, h), color
    let parts: [([f32; 3], [f32; 2], [f32; 3]); 6] = [
        ([-0.13, 0.0, 0.0], [0.16, 0.80], [0.20, 0.24, 0.42]), // left leg
        ([0.13, 0.0, 0.0], [0.16, 0.80], [0.20, 0.24, 0.42]),  // right leg
        ([0.0, 0.80, 0.0], [0.42, 0.55], [0.30, 0.45, 0.90]),  // torso
        ([0.0, 1.42, 0.0], [0.26, 0.26], [0.95, 0.80, 0.70]),  // head
        ([-0.32, 0.85 + arm_lift, 0.0], [0.14, 0.45], [0.95, 0.80, 0.70]), // left arm
        ([0.32, 0.85 + arm_lift, 0.0], [0.14, 0.45], [0.95, 0.80, 0.70]),  // right arm
    ];
    parts
        .iter()
        .map(|(l, size, color)| {
            // rotate local x,z by body yaw, then translate to the danced root.
            let (rx, rz) = (l[0] * c - l[2] * s, l[0] * s + l[2] * c);
            Instance {
                pos: [base[0] + rx, base[1] + l[1], base[2] + rz],
                color: *color,
                size: *size,
                yaw: p.root_yaw,
                metallic: 0.0,
                roughness: 0.7,
                emissive: 0.0,
            }
        })
        .collect()
}

fn main() {
    let mut scene = DanceScene::from_edn(SCENE).expect("reference scene loads");
    scene.show.start();
    let (w, h) = (480u32, 360u32);
    let dt = 1.0 / 60.0;
    // advance ~31s into the Verse (shuffle: big side-step).
    for _ in 0..(31.0 / dt) as i32 {
        scene.frame(dt);
    }

    let mut gif_frames = Vec::new();
    for i in 0..32 {
        for _ in 0..2 {
            scene.frame(dt);
        }
        let f = scene.frame(dt);
        // crowd from the render-IR; performer = our humanoid (drop the box at [0]).
        let (mut g, insts) = kami_webgpu_rs::parse_ir(&f.render_ir_edn());
        g.eye = Some([0.0, 1.5, 4.2]);
        g.target = Some([0.0, 0.95, 0.0]);
        let mut scene_insts: Vec<Instance> = insts.into_iter().skip(1).collect();
        scene_insts.extend(humanoid(&scene.show.snapshot().performer_pose));
        let px = kami_webgpu_rs::render(&g, &scene_insts, w, h);
        if i % 8 == 0 {
            image::save_buffer(format!("dance_{i:02}.png"), &px, w, h, image::ExtendedColorType::Rgba8).unwrap();
        }
        let img = image::RgbaImage::from_raw(w, h, px).unwrap();
        let mut frame = image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(60, 1));
        gif_frames.push(frame);
    }
    // encode the animated GIF (looping).
    let file = std::fs::File::create("dance.gif").unwrap();
    let mut enc = image::codecs::gif::GifEncoder::new(file);
    enc.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();
    enc.encode_frames(gif_frames.into_iter()).unwrap();
    println!("wrote dance.gif (32 frames) + sample PNGs — the humanoid dances the shuffle");
}
