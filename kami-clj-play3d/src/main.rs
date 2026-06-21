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
  @location(2) color: vec3<f32>,
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
  o.color = color.rgb;
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
  return vec4<f32>(shade(i.wpos, i.wnormal, i.color), 1.0);
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
  o.wpos = world; o.wnormal = vec3<f32>(0.0,1.0,0.0); o.color = g.ground_col.rgb;
  return o;
}
@fragment
fn ground_fs(i: VO) -> @location(0) vec4<f32> {
  let gp = abs(fract(i.wpos.xz / 4.0) - 0.5);
  let line = smoothstep(0.47, 0.5, max(gp.x, gp.y));
  let base = mix(i.color, i.color * 1.25, line);
  return vec4<f32>(shade(i.wpos, i.wnormal, base), 1.0);
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
    box_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    shadow_view: wgpu::TextureView,
    shadow_bind: wgpu::BindGroup,
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
    walls: Vec<Instance>, // player-built wall pieces
    particles: Vec<Particle3>,
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
        let props = scatter_props(&scene);
        Self {
            window: None,
            gpu: None,
            game: Game::new(logic),
            scene,
            keys: Keys::default(),
            props,
            walls: Vec::new(),
            particles: Vec::new(),
            prev_bots: HashMap::new(),
            score: 0,
            build_pressed: false,
            rng: 0x1357_2468,
            storm_radius: 600.0,
            face_yaw: 0.0,
            cam_yaw: 0.6,
            cam_pitch: 0.5,
            jump_v: 0.0,
            height: 0.0,
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
            device, queue, surface, config, depth, sky_pipeline, ground_pipeline, box_pipeline,
            shadow_pipeline, shadow_view, shadow_bind,
            globals_buf, bind, vbuf, ibuf, index_count: indices.len() as u32, instance_buf, instance_cap,
        });
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

        // --- camera orbit from arrows ---
        let dt = 0.016;
        if self.keys.left { self.cam_yaw -= 1.6 * dt; }
        if self.keys.right { self.cam_yaw += 1.6 * dt; }
        if self.keys.up { self.cam_pitch = (self.cam_pitch + 1.2 * dt).min(1.3); }
        if self.keys.down { self.cam_pitch = (self.cam_pitch - 1.2 * dt).max(0.1); }

        // --- camera-relative ground movement → feed CLJ ---
        let (sy, cy) = self.cam_yaw.sin_cos();
        // forward = direction the camera looks, on the ground (toward target)
        let fwd = glam::Vec2::new(-sy, -cy);
        let right = glam::Vec2::new(cy, -sy);
        let mut mv = glam::Vec2::ZERO;
        if self.keys.w { mv += fwd; }
        if self.keys.s { mv -= fwd; }
        if self.keys.d { mv += right; }
        if self.keys.a { mv -= right; }
        if mv.length_squared() > 0.0 { mv = mv.normalize(); }
        let player_moving = mv.length_squared() > 0.0;
        if player_moving {
            self.face_yaw = (-mv.x).atan2(-mv.y); // face the movement direction
        }
        let sp = self.scene.player_speed;
        self.game.step(mv.x * sp, mv.y * sp);

        // storm closes in over time (down to a small final circle).
        self.storm_radius = (self.storm_radius - 6.0 * 0.016).max(90.0);

        // --- gravity / jump (host owns height) ---
        let (h, v) = integrate_jump(self.height, self.jump_v, self.scene.gravity, dt);
        self.height = h;
        self.jump_v = v;

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

        // --- camera follow ---
        let target = pw + Vec3::new(0.0, self.scene.camera_height * 0.5, 0.0);
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
                // blob shadow on the ground (stays grounded even mid-jump)
                push_shadow(&mut inst, Vec3::new(pos[0] * gs, 0.0, pos[1] * gs), p.w * 0.7);
                push_character(&mut inst, ground, yaw, walk, moving, p.h / 1.9, p.color);
            }
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
            if count > 0 {
                rp.set_pipeline(&gpu.box_pipeline);
                rp.set_vertex_buffer(0, gpu.vbuf.slice(..));
                rp.set_vertex_buffer(1, gpu.instance_buf.slice(..));
                rp.set_index_buffer(gpu.ibuf.slice(..), wgpu::IndexFormat::Uint16);
                rp.draw_indexed(0..gpu.index_count, 0, 0..count);
            }
        }
        gpu.queue.submit(Some(enc.finish()));
        frame.present();

        // keep the player on the ground plane (no leaving the map limits)
        let lim = self.scene.prop_spread * 1.4 / gs.max(1e-4);
        let cp = [player[0].clamp(-lim, lim), player[1].clamp(-lim, lim)];
        self.game.set_player(cp[0], cp[1]);

        self.frames += 1;
        if self.frames % 120 == 0 {
            println!("perf[{BACKEND}]: {:.0} fps · bots {} · kills {}", self.fps, ents.iter().filter(|(t, _, _)| t == "bot").count(), self.score);
        }
        if let Some(w) = self.window.as_ref() {
            w.set_title(&format!(
                "{} · {:.0} fps · kills {} · {} bots · [B] build",
                self.scene.title, self.fps, self.score,
                ents.iter().filter(|(t, _, _)| t == "bot").count()
            ));
        }
    }
}

fn scatter_props(s: &Scene3) -> Vec<Instance> {
    let mut rng = 0x9E37_79B9u32;
    let mut rnd = || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        (rng as f32 / u32::MAX as f32)
    };
    let mut out = Vec::new();
    for _ in 0..s.prop_count {
        let x = (rnd() * 2.0 - 1.0) * s.prop_spread;
        let z = (rnd() * 2.0 - 1.0) * s.prop_spread;
        if (x * x + z * z).sqrt() < 6.0 {
            continue; // keep spawn area clear
        }
        let base = Vec3::new(x, 0.0, z);
        if rnd() < s.tree_ratio {
            // tree: trunk + canopy
            out.push(Instance { model: model_box(base, s.tree_w * 0.3, s.tree_h * 0.5), color: [0.45, 0.32, 0.2, 1.0] });
            out.push(Instance { model: model_box(base + Vec3::new(0.0, s.tree_h * 0.5, 0.0), s.tree_w, s.tree_h * 0.6), color: [s.tree_color[0], s.tree_color[1], s.tree_color[2], 1.0] });
        } else {
            let b = &s.buildings[(rnd() * s.buildings.len() as f32) as usize % s.buildings.len()];
            let h = b.min_h + rnd() * (b.max_h - b.min_h);
            out.push(Instance { model: model_box(base, b.w, h), color: [b.color[0], b.color[1], b.color[2], 1.0] });
        }
    }
    out
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
