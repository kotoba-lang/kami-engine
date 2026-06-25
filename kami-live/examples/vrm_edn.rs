//! VRM dance, fully clj/edn-driven, via the shared `common/vrm.rs` renderer.
//! Reads `:dance/avatar` (vrm path / spring-bones / scale) from scene.edn and
//! renders the real VRM with skinning + textures + MToon + render-IR multi-light
//! + expression morph + spring bones. Change the EDN → change the render.
//! `cargo run -p kami-live --example vrm_edn --target aarch64-apple-darwin`

#[path = "common/vrm.rs"]
mod vrm;

use glam::{Mat4, Vec3};
use kami_live::scene::DanceScene;
use vrm::{Globals, GpuLight, GpuRenderer, VrmDance, MAX_LIGHTS};

const SCENE: &str = include_str!("../../kami-clj-play3d/games/dance/scene.edn");

fn main() { pollster::block_on(run()); }

async fn run() {
    // clj/edn drives the render: VRM path + spring toggle + scale from :dance/avatar.
    let cfg = DanceScene::from_edn(SCENE).expect("scene");
    let av = &cfg.avatar;
    let edn_path = format!("kami-clj-play3d/games/dance/{}", av.vrm);
    let vrm_path = if std::path::Path::new(&edn_path).exists() { edn_path } else { "assets/Seed-san.vrm".to_string() };
    let spring_enabled = av.spring_bones;
    let avatar_scale = av.scale;
    println!("EDN-driven :dance/avatar → vrm={:?} (→ {}), spring-bones={}, scale={}", av.vrm, vrm_path, spring_enabled, avatar_scale);

    let bytes = std::fs::read(&vrm_path).expect("vrm asset (set :dance/avatar :vrm, or download assets/Seed-san.vrm)");
    let mut model = VrmDance::load(&bytes);
    println!("loaded: {} verts, {} tris, {} morph-prims, {} spring chains",
        model.verts.len(), model.indices.len()/3, model.morph_prims.len(), model.spring.chain_count());

    let (w, h) = (420u32, 620u32);
    let r = GpuRenderer::new(&model, w, h).await;
    let proj = Mat4::perspective_rh(0.8, w as f32 / h as f32, 0.05, 100.0);
    // map the dance-world camera offsets (sized for a ~1.8 m performer) to the VRM.
    let ms = (model.height / 1.8 / avatar_scale.max(0.1)).max(0.1);

    let mut scene = DanceScene::from_edn(SCENE).unwrap();
    scene.show.start();

    // Render across the whole show so the :dance/camera :shots choreography
    // (wide → dolly-in → side → pull-back) plays out over the set.
    let mut gif = Vec::new();
    let (mut saved, mut tick) = (0usize, 0usize);
    while saved < 72 {
        let fr = scene.frame(1.0 / 60.0);
        tick += 1;
        if tick % 75 != 0 { continue; } // sample ~0.67 bar of show time per frame
        let ir = kami_webgpu_rs::parse_render_ir(&fr.render_ir_edn());
        let mut lights = [GpuLight { dir: [0.0; 4], color: [0.0; 4] }; MAX_LIGHTS];
        let nl = ir.lights.len().min(MAX_LIGHTS);
        for (k, l) in ir.lights.iter().take(MAX_LIGHTS).enumerate() {
            lights[k] = GpuLight { dir: [l.dir[0], l.dir[1], l.dir[2], 0.0], color: [l.color[0], l.color[1], l.color[2], l.intensity.max(0.3)] };
        }
        let n_used = if nl == 0 { lights[0] = GpuLight { dir: [-0.3, -0.5, -0.75, 0.0], color: [1.0, 0.96, 0.85, 1.0] }; 1 } else { nl };
        let amb = ir.env.ambient;
        let snap = scene.show.snapshot();
        let pose = snap.performer_pose;
        // camera: EDN :dance/camera :shots, dollied by the current bar, framing the VRM.
        let barf = snap.phase.bar as f32 + snap.phase.bar_frac;
        let (off, lk) = cfg.camera.framing_at(barf);
        let eye = Vec3::new(model.center.x + off.x * ms, off.y * ms, model.center.z + off.z * ms);
        let target = Vec3::new(model.center.x + lk.x * ms, lk.y * ms, model.center.z + lk.z * ms);
        let vp = (proj * Mat4::look_at_rh(eye, target, Vec3::Y)).to_cols_array_2d();
        // expression weights are authored in clj/edn (:dance/avatar :expressions).
        let expr = cfg.avatar.expression_weights(snap.cheer_loudness, snap.phase.beat_frac, snap.phase.time);
        let (mv, palette) = model.frame(&pose, &expr, spring_enabled);
        let g = Globals { vp, ambient: [amb[0]*0.45, amb[1]*0.45, amb[2]*0.5, 1.0], n_lights: [n_used as u32,0,0,0], lights };
        let px = r.render(&mv, &palette, g);
        if saved % 18 == 0 { image::save_buffer(format!("seededn_{saved:02}.png"), &px, w, h, image::ExtendedColorType::Rgba8).unwrap(); }
        gif.push(image::Frame::from_parts(image::RgbaImage::from_raw(w, h, px).unwrap(), 0, 0, image::Delay::from_numer_denom_ms(60, 1)));
        saved += 1;
    }
    let fl = std::fs::File::create("seed_edn.gif").unwrap();
    let mut e = image::codecs::gif::GifEncoder::new(fl);
    e.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();
    e.encode_frames(gif.into_iter()).unwrap();
    println!("wrote seed_edn.gif + seededn_*.png — clj/edn-driven VRM dance (via common/vrm.rs)");
}
