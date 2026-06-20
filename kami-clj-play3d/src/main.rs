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
    params: [f32; 4], // fog, time, res_w, res_h
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

const SHADER: &str = r#"
struct G {
  view_proj: mat4x4<f32>, cam_pos: vec4<f32>, sun_dir: vec4<f32>, sun_col: vec4<f32>,
  sky_zenith: vec4<f32>, sky_horizon: vec4<f32>, ground_col: vec4<f32>, params: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: G;

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
fn shade(wpos: vec3<f32>, n: vec3<f32>, base: vec3<f32>) -> vec3<f32> {
  let L = normalize(-g.sun_dir.xyz);
  let lambert = max(dot(normalize(n), L), 0.0);
  var col = base * (0.38 + lambert * g.sun_col.rgb * 0.85);
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
    fn snapshot(&self) -> ([f32; 2], Vec<(String, [f32; 2])>) {
        let w = self.world.lock().unwrap();
        let mut player = [0.0, 0.0];
        let mut out = Vec::new();
        for (_, (t, p)) in w.query::<(&Tag, &Position)>().iter() {
            if t.0 == "player" {
                player = [p.0[0], p.0[1]];
            }
            out.push((t.0.clone(), [p.0[0], p.0[1]]));
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
            layout: Some(&pl),
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
        let box_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("box"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("box_vs"), buffers: &[vbl, ibl], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("box_fs"), targets: &[Some(config.format.into())], compilation_options: Default::default() }),
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
            globals_buf, bind, vbuf, ibuf, index_count: indices.len() as u32, instance_buf, instance_cap,
        });
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
        let sp = self.scene.player_speed;
        self.game.step(mv.x * sp, mv.y * sp);

        // --- gravity / jump (host owns height) ---
        let (h, v) = integrate_jump(self.height, self.jump_v, self.scene.gravity, dt);
        self.height = h;
        self.jump_v = v;

        let (player, ents) = self.game.snapshot();
        let gs = self.scene.ground_scale;
        let pw = Vec3::new(player[0] * gs, self.height, player[1] * gs);

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

        // --- build instance list: static props + entities ---
        let mut inst = self.props.clone();
        for (tag, pos) in &ents {
            if let Some(p) = self.scene.profiles.get(tag) {
                let h = if tag == "player" { self.height } else { 0.0 };
                let base = Vec3::new(pos[0] * gs, h, pos[1] * gs);
                inst.push(Instance { model: model_box(base, p.w, p.h), color: [p.color[0], p.color[1], p.color[2], 1.0] });
                // little "head" cube so characters read as figures
                let head = Vec3::new(pos[0] * gs, h + p.h, pos[1] * gs);
                inst.push(Instance { model: model_box(head, p.w * 0.6, p.w * 0.6), color: [p.color[0] * 1.1, p.color[1] * 1.1, p.color[2] * 1.1, 1.0] });
            }
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
            println!("perf[{BACKEND}]: {:.0} fps · bots {}", self.fps, ents.iter().filter(|(t, _)| t == "bot").count());
        }
        if let Some(w) = self.window.as_ref() {
            w.set_title(&format!("{} · {:.0} fps · {} bots", self.scene.title, self.fps, ents.iter().filter(|(t, _)| t == "bot").count()));
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
