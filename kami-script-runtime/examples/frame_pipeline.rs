//! Headless end-to-end frame pipeline (ADR-0045) — the integration that ties
//! everything together without a window:
//!
//!   demo game (logic.clj)  ──tick──▶  host (KamiScriptRuntime)
//!         │                                 │
//!         │ rt-enable! / set-listener! / play-sound-at
//!         ▼                                 ▼
//!   HostState.rt_recipe / listener / audio_queue
//!         │                                 │
//!         ├─▶ RT executor: kami-rt LBVH + kami-render compute dispatch (real GPU)
//!         └─▶ audio executor: kami-audio binaural spatialize → mix → WAV sink
//!
//! Run:  cargo run -p kami-script-runtime --example frame_pipeline
//! Outputs /tmp/frame_pipeline.wav and prints the RT hit count. Skips the GPU
//! leg if no adapter is present; the audio leg always runs.

use std::sync::{Arc, Mutex};

use glam::Vec3;
use kami_audio::Listener;
use kami_audio::binaural::{Hrtf, Rolloff, Voice, mix_stereo, spatialize};
use kami_audio::wav::encode_pcm16_stereo;
use kami_render::raytrace::{HIT_STRIDE, RayTracePipeline, RtGlobals};
use kami_rt::bvh::{Bvh, Tri};
use kami_script_runtime::KamiScriptRuntime;

const W: u32 = 64;
const H: u32 = 64;
const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))?;
    pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("frame-pipeline"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .ok()
}

/// A small scene in front of an origin camera (rays go +z): a wall quad at z=6
/// and a nearer quad at z=4 — two boxes to trace against.
fn scene_bvh() -> Bvh {
    let mut tris = Vec::new();
    let mut quad = |z: f32, lo: f32, hi: f32, id0: u32| {
        let a = Vec3::new(lo, lo, z);
        let b = Vec3::new(hi, lo, z);
        let c = Vec3::new(lo, hi, z);
        let d = Vec3::new(hi, hi, z);
        tris.push(Tri {
            v0: a,
            v1: b,
            v2: c,
            id: id0,
        });
        tris.push(Tri {
            v0: b,
            v1: d,
            v2: c,
            id: id0 + 1,
        });
    };
    quad(6.0, -3.0, 3.0, 0); // far wall
    quad(4.0, -1.0, 1.0, 2); // near box
    Bvh::build(tris)
}

/// Read back the hit buffer and count pixels that hit something (t >= 0).
fn count_hits(device: &wgpu::Device, queue: &wgpu::Queue, out: &wgpu::Buffer) -> u32 {
    let size = (W as u64) * (H as u64) * HIT_STRIDE;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    enc.copy_buffer_to_buffer(out, 0, &staging, 0, size);
    queue.submit([enc.finish()]);
    let slice = staging.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::Maintain::Wait);
    let data = slice.get_mapped_range();
    let hits: &[f32] = bytemuck::cast_slice(&data);
    hits.chunks_exact(4).filter(|h| h[0] >= 0.0).count() as u32
}

fn main() {
    // 1. Host drives the actual shipped demo game.
    let world = Arc::new(Mutex::new(hecs::World::new()));
    let mut rt = KamiScriptRuntime::new(world).expect("runtime");
    let demo = include_str!("../../kami-clj-play3d/games/rt-audio-demo/logic.clj");
    rt.load_clj("demo", demo).expect("compile+load demo");
    rt.call_init("demo").expect("init");
    println!("init → rt_recipe = {:?}", rt.rt_recipe());

    // 2. Executors.
    let gpu = headless_device();
    if gpu.is_none() {
        println!("(no GPU adapter — RT leg skipped, audio leg still runs)");
    }
    let bvh = scene_bvh();

    let sr = 48_000u32;
    let ticks = 120usize;
    let spt = (sr as usize * 16) / 1000; // samples per 16 ms tick
    let total = ticks * spt;
    let mut master = vec![0.0f32; total * 2];
    let hrtf = Hrtf::default();
    let rolloff = Rolloff::default();

    // a short decaying blip per footstep cue.
    let blip: Vec<f32> = (0..spt)
        .map(|i| {
            let t = i as f32 / sr as f32;
            (t * 660.0 * std::f32::consts::TAU).sin() * (1.0 - i as f32 / spt as f32) * 0.5
        })
        .collect();
    // a continuous tone for the orbiting ambient source.
    let tone: Vec<f32> = (0..spt)
        .map(|i| (i as f32 / sr as f32 * 330.0 * std::f32::consts::TAU).sin() * 0.25)
        .collect();

    let mut rt_frames = 0u32;
    let mut rt_hits = 0u32;

    for tick in 0..ticks {
        // 3. one game tick → host state.
        rt.call_systems("demo", 16).expect("systems");
        let l = rt.listener();
        let listener = Listener {
            position: Vec3::new(l[0], l[1], l[2]),
            forward: Vec3::new(l[3], l[4], l[5]),
            up: Vec3::Y,
        };

        // 3a. RT leg — trace one frame the first time the game enables a recipe.
        if rt_frames == 0 {
            if let (Some((dev, q)), Some(recipe)) = (&gpu, rt.rt_recipe()) {
                let pipeline = RayTracePipeline::new(dev);
                let globals = RtGlobals::new(IDENTITY, listener.position.to_array(), W, H);
                let out = pipeline.trace(dev, q, &bvh, globals, W, H);
                rt_hits = count_hits(dev, q, &out);
                rt_frames += 1;
                println!(
                    "tick {tick}: traced recipe {recipe:?} → {rt_hits}/{} pixels hit",
                    W * H
                );
            }
        }

        // 3b. audio leg — spatialize each game cue at its world position.
        let base = tick * spt;
        for (_name, pos) in rt.drain_audio_queue() {
            let p = spatialize(&listener, &hrtf, &rolloff, Vec3::from(pos), 1.0);
            let chunk = mix_stereo(
                &[Voice {
                    params: p,
                    mono: &blip,
                }],
                sr,
                spt + 128,
            );
            for f in 0..spt {
                master[(base + f) * 2] += chunk[f * 2];
                master[(base + f) * 2 + 1] += chunk[f * 2 + 1];
            }
        }

        // an orbiting ambient source (3 m radius) so the WAV audibly pans — the
        // demo footsteps sit on the listener (centered), this proves the HRTF.
        let angle = (tick as f32 / ticks as f32) * std::f32::consts::TAU;
        let amb = Vec3::new(angle.sin() * 3.0, 0.0, -angle.cos() * 3.0) + listener.position;
        let pa = spatialize(&listener, &hrtf, &rolloff, amb, 1.0);
        let chunk = mix_stereo(
            &[Voice {
                params: pa,
                mono: &tone,
            }],
            sr,
            spt + 128,
        );
        for f in 0..spt {
            master[(base + f) * 2] += chunk[f * 2];
            master[(base + f) * 2 + 1] += chunk[f * 2 + 1];
        }
    }

    let wav = encode_pcm16_stereo(&master, sr);
    let path = "/tmp/frame_pipeline.wav";
    std::fs::write(path, &wav).expect("write wav");
    println!(
        "done: RT {} frame(s) ({} hits), audio {} bytes → {path}",
        rt_frames,
        rt_hits,
        wav.len()
    );
}
