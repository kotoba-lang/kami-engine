//! kami-clj-play3d — a stylized 3rd-person battle-royale demo, authored in
//! kami-clj/EDN, run on this Mac in 3D (wgpu/Metal).
//!
//! ADR-0036/0037: the GAME is data (games/royale/logic.clj + scene.edn); this
//! Rust binary is only the GPU arm + host. logic.clj (CLJ→WASM, driven by
//! kami-script-runtime over hecs) owns the player's ground movement and the bot
//! AI; the host owns the 3rd-person camera, gravity/jump, lit 3D rendering with
//! a procedural sky, ground grid, and distance fog.
//!
//!   cargo run --target aarch64-apple-darwin -p kami-clj-play3d
//!
//! WASD move (camera-relative) · arrows orbit camera · Space jump · Esc quit.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use glam::{Mat4, Vec3};
use kami_core::actor::components::Position;
use kami_script_runtime::{KamiScriptRuntime, Tag, BACKEND};
use kami_scene::{kw_key, mget, num, vec3};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

// ── Scene data (parsed from scene.edn) ──────────────────────────────────────
#[derive(Clone)]
struct Prof {
    color: [f32; 3],
    w: f32,
    h: f32,
}
#[derive(Clone)]
struct Building {
    color: [f32; 3],
    min_h: f32,
    max_h: f32,
    w: f32,
}
struct Scene3 {
    title: String,
    player_speed: f32,
    camera_dist: f32,
    camera_height: f32,
    ground_scale: f32,
    gravity: f32,
    jump: f32,
    profiles: HashMap<String, Prof>,
    prop_count: usize,
    prop_spread: f32,
    buildings: Vec<Building>,
    tree_color: [f32; 3],
    tree_h: f32,
    tree_w: f32,
    tree_ratio: f32,
    sky_zenith: [f32; 3],
    sky_horizon: [f32; 3],
    sun_dir: [f32; 3],
    sun_col: [f32; 3],
    fog: f32,
    ground_col: [f32; 3],
}


/// Parse the scene; `None` (not a panic) if `src` isn't a valid EDN map.
fn parse_scene(src: &str) -> Option<Scene3> {
    let parsed = kami_scene::root_map(src)?;
    let root = &parsed;
    let world = mget(root, "world").and_then(|w| w.as_map());
    let wget = |k: &str| world.and_then(|w| mget(w, k));

    let mut profiles = HashMap::new();
    if let Some(pm) = mget(root, "render/profiles").and_then(|p| p.as_map()) {
        for (k, v) in pm {
            if let (Some(tag), Some(p)) = (kw_key(k), v.as_map()) {
                profiles.insert(
                    tag,
                    Prof { color: vec3(mget(p, "color")), w: num(mget(p, "w")), h: num(mget(p, "h")) },
                );
            }
        }
    }

    let props = mget(root, "render/props").and_then(|p| p.as_map());
    let pget = |k: &str| props.and_then(|p| mget(p, k));
    let mut buildings = Vec::new();
    if let Some(bs) = pget("buildings").and_then(|b| b.as_vector()) {
        for b in bs {
            if let Some(bm) = b.as_map() {
                buildings.push(Building {
                    color: vec3(mget(bm, "color")),
                    min_h: num(mget(bm, "min-h")),
                    max_h: num(mget(bm, "max-h")),
                    w: num(mget(bm, "w")),
                });
            }
        }
    }
    if buildings.is_empty() {
        buildings.push(Building { color: [0.6, 0.6, 0.65], min_h: 2.0, max_h: 6.0, w: 2.0 });
    }
    let trees = pget("trees").and_then(|t| t.as_map());
    let tget = |k: &str| trees.and_then(|t| mget(t, k));

    let sky = mget(root, "render/sky").and_then(|s| s.as_map());
    let sget = |k: &str| sky.and_then(|s| mget(s, k));

    Some(Scene3 {
        title: mget(root, "game/title").and_then(|t| t.as_string()).unwrap_or("kami-clj-play3d").to_string(),
        player_speed: num(wget("player-speed")),
        camera_dist: num(wget("camera-dist")),
        camera_height: num(wget("camera-height")),
        ground_scale: num(wget("ground-scale")),
        gravity: num(wget("gravity")),
        jump: num(wget("jump")),
        profiles,
        prop_count: num(pget("count")) as usize,
        prop_spread: num(pget("spread")),
        buildings,
        tree_color: vec3(tget("color")),
        tree_h: num(tget("h")),
        tree_w: num(tget("w")),
        tree_ratio: num(tget("ratio")),
        sky_zenith: vec3(sget("zenith")),
        sky_horizon: vec3(sget("horizon")),
        sun_dir: vec3(sget("sun-dir")),
        sun_col: vec3(sget("sun")),
        fog: num(sget("fog")),
        ground_col: vec3(sget("ground")),
    })
}

/// Read a game file or exit with a clear message (no panic backtrace).
fn read_or_exit(base: &std::path::Path, name: &str) -> String {
    std::fs::read_to_string(base.join(name)).unwrap_or_else(|e| {
        eprintln!("kami-clj-play3d: cannot read {}: {e}", base.join(name).display());
        std::process::exit(1);
    })
}

// ── GPU types ───────────────────────────────────────────────────────────────
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    sun_dir: [f32; 4],
    sun_col: [f32; 4],
    sky_zenith: [f32; 4],
    sky_horizon: [f32; 4],
    ground_col: [f32; 4],
    params: [f32; 4],  // fog, time, res_w, res_h
    params2: [f32; 4], // storm_radius, _, _, _
    light_vp: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    model: [[f32; 4]; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
}

/// A screen-space UI quad (NDC rect + colour) for the HUD / minimap.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct UiRect {
    rect: [f32; 4], // x, y, w, h in NDC (origin centre, y up)
    color: [f32; 4],
}
impl UiRect {
    /// from screen fractions (0..1, origin top-left) → NDC rect.
    fn screen(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> UiRect {
        UiRect { rect: [x * 2.0 - 1.0, 1.0 - (y + h) * 2.0, w * 2.0, h * 2.0], color }
    }
}

fn cube() -> (Vec<Vertex>, Vec<u16>) {
    // 6 faces, per-face normal, unit cube centered at origin.
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([0.0, 0.0, 1.0], [[-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]]),
        ([0.0, 0.0, -1.0], [[0.5, -0.5, -0.5], [-0.5, -0.5, -0.5], [-0.5, 0.5, -0.5], [0.5, 0.5, -0.5]]),
        ([1.0, 0.0, 0.0], [[0.5, -0.5, 0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [0.5, 0.5, 0.5]]),
        ([-1.0, 0.0, 0.0], [[-0.5, -0.5, -0.5], [-0.5, -0.5, 0.5], [-0.5, 0.5, 0.5], [-0.5, 0.5, -0.5]]),
        ([0.0, 1.0, 0.0], [[-0.5, 0.5, 0.5], [0.5, 0.5, 0.5], [0.5, 0.5, -0.5], [-0.5, 0.5, -0.5]]),
        ([0.0, -1.0, 0.0], [[-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, -0.5, 0.5], [-0.5, -0.5, 0.5]]),
    ];
    let mut v = Vec::new();
    let mut idx = Vec::new();
    for (n, quad) in faces {
        let base = v.len() as u16;
        for p in quad {
            v.push(Vertex { pos: p, normal: n });
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (v, idx)
}

fn model_box(base: Vec3, w: f32, h: f32) -> [[f32; 4]; 4] {
    // a box of footprint w×w and height h whose base sits on `base`.
    (Mat4::from_translation(base + Vec3::new(0.0, h * 0.5, 0.0)) * Mat4::from_scale(Vec3::new(w, h, w)))
        .to_cols_array_2d()
}

/// A thin wall panel (width `w`, height `h`, thin `depth`) yawed to face the
/// builder, base on the ground at `base` — the Fortnite-style build piece.
fn model_wall(base: Vec3, yaw: f32, w: f32, h: f32, depth: f32) -> [[f32; 4]; 4] {
    (Mat4::from_translation(base + Vec3::new(0.0, h * 0.5, 0.0))
        * Mat4::from_rotation_y(yaw)
        * Mat4::from_scale(Vec3::new(w, h, depth)))
    .to_cols_array_2d()
}

/// A short-lived hit/impact particle (host CPU), drawn as a small glowing cube.
struct Particle3 {
    pos: Vec3,
    vel: Vec3,
    age: f32,
    life: f32,
}

/// A travelling bullet (real ballistics: it moves and can miss).
struct Bullet {
    pos: Vec3,
    vel: Vec3,
    life: f32,
}

/// Match flow: drop in from the sky, then fight on the ground.
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Skydive,
    Playing,
}

#[derive(Clone, Copy, PartialEq)]
enum Weapon {
    Pistol,
    Rifle,
    Shotgun,
}
impl Weapon {
    fn name(self) -> &'static str {
        match self {
            Weapon::Pistol => "PISTOL",
            Weapon::Rifle => "RIFLE",
            Weapon::Shotgun => "SHOTGUN",
        }
    }
    /// (frames between shots, range (world), pellets/shot, spread radians, bullet color)
    fn params(self) -> (u64, f32, u32, f32, [f32; 3]) {
        match self {
            Weapon::Pistol => (12, 9.0, 1, 0.0, [1.0, 0.95, 0.45]),
            Weapon::Rifle => (5, 8.0, 1, 0.04, [0.6, 0.9, 1.0]),
            Weapon::Shotgun => (30, 5.0, 6, 0.20, [1.0, 0.6, 0.3]),
        }
    }
    /// (magazine size, reload time in frames)
    fn ammo_spec(self) -> (u32, u32) {
        match self {
            Weapon::Pistol => (12, 50),
            Weapon::Rifle => (30, 80),
            Weapon::Shotgun => (6, 70),
        }
    }
}

/// Enemy archetype by entity id → (size scale, colour, hit points).
fn bot_kind(id: u32) -> (f32, [f32; 3], u32) {
    match id % 3 {
        0 => (1.5, [0.9, 0.3, 0.25], 3), // brute: big, red, tanky
        2 => (0.8, [0.95, 0.85, 0.3], 1), // runner: small, yellow
        _ => (1.0, [0.6, 0.7, 0.85], 1),  // grunt: normal
    }
}

#[derive(Clone, Copy)]
enum Loot {
    Health,
    Rifle,
    Shotgun,
}
struct Item {
    pos: Vec3, // ground spot (world)
    kind: Loot,
}

/// Native synth audio (no asset files) — short decaying sine blips per event.
struct Audio {
    _stream: rodio::OutputStream,
    handle: rodio::OutputStreamHandle,
}
impl Audio {
    fn new() -> Option<Self> {
        let (s, h) = rodio::OutputStream::try_default().ok()?;
        Some(Audio { _stream: s, handle: h })
    }
    fn blip(&self, freq: f32, ms: u64, amp: f32) {
        use rodio::Source;
        let src = rodio::source::SineWave::new(freq)
            .take_duration(std::time::Duration::from_millis(ms))
            .amplify(amp);
        let _ = self.handle.play_raw(src);
    }
}

/// Append a blocky humanoid (legs/torso/arms/head/visor) to `inst`, standing on
/// `ground`, facing `yaw`, with a walk cycle when `moving`. Stylized boxes — no
/// skinned mesh, but reads as a character with motion.
fn push_character(inst: &mut Vec<Instance>, ground: Vec3, yaw: f32, walk: f32, moving: bool, scale: f32, color: [f32; 3]) {
    let rot = Mat4::from_rotation_y(yaw);
    let tint = |m: f32| [color[0] * m, color[1] * m, color[2] * m, 1.0];
    let mut part = |local: Vec3, size: Vec3, col: [f32; 4]| {
        let m = Mat4::from_translation(ground) * rot * Mat4::from_translation(local * scale) * Mat4::from_scale(size * scale);
        inst.push(Instance { model: m.to_cols_array_2d(), color: col });
    };
    let sw = if moving { walk.sin() } else { 0.0 };
    let bob = if moving { (walk * 2.0).sin().abs() * 0.07 } else { 0.0 };
    // forward is -z; legs/arms swing along z, opposite each other
    part(Vec3::new(-0.18, 0.45 + bob, sw * 0.35), Vec3::new(0.26, 0.95, 0.30), tint(0.7));
    part(Vec3::new(0.18, 0.45 + bob, -sw * 0.35), Vec3::new(0.26, 0.95, 0.30), tint(0.7));
    part(Vec3::new(0.0, 1.25 + bob, 0.0), Vec3::new(0.70, 0.90, 0.45), tint(1.0)); // torso
    part(Vec3::new(-0.5, 1.30 + bob, -sw * 0.30), Vec3::new(0.20, 0.80, 0.24), tint(0.92)); // arms
    part(Vec3::new(0.5, 1.30 + bob, sw * 0.30), Vec3::new(0.20, 0.80, 0.24), tint(0.92));
    part(Vec3::new(0.0, 1.95 + bob, 0.0), Vec3::new(0.45, 0.45, 0.45), tint(1.15)); // head
    part(Vec3::new(0.0, 1.95 + bob, -0.24), Vec3::new(0.30, 0.18, 0.05), [0.1, 0.1, 0.12, 1.0]); // visor (shows facing)
}

/// A soft blob shadow: a dark flat box on the ground (cheap grounding, no shadow map).
fn push_shadow(inst: &mut Vec<Instance>, ground: Vec3, r: f32) {
    let m = Mat4::from_translation(ground + Vec3::new(0.0, 0.05, 0.0)) * Mat4::from_scale(Vec3::new(r, 0.05, r));
    inst.push(Instance { model: m.to_cols_array_2d(), color: [0.05, 0.06, 0.07, 1.0] });
}

/// An axis-aligned wall footprint in world XZ, for player collision.
#[derive(Clone, Copy)]
struct Aabb {
    min: glam::Vec2,
    max: glam::Vec2,
}

/// A hollow building you can walk into: 4 walls (a door gap on the south side) +
/// a roof. Pushes the wall boxes to `inst` and their footprints to `aabbs`.
fn make_building(c: Vec3, room: f32, h: f32, color: [f32; 3], inst: &mut Vec<Instance>, aabbs: &mut Vec<Aabb>) {
    let half = room * 0.5;
    let th = 0.5;
    let col = [color[0], color[1], color[2], 1.0];
    let mut wall = |center: Vec3, yaw: f32, w: f32, depth: f32, ax_min: glam::Vec2, ax_max: glam::Vec2| {
        inst.push(Instance { model: model_wall(center, yaw, w, h, depth), color: col });
        aabbs.push(Aabb { min: ax_min, max: ax_max });
    };
    let (cx, cz) = (c.x, c.z);
    // north / east / west walls (full)
    wall(Vec3::new(cx, 0.0, cz + half), 0.0, room, th,
         glam::vec2(cx - half, cz + half - th * 0.5), glam::vec2(cx + half, cz + half + th * 0.5));
    wall(Vec3::new(cx + half, 0.0, cz), std::f32::consts::FRAC_PI_2, room, th,
         glam::vec2(cx + half - th * 0.5, cz - half), glam::vec2(cx + half + th * 0.5, cz + half));
    wall(Vec3::new(cx - half, 0.0, cz), std::f32::consts::FRAC_PI_2, room, th,
         glam::vec2(cx - half - th * 0.5, cz - half), glam::vec2(cx - half + th * 0.5, cz + half));
    // south wall with a centred door gap → two segments
    let gd = room * 0.45;
    let seg = (room - gd) * 0.5;
    wall(Vec3::new(cx - half + seg * 0.5, 0.0, cz - half), 0.0, seg, th,
         glam::vec2(cx - half, cz - half - th * 0.5), glam::vec2(cx - half + seg, cz - half + th * 0.5));
    wall(Vec3::new(cx + half - seg * 0.5, 0.0, cz - half), 0.0, seg, th,
         glam::vec2(cx + half - seg, cz - half - th * 0.5), glam::vec2(cx + half, cz - half + th * 0.5));
    // roof (no collision; player can't reach it)
    inst.push(Instance { model: model_box(Vec3::new(cx, h, cz), room, th), color: [color[0] * 0.85, color[1] * 0.85, color[2] * 0.85, 1.0] });
    // lit window insets on the north + south facades (alpha 0.0 = emissive sentinel)
    let win = |cyx: f32, cyy: f32, cyz: f32, w: f32, hh: f32, d: f32| {
        let m = Mat4::from_translation(Vec3::new(cyx, cyy, cyz)) * Mat4::from_scale(Vec3::new(w, hh, d));
        Instance { model: m.to_cols_array_2d(), color: [1.0, 0.85, 0.5, 0.0] }
    };
    inst.push(win(cx, h * 0.55, cz + half + 0.07, room * 0.34, h * 0.34, 0.14));
    inst.push(win(cx, h * 0.55, cz - half - 0.07, room * 0.34, h * 0.34, 0.14));
}

/// Push the player (world XZ) out of any wall AABB it has entered (with radius `r`).
fn resolve_collision(mut p: glam::Vec2, r: f32, aabbs: &[Aabb]) -> glam::Vec2 {
    for a in aabbs {
        let (minx, maxx) = (a.min.x - r, a.max.x + r);
        let (minz, maxz) = (a.min.y - r, a.max.y + r);
        if p.x > minx && p.x < maxx && p.y > minz && p.y < maxz {
            let (dl, dr, dd, du) = (p.x - minx, maxx - p.x, p.y - minz, maxz - p.y);
            let m = dl.min(dr).min(dd).min(du);
            if m == dl {
                p.x = minx;
            } else if m == dr {
                p.x = maxx;
            } else if m == dd {
                p.y = minz;
            } else {
                p.y = maxz;
            }
        }
    }
    p
}

const SHADER: &str = r#"
struct G {
  view_proj: mat4x4<f32>, cam_pos: vec4<f32>, sun_dir: vec4<f32>, sun_col: vec4<f32>,
  sky_zenith: vec4<f32>, sky_horizon: vec4<f32>, ground_col: vec4<f32>, params: vec4<f32>,
  params2: vec4<f32>,
  light_vp: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> g: G;
@group(1) @binding(0) var shadow_tex: texture_depth_2d;
@group(1) @binding(1) var shadow_smp: sampler_comparison;

// real cast shadow from the sun's depth map (3×3 PCF, slope bias).
fn sun_shadow(wpos: vec3<f32>, ndl: f32) -> f32 {
  let lp = g.light_vp * vec4<f32>(wpos, 1.0);
  let proj = lp.xyz / lp.w;
  let uv = vec2<f32>(proj.x * 0.5 + 0.5, proj.y * -0.5 + 0.5);
  let bias = max(0.004 * (1.0 - ndl), 0.0010);
  let texel = 1.0 / 2048.0;
  var sh = 0.0;
  for (var x = -1; x <= 1; x = x + 1) {
    for (var y = -1; y <= 1; y = y + 1) {
      let o = clamp(uv + vec2<f32>(f32(x), f32(y)) * texel, vec2<f32>(0.0), vec2<f32>(1.0));
      sh = sh + textureSampleCompare(shadow_tex, shadow_smp, o, proj.z - bias);
    }
  }
  let lit = mix(0.28, 1.0, sh / 9.0); // shadowed keeps ambient + a little fill
  let inside = proj.z <= 1.0 && abs(proj.x) <= 1.0 && abs(proj.y) <= 1.0;
  return select(1.0, lit, inside);
}

// depth-only pass from the light's POV (writes the shadow map).
@vertex
fn shadow_vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>,
             @location(2) m0: vec4<f32>, @location(3) m1: vec4<f32>,
             @location(4) m2: vec4<f32>, @location(5) m3: vec4<f32>,
             @location(6) color: vec4<f32>) -> @builtin(position) vec4<f32> {
  let model = mat4x4<f32>(m0, m1, m2, m3);
  return g.light_vp * model * vec4<f32>(pos, 1.0);
}

// ---- sky (fullscreen) ----
@vertex
fn sky_vs(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
  var p = array<vec2<f32>,3>(vec2<f32>(-1.0,-3.0), vec2<f32>(-1.0,1.0), vec2<f32>(3.0,1.0));
  return vec4<f32>(p[vi], 0.0, 1.0);
}
@fragment
fn sky_fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
  let t = clamp(frag.y / g.params.w, 0.0, 1.0); // 0 top .. 1 bottom (frag.y grows down)
  var col = mix(g.sky_zenith.rgb, g.sky_horizon.rgb, t);
  col += g.sun_col.rgb * pow(t, 5.0) * 0.30;     // warm glow toward horizon
  return vec4<f32>(col, 1.0);
}

// ---- lit instanced boxes ----
struct VO {
  @builtin(position) clip: vec4<f32>,
  @location(0) wpos: vec3<f32>,
  @location(1) wnormal: vec3<f32>,
  @location(2) color: vec4<f32>,
};
@vertex
fn box_vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>,
          @location(2) m0: vec4<f32>, @location(3) m1: vec4<f32>,
          @location(4) m2: vec4<f32>, @location(5) m3: vec4<f32>,
          @location(6) color: vec4<f32>) -> VO {
  let model = mat4x4<f32>(m0, m1, m2, m3);
  let world = model * vec4<f32>(pos, 1.0);
  var o: VO;
  o.clip = g.view_proj * world;
  o.wpos = world.xyz;
  o.wnormal = normalize((model * vec4<f32>(normal, 0.0)).xyz);
  o.color = color;
  return o;
}
const PI: f32 = 3.14159265;

// Cook-Torrance PBR: GGX NDF + Smith G + Schlick Fresnel, sun + hemisphere ambient,
// modulated by a real shadow-map term, then Reinhard tonemap + fog + storm.
fn shade(wpos: vec3<f32>, n: vec3<f32>, base: vec3<f32>) -> vec3<f32> {
  let N = normalize(n);
  let L = normalize(-g.sun_dir.xyz);
  let V = normalize(g.cam_pos.xyz - wpos);
  let H = normalize(L + V);
  let ndl = max(dot(N, L), 0.0);
  let ndv = max(dot(N, V), 1e-3);
  let ndh = max(dot(N, H), 0.0);
  let vdh = max(dot(V, H), 0.0);
  let rough = 0.55;
  let metallic = 0.0;
  let a = rough * rough;
  let a2 = a * a;
  let denom = ndh * ndh * (a2 - 1.0) + 1.0;
  let ndf = a2 / (PI * denom * denom);
  let k = (rough + 1.0) * (rough + 1.0) / 8.0;
  let gg = (ndv / (ndv * (1.0 - k) + k)) * (ndl / (ndl * (1.0 - k) + k));
  let f0 = mix(vec3<f32>(0.04), base, metallic);
  let fr = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - vdh, 5.0);
  let spec = (ndf * gg) * fr / max(4.0 * ndv * ndl, 1e-3);
  let kd = (vec3<f32>(1.0) - fr) * (1.0 - metallic);
  let shadow = sun_shadow(wpos, ndl);
  let direct = (kd * base / PI + spec) * g.sun_col.rgb * 2.4 * ndl * shadow;
  let amb = mix(g.ground_col.rgb * 0.3, g.sky_zenith.rgb * 0.6, N.y * 0.5 + 0.5) * base;
  var col = amb + direct;
  let sd = length(wpos.xz);
  let storm = smoothstep(g.params2.x - 5.0, g.params2.x + 5.0, sd);
  col = mix(col, vec3<f32>(0.55, 0.25, 0.8), storm * 0.55);
  col = col / (col + vec3<f32>(1.0)); // Reinhard tonemap (HDR → LDR)
  let dist = length(wpos - g.cam_pos.xyz);
  let fog = clamp(1.0 - exp(-dist * g.params.x), 0.0, 1.0);
  return mix(col, g.sky_horizon.rgb, fog);
}
@fragment
fn box_fs(i: VO) -> @location(0) vec4<f32> {
  // subtle material grain so flat faces read as a surface, not plastic
  let grain = (fract(sin(dot(floor(i.wpos.xz * 3.0 + i.wpos.y * 3.0), vec2<f32>(12.99, 78.23))) * 43758.5) - 0.5) * 0.06;
  let lit = shade(i.wpos, i.wnormal, i.color.rgb * (1.0 + grain)); // always run → shadow sample stays uniform
  // emissive sentinel (alpha < 0.5): lit windows / glowing elements
  let dist = length(i.wpos - g.cam_pos.xyz);
  let fog = clamp(1.0 - exp(-dist * g.params.x), 0.0, 1.0);
  let emis = mix(i.color.rgb * 1.7, g.sky_horizon.rgb, fog);
  return vec4<f32>(select(lit, emis, i.color.a < 0.5), 1.0);
}

// ---- ground (big quad with grid) ----
@vertex
fn ground_vs(@builtin(vertex_index) vi: u32) -> VO {
  var q = array<vec2<f32>,6>(
    vec2<f32>(-1.0,-1.0), vec2<f32>(1.0,-1.0), vec2<f32>(1.0,1.0),
    vec2<f32>(-1.0,-1.0), vec2<f32>(1.0,1.0), vec2<f32>(-1.0,1.0));
  let s = 600.0;
  let world = vec3<f32>(q[vi].x * s, 0.0, q[vi].y * s);
  var o: VO;
  o.clip = g.view_proj * vec4<f32>(world, 1.0);
  o.wpos = world; o.wnormal = vec3<f32>(0.0,1.0,0.0); o.color = vec4<f32>(g.ground_col.rgb, 1.0);
  return o;
}
@fragment
fn ground_fs(i: VO) -> @location(0) vec4<f32> {
  let gp = abs(fract(i.wpos.xz / 4.0) - 0.5);
  let line = smoothstep(0.47, 0.5, max(gp.x, gp.y));
  let base = mix(i.color.rgb, i.color.rgb * 1.25, line);
  return vec4<f32>(shade(i.wpos, i.wnormal, base), 1.0);
}

// ---- water (animated lake) ----
@vertex
fn water_vs(@builtin(vertex_index) vi: u32) -> VO {
  var q = array<vec2<f32>,6>(
    vec2<f32>(-1.0,-1.0), vec2<f32>(1.0,-1.0), vec2<f32>(1.0,1.0),
    vec2<f32>(-1.0,-1.0), vec2<f32>(1.0,1.0), vec2<f32>(-1.0,1.0));
  let cx = -22.0; let cz = 14.0; let hf = 12.0; let lvl = 0.35;
  var world = vec3<f32>(cx + q[vi].x * hf, lvl, cz + q[vi].y * hf);
  let t = g.params.y;
  world.y += sin(world.x * 0.5 + t * 1.6) * 0.18 + cos(world.z * 0.6 + t * 1.3) * 0.15;
  var o: VO;
  o.clip = g.view_proj * vec4<f32>(world, 1.0);
  o.wpos = world; o.wnormal = vec3<f32>(0.0, 1.0, 0.0); o.color = vec4<f32>(0.05, 0.2, 0.3, 1.0);
  return o;
}
@fragment
fn water_fs(i: VO) -> @location(0) vec4<f32> {
  let t = g.params.y;
  let nx = cos(i.wpos.x * 0.5 + t * 1.6) * 0.25 + sin(i.wpos.x * 1.3 + t * 2.1) * 0.12;
  let nz = -sin(i.wpos.z * 0.6 + t * 1.3) * 0.25 + cos(i.wpos.z * 1.1 + t * 1.7) * 0.12;
  let N = normalize(vec3<f32>(-nx, 1.0, -nz));
  let V = normalize(g.cam_pos.xyz - i.wpos);
  let fres = pow(1.0 - max(dot(N, V), 0.0), 3.0);
  let deep = vec3<f32>(0.04, 0.16, 0.26);
  let sky = mix(g.sky_horizon.rgb, g.sky_zenith.rgb, 0.5);
  var col = mix(deep, sky, clamp(fres * 0.85 + 0.08, 0.0, 1.0));
  let L = normalize(-g.sun_dir.xyz);
  let H = normalize(L + V);
  col += g.sun_col.rgb * pow(max(dot(N, H), 0.0), 120.0) * 1.6;
  let dist = length(i.wpos - g.cam_pos.xyz);
  let fog = clamp(1.0 - exp(-dist * g.params.x), 0.0, 1.0);
  return vec4<f32>(mix(col, g.sky_horizon.rgb, fog), 1.0);
}

// ---- screen-space UI (HUD + minimap), NDC quads, alpha-blended ----
struct UO { @builtin(position) clip: vec4<f32>, @location(0) color: vec4<f32> };
@vertex
fn ui_vs(@builtin(vertex_index) vi: u32,
         @location(0) rect: vec4<f32>, @location(1) color: vec4<f32>) -> UO {
  var q = array<vec2<f32>,6>(
    vec2<f32>(0.0,0.0), vec2<f32>(1.0,0.0), vec2<f32>(1.0,1.0),
    vec2<f32>(0.0,0.0), vec2<f32>(1.0,1.0), vec2<f32>(0.0,1.0));
  let p = rect.xy + q[vi] * rect.zw;
  var o: UO;
  o.clip = vec4<f32>(p, 0.0, 1.0);
  o.color = color;
  return o;
}
@fragment
fn ui_fs(o: UO) -> @location(0) vec4<f32> {
  return o.color;
}
"#;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    sky_pipeline: wgpu::RenderPipeline,
    ground_pipeline: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
    box_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    shadow_view: wgpu::TextureView,
    shadow_bind: wgpu::BindGroup,
    ui_pipeline: wgpu::RenderPipeline,
    ui_buf: wgpu::Buffer,
    ui_cap: u32,
    globals_buf: wgpu::Buffer,
    bind: wgpu::BindGroup,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
    instance_buf: wgpu::Buffer,
    instance_cap: u32,
}

fn make_depth(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

struct Game {
    rt: KamiScriptRuntime,
    world: Arc<Mutex<hecs::World>>,
}
impl Game {
    fn new(logic: &str) -> Self {
        let world = Arc::new(Mutex::new(hecs::World::new()));
        let mut rt = KamiScriptRuntime::new(world.clone()).expect("runtime");
        rt.set_seed(0x2025_0620);
        rt.load_clj("game", logic).expect("compile+load logic.clj");
        rt.call_init("game").expect("init");
        Self { rt, world }
    }
    fn step(&mut self, mx: f32, my: f32) {
        self.rt.feed_stick("MoveX", "MoveY", [mx, my]);
        self.rt.call_systems("game", 16).expect("systems");
        self.rt.integrate(16);
    }
    fn snapshot(&self) -> ([f32; 2], Vec<(String, [f32; 2], u32)>) {
        let w = self.world.lock().unwrap();
        let mut player = [0.0, 0.0];
        let mut out = Vec::new();
        for (e, (t, p)) in w.query::<(&Tag, &Position)>().iter() {
            if t.0 == "player" {
                player = [p.0[0], p.0[1]];
            }
            out.push((t.0.clone(), [p.0[0], p.0[1]], e.id()));
        }
        (player, out)
    }
    fn despawn(&mut self, id: u32) {
        self.rt.despawn_id(id);
    }
    fn set_player(&self, x: f32, y: f32) {
        let w = self.world.lock().unwrap();
        for (_, (t, p)) in w.query::<(&Tag, &mut Position)>().iter() {
            if t.0 == "player" {
                p.0[0] = x;
                p.0[1] = y;
            }
        }
    }
}

#[derive(Default)]
struct Keys {
    w: bool,
    a: bool,
    s: bool,
    d: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    game: Game,
    scene: Scene3,
    keys: Keys,
    props: Vec<Instance>, // static world dressing
    wall_aabbs: Vec<Aabb>, // building walls, for player collision
    walls: Vec<Instance>, // player-built wall pieces
    particles: Vec<Particle3>,
    bullets: Vec<Bullet>,
    phase: Phase,
    weapon: Weapon,
    ammo: u32,
    reserve: u32,
    reload_t: u32, // frames left in a reload (0 = ready)
    bot_hp: HashMap<u32, u32>,
    items: Vec<Item>, // building loot
    gilrs: Option<gilrs::Gilrs>,
    active_pad: Option<gilrs::GamepadId>,
    pad_move: glam::Vec2,
    pad_look: glam::Vec2,
    audio: Option<Audio>,
    hp: f32,
    lives: u32,
    prev_bots: HashMap<u32, Vec3>, // for hit-burst detection
    score: u32,
    build_pressed: bool,
    rng: u32,
    storm_radius: f32,
    face_yaw: f32, // player facing (kept while idle)
    cam_yaw: f32,
    cam_pitch: f32,
    jump_v: f32,
    height: f32,
    time: f32,
    frames: u64,
    last: Option<Instant>,
    fps: f32,
}

impl App {
    fn new(logic: &str, scene: Scene3) -> Self {
        let (props, wall_aabbs, centers) = scatter_props(&scene);
        // one loot item per building, kind cycling health / rifle / shotgun
        let items = centers
            .iter()
            .enumerate()
            .map(|(i, c)| Item {
                pos: *c,
                kind: match i % 3 {
                    0 => Loot::Health,
                    1 => Loot::Rifle,
                    _ => Loot::Shotgun,
                },
            })
            .collect();
        Self {
            window: None,
            gpu: None,
            game: Game::new(logic),
            scene,
            keys: Keys::default(),
            props,
            wall_aabbs,
            walls: Vec::new(),
            particles: Vec::new(),
            bullets: Vec::new(),
            phase: Phase::Skydive,
            weapon: Weapon::Pistol,
            ammo: 12,
            reserve: 240,
            reload_t: 0,
            bot_hp: HashMap::new(),
            items,
            gilrs: gilrs::Gilrs::new().ok(),
            active_pad: None,
            pad_move: glam::Vec2::ZERO,
            pad_look: glam::Vec2::ZERO,
            audio: Audio::new(),
            hp: 100.0,
            lives: 3,
            prev_bots: HashMap::new(),
            score: 0,
            build_pressed: false,
            rng: 0x1357_2468,
            storm_radius: 600.0,
            face_yaw: 0.0,
            cam_yaw: 0.6,
            cam_pitch: 0.5,
            jump_v: 0.0,
            height: 260.0, // start the match high in the sky (skydive)
            time: 0.0,
            frames: 0,
            last: None,
            fps: 0.0,
        }
    }

    fn init_gpu(&mut self, window: Arc<Window>) {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).unwrap();
        let (device, queue, config) = pollster::block_on(async {
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                })
                .await
                .expect("no GPU adapter");
            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("kami3d"),
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits::default(),
                        memory_hints: wgpu::MemoryHints::Performance,
                    },
                    None,
                )
                .await
                .unwrap();
            let caps = surface.get_capabilities(&adapter);
            let format = caps.formats.iter().find(|f| f.is_srgb()).copied().unwrap_or(caps.formats[0]);
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: size.width.max(1),
                height: size.height.max(1),
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode: caps.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &config);
            (device, queue, config)
        });

        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bind"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals_buf.as_entire_binding() }],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        // --- shadow map: sun depth texture + comparison sampler + bind group ---
        let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow"),
            size: wgpu::Extent3d { width: 2048, height: 2048, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let shadow_smp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow-cmp"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shadow_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let shadow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow-bind"),
            layout: &shadow_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&shadow_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&shadow_smp) },
            ],
        });
        // box/ground sample the shadow map → they use a 2-group layout.
        let pl_lit = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl-lit"),
            bind_group_layouts: &[&bgl, &shadow_bgl],
            push_constant_ranges: &[],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kami3d"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let depth_state = |write: bool, cmp: wgpu::CompareFunction| wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: write,
            depth_compare: cmp,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };

        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("sky_vs"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("sky_fs"), targets: &[Some(config.format.into())], compilation_options: Default::default() }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(depth_state(false, wgpu::CompareFunction::Always)),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let ground_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ground"),
            layout: Some(&pl_lit),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("ground_vs"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("ground_fs"), targets: &[Some(config.format.into())], compilation_options: Default::default() }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::LessEqual)),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        // water reuses the globals-only layout (it doesn't sample the shadow map)
        let water_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("water"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("water_vs"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("water_fs"), targets: &[Some(config.format.into())], compilation_options: Default::default() }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::LessEqual)),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 12, shader_location: 1 },
            ],
        };
        let ibl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: 2 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 3 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 4 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 5 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 64, shader_location: 6 },
            ],
        };
        let vbuffers = [vbl, ibl];
        let box_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("box"),
            layout: Some(&pl_lit),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("box_vs"), buffers: &vbuffers, compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("box_fs"), targets: &[Some(config.format.into())], compilation_options: Default::default() }),
            primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::LessEqual)),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        // depth-only pass writing the sun shadow map (reuses the cube + instances).
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("shadow_vs"), buffers: &vbuffers, compilation_options: Default::default() },
            fragment: None,
            primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::LessEqual)),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // screen-space UI pipeline (HUD + minimap): NDC quads, alpha blend, no depth write
        let ui_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-pl"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let ui_ibl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<UiRect>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: 0 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 1 },
            ],
        };
        let ui_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&ui_pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("ui_vs"), buffers: &[ui_ibl], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("ui_fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(depth_state(false, wgpu::CompareFunction::Always)),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let ui_cap = 512u32;
        let ui_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui"),
            size: (ui_cap as usize * std::mem::size_of::<UiRect>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let (verts, indices) = cube();
        let vbuf = create_init_buffer(&device, "v", bytemuck::cast_slice(&verts), wgpu::BufferUsages::VERTEX);
        let ibuf = create_init_buffer(&device, "i", bytemuck::cast_slice(&indices), wgpu::BufferUsages::INDEX);
        let instance_cap = 2048u32;
        let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("inst"),
            size: (instance_cap as usize * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let depth = make_depth(&device, config.width, config.height);

        self.gpu = Some(Gpu {
            device, queue, surface, config, depth, sky_pipeline, ground_pipeline, water_pipeline, box_pipeline,
            shadow_pipeline, shadow_view, shadow_bind, ui_pipeline, ui_buf, ui_cap,
            globals_buf, bind, vbuf, ibuf, index_count: indices.len() as u32, instance_buf, instance_cap,
        });
    }

    /// Swap weapon, top up its magazine, cancel any reload in progress.
    fn set_weapon(&mut self, w: Weapon) {
        self.weapon = w;
        self.ammo = w.ammo_spec().0;
        self.reload_t = 0;
        self.sfx(660.0, 50, 0.15);
    }

    /// Play a synth blip if audio is available.
    fn sfx(&self, freq: f32, ms: u64, amp: f32) {
        if let Some(a) = &self.audio {
            a.blip(freq, ms, amp);
        }
    }

    /// Poll the gamepad: drain events (buttons), read sticks → pad_move/pad_look,
    /// apply right-stick camera look. PS5/Xbox pads work via gilrs (HID/XInput).
    fn poll_pad(&mut self, dt: f32) {
        use gilrs::{Axis, Button, EventType};
        let mut events = Vec::new();
        if let Some(g) = self.gilrs.as_mut() {
            while let Some(ev) = g.next_event() {
                events.push(ev);
            }
        }
        for ev in events {
            self.active_pad = Some(ev.id);
            if let EventType::ButtonPressed(btn, _) = ev.event {
                match btn {
                    Button::South => {
                        if self.phase == Phase::Playing && self.height <= 0.0 {
                            self.jump_v = self.scene.jump;
                            self.sfx(160.0, 120, 0.18);
                        }
                    }
                    Button::East => self.build_pressed = true,
                    Button::West => {
                        let (mag, rf) = self.weapon.ammo_spec();
                        if self.reload_t == 0 && self.ammo < mag && self.reserve > 0 {
                            self.reload_t = rf;
                            self.sfx(330.0, 60, 0.15);
                        }
                    }
                    Button::North => {
                        let next = match self.weapon {
                            Weapon::Pistol => Weapon::Rifle,
                            Weapon::Rifle => Weapon::Shotgun,
                            Weapon::Shotgun => Weapon::Pistol,
                        };
                        self.set_weapon(next);
                    }
                    _ => {}
                }
            }
        }
        let dz = |x: f32, y: f32| {
            let v = glam::vec2(x, y);
            if v.length() < 0.18 { glam::Vec2::ZERO } else { v }
        };
        if let (Some(g), Some(id)) = (self.gilrs.as_ref(), self.active_pad) {
            let gp = g.gamepad(id);
            self.pad_move = dz(gp.value(Axis::LeftStickX), gp.value(Axis::LeftStickY));
            self.pad_look = dz(gp.value(Axis::RightStickX), gp.value(Axis::RightStickY));
        }
        self.cam_yaw += self.pad_look.x * 2.2 * dt;
        self.cam_pitch = (self.cam_pitch - self.pad_look.y * 1.6 * dt).clamp(0.1, 1.3);
    }

    /// xorshift → [0,1) for hit-particle scatter.
    fn rng_next(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x as f32 / u32::MAX as f32
    }

    fn frame(&mut self) {
        self.time += 0.016;
        let now = Instant::now();
        if let Some(p) = self.last {
            let ms = now.duration_since(p).as_secs_f32() * 1000.0;
            self.fps = self.fps * 0.9 + (1000.0 / ms.max(0.1)) * 0.1;
        }
        self.last = Some(now);

        // --- camera orbit from arrows + right stick ---
        let dt = 0.016;
        if self.keys.left { self.cam_yaw -= 1.6 * dt; }
        if self.keys.right { self.cam_yaw += 1.6 * dt; }
        if self.keys.up { self.cam_pitch = (self.cam_pitch + 1.2 * dt).min(1.3); }
        if self.keys.down { self.cam_pitch = (self.cam_pitch - 1.2 * dt).max(0.1); }
        self.poll_pad(dt); // gamepad: sticks + buttons (also nudges cam_yaw/pitch)

        // --- camera-relative ground movement → feed CLJ (keyboard + left stick) ---
        let (sy, cy) = self.cam_yaw.sin_cos();
        // forward = direction the camera looks, on the ground (toward target)
        let fwd = glam::Vec2::new(-sy, -cy);
        let right = glam::Vec2::new(cy, -sy);
        let mut mv = glam::Vec2::ZERO;
        if self.keys.w { mv += fwd; }
        if self.keys.s { mv -= fwd; }
        if self.keys.d { mv += right; }
        if self.keys.a { mv -= right; }
        mv += fwd * self.pad_move.y + right * self.pad_move.x; // left stick
        if mv.length_squared() > 0.0 { mv = mv.normalize(); }
        let player_moving = mv.length_squared() > 0.0;
        if player_moving {
            self.face_yaw = (-mv.x).atan2(-mv.y); // face the movement direction
        }
        let sp = self.scene.player_speed;
        self.game.step(mv.x * sp, mv.y * sp);

        // --- vertical: skydive descent until landing, then normal jump physics ---
        match self.phase {
            Phase::Skydive => {
                let glider_alt = 45.0; // glider deploys here → descent slows
                let fall = if self.height > glider_alt { 95.0 } else { 22.0 };
                self.height -= fall * dt;
                if self.height <= 0.0 {
                    self.height = 0.0;
                    self.jump_v = 0.0;
                    self.phase = Phase::Playing; // touchdown → fight
                    self.sfx(140.0, 200, 0.22); // landing thud
                }
            }
            Phase::Playing => {
                let (h, v) = integrate_jump(self.height, self.jump_v, self.scene.gravity, dt);
                self.height = h;
                self.jump_v = v;
                // storm only closes in once you're on the ground.
                self.storm_radius = (self.storm_radius - 6.0 * 0.016).max(90.0);
            }
        }

        let (player, ents) = self.game.snapshot();
        let gs = self.scene.ground_scale;
        let pw = Vec3::new(player[0] * gs, self.height, player[1] * gs);

        // --- shooting feedback: the CLJ weapon despawns bots; burst where one vanished ---
        let cur_bots: HashMap<u32, Vec3> = ents
            .iter()
            .filter(|(t, _, _)| t == "bot")
            .map(|(_, p, id)| (*id, Vec3::new(p[0] * gs, 0.0, p[1] * gs)))
            .collect();
        let kills: Vec<Vec3> = self
            .prev_bots
            .iter()
            .filter(|(id, _)| !cur_bots.contains_key(id))
            .map(|(_, p)| *p)
            .collect();
        for kpos in kills {
            self.score += 1;
            self.sfx(880.0, 90, 0.18); // kill confirm
            for _ in 0..14 {
                let vx = (self.rng_next() * 2.0 - 1.0) * 4.0;
                let vy = self.rng_next() * 5.0 + 1.0;
                let vz = (self.rng_next() * 2.0 - 1.0) * 4.0;
                let life = 0.5 + self.rng_next() * 0.4;
                self.particles.push(Particle3 {
                    pos: kpos + Vec3::new(0.0, 1.0, 0.0),
                    vel: Vec3::new(vx, vy, vz),
                    age: 0.0,
                    life,
                });
            }
            // bullet tracer: a brief streak of points from the player to the hit.
            let chest = pw + Vec3::new(0.0, 1.4, 0.0);
            let hit = kpos + Vec3::new(0.0, 1.0, 0.0);
            for k in 0..9 {
                let t = k as f32 / 8.0;
                self.particles.push(Particle3 {
                    pos: chest.lerp(hit, t),
                    vel: Vec3::ZERO,
                    age: 0.0,
                    life: 0.12,
                });
            }
        }
        // enemy variety: give newly-seen bots their archetype HP, forget the gone
        for id in cur_bots.keys() {
            self.bot_hp.entry(*id).or_insert_with(|| bot_kind(*id).2);
        }
        self.bot_hp.retain(|id, _| cur_bots.contains_key(id));
        self.prev_bots = cur_bots;

        // --- building: place a wall in front of the player on B ---
        if self.build_pressed {
            self.build_pressed = false;
            let fwd = Vec3::new(-sy, 0.0, -cy); // camera-forward on the ground
            let base = Vec3::new(player[0] * gs, 0.0, player[1] * gs) + fwd * 5.0;
            self.walls.push(Instance {
                model: model_wall(base, self.cam_yaw, 5.0, 4.0, 0.4),
                color: [0.74, 0.62, 0.44, 1.0], // wood
            });
        }

        // --- advance hit particles (gravity + fade) ---
        let dt_p = 0.016;
        for p in &mut self.particles {
            p.age += dt_p;
            p.vel.y -= 14.0 * dt_p;
            p.pos += p.vel * dt_p;
        }
        self.particles.retain(|p| p.age < p.life);

        // --- real bullets: per-weapon fire at the nearest bot in range ---
        let chest = pw + Vec3::new(0.0, 1.4, 0.0);
        let (fire_period, range, pellets, spread, _) = self.weapon.params();
        let (mag, reload_frames) = self.weapon.ammo_spec();
        if self.phase == Phase::Playing {
            if self.reload_t > 0 {
                self.reload_t -= 1;
                if self.reload_t == 0 {
                    let take = (mag - self.ammo).min(self.reserve);
                    self.ammo += take;
                    self.reserve -= take;
                }
            } else if self.ammo == 0 && self.reserve > 0 {
                self.reload_t = reload_frames; // auto-reload when the magazine runs dry
                self.sfx(330.0, 60, 0.15);
            }
        }
        let can_fire = self.phase == Phase::Playing && self.reload_t == 0 && self.ammo > 0;
        if can_fire && self.frames % fire_period == 0 {
            let mut best: Option<(Vec3, f32)> = None;
            for (t, p, _) in &ents {
                if t == "bot" {
                    let bw = Vec3::new(p[0] * gs, 1.0, p[1] * gs);
                    let d = (bw - chest).length();
                    if d < range && best.map_or(true, |(_, bd)| d < bd) {
                        best = Some((bw, d));
                    }
                }
            }
            if let Some((bw, _)) = best {
                let base_dir = (bw - chest).normalize_or_zero();
                for _ in 0..pellets {
                    let jitter = Vec3::new(
                        (self.rng_next() * 2.0 - 1.0) * spread,
                        (self.rng_next() * 2.0 - 1.0) * spread,
                        (self.rng_next() * 2.0 - 1.0) * spread,
                    );
                    let dir = (base_dir + jitter).normalize_or_zero();
                    self.bullets.push(Bullet { pos: chest, vel: dir * 48.0, life: 0.8 });
                }
                self.ammo -= 1; // one trigger pull = one round (shotgun pellets included)
                let fhz = match self.weapon { Weapon::Pistol => 520.0, Weapon::Rifle => 700.0, Weapon::Shotgun => 300.0 };
                self.sfx(fhz, 28, 0.09); // muzzle blip
            }
        }
        for b in &mut self.bullets {
            b.pos += b.vel * dt_p;
            b.life -= dt_p;
        }
        let mut hit_ids: Vec<u32> = Vec::new();
        let mut surviving = Vec::new();
        for b in std::mem::take(&mut self.bullets) {
            let mut hit_at: Option<Vec3> = None;
            for (t, p, id) in &ents {
                if t == "bot" {
                    let bw = Vec3::new(p[0] * gs, 1.0, p[1] * gs);
                    if (b.pos - bw).length() < 1.2 {
                        hit_ids.push(*id);
                        hit_at = Some(b.pos);
                        break;
                    }
                }
            }
            match hit_at {
                Some(at) => {
                    // hit-detection viz: a quick white impact spark at contact
                    for _ in 0..6 {
                        let j = Vec3::new(self.rng_next() * 2.0 - 1.0, self.rng_next() * 2.0 - 1.0, self.rng_next() * 2.0 - 1.0);
                        self.particles.push(Particle3 { pos: at, vel: j * 5.0, age: 0.0, life: 0.18 });
                    }
                }
                None if b.life > 0.0 => surviving.push(b),
                None => {}
            }
        }
        self.bullets = surviving;
        for id in hit_ids {
            // enemy variety: brutes take several hits; despawn only at 0 HP
            match self.bot_hp.get_mut(&id) {
                Some(h) => {
                    *h = h.saturating_sub(1);
                    if *h == 0 {
                        self.game.despawn(id); // → next frame's diff fires the kill burst + score
                    }
                }
                None => self.game.despawn(id),
            }
        }

        // --- storm: take damage outside the safe circle; respawn / new match on 0 lives ---
        let pdist = (player[0] * player[0] + player[1] * player[1]).sqrt();
        if self.phase == Phase::Playing && pdist > self.storm_radius {
            self.hp -= 18.0 * dt_p;
            if self.hp <= 0.0 {
                self.lives = self.lives.saturating_sub(1);
                self.hp = 100.0;
                self.game.set_player(0.0, 0.0); // respawn at the centre
                if self.lives == 0 {
                    self.lives = 3;
                    self.score = 0;
                    self.storm_radius = 600.0; // new match
                }
            }
        } else if self.hp < 100.0 {
            self.hp = (self.hp + 8.0 * dt_p).min(100.0); // heal inside the circle
        }

        // --- building loot: pick up items the player walks over ---
        let pxz = glam::vec2(pw.x, pw.z);
        let mut picked: Vec<usize> = Vec::new();
        for (i, it) in self.items.iter().enumerate() {
            if glam::vec2(it.pos.x, it.pos.z).distance(pxz) < 1.6 {
                picked.push(i);
            }
        }
        for &i in picked.iter().rev() {
            let it = self.items.remove(i);
            self.sfx(988.0, 80, 0.18); // pickup
            match it.kind {
                Loot::Health => self.hp = 100.0,
                Loot::Rifle => {
                    self.set_weapon(Weapon::Rifle);
                    self.reserve += 90;
                }
                Loot::Shotgun => {
                    self.set_weapon(Weapon::Shotgun);
                    self.reserve += 36;
                }
            }
            for _ in 0..12 {
                let j = Vec3::new(self.rng_next() * 2.0 - 1.0, self.rng_next() * 2.0, self.rng_next() * 2.0 - 1.0);
                self.particles.push(Particle3 { pos: it.pos + Vec3::new(0.0, 1.0, 0.0), vel: j * 4.0, age: 0.0, life: 0.5 });
            }
        }

        // --- camera follow: Fortnite-style over-the-shoulder ---
        let shoulder = Vec3::new(cy, 0.0, -sy) * 2.2; // lateral offset → player sits left of centre
        let target = pw + Vec3::new(0.0, self.scene.camera_height * 0.5, 0.0) + shoulder;
        let (spi, cpi) = self.cam_pitch.sin_cos();
        let off = Vec3::new(sy * cpi, spi, cy * cpi) * self.scene.camera_dist;
        let cam = target + off + Vec3::new(0.0, self.scene.camera_height, 0.0);

        let Some(gpu) = self.gpu.as_mut() else { return };
        let aspect = gpu.config.width as f32 / gpu.config.height as f32;
        let proj = Mat4::perspective_rh(60f32.to_radians(), aspect.max(0.1), 0.1, 1200.0);
        let view = Mat4::look_at_rh(cam, target, Vec3::Y);
        let vp = proj * view;

        // sun shadow frustum: orthographic, centred on the player's ground spot.
        let sun = Vec3::new(self.scene.sun_dir[0], self.scene.sun_dir[1], self.scene.sun_dir[2]).normalize();
        let s_center = Vec3::new(pw.x, 0.0, pw.z);
        let light_vp = Mat4::orthographic_rh(-45.0, 45.0, -45.0, 45.0, 1.0, 200.0)
            * Mat4::look_at_rh(s_center - sun * 80.0, s_center, Vec3::Y);

        // --- build instance list: static props + built walls + entities + particles ---
        let mut inst = self.props.clone();
        inst.extend(self.walls.iter().cloned());
        for (tag, pos, id) in &ents {
            if let Some(p) = self.scene.profiles.get(tag) {
                let h = if tag == "player" { self.height } else { 0.0 };
                let ground = Vec3::new(pos[0] * gs, h, pos[1] * gs);
                // facing: player faces its movement; bots face the player
                let (yaw, moving) = if tag == "player" {
                    (self.face_yaw, player_moving)
                } else {
                    ((-(player[0] - pos[0])).atan2(-(player[1] - pos[1])), true)
                };
                let walk = self.time * 9.0 + (*id as f32) * 0.6;
                // enemy variety: grunt / runner / brute differ in size + colour
                let (scl, col) = if tag == "bot" {
                    let (s, c, _) = bot_kind(*id);
                    (s, c)
                } else {
                    (1.0, p.color)
                };
                // blob shadow on the ground (stays grounded even mid-jump)
                push_shadow(&mut inst, Vec3::new(pos[0] * gs, 0.0, pos[1] * gs), p.w * 0.7 * scl);
                push_character(&mut inst, ground, yaw, walk, moving, p.h / 1.9 * scl, col);
            }
        }
        // glider canopy above the player while skydiving
        if self.phase == Phase::Skydive {
            let canopy = pw + Vec3::new(0.0, 3.0, 0.0);
            inst.push(Instance { model: model_box(canopy, 3.4, 0.2), color: [0.95, 0.45, 0.32, 1.0] });
        }
        // floating HP bar above the player (length + colour = health)
        {
            let frac = (self.hp / 100.0).clamp(0.06, 1.0);
            let bar = pw + Vec3::new(0.0, 3.6, 0.0);
            inst.push(Instance { model: model_wall(bar, self.cam_yaw, 2.2 * frac, 0.26, 0.08), color: [1.0 - frac, 0.15 + frac * 0.8, 0.18, 1.0] });
        }
        // storm wall: a ring of glowing pillars at the current safe radius
        let sr = self.storm_radius * gs;
        let ring = 72usize;
        for kk in 0..ring {
            let ang = (kk as f32 / ring as f32) * std::f32::consts::TAU;
            let rp = Vec3::new(ang.cos() * sr, 0.0, ang.sin() * sr);
            inst.push(Instance { model: model_box(rp, 0.6, 7.0), color: [0.72, 0.32, 1.0, 1.0] });
        }
        // building loot: a bobbing, spinning crate colored by kind
        for it in &self.items {
            let bob = (self.time * 2.0).sin() * 0.2 + 1.0;
            let col = match it.kind {
                Loot::Health => [0.3, 1.0, 0.45],
                Loot::Rifle => [0.5, 0.8, 1.0],
                Loot::Shotgun => [1.0, 0.6, 0.3],
            };
            let m = Mat4::from_translation(it.pos + Vec3::new(0.0, bob, 0.0))
                * Mat4::from_rotation_y(self.time * 2.0)
                * Mat4::from_scale(Vec3::splat(0.6));
            inst.push(Instance { model: m.to_cols_array_2d(), color: [col[0], col[1], col[2], 1.0] });
        }
        // bullets: bright tracer cubes (tinted by weapon)
        let (_, _, _, _, bcol) = self.weapon.params();
        for b in &self.bullets {
            inst.push(Instance { model: model_box(b.pos, 0.16, 0.16), color: [bcol[0], bcol[1], bcol[2], 1.0] });
        }
        // hit particles: small bright cubes that fade as they age
        for p in &self.particles {
            let f = 1.0 - p.age / p.life;
            inst.push(Instance {
                model: model_box(p.pos, 0.25 * f + 0.05, 0.25 * f + 0.05),
                color: [1.0, 0.55 + 0.35 * f, 0.2, 1.0],
            });
        }
        let count = inst.len().min(gpu.instance_cap as usize) as u32;

        let sky = &self.scene;
        let globals = Globals {
            view_proj: vp.to_cols_array_2d(),
            cam_pos: [cam.x, cam.y, cam.z, 1.0],
            sun_dir: [sky.sun_dir[0], sky.sun_dir[1], sky.sun_dir[2], 0.0],
            sun_col: [sky.sun_col[0], sky.sun_col[1], sky.sun_col[2], 1.0],
            sky_zenith: [sky.sky_zenith[0], sky.sky_zenith[1], sky.sky_zenith[2], 1.0],
            sky_horizon: [sky.sky_horizon[0], sky.sky_horizon[1], sky.sky_horizon[2], 1.0],
            ground_col: [sky.ground_col[0], sky.ground_col[1], sky.ground_col[2], 1.0],
            params: [sky.fog, self.time, gpu.config.width as f32, gpu.config.height as f32],
            params2: [self.storm_radius, 0.0, 0.0, 0.0],
            light_vp: light_vp.to_cols_array_2d(),
        };
        gpu.queue.write_buffer(&gpu.globals_buf, 0, bytemuck::bytes_of(&globals));
        gpu.queue.write_buffer(&gpu.instance_buf, 0, bytemuck::cast_slice(&inst[..count as usize]));

        // --- HUD + minimap (screen-space quads) ---
        let aspect = gpu.config.width as f32 / gpu.config.height as f32;
        let (mag, reload_frames) = self.weapon.ammo_spec();
        let mut ui: Vec<UiRect> = Vec::new();
        // HP bar (bottom-left)
        ui.push(UiRect::screen(0.03, 0.93, 0.26, 0.028, [0.0, 0.0, 0.0, 0.5]));
        let hpf = (self.hp / 100.0).clamp(0.0, 1.0);
        ui.push(UiRect::screen(0.03, 0.93, 0.26 * hpf, 0.028, [1.0 - hpf, 0.2 + 0.7 * hpf, 0.25, 0.95]));
        // ammo / reload bar (above HP)
        ui.push(UiRect::screen(0.03, 0.885, 0.26, 0.024, [0.0, 0.0, 0.0, 0.5]));
        if self.reload_t > 0 {
            let rf = 1.0 - self.reload_t as f32 / reload_frames as f32;
            ui.push(UiRect::screen(0.03, 0.885, 0.26 * rf, 0.024, [1.0, 0.7, 0.1, 0.95]));
        } else {
            let af = self.ammo as f32 / mag as f32;
            ui.push(UiRect::screen(0.03, 0.885, 0.26 * af, 0.024, [0.85, 0.85, 0.9, 0.95]));
        }
        // weapon swatch + lives pips
        let (_, _, _, _, wcol) = self.weapon.params();
        ui.push(UiRect::screen(0.03, 0.845, 0.03, 0.03, [wcol[0], wcol[1], wcol[2], 1.0]));
        for k in 0..self.lives {
            ui.push(UiRect::screen(0.075 + k as f32 * 0.022, 0.85, 0.016, 0.02, [0.3, 1.0, 0.4, 0.95]));
        }
        // minimap (top-right)
        let (mmx, mmy, mmw) = (0.80, 0.04, 0.17);
        let mmh = mmw * aspect;
        ui.push(UiRect::screen(mmx, mmy, mmw, mmh, [0.05, 0.07, 0.1, 0.55]));
        let (mcx, mcy) = (mmx + mmw * 0.5, mmy + mmh * 0.5);
        let range = 600.0_f32;
        let dot = |wx: f32, wz: f32, ds: f32, col: [f32; 4], list: &mut Vec<UiRect>| {
            let (dx, dz) = ((wx - player[0]) / range, (wz - player[1]) / range);
            if dx.abs() <= 1.0 && dz.abs() <= 1.0 {
                list.push(UiRect::screen(mcx + dx * mmw * 0.5 - ds * 0.5, mcy + dz * mmh * 0.5 - ds * 0.5, ds, ds, col));
            }
        };
        for (t, p, id) in &ents {
            if t == "bot" {
                let c = bot_kind(*id).1;
                dot(p[0], p[1], 0.012, [c[0], c[1], c[2], 1.0], &mut ui);
            }
        }
        for it in &self.items {
            dot(it.pos.x / gs, it.pos.z / gs, 0.012, [0.3, 1.0, 0.45, 1.0], &mut ui);
        }
        dot(player[0], player[1], 0.016, [1.0, 1.0, 1.0, 1.0], &mut ui);
        let ui_count = ui.len().min(gpu.ui_cap as usize) as u32;
        gpu.queue.write_buffer(&gpu.ui_buf, 0, bytemuck::cast_slice(&ui[..ui_count as usize]));

        let frame = match gpu.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                return;
            }
        };
        let v = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("enc") });
        // --- pass 1: render the scene depth from the sun into the shadow map ---
        {
            let mut sp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &gpu.shadow_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if count > 0 {
                sp.set_pipeline(&gpu.shadow_pipeline);
                sp.set_bind_group(0, &gpu.bind, &[]);
                sp.set_vertex_buffer(0, gpu.vbuf.slice(..));
                sp.set_vertex_buffer(1, gpu.instance_buf.slice(..));
                sp.set_index_buffer(gpu.ibuf.slice(..), wgpu::IndexFormat::Uint16);
                sp.draw_indexed(0..gpu.index_count, 0, 0..count);
            }
        }
        // --- pass 2: the lit scene, sampling the shadow map ---
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &v,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.5, g: 0.6, b: 0.8, a: 1.0 }), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &gpu.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rp.set_bind_group(0, &gpu.bind, &[]);
            rp.set_pipeline(&gpu.sky_pipeline);
            rp.draw(0..3, 0..1);
            rp.set_pipeline(&gpu.ground_pipeline);
            rp.set_bind_group(1, &gpu.shadow_bind, &[]); // shadow map (ground + box use it)
            rp.draw(0..6, 0..1);
            rp.set_pipeline(&gpu.water_pipeline); // animated lake (group 0 only)
            rp.draw(0..6, 0..1);
            if count > 0 {
                rp.set_pipeline(&gpu.box_pipeline);
                rp.set_bind_group(1, &gpu.shadow_bind, &[]); // re-bind (water pipeline cleared group 1)
                rp.set_vertex_buffer(0, gpu.vbuf.slice(..));
                rp.set_vertex_buffer(1, gpu.instance_buf.slice(..));
                rp.set_index_buffer(gpu.ibuf.slice(..), wgpu::IndexFormat::Uint16);
                rp.draw_indexed(0..gpu.index_count, 0, 0..count);
            }
            // HUD + minimap on top (screen-space, alpha-blended, no bind groups)
            if ui_count > 0 {
                rp.set_pipeline(&gpu.ui_pipeline);
                rp.set_vertex_buffer(0, gpu.ui_buf.slice(..));
                rp.draw(0..6, 0..ui_count);
            }
        }
        gpu.queue.submit(Some(enc.finish()));
        frame.present();

        // keep the player in-bounds, then resolve collision against building walls
        // (blocked by walls, but can walk in through the door gap).
        let lim = self.scene.prop_spread * 1.4 / gs.max(1e-4);
        let cx = player[0].clamp(-lim, lim);
        let cz = player[1].clamp(-lim, lim);
        let world = resolve_collision(glam::vec2(cx * gs, cz * gs), 0.6, &self.wall_aabbs);
        self.game.set_player(world.x / gs, world.y / gs);

        self.frames += 1;
        if self.frames % 120 == 0 {
            println!("perf[{BACKEND}]: {:.0} fps · bots {} · kills {}", self.fps, ents.iter().filter(|(t, _, _)| t == "bot").count(), self.score);
        }
        if let Some(w) = self.window.as_ref() {
            if self.phase == Phase::Skydive {
                w.set_title(&format!("{} · {:.0} fps · SKYDIVE — alt {:.0}m · WASD steer", self.scene.title, self.fps, self.height));
            } else {
                let ammo_str = if self.reload_t > 0 { "RELOADING".to_string() } else { format!("{}/{}", self.ammo, self.reserve) };
                w.set_title(&format!(
                    "{} · {:.0} fps · {} {} · HP {:.0} · lives {} · kills {} · {} bots · [1/2/3] wpn [R] reload [B] build",
                    self.scene.title, self.fps, self.weapon.name(), ammo_str, self.hp, self.lives, self.score,
                    ents.iter().filter(|(t, _, _)| t == "bot").count()
                ));
            }
        }
    }
}

fn scatter_props(s: &Scene3) -> (Vec<Instance>, Vec<Aabb>, Vec<Vec3>) {
    let mut rng = 0x9E37_79B9u32;
    let mut rnd = || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        (rng as f32 / u32::MAX as f32)
    };
    let mut out = Vec::new();
    let mut aabbs = Vec::new();
    let mut centers = Vec::new();
    for _ in 0..s.prop_count {
        let x = (rnd() * 2.0 - 1.0) * s.prop_spread;
        let z = (rnd() * 2.0 - 1.0) * s.prop_spread;
        if (x * x + z * z).sqrt() < 11.0 {
            continue; // keep the spawn area clear (room-sized buffer)
        }
        let base = Vec3::new(x, 0.0, z);
        let kind = rnd();
        if kind < 0.18 {
            // rock cluster (spatial detail, no collision)
            out.push(Instance { model: model_box(base, 1.7, 0.9), color: [0.5, 0.5, 0.55, 1.0] });
            out.push(Instance { model: model_box(base + Vec3::new(0.7, 0.0, 0.4), 1.0, 0.6), color: [0.44, 0.44, 0.5, 1.0] });
        } else if kind < 0.30 {
            // supply crate
            out.push(Instance { model: model_box(base, 1.1, 1.0), color: [0.6, 0.45, 0.28, 1.0] });
            out.push(Instance { model: model_box(base + Vec3::new(0.0, 1.0, 0.0), 0.7, 0.5), color: [0.52, 0.38, 0.22, 1.0] });
        } else if rnd() < s.tree_ratio {
            // tree: trunk + canopy (solid decoration, no collision)
            out.push(Instance { model: model_box(base, s.tree_w * 0.3, s.tree_h * 0.5), color: [0.45, 0.32, 0.2, 1.0] });
            out.push(Instance { model: model_box(base + Vec3::new(0.0, s.tree_h * 0.5, 0.0), s.tree_w, s.tree_h * 0.6), color: [s.tree_color[0], s.tree_color[1], s.tree_color[2], 1.0] });
        } else {
            // a hollow, enterable building with collidable walls + a door
            let b = &s.buildings[(rnd() * s.buildings.len() as f32) as usize % s.buildings.len()];
            let h = b.min_h + rnd() * (b.max_h - b.min_h);
            let room = 6.0 + rnd() * 3.0;
            make_building(base, room, h.max(3.0), b.color, &mut out, &mut aabbs);
            centers.push(base);
        }
    }
    (out, aabbs, centers)
}

fn create_init_buffer(device: &wgpu::Device, label: &str, data: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
    use wgpu::util::DeviceExt;
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(label), contents: data, usage })
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(self.scene.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        self.init_gpu(window.clone());
        window.request_redraw();
        self.window = Some(window);
        println!("kami-clj-play3d: window open — WASD move, arrows orbit, Space jump, Esc quit.");
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.config.width = size.width.max(1);
                    gpu.config.height = size.height.max(1);
                    gpu.surface.configure(&gpu.device, &gpu.config);
                    gpu.depth = make_depth(&gpu.device, gpu.config.width, gpu.config.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let down = event.state == ElementState::Pressed;
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) if down => event_loop.exit(),
                    PhysicalKey::Code(KeyCode::Space) if down => {
                        if self.height <= 0.001 {
                            self.jump_v = self.scene.jump;
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyB) if down => self.build_pressed = true,
                    PhysicalKey::Code(KeyCode::Digit1) if down => self.set_weapon(Weapon::Pistol),
                    PhysicalKey::Code(KeyCode::Digit2) if down => self.set_weapon(Weapon::Rifle),
                    PhysicalKey::Code(KeyCode::Digit3) if down => self.set_weapon(Weapon::Shotgun),
                    PhysicalKey::Code(KeyCode::KeyR) if down => {
                        let (mag, rf) = self.weapon.ammo_spec();
                        if self.reload_t == 0 && self.ammo < mag && self.reserve > 0 {
                            self.reload_t = rf;
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyW) => self.keys.w = down,
                    PhysicalKey::Code(KeyCode::KeyA) => self.keys.a = down,
                    PhysicalKey::Code(KeyCode::KeyS) => self.keys.s = down,
                    PhysicalKey::Code(KeyCode::KeyD) => self.keys.d = down,
                    PhysicalKey::Code(KeyCode::ArrowLeft) => self.keys.left = down,
                    PhysicalKey::Code(KeyCode::ArrowRight) => self.keys.right = down,
                    PhysicalKey::Code(KeyCode::ArrowUp) => self.keys.up = down,
                    PhysicalKey::Code(KeyCode::ArrowDown) => self.keys.down = down,
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.frame();
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

/// Fixed-step jump/gravity integration → (height, vertical velocity), clamped to
/// the ground (height ≥ 0, velocity reset on landing). Pure, so it's unit-tested.
fn integrate_jump(height: f32, vel: f32, gravity: f32, dt: f32) -> (f32, f32) {
    let v = vel - gravity * dt;
    let h = height + v * dt;
    if h <= 0.0 {
        (0.0, 0.0)
    } else {
        (h, v)
    }
}

fn game_dir() -> std::path::PathBuf {
    if let Ok(d) = std::env::var("KAMI_GAME_DIR") {
        return std::path::PathBuf::from(d);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let bundled = dir.join("../Resources/game");
            if bundled.join("scene.edn").exists() {
                return bundled;
            }
        }
    }
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("games/royale")
}

fn main() {
    let base = game_dir();
    let logic = read_or_exit(&base, "logic.clj");
    let scene = parse_scene(&read_or_exit(&base, "scene.edn")).unwrap_or_else(|| {
        eprintln!("kami-clj-play3d: {} is not a valid EDN scene map", base.join("scene.edn").display());
        std::process::exit(2);
    });
    println!("kami-clj-play3d: loaded '{}' (CLJ→WASM {BACKEND}, {} profiles).", scene.title, scene.profiles.len());
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut app = App::new(&logic, scene);
    event_loop.run_app(&mut app).expect("run");
}

#[cfg(test)]
mod tests {
    use super::integrate_jump;

    #[test]
    fn at_rest_stays_grounded() {
        assert_eq!(integrate_jump(0.0, 0.0, 26.0, 0.016), (0.0, 0.0));
    }

    #[test]
    fn jump_rises_then_lands_and_resets() {
        let (g, dt) = (26.0f32, 0.016f32);
        let (mut h, mut v) = (0.0f32, 9.5f32); // jump impulse
        let mut peak = 0.0f32;
        for _ in 0..400 {
            let (nh, nv) = integrate_jump(h, v, g, dt);
            h = nh;
            v = nv;
            peak = peak.max(h);
            assert!(h >= 0.0, "player never sinks below the ground");
        }
        assert!(peak > 1.0, "the jump reaches a real height (peak {peak})");
        assert_eq!(h, 0.0, "and eventually lands");
        assert_eq!(v, 0.0, "with vertical velocity reset on landing");
    }
}
