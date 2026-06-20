//! kami-clj-play — a data-driven game player. The GAME (logic + scene) is CLJ +
//! EDN; this Rust binary is only the GPU arm (ADR-0036).
//!
//! At startup it loads two files and contains no game content itself:
//!   games/<g>/logic.clj  — gameplay in the kami-clj subset → compiled to WASM,
//!                          driven by kami-script-runtime over hecs (ADR-0035).
//!   games/<g>/scene.edn  — the Datomic-shaped data datalevin owns (ADR-0036):
//!                          world tuning + per-tag render profiles. Edit it and
//!                          re-run; the game changes with no Rust rebuild.
//!
//! Rust supplies only the wgpu (Metal) renderer + the host loop + the
//! device-neutral input seam. Same path iOS/console use; same CLJ+EDN game.
//!
//!   cargo run --target aarch64-apple-darwin -p kami-clj-play
//!
//! Arrows move · survive the swarm · Esc quits.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use kami_core::actor::components::Position;
use kami_script_runtime::{KamiScriptRuntime, Tag, BACKEND};
use kami_scene::{kw_key, mget, num, vec3};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

// ── Scene data (parsed from scene.edn — Datomic's half of the game) ─────────
#[derive(Clone)]
struct RenderProfile {
    color: [f32; 3],
    size: f32,
    glow: f32,
    pulse: bool,
}

struct Scene {
    title: String,
    player_speed: f32,
    camera_scale: f32, // world px per NDC half-extent
    arena: f32,
    profiles: HashMap<String, RenderProfile>,
    burst_count: usize,
    burst_speed: f32,
    burst_life: f32,
    burst_color: [f32; 3],
    burst_size: f32,
}


/// Parse the scene; `None` (not a panic) if `src` isn't a valid EDN map.
fn parse_scene(src: &str) -> Option<Scene> {
    let parsed = kami_scene::root_map(src)?;
    let root = &parsed;

    let world = mget(root, "world").and_then(|w| w.as_map());
    let wget = |k: &str| world.and_then(|w| mget(w, k));

    let mut profiles = HashMap::new();
    if let Some(pm) = mget(root, "render/profiles").and_then(|p| p.as_map()) {
        for (k, v) in pm {
            if let (Some(tag), Some(prof)) = (kw_key(k), v.as_map()) {
                profiles.insert(
                    tag,
                    RenderProfile {
                        color: vec3(mget(prof, "color")),
                        size: num(mget(prof, "size")),
                        glow: num(mget(prof, "glow")),
                        pulse: mget(prof, "pulse").and_then(|b| b.as_bool()).unwrap_or(false),
                    },
                );
            }
        }
    }

    let burst = mget(root, "fx/burst").and_then(|b| b.as_map());
    let bget = |k: &str| burst.and_then(|b| mget(b, k));

    Some(Scene {
        title: mget(root, "game/title")
            .and_then(|t| t.as_string())
            .unwrap_or("kami-clj-play")
            .to_string(),
        player_speed: num(wget("player-speed")),
        camera_scale: num(wget("camera-scale")),
        arena: num(wget("arena")),
        profiles,
        burst_count: num(bget("count")) as usize,
        burst_speed: num(bget("speed")),
        burst_life: num(bget("life")),
        burst_color: vec3(bget("color")),
        burst_size: num(bget("size")),
    })
}

/// Read a game file or exit with a clear message (no panic backtrace).
fn read_or_exit(base: &std::path::Path, name: &str) -> String {
    std::fs::read_to_string(base.join(name)).unwrap_or_else(|e| {
        eprintln!("kami-clj-play: cannot read {}: {e}", base.join(name).display());
        std::process::exit(1);
    })
}

// ── Debug overlay: a 3×5 LED font drawn as little glowing discs ─────────────
// Each glyph is 5 rows of 3 bits (left→right). Lit cells become sprite instances,
// so the perf HUD reuses the same instanced pipeline — no font texture needed.
fn glyph(c: char) -> [u8; 5] {
    match c {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '.' => [0b000, 0b000, 0b000, 0b000, 0b010],
        ':' => [0b000, 0b010, 0b000, 0b010, 0b000],
        'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
        'B' => [0b110, 0b101, 0b110, 0b101, 0b110],
        'C' => [0b111, 0b100, 0b100, 0b100, 0b111],
        'D' => [0b110, 0b101, 0b101, 0b101, 0b110],
        'E' => [0b111, 0b100, 0b111, 0b100, 0b111],
        'F' => [0b111, 0b100, 0b111, 0b100, 0b100],
        'G' => [0b111, 0b100, 0b101, 0b101, 0b111],
        'I' => [0b111, 0b010, 0b010, 0b010, 0b111],
        'M' => [0b101, 0b111, 0b111, 0b101, 0b101],
        'N' => [0b101, 0b111, 0b111, 0b111, 0b101],
        'P' => [0b111, 0b101, 0b111, 0b100, 0b100],
        'R' => [0b111, 0b101, 0b111, 0b110, 0b101],
        'S' => [0b111, 0b100, 0b111, 0b001, 0b111],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        'U' => [0b101, 0b101, 0b101, 0b101, 0b111],
        'W' => [0b101, 0b101, 0b111, 0b111, 0b101],
        _ => [0; 5], // space + unknown
    }
}

fn push_text(inst: &mut Vec<Instance>, text: &str, x0: f32, y0: f32, cell: f32, aspect: f32, color: [f32; 3]) {
    let mut col0 = 0.0f32;
    for ch in text.chars() {
        let g = glyph(ch);
        for (row, bits) in g.iter().enumerate() {
            for col in 0..3 {
                if (bits >> (2 - col)) & 1 == 1 {
                    inst.push(Instance {
                        center: [x0 + (col0 + col as f32) * cell / aspect, y0 - row as f32 * cell],
                        radius: cell * 0.42,
                        glow: 0.5,
                        color,
                        _pad: 0.0,
                    });
                }
            }
        }
        col0 += 4.0; // 3 wide + 1 space
    }
}

// ── Shared per-frame uniform (both passes) ──────────────────────────────────
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    cam: [f32; 2],
    aspect: f32,
    time: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    center: [f32; 2],
    radius: f32,
    glow: f32,
    color: [f32; 3],
    _pad: f32,
}

const BG_SHADER: &str = r#"
struct G { cam: vec2<f32>, aspect: f32, time: f32 };
@group(0) @binding(0) var<uniform> g: G;
@vertex
fn vs(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0,-3.0), vec2<f32>(-1.0,1.0), vec2<f32>(3.0,1.0));
    return vec4<f32>(p[vi], 0.0, 1.0);
}
@fragment
fn fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let res = vec2<f32>(1280.0, 800.0);
    let uv = frag.xy / res;
    let ndc = (uv - 0.5) * vec2<f32>(2.0, -2.0);
    var col = mix(vec3<f32>(0.06,0.07,0.12), vec3<f32>(0.16,0.10,0.20), uv.y);
    let world = ndc / vec2<f32>(g.aspect, 1.0) / (1.0/620.0) + g.cam;
    let gp = abs(fract(world / 80.0) - 0.5);
    col += vec3<f32>(0.10,0.16,0.24) * smoothstep(0.46, 0.5, max(gp.x, gp.y)) * 0.5;
    let d = length(world);
    col += vec3<f32>(0.10,0.08,0.18) * (0.5+0.5*sin(d*0.03 - g.time*2.0)) * 0.05;
    col *= mix(0.55, 1.0, smoothstep(1.3, 0.4, length(ndc)));
    return vec4<f32>(col, 1.0);
}
"#;

const SPRITE_SHADER: &str = r#"
struct G { cam: vec2<f32>, aspect: f32, time: f32 };
@group(0) @binding(0) var<uniform> g: G;
struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) glow: f32,
};
@vertex
fn vs(@builtin(vertex_index) vi: u32,
      @location(0) center: vec2<f32>, @location(1) radius: f32,
      @location(2) glow: f32, @location(3) color: vec3<f32>) -> VSOut {
    var q = array<vec2<f32>, 6>(
        vec2<f32>(-1.0,-1.0), vec2<f32>(1.0,-1.0), vec2<f32>(1.0,1.0),
        vec2<f32>(-1.0,-1.0), vec2<f32>(1.0,1.0), vec2<f32>(-1.0,1.0));
    let corner = q[vi];
    var o: VSOut;
    o.pos = vec4<f32>(center + corner * vec2<f32>(radius / g.aspect, radius), 0.0, 1.0);
    o.uv = corner; o.color = color; o.glow = glow;
    return o;
}
@fragment
fn fs(in: VSOut) -> @location(0) vec4<f32> {
    let d = length(in.uv);
    let disc = 1.0 - smoothstep(0.62, 0.82, d);
    let halo = in.glow * (1.0 - smoothstep(0.0, 1.0, d)) * 0.8;
    let a = clamp(max(disc, halo), 0.0, 1.0);
    if (a <= 0.003) { discard; }
    return vec4<f32>(in.color + vec3<f32>(0.35) * halo, a);
}
"#;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    bg_pipeline: wgpu::RenderPipeline,
    sprite_pipeline: wgpu::RenderPipeline,
    globals_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    instance_cap: u32,
}

struct Particle {
    pos: [f32; 2],
    vel: [f32; 2],
    age: f32,
    life: f32,
}

struct Game {
    rt: KamiScriptRuntime,
    world: Arc<Mutex<hecs::World>>,
}

impl Game {
    fn new(logic: &str) -> Self {
        let world = Arc::new(Mutex::new(hecs::World::new()));
        let mut rt = KamiScriptRuntime::new(world.clone()).expect("runtime");
        rt.set_seed(0x5151_2737);
        rt.load_clj("game", logic).expect("compile+load logic.clj");
        rt.call_init("game").expect("init");
        Self { rt, world }
    }

    fn step(&mut self, mx: f32, my: f32, player_speed: f32, arena: f32) {
        self.rt
            .feed_stick("MoveX", "MoveY", [mx * player_speed, my * player_speed]);
        self.rt.call_systems("game", 16).expect("systems");
        self.rt.integrate(16);
        // Host-side f32 clamp keeps the player in the arena (guest is integer-only).
        let w = self.world.lock().unwrap();
        for (_, (t, p)) in w.query::<(&Tag, &mut Position)>().iter() {
            if t.0 == "player" {
                p.0[0] = p.0[0].clamp(-arena, arena);
                p.0[1] = p.0[1].clamp(-arena, arena);
            }
        }
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
}

#[derive(Default)]
struct Keys {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    game: Game,
    scene: Scene,
    keys: Keys,
    time: f32,
    score: u32,
    prev_enemies: HashMap<u32, [f32; 2]>,
    particles: Vec<Particle>,
    rng: u32,
    // --- debug / perf overlay ---
    debug: bool,
    last_frame: Option<Instant>,
    fps: f32,
    frame_ms: f32,
    step_ms: f32,         // CLJ game step (host + wasm) wall time
    frame_hist: Vec<f32>, // recent frame times (ms) for the graph
    frames: u64,          // total frames (for periodic perf logging)
}

impl App {
    fn new(logic: &str, scene: Scene) -> Self {
        Self {
            window: None,
            gpu: None,
            game: Game::new(logic),
            scene,
            keys: Keys::default(),
            time: 0.0,
            score: 0,
            prev_enemies: HashMap::new(),
            particles: Vec::new(),
            rng: 0x1234_5678,
            debug: true, // start with the perf HUD on; F1 toggles
            last_frame: None,
            fps: 0.0,
            frame_ms: 0.0,
            step_ms: 0.0,
            frame_hist: Vec::new(),
            frames: 0,
        }
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
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
                        label: Some("kami-clj-play"),
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits::default(),
                        memory_hints: wgpu::MemoryHints::Performance,
                    },
                    None,
                )
                .await
                .unwrap();
            let caps = surface.get_capabilities(&adapter);
            let format = caps
                .formats
                .iter()
                .find(|f| f.is_srgb())
                .copied()
                .unwrap_or(caps.formats[0]);
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

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("g-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("g-bind"),
            layout: &bind_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl"),
            bind_group_layouts: &[&bind_layout],
            push_constant_ranges: &[],
        });

        let bg_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bg"),
            source: wgpu::ShaderSource::Wgsl(BG_SHADER.into()),
        });
        let bg_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bg-pipe"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &bg_mod,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &bg_mod,
                entry_point: Some("fs"),
                targets: &[Some(config.format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let sprite_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprite"),
            source: wgpu::ShaderSource::Wgsl(SPRITE_SHADER.into()),
        });
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32,   offset: 8, shader_location: 1 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32,   offset: 12, shader_location: 2 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 16, shader_location: 3 },
            ],
        };
        let sprite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprite-pipe"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &sprite_mod,
                entry_point: Some("vs"),
                buffers: &[instance_layout],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &sprite_mod,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let instance_cap = 4096;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (instance_cap as usize * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        self.gpu = Some(Gpu {
            device, queue, surface, config, bg_pipeline, sprite_pipeline,
            globals_buffer, bind_group, instance_buffer, instance_cap,
        });
    }

    fn update_and_render(&mut self) {
        self.time += 0.016;

        // Real wall-clock frame timing (EMA-smoothed) for the perf HUD.
        let now = Instant::now();
        if let Some(prev) = self.last_frame {
            let ms = now.duration_since(prev).as_secs_f32() * 1000.0;
            self.frame_ms = self.frame_ms * 0.9 + ms * 0.1;
            self.fps = if self.frame_ms > 0.0 { 1000.0 / self.frame_ms } else { 0.0 };
            self.frame_hist.push(ms);
            if self.frame_hist.len() > 96 {
                self.frame_hist.remove(0);
            }
        }
        self.last_frame = Some(now);

        // Periodic perf line to stdout — makes the wasmtime-vs-wasmi (JIT vs
        // no-JIT) game-step cost measurable from the logs, not just the on-screen HUD.
        self.frames += 1;
        if self.frames % 120 == 0 {
            println!(
                "perf[{BACKEND}]: {:.0} fps · frame {:.2}ms · step {:.3}ms",
                self.fps, self.frame_ms, self.step_ms
            );
        }

        let mx = (self.keys.right as i32 - self.keys.left as i32) as f32;
        let my = (self.keys.up as i32 - self.keys.down as i32) as f32;
        // Time just the CLJ game step (host loop + wasm systems + integrate).
        let t0 = Instant::now();
        self.game.step(mx, my, self.scene.player_speed, self.scene.arena);
        self.step_ms = self.step_ms * 0.9 + t0.elapsed().as_secs_f32() * 1000.0 * 0.1;

        let (player, ents) = self.game.snapshot();

        // enemy deaths → score + bursts (driven by scene.edn :fx/burst)
        let mut cur: HashMap<u32, [f32; 2]> = HashMap::new();
        for (tag, pos, id) in &ents {
            if tag == "enemy" {
                cur.insert(*id, *pos);
            }
        }
        let dead: Vec<[f32; 2]> = self
            .prev_enemies
            .iter()
            .filter(|(id, _)| !cur.contains_key(id))
            .map(|(_, p)| *p)
            .collect();
        let (bn, bs, bl) = (self.scene.burst_count, self.scene.burst_speed, self.scene.burst_life);
        for p in dead {
            self.score += 1;
            for _ in 0..bn {
                let vx = self.rand() * bs;
                let vy = self.rand() * bs;
                let life = bl * (0.7 + self.rand().abs() * 0.6);
                self.particles.push(Particle { pos: p, vel: [vx, vy], age: 0.0, life });
            }
        }
        self.prev_enemies = cur;

        for p in &mut self.particles {
            p.age += 0.016;
            p.pos[0] += p.vel[0] * 0.016;
            p.pos[1] += p.vel[1] * 0.016;
            p.vel[0] *= 0.90;
            p.vel[1] *= 0.90;
        }
        self.particles.retain(|p| p.age < p.life);

        let scale = if self.scene.camera_scale > 1.0 { 1.0 / self.scene.camera_scale } else { 1.0 / 620.0 };
        let burst_color = self.scene.burst_color;
        let burst_size = self.scene.burst_size;
        let time = self.time;
        // Snapshot profile lookups we need (avoid borrowing self in the loop).
        let profiles = self.scene.profiles.clone();
        // Perf-HUD locals captured before the &mut self.gpu borrow.
        let debug = self.debug;
        let (fps, frame_ms, step_ms) = (self.fps, self.frame_ms, self.step_ms);
        let frame_hist = self.frame_hist.clone();
        let par_count = self.particles.len();
        let backend_label = BACKEND.to_uppercase();

        let Some(gpu) = self.gpu.as_mut() else { return };
        let aspect = gpu.config.width as f32 / gpu.config.height as f32;
        let to_ndc = |w: [f32; 2]| -> [f32; 2] {
            [(w[0] - player[0]) * scale * aspect, (w[1] - player[1]) * scale]
        };

        let mut inst: Vec<Instance> = Vec::with_capacity(ents.len() + self.particles.len());
        for p in &self.particles {
            let f = 1.0 - p.age / p.life;
            inst.push(Instance {
                center: to_ndc(p.pos),
                radius: burst_size * f + 0.004,
                glow: 0.9 * f,
                color: [burst_color[0], burst_color[1] * (0.6 + 0.4 * f), burst_color[2]],
                _pad: 0.0,
            });
        }
        let mut enemies = 0;
        for (tag, pos, _) in &ents {
            if tag == "enemy" {
                enemies += 1;
            }
            // Render purely from the data-driven profile — no hardcoded look.
            let Some(prof) = profiles.get(tag) else { continue };
            let r = if prof.pulse { prof.size + 0.004 * (time * 6.0).sin() } else { prof.size };
            inst.push(Instance {
                center: to_ndc(*pos),
                radius: r,
                glow: prof.glow,
                color: prof.color,
                _pad: 0.0,
            });
        }

        // ── debug / perf overlay (screen-space, not camera-relative) ────────
        if debug {
            let cell = 0.017;
            let cyan = [0.55, 1.0, 0.95];
            push_text(&mut inst, &format!("FPS {:.0}", fps), -0.97, 0.95, cell, aspect, cyan);
            push_text(&mut inst, &format!("FRAME {:.1}MS", frame_ms), -0.97, 0.87, cell, aspect, cyan);
            push_text(&mut inst, &format!("STEP {:.2}MS", step_ms), -0.97, 0.79, cell, aspect, cyan);
            push_text(&mut inst, &format!("ENT {} PAR {}", enemies, par_count), -0.97, 0.71, cell, aspect, cyan);
            push_text(&mut inst, &backend_label, -0.97, 0.63, cell, aspect, [0.6, 0.8, 1.0]);
            // frametime graph: dotted curve of recent frames, coloured by budget.
            let (base_y, height, budget) = (0.45, 0.13, 16.7f32);
            for (i, ms) in frame_hist.iter().enumerate() {
                let x = -0.97 + (i as f32) * 0.007 / aspect;
                let y = base_y + (ms / 40.0).min(1.0) * height;
                let c = if *ms < budget {
                    [0.4, 1.0, 0.5]
                } else if *ms < budget * 2.0 {
                    [1.0, 0.85, 0.3]
                } else {
                    [1.0, 0.35, 0.35]
                };
                inst.push(Instance { center: [x, y], radius: 0.006, glow: 0.4, color: c, _pad: 0.0 });
            }
        }

        let count = inst.len().min(gpu.instance_cap as usize) as u32;

        let globals = Globals { cam: player, aspect, time };
        gpu.queue.write_buffer(&gpu.globals_buffer, 0, bytemuck::bytes_of(&globals));
        if count > 0 {
            gpu.queue
                .write_buffer(&gpu.instance_buffer, 0, bytemuck::cast_slice(&inst[..count as usize]));
        }

        let frame = match gpu.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                return;
            }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("enc") });
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.05, g: 0.06, b: 0.10, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rp.set_bind_group(0, &gpu.bind_group, &[]);
            rp.set_pipeline(&gpu.bg_pipeline);
            rp.draw(0..3, 0..1);
            if count > 0 {
                rp.set_pipeline(&gpu.sprite_pipeline);
                rp.set_vertex_buffer(0, gpu.instance_buffer.slice(..));
                rp.draw(0..6, 0..count);
            }
        }
        gpu.queue.submit(Some(enc.finish()));
        frame.present();

        if let Some(w) = self.window.as_ref() {
            w.set_title(&format!(
                "{} · score {} · enemies {} · {:.0} fps  [F1 debug]",
                self.scene.title, self.score, enemies, self.fps
            ));
        }
    }
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
        println!("kami-clj-play: window open — arrows move, auto-fire culls the swarm, Esc quits.");
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.config.width = size.width.max(1);
                    gpu.config.height = size.height.max(1);
                    gpu.surface.configure(&gpu.device, &gpu.config);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let down = event.state == ElementState::Pressed;
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) if down => event_loop.exit(),
                    PhysicalKey::Code(KeyCode::F1) if down => self.debug = !self.debug,
                    PhysicalKey::Code(KeyCode::ArrowLeft) | PhysicalKey::Code(KeyCode::KeyA) => self.keys.left = down,
                    PhysicalKey::Code(KeyCode::ArrowRight) | PhysicalKey::Code(KeyCode::KeyD) => self.keys.right = down,
                    PhysicalKey::Code(KeyCode::ArrowUp) | PhysicalKey::Code(KeyCode::KeyW) => self.keys.up = down,
                    PhysicalKey::Code(KeyCode::ArrowDown) | PhysicalKey::Code(KeyCode::KeyS) => self.keys.down = down,
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.update_and_render();
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

/// Resolve the game data directory. Relocatable for packaging:
///   1. `$KAMI_GAME_DIR`                         — explicit override
///   2. `<exe>/../Resources/game`                — inside a packaged .app bundle
///   3. `$CARGO_MANIFEST_DIR/games/survivors`    — dev / `cargo run`
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
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("games/survivors")
}

fn main() {
    // The game is data: load CLJ logic + Datomic-shaped scene from files.
    let base = game_dir();
    let logic = read_or_exit(&base, "logic.clj");
    let scene_src = read_or_exit(&base, "scene.edn");
    let scene = parse_scene(&scene_src).unwrap_or_else(|| {
        eprintln!("kami-clj-play: {} is not a valid EDN scene map", base.join("scene.edn").display());
        std::process::exit(2);
    });
    println!(
        "kami-clj-play: loaded '{}' — logic.clj (CLJ→WASM, {BACKEND}) + scene.edn ({} render profiles).",
        scene.title,
        scene.profiles.len()
    );

    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut app = App::new(&logic, scene);
    event_loop.run_app(&mut app).expect("run");
}
