# ADR-0045 — Ray tracing & binaural audio as EDN/clj data, executed per platform

- Status: accepted (phase-1 clj IR + WGSL emit; phase-2 native + WIT; **phase-3 real execution: GPU compute run + WAV sink + headless frame pipeline**)
- Date: 2026-06-24
- Builds on: ADR-0036 (kami-engine-sdk-clj — clj brain / Rust arm), ADR-0037 (cross-platform
  packaging: web/mac/iOS/Android/PS5/Switch), ADR-0040 (everything describable is EDN),
  ADR-0044 (EDN render-IR vocabulary)
- Related crates: `kami-audio` (Rust spatial mixer / HRTF), `kami-rtc` (WebRTC spatial bridge),
  `kami-rt` / `kami-pbrt` / `kami-rtx-native` (RT path reservations, ADR-2605261800)

## Context

An audit of the clj/edn layer found:

- **Ray tracing** was **reserved but unimplemented in clj/edn**. `kami-rt` / `kami-pbrt` /
  `kami-rtx-native` are Rust stubs (constants only, no runtime); the render-IR has only
  rasterization passes; the game/WIT surface has no RT primitive (`:physics/raycast` is a
  physics query, not rendering). Clojure games could not describe a ray-traced frame.
- **Binaural audio** existed only as a **simplified dot-product pan** (`kami-audio::AudioMixer::spatialize`,
  Rust) plus a **mono** cue-bank in `kami-webgpu/audio.cljs`. There was no data-driven,
  platform-neutral description of a 3D soundscape, and no real ITD/ILD model.

Both gaps are the same shape: the *capability* belongs in a per-platform executor (WGSL
ray-query, Metal RT, a native mixer, Web Audio), but the *description* — the thing an author
writes and a game ships — belongs in clj/edn. This is the established kami split (clj is the
brain, the Rust/GPU side is a dumb arm; ADR-0036/0044) applied to two more domains.

## Decision

Add two **backend-neutral, pure-`.cljc`** modules to `kami-engine-sdk-clj` that turn EDN
recipes into a canonical IR, then **delegate execution to the runtime** via an `emit`
multimethod — one descriptor per platform. The same EDN drives every target; no renderer or
mixer is re-implemented in clj.

### `kami.rt` — ray tracing

- EDN recipe → `pipeline` → canonical RT-IR (`:rt/accel :rt/integrator :rt/sampler
  :rt/camera :rt/output` with defaults filled + a deterministic `:rt/passes` list:
  `primary → trace → [denoise] → present`). Pure + serializable (record/replay surface).
- `valid?` rejects malformed recipes at author time (lenient on *missing* fields — defaults
  merge in `pipeline`; strict on *present-but-wrong* ones).
- `targets` is the **capability matrix**. `:status` says how kami reaches each platform:
  - `:emit` — clj emits the shader here. **`:wgsl`** is the one emit backend: `emit-wgsl`
    produces a WebGPU **ray-query** compute shader (`chromium_experimental_ray_query`),
    baking integrator params (bounces/spp/clamp/seed) as WGSL `override` constants.
  - `:delegate` — clj emits an IR **plan**; the native backend owns the RT API:
    `:metal` (MPSRayIntersector), `:vulkan` (VK_KHR_ray_tracing), `:dx12` (DXR),
    `:unity` (HDRP), `:unreal` (Lumen/HWRT).
  - `:nda` — console RT API under NDA (`:ps5` AGC, `:switch` NVN): plan only, impl out of tree.
- `cpu-trace` / `intersect-sphere` — a tiny analytic CPU intersector used **only as a test
  oracle**, so the IR's trace semantics are verifiable with no GPU.

### `kami.binaural` — spatial audio

- EDN scene (`:binaural/listener :binaural/hrtf :binaural/rolloff :binaural/sources`) →
  `mix` → spatialization IR: per source `{:itd-s :ild-db :gain-l :gain-r :delay-l-s
  :delay-r-s :azimuth :elevation :distance}`. Pure + serializable.
- Upgrades the old pan to a **spherical-head** model, all in data:
  - **ITD** — Woodworth `itd = (a/c)(θ + sin θ)` on the lateral angle (front/back symmetric,
    elevation-aware); bounded ≈ 0.66 ms for a 0.0875 m head.
  - **ILD** — frequency-independent head-shadow: the contralateral ear is attenuated by |ILD|.
  - **distance** — OpenAL inverse / linear / exponential / none rolloff.
- `emit` lowers to `:web-audio` (DelayNode+GainNode+pan node-graph recipe for the cljs
  runtime) or `:native` (voice fields matching `kami-audio::AudioMixer`, ITD as integer
  sample delays at the mixer rate). Unknown backends get the neutral IR to lower themselves.

### Shared

- vec3 helpers (`v- v+ v* dot cross length normalize clamp`) added to `kami.math`.

## Phase 2 — native execution + game-facing WIT (done)

Three follow-ups from phase 1, all landed and tested:

1. **WGSL ray-query executor (`kami-rt`).** Promoted from a constants stub to a real crate:
   a CPU **LBVH** acceleration structure (`bvh`: 30-bit Morton-code linear build +
   Möller–Trumbore + slab traversal, verified against brute force over a triangle grid) and a
   host-side `wgsl_ray_query` generator mirroring clj `emit-wgsl` (override-constant integrator
   bake). GPU dispatch on a `wgpu::Device` remains the host's wiring step; everything in the
   crate is GPU-free and unit-tested (7 tests).
2. **Native binaural mixer (`kami-audio::binaural`).** The spherical-head model reimplemented
   in Rust with **identical math** to `kami.binaural`, so a recipe spatializes the same on web
   (Web Audio) and native; `BinauralParams::sample_delays` emits the integer ITD delays of the
   `:native` IR. 6 tests (right-leads, left-mirror, dead-ahead-centered, ITD bound, sample
   delays, rolloff).
3. **Game-facing WIT host imports.** Extended all three gate sources together so `kbb -M:wit-check`
   stays green (now 40 functions): `audio.set-listener` (binaural listener pose) and
   `render.rt-enable` (select a named RT recipe). Wired end-to-end — clj builtin
   (`set-listener!` / `rt-enable!`) → WASM import → `kami-script-runtime` `func_wrap` binding →
   `HostState` (`listener` / `rt_recipe`) → drain accessors. `kami-engine-clj` and
   `kami-script-runtime` both compile.

Still delegated (unchanged): `:metal`/`:vulkan`/`:dx12`/`:unity`/`:unreal` execute via their
own RT APIs from the emitted plan; `:ps5`/`:switch` stay NDA plan-only. The emitted `:wgsl`
ray-query shader needs a host with the ray-query extension enabled to actually dispatch.

## Phase 3 — real execution, headless-verified (done)

The portable path now actually runs, proven without a window:

1. **RT compute on a real GPU.** `kami_rt::gpu` packs the LBVH into `#[repr(C)]` Pod buffers;
   `kami_render::raytrace::RayTracePipeline` builds a wgpu-24 compute pipeline over
   `shaders/rt_bvh_compute.wgsl` (software-BVH traversal — stable WebGPU, no ray-query
   extension) and dispatches one thread/pixel. `kami-render/tests/rt_gpu.rs` runs it on the
   actual adapter (Metal here), reads the hit buffer back, and asserts the centre pixel hits
   the triangle at t≈5 (skips if no adapter).
2. **Binaural to a real sink.** `kami_audio::wav::encode_pcm16_stereo` turns `mix_stereo`
   output into WAV bytes; the `binaural_orbit` example renders a tone orbiting the head to a
   768 KB stereo WAV.
3. **Headless frame pipeline.** `kami-script-runtime/examples/frame_pipeline.rs` drives the
   shipped `rt-audio-demo` logic.clj: each tick it reads `rt_recipe()` / `listener()` /
   `drain_audio_queue()`, dispatches the RT compute over a scene BVH (1023/4096 px hit), and
   spatializes the game's cues + an orbiting ambient into a WAV — the whole
   game → host-import → RT-GPU + binaural loop, end to end, no display.

Remaining (needs real hardware/display, not headless-testable): wire the RT frame + binaural
buffer into the on-screen winit player (`kami-clj-play3d`) and a live audio device (cpal /
Web Audio); upload BVH per-frame with refit; the hardware ray-query path on a supporting host.

## Consequences

- The clj/edn layer can now **describe** ray-traced frames and 3D binaural soundscapes; the
  description is the same bytes on web/mac/iOS/Android/PS5/Switch/Unity/Unreal — only the
  executor differs. This matches ADR-0037's "author once, ship everywhere".
- Tests: `kami.rt-test` + `kami.binaural-test` (GPU-free, mixer-free) pin the IR contract,
  WGSL emission, backend lowering, and — via the CPU oracle and Woodworth bounds — the
  numerical semantics. `kbb -M:test` green; no regression in `kami.contract-test`.
- Next: (1) host-side WGSL ray-query executor in `kami-rt`/`kami-render`; (2) a real native
  binaural mixer reading the `:native` IR in `kami-audio`; (3) optional WIT host imports to
  call these from compiled-guest games; (4) HRTF dataset model (`:model :dataset`) beyond the
  analytic spherical head.
```
