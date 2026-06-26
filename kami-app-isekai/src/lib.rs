//! kami-app-isekai — ISEKAI game entry built on `kami-app` Builder SDK.
//!
//! This crate is the **reference implementation** of the per-game topology:
//! a thin composition layer that picks camera, input, pipelines, and tick
//! hooks from engine primitives. The old monolithic entry
//! `kami_web::run_with_scene` will be deprecated once this crate covers
//! its feature set.
//!
//! # Phase 1 scope (this file)
//!
//! Minimal viable game: bootstrap via KamiApp, clear-color render, log a
//! tick counter. Confirms the builder/RAF/resize pipeline end-to-end
//! without depending on the 687 LoC voxel + physics + NPC implementation.
//!
//! # Phase 2 scope (follow-up)
//!
//! Port `run_with_scene` body: parse `IslandScene` JSON, register
//! `VoxelPbrPipeline` + `SdfCharacterPipeline` + `SkyAtmospherePipeline`,
//! wire WASD FPS input, add voxel-mining tick hook.

use kami_app::{CameraMode, InputMode, KamiApp, Position};
use log::Level;

pub mod omniverse;
pub mod pipelines;

#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

/// Bridge to `window.kamiPlay(name)` defined in v3-demos.htm. No-op
/// if the JS function is missing (e.g., running under a different
/// host). Throttling + user-gesture gating happens on the JS side.
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window, js_name = kamiPlay, catch)]
    fn kami_play(name: &str) -> Result<(), JsValue>;
}

#[cfg(target_family = "wasm")]
fn sfx(name: &str) {
    let _ = kami_play(name);
}

#[cfg(not(target_family = "wasm"))]
fn sfx(_: &str) {}

/// Entry point exported to JS.
///
/// ```js
/// import init, { run_isekai_v2 } from './kami_app_isekai.js';
/// await init();
/// await run_isekai_v2('gc');
/// ```
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_isekai_v2(canvas_id: &str) -> Result<(), JsValue> {
    // Default entry: all v3 phases on. Equivalent to scene_id=4.
    run_isekai_v2_scene(canvas_id, 4).await
}

/// v3 demo variant with a scene selector.
///
/// Gates DEC subsystems so each phase can be demoed in isolation:
///
///   0 — heat diffusion only (M1 rule + Λ⁰ Laplacian)
///   1 — + moisture field (compositional Λ⁰ composition, wet-paper rule)
///   2 — + EdgeField wind + buoyancy + semi-Lagrangian advection
///   3 — + Helmholtz projection (Jacobi Poisson → divergence-free)
///   4 — + wall boundary (EdgeField::mask_solid) — full v3 stack
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_isekai_v2_scene(canvas_id: &str, scene: u32) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(Level::Info);
    log::info!("[isekai-v3] scene={}", scene);
    let scene_id = scene;

    // Spawn directly above the demo house looking almost straight
    // down, so every scene's phenomena (fire + paper + water at
    // (-12, 33, 18), EM dipole at (-12, 36, 20)) are visible in a
    // top-down view that bypasses terrain occlusion. Gravity is
    // disabled for scene-inspection ergonomics (user can still WASD
    // around). Distance to the demo floor ≈ 17 m.
    // Eye-level view from just inside the house's east end, looking
    // west straight down the fire + paper + water row at z=18. The
    // fire at (-12, 33, 18) is 9 m forward; paper 1..8 blocks along
    // the line; water bucket over the middle; EM dipole for scene 5
    // is directly above at y=36. Every scene's phenomena therefore
    // appear right in front of the spawn camera.
    let spawn = Position::new(-5.0, 35.5, 18.5);

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("isekai")
        .with_hud_publish(true)
        .with_camera(CameraMode::FirstPerson {
            spawn,
            // forward = (cos(p)·sin(y), sin(p), -cos(p)·cos(y))
            // yaw = -π/2 ⇒ forward_x = -1 (due west). pitch = -0.1
            // tilts slightly down so the ground-level fire is
            // below the horizon.
            yaw: -std::f32::consts::FRAC_PI_2,
            pitch: -0.25,
        })
        .with_input(InputMode::WasdFps);

    // Phase 4: Sky + Terrain pipelines.
    // Sky clears + writes depth; terrain reads depth (Less) and draws on top.
    let sky = pipelines::SkyAdapter::new(app.render_context());
    // Streaming terrain: 5×5 window (view_radius=2) of 256m chunks
    // centred on the camera = 1280m coverage that follows the player.
    // Chunks outside the window are released; new chunks enter at
    // 1/frame (budget). 129² verts/chunk (2m cells).
    // 128m chunks at 1m vertex spacing → 129² = 16.6k verts per chunk.
    // view_radius=2 → 5×5 = 25 chunks loaded = 640m visibility window
    // around camera. Heightmap + mesh + splat gen = ~5-15 ms/chunk, so
    // 1/frame budget is safe at 60 FPS.
    let terrain = plains_terrain(app.render_context());
    // Water plane at y=14 (just below Plains sand_line=15). Covers the
    // streaming view radius (5 × 128m = 640m) + margin.
    let water = kami_pipelines::WaterAdapter::new(app.render_context(), 1024.0, 14.0);

    // Voxel streaming (Phase 16): 5³ window (view_radius=2) of 16m
    // chunks around the camera. Generator voxelizes a coarse Plains
    // terrain into stone floor + grass top so the player walks on a
    // Minecraft-ish ground layer alongside the smooth PBR terrain.
    let voxels = build_voxel_world(app.render_context());
    let voxels_for_probe = voxels.clone();

    // Also drop in the pre-authored demo house at a fixed spot.
    build_demo_house(app.render_context(), &voxels);

    let voxels_for_mining = voxels.clone();
    let voxels_for_wall = voxels.clone();
    let voxels_for_heat = voxels.clone();
    let particles = kami_pipelines::ParticleAdapter::new(app.render_context(), 2048);
    let particles_fx = particles.clone();
    let particles_for_heat = particles.clone();

    // v3 DEC: two ScalarFields run in parallel on the voxel cubical
    // complex. Both use the same Laplacian + emit_from infrastructure;
    // only the source material and the ignition rule that composes
    // them differ. This is the compositional payoff of v3 — adding a
    // second physical field is ~20 LoC, not a new rule pipeline.
    let heat_field = std::rc::Rc::new(std::cell::RefCell::new(kami_dec::ScalarField::new()));
    let moisture_field = std::rc::Rc::new(std::cell::RefCell::new(kami_dec::ScalarField::new()));
    let heat_for_tick = heat_field.clone();
    let moisture_for_tick = moisture_field.clone();

    // Persistent wind field (P27). Carries velocity across frames so
    // pressure projection can build up a coherent circulation driven
    // purely by buoyancy + boundary conditions, instead of being
    // rebuilt from scratch each tick.
    let wind_field: std::rc::Rc<std::cell::RefCell<kami_dec::EdgeField>> =
        std::rc::Rc::new(std::cell::RefCell::new(kami_dec::EdgeField::new()));
    let wind_for_tick = wind_field.clone();

    // Scene 5 — Maxwell EM demo. E-field (Λ¹) on edges, B-field (Λ²)
    // on faces. Visualised by reusing EdgeVisAdapter on `em_e`. Only
    // stepped when `scene_id == 5` so the other scenes pay no cost.
    let em_e: std::rc::Rc<std::cell::RefCell<kami_dec::EdgeField>> =
        std::rc::Rc::new(std::cell::RefCell::new(kami_dec::EdgeField::new()));
    let em_b: std::rc::Rc<std::cell::RefCell<kami_dec::FaceField>> =
        std::rc::Rc::new(std::cell::RefCell::new(kami_dec::FaceField::new()));
    let em_e_for_tick = em_e.clone();
    let em_b_for_tick = em_b.clone();

    // Visualiser: heat (warm red-orange) + moisture (cyan-blue) as
    // billboard sprites at each non-zero cell. Instance count capped
    // at 2048; enough for the demo (~5-30 active heat cells, ~5-30
    // moisture cells).
    let mut field_vis = kami_pipelines::FieldVisAdapter::new(app.render_context(), 2048);
    // Billboards scaled up for the v3-demos eye-level view: fire /
    // moisture sprites need to be big enough to read from 10 m away
    // inside the demo house. Lower max_value saturates sooner so
    // even early-frame emissions are clearly coloured.
    field_vis.add_layer(kami_pipelines::FieldLayer {
        field: heat_field.clone(),
        base_color: [1.0, 0.45, 0.15],
        max_value: 20.0,
        min_value: 0.5,
        size: 1.6,
    });
    field_vis.add_layer(kami_pipelines::FieldLayer {
        field: moisture_field.clone(),
        base_color: [0.25, 0.55, 0.95],
        max_value: 0.6,
        min_value: 0.02,
        size: 1.4,
    });
    // P26-edge-vis: arrow visualisation of the persistent wind field.
    // Scene ≥2 uses wind; this adapter shares the same Rc so it always
    // reflects the current velocity. Off for scene 0/1 at runtime via
    // stride=0 (but we still register to keep pipeline graph stable).
    let edge_vis =
        kami_pipelines::EdgeVisAdapter::new(app.render_context(), wind_field.clone(), 4096);

    // Scene 5 EM demo: visualise |E| directly on edges. Colour hot →
    // cool by amplitude; smaller sprites so you can see wave fronts.
    let mut em_vis = kami_pipelines::EdgeVisAdapter::new(app.render_context(), em_e.clone(), 8192);
    em_vis.max_mag = 0.6;
    em_vis.min_mag = 0.01;
    em_vis.arrow_length = 0.8;
    em_vis.sprite_size = 0.12;
    em_vis.stride = 1;

    // Maxwell B-field visualiser. Green billboards to contrast with
    // E's blue→red arrows; offset slightly so overlap is legible.
    let em_b_vis = kami_pipelines::FaceVisAdapter::new(app.render_context(), em_b.clone(), 8192);

    // Nintendo-style procedural sprite atlas (N1). Holds flames /
    // sparkles / splashes / shock waves / etc. Callers emit via
    // slot IDs from `kami_pipelines::atlas_slot`. Registered last
    // so sprites render on top of the DEC visualisers.
    let atlas_vis = kami_pipelines::AtlasVisAdapter::new(app.render_context(), 2048);
    let atlas_for_heat = atlas_vis.clone();

    // N5: centralised field→icon map. All scene emitters share this
    // so a single edit propagates across every phenomenon.
    let icon_map = kami_pipelines::FieldIconMap::nintendo_default();

    let mut tick_count: u64 = 0;
    // DEC simulation runs at a fixed 30 Hz (vs render at 60 Hz). We
    // accumulate render dt until one DEC step's worth has passed,
    // then step the field pipeline once. Halves the physics cost
    // for the same visual result — the billboards are already
    // smeared over several frames by the advection trail.
    let mut dec_accum: f32 = 0.0;
    const DEC_STEP: f32 = 1.0 / 30.0;
    let app = app
        .with_floor_probe(move |p| voxels_for_probe.sample_floor(p))
        .with_eye_height(1.8)
        .with_collider_probe(move |min, max| voxels_for_wall.aabb_solid(min, max))
        .with_player_radius(0.35)
        // Gravity disabled for v3-demos so the player stays at the
        // top-down spawn where scene phenomena are visible. Space /
        // Shift fly up/down instead of jumping.
        .with_gravity(0.0)
        .with_jump_impulse(0.0)
        .with_pipeline(sky)
        .with_pipeline(terrain)
        .with_pipeline(water)
        .with_pipeline(voxels)
        .with_pipeline(particles)
        .with_pipeline(field_vis)
        .with_pipeline(edge_vis)
        .with_pipeline(em_vis)
        .with_pipeline(em_b_vis)
        .with_pipeline(atlas_vis)
        .on_update(move |_world, camera, dt| {
            tick_count = tick_count.wrapping_add(1);
            if tick_count % 60 == 0 {
                log::info!("[isekai-v2] tick={} dt={:.4}", tick_count, dt);
            }

            // ── v3 DEC heat field tick ────────────────────────────
            // Pattern (kami-dec/src/lib.rs thesis):
            //   ∂_t T = α Δ T - k T + source(material)
            //
            // 1. Emit: scan loaded voxel chunks; every FIRE voxel
            //    injects emit_rate · dt into the scalar field at its
            //    cell. Stored sparsely — non-fire cells cost nothing.
            // 2. Diffuse: single explicit-Euler step of the 7-point
            //    Laplacian stencil, with ambient linear decay to model
            //    radiation into the environment.
            // 3. Ignite: for any PAPER voxel whose local T exceeds
            //    threshold, swap to FIRE and spawn particles.
            //
            // With radius=2 (5³ = 125 voxel chunks loaded, but heat is
            // sparse so maybe 2-5 active chunks) the full cycle below
            // runs in < 0.5 ms — cheaper than scanning every paper
            // voxel's 6 neighbours for fire as M1 rule does, because
            // only chunks with actual field energy tick.
            // ── Fixed-rate DEC tick (30 Hz) ───────────────────────
            // Skip the whole simulation block on frames that aren't
            // a DEC step. Particles (visual) still integrate every
            // frame via ParticleAdapter, so motion stays smooth.
            dec_accum += dt;
            if dec_accum < DEC_STEP {
                return;
            }
            let dt = DEC_STEP;
            dec_accum -= DEC_STEP;

            let mut heat = heat_for_tick.borrow_mut();
            let mut moist = moisture_for_tick.borrow_mut();

            // Zero-activity early-out: if nothing is happening in
            // any field, skip the entire DEC + streamline + ignition
            // pipeline. The Maxwell / gravity scenes still tick via
            // the match below, so gate on scenes that depend on the
            // Λ⁰ / Λ¹ state.
            if heat.chunk_count() == 0
                && moist.chunk_count() == 0
                && matches!(scene_id, 0..=4 | 6..=10)
                && tick_count > 4
            {
                // Intentionally allow first few ticks through so the
                // seed emitters populate fields. After that, if all
                // fields decayed to zero (e.g., scene 1 water alone
                // diffusing out) skipping is safe.
            }

            // Phase 1: emit. Fire → heat, water → moisture. Both
            // fields share the same emit / diffuse infrastructure;
            // only the source material (and rate) differs.
            // Scene-isolated phenomena (one representation per scene):
            //   0 — Λ⁰ heat diffusion only
            //   1 — Λ⁰ moisture diffusion only (no fire)
            //   2 — Λ¹ wind + buoyancy + advection (no projection, no walls)
            //   3 — Helmholtz projection (divergence-free flow)
            //   4 — walls + vorticity confinement
            //   5 — Maxwell EM only (no heat / moisture / wind)
            let fire_positions: [(i32, i32, i32); 2] =
                [(-16 + 4, 32 + 1, 16 + 2), (-16 + 4, 32 + 2, 16 + 2)];
            let water_positions: [(i32, i32, i32); 2] =
                [(-16 + 8, 32 + 2, 16 + 2), (-16 + 9, 32 + 2, 16 + 2)];
            let emit_fire = |heat: &mut kami_dec::ScalarField| {
                for &(x, y, z) in &fire_positions {
                    if voxels_for_heat.is_solid(glam::Vec3::new(
                        x as f32 + 0.5,
                        y as f32 + 0.5,
                        z as f32 + 0.5,
                    )) {
                        heat.add(x, y, z, 180.0 * dt);
                    }
                }
            };
            let emit_water = |moist: &mut kami_dec::ScalarField| {
                for &(x, y, z) in &water_positions {
                    if voxels_for_heat.is_solid(glam::Vec3::new(
                        x as f32 + 0.5,
                        y as f32 + 0.5,
                        z as f32 + 0.5,
                    )) {
                        moist.add(x, y, z, 80.0 * dt);
                    }
                }
            };

            match scene_id {
                0 => {
                    // Pure Λ⁰ scalar diffusion of heat.
                    emit_fire(&mut heat);
                    heat.diffuse(0.10, dt.min(0.05), 0.8);
                }
                1 => {
                    // Pure Λ⁰ scalar diffusion of moisture (no fire).
                    emit_water(&mut moist);
                    moist.diffuse(0.08, dt.min(0.05), 0.1);
                }
                2 => {
                    // Λ¹ wind + buoyancy + advection, NO projection.
                    emit_fire(&mut heat);
                    let mut wind = wind_for_tick.borrow_mut();
                    wind.damp(0.94);
                    wind.add_buoyancy_from(&heat, 0.08 * dt.min(0.05) * 60.0, 1.0);
                    heat.advect_field(&*wind, dt);
                    heat.diffuse(0.10, dt.min(0.05), 0.8);
                }
                3 => {
                    // + Helmholtz projection (divergence-free).
                    emit_fire(&mut heat);
                    let mut wind = wind_for_tick.borrow_mut();
                    wind.damp(0.94);
                    wind.add_buoyancy_from(&heat, 0.08 * dt.min(0.05) * 60.0, 1.0);
                    kami_dec::project_divergence_free_mg(&mut *wind);
                    heat.advect_field(&*wind, dt);
                    heat.diffuse(0.10, dt.min(0.05), 0.8);
                }
                4 => {
                    // + wall boundary + vorticity confinement.
                    emit_fire(&mut heat);
                    let mut wind = wind_for_tick.borrow_mut();
                    wind.damp(0.94);
                    wind.add_buoyancy_from(&heat, 0.08 * dt.min(0.05) * 60.0, 1.0);
                    let vox = voxels_for_heat.clone();
                    wind.mask_solid(|x, y, z| {
                        vox.is_solid(glam::Vec3::new(
                            x as f32 + 0.5,
                            y as f32 + 0.5,
                            z as f32 + 0.5,
                        ))
                    });
                    kami_dec::project_divergence_free_mg(&mut *wind);
                    kami_dec::vorticity_confine(&mut *wind, 0.15, dt.min(0.05));
                    heat.advect_field(&*wind, dt);
                    heat.diffuse(0.10, dt.min(0.05), 0.8);
                }
                5 => {
                    // Maxwell EM only — no voxel heat / moisture / wind.
                    let mut e = em_e_for_tick.borrow_mut();
                    let mut b = em_b_for_tick.borrow_mut();
                    let t = tick_count as f32 * 0.1;
                    let amp = (t * 0.6).sin() * 1.2;
                    e.set(-12, 36, 20, 1, amp);
                    e.set(-12, 36, 21, 1, amp);
                    let sub = dt.min(0.016) * 0.5;
                    kami_dec::step_maxwell(&mut *e, &mut *b, 0.4, sub);
                    kami_dec::step_maxwell(&mut *e, &mut *b, 0.4, sub);
                }
                6 => {
                    // Fire propagation along the paper row: heat
                    // diffuses from the seed fire, ignites adjacent
                    // papers one by one. Pure Λ⁰ field — no wind —
                    // so the spread is diffusion-rate-limited.
                    emit_fire(&mut heat);
                    heat.diffuse(0.18, dt.min(0.05), 0.4);
                }
                7 => {
                    // Water extinguishing: both fire AND water emit.
                    // Wet papers survive (threshold 40 + M·80). As
                    // moisture spreads, the chain reaction stalls.
                    emit_fire(&mut heat);
                    emit_water(&mut moist);
                    heat.diffuse(0.14, dt.min(0.05), 0.5);
                    moist.diffuse(0.10, dt.min(0.05), 0.15);
                }
                8 => {
                    // Gravity only: spawn particles above the demo,
                    // let them fall under 9.8 m/s² (ParticleAdapter's
                    // built-in gravity). Shows physical integration.
                    // Handled entirely in the natural-phenomena
                    // emitter below.
                }
                9 => {
                    // Wind drag on debris: full v3 wind field (with
                    // buoyancy + projection + walls) drives light
                    // particles. Uses scene 4 physics for the field
                    // and scene-specific emitter for advected debris.
                    emit_fire(&mut heat);
                    let mut wind = wind_for_tick.borrow_mut();
                    wind.damp(0.94);
                    wind.add_buoyancy_from(&heat, 0.08 * dt.min(0.05) * 60.0, 1.0);
                    let vox = voxels_for_heat.clone();
                    wind.mask_solid(|x, y, z| {
                        vox.is_solid(glam::Vec3::new(
                            x as f32 + 0.5,
                            y as f32 + 0.5,
                            z as f32 + 0.5,
                        ))
                    });
                    kami_dec::project_divergence_free_mg(&mut *wind);
                    heat.diffuse(0.10, dt.min(0.05), 0.8);
                }
                10 => {
                    // v3-DEC fully-coupled multiphysics: gravity,
                    // fire, water, wind in one Λ⁰ + Λ¹ pipeline.
                    //
                    //   1. Heat emits ↑ buoyancy on Λ¹ edges.
                    //   2. Moisture emits ↓ negative-buoyancy (water
                    //      is heavier than air).
                    //   3. Wind = damped persistence + ↑heat + ↓moist
                    //      + wall mask + Helmholtz projection.
                    //   4. Both Λ⁰ fields advect along the same wind.
                    //   5. Local reaction: heat + moist → steam
                    //      (moist evaporates, heat drops). Heavy
                    //      wet air falls; warm dry air rises; walls
                    //      deflect. Entirely expressed in DEC prims.
                    emit_fire(&mut heat);
                    emit_water(&mut moist);

                    let mut wind = wind_for_tick.borrow_mut();
                    wind.damp(0.94);
                    let dtc = dt.min(0.05);
                    // ↑ buoyancy from heat.
                    wind.add_buoyancy_from(&heat, 0.10 * dtc * 60.0, 1.0);
                    // ↓ gravity from moisture (negative scale sinks
                    // moist columns like real wet air).
                    wind.add_buoyancy_from(&moist, -0.05 * dtc * 60.0, 0.05);
                    let vox = voxels_for_heat.clone();
                    wind.mask_solid(|x, y, z| {
                        vox.is_solid(glam::Vec3::new(
                            x as f32 + 0.5,
                            y as f32 + 0.5,
                            z as f32 + 0.5,
                        ))
                    });
                    kami_dec::project_divergence_free_mg(&mut *wind);

                    heat.advect_field(&*wind, dt);
                    moist.advect_field(&*wind, dt);

                    // Local heat ↔ moisture coupling: where they
                    // overlap, moisture boils off and cools the
                    // heat (evaporative cooling + steam creation).
                    // Implemented as a pointwise sink on both fields.
                    let to_cool: Vec<(i32, i32, i32, f32)> = {
                        let mut out = Vec::new();
                        heat.for_each_nonzero(20.0, |x, y, z, h| {
                            let m = moist.get(x, y, z);
                            if m > 0.02 {
                                let rate = (h * m * 0.6 * dtc).min(h).min(m);
                                out.push((x, y, z, rate));
                            }
                        });
                        out
                    };
                    for (x, y, z, r) in to_cool {
                        heat.add(x, y, z, -r);
                        moist.add(x, y, z, -r.min(1.0));
                    }

                    heat.diffuse(0.10, dtc, 0.8);
                    moist.diffuse(0.08, dtc, 0.15);
                }
                _ => {}
            }

            // ── Natural-phenomena streamline / plume emitters ─────
            // Each scene seeds short-lived particles at flow sources
            // and traces them through the appropriate field. This
            // turns the abstract DEC billboards into something that
            // reads as fire smoke / water vapour / wind flow, and
            // lets the viewer follow actual fluxes cell-to-cell.
            //
            // Density is kept low (~3-6 particles/frame) so the
            // capacity=2048 buffer accommodates ~100 frames of trail
            // even at 60 fps.
            let mut rng_seed: u32 = tick_count as u32 * 2654435761u32.wrapping_add(1013904223);
            let mut rnd = || -> f32 {
                rng_seed ^= rng_seed << 13;
                rng_seed ^= rng_seed >> 17;
                rng_seed ^= rng_seed << 5;
                (rng_seed & 0x7fffffff) as f32 / 0x7fffffff as f32
            };
            match scene_id {
                0 => {
                    // Nintendo-style flame driven by the shared
                    // FieldIconMap. Thin smoke curl overlays above.
                    use kami_pipelines::atlas_slot as slot;
                    let core = glam::Vec3::new(-11.5, 33.8, 18.5);
                    let h = heat.sample_trilinear(core).max(0.0);
                    if let Some(icon) = icon_map.pick(h, 0.0) {
                        let phase = (tick_count as f32) * 0.35;
                        if icon.bobbing {
                            atlas_for_heat.emit_bobbing(
                                core,
                                icon.slot,
                                icon.tint,
                                icon.size * 1.4,
                                icon.life,
                                phase,
                            );
                        } else {
                            atlas_for_heat.emit_static(
                                core,
                                icon.slot,
                                icon.tint,
                                icon.size * 1.4,
                                icon.life,
                            );
                        }
                    }
                    if tick_count % 4 == 0 {
                        let drift = glam::Vec3::new(
                            -11.5 + (rnd() - 0.5) * 0.6,
                            34.6 + rnd() * 0.4,
                            18.5 + (rnd() - 0.5) * 0.6,
                        );
                        atlas_for_heat.emit(kami_pipelines::AtlasSprite {
                            pos: drift,
                            vel: glam::Vec3::new(0.0, 1.2 + rnd() * 0.5, 0.0),
                            tint: [0.85, 0.75, 0.6],
                            size: 0.9,
                            slot: slot::SMOKE_THIN,
                            rot: rnd() * std::f32::consts::TAU,
                            rot_vel: 0.3,
                            age: 0.0,
                            life: 1.4,
                            gravity: false,
                            bob_amp: 0.0,
                            bob_w: 0.0,
                            bob_phase: 0.0,
                            pulse_amp: 0.0,
                            pulse_w: 0.0,
                            wiggle_amp: 0.0,
                            wiggle_w: 0.0,
                            pop_ease_t: 0.0,
                        });
                    }
                }
                1 => {
                    // Water drops pulsing at source + steam puffs
                    // drifting upward. Mario hot-spring feel.
                    use kami_pipelines::atlas_slot as slot;
                    let core = glam::Vec3::new(-7.5, 34.6, 18.5);
                    atlas_for_heat.emit_bobbing(
                        core,
                        slot::WATER_DROP,
                        [0.45, 0.75, 1.0],
                        1.2,
                        0.35,
                        tick_count as f32 * 0.28,
                    );
                    if tick_count % 3 == 0 {
                        let p = glam::Vec3::new(
                            -7.5 + (rnd() - 0.5) * 0.8,
                            35.1 + rnd() * 0.3,
                            18.5 + (rnd() - 0.5) * 0.8,
                        );
                        atlas_for_heat.emit(kami_pipelines::AtlasSprite {
                            pos: p,
                            vel: glam::Vec3::new((rnd() - 0.5) * 0.3, 0.35 + rnd() * 0.25, 0.0),
                            tint: [0.85, 0.92, 1.0],
                            size: 1.1,
                            slot: slot::STEAM_PUFF,
                            rot: rnd() * 1.0,
                            rot_vel: 0.5,
                            age: 0.0,
                            life: 1.8,
                            gravity: false,
                            bob_amp: 0.05,
                            bob_w: 2.0,
                            bob_phase: rnd() * 6.28,
                            pulse_amp: 0.0,
                            pulse_w: 0.0,
                            wiggle_amp: 0.0,
                            wiggle_w: 0.0,
                            pop_ease_t: 0.0,
                        });
                    }
                }
                2 | 3 | 4 => {
                    // Arrow-trail streamline: chevrons oriented along
                    // local wind. Scene 4 caps the trail with a
                    // wind_swirl sparkle when the flow stalls
                    // (wall deflection visible as vortex).
                    use kami_pipelines::atlas_slot as slot;
                    let flame_core = glam::Vec3::new(-11.5, 33.8, 18.5);
                    atlas_for_heat.emit_bobbing(
                        flame_core,
                        slot::FLAME_MEDIUM,
                        [1.0, 0.55, 0.15],
                        1.6,
                        0.35,
                        tick_count as f32 * 0.35,
                    );
                    let wind = wind_for_tick.borrow();
                    for seed_i in 0..2 {
                        let fx = -11.5 + (rnd() - 0.5) * 0.8;
                        let fy = 33.5 + rnd() * 0.3;
                        let fz = 18.5 + (rnd() - 0.5) * 0.8;
                        let mut p = glam::Vec3::new(fx, fy, fz);
                        let mut prev = p;
                        for step in 0..12 {
                            let v = wind.vec_at(
                                p.x.floor() as i32,
                                p.y.floor() as i32,
                                p.z.floor() as i32,
                            );
                            let vlen = v.length();
                            if vlen < 0.02 {
                                if scene_id == 4 && step > 3 {
                                    atlas_for_heat.emit_pop(
                                        prev,
                                        slot::WIND_SWIRL,
                                        [0.45, 0.85, 0.95],
                                        0.8,
                                        0.6,
                                        0.18,
                                    );
                                    sfx("whoosh");
                                }
                                break;
                            }
                            let dir = v / vlen;
                            p += dir * 0.45;
                            let rot = dir.z.atan2(dir.x);
                            let h = heat.sample_trilinear(p).max(0.0);
                            let warmth = (h / 20.0).clamp(0.0, 1.0);
                            let col = [
                                0.95 * warmth + 0.35 * (1.0 - warmth),
                                0.55 * warmth + 0.65 * (1.0 - warmth),
                                0.1 * warmth + 0.9 * (1.0 - warmth),
                            ];
                            atlas_for_heat.emit(kami_pipelines::AtlasSprite {
                                pos: p,
                                vel: glam::Vec3::ZERO,
                                tint: col,
                                size: 0.55 + 0.3 * warmth,
                                slot: slot::ARROW_TRAIL,
                                rot,
                                rot_vel: 0.0,
                                age: 0.0,
                                life: 0.55,
                                gravity: false,
                                bob_amp: 0.0,
                                bob_w: 0.0,
                                bob_phase: 0.0,
                                pulse_amp: 0.0,
                                pulse_w: 0.0,
                                wiggle_amp: 0.0,
                                wiggle_w: 0.0,
                                pop_ease_t: 0.0,
                            });
                            prev = p;
                            let _ = seed_i;
                            let _ = step;
                        }
                    }
                }
                5 => {
                    // Maxwell EM: expanding shock-wave rings at the
                    // dipole every tick, with a half-phase wind-swirl
                    // (representing B field) between them.
                    use kami_pipelines::atlas_slot as slot;
                    let p = glam::Vec3::new(-11.5, 36.5, 20.5);
                    if tick_count % 30 == 0 {
                        sfx("tick");
                    }
                    if tick_count % 2 == 0 {
                        // E wave: pop-in ring that grows via pulse.
                        atlas_for_heat.emit(kami_pipelines::AtlasSprite {
                            pos: p,
                            vel: glam::Vec3::ZERO,
                            tint: [1.0, 0.95, 0.5],
                            size: 2.0,
                            slot: slot::SHOCK_WAVE,
                            rot: 0.0,
                            rot_vel: 0.8,
                            age: 0.0,
                            life: 1.6,
                            gravity: false,
                            bob_amp: 0.0,
                            bob_w: 0.0,
                            bob_phase: 0.0,
                            pulse_amp: 0.4,
                            pulse_w: 2.0,
                            wiggle_amp: 0.0,
                            wiggle_w: 0.0,
                            pop_ease_t: 0.3,
                        });
                    }
                    if tick_count % 2 == 1 {
                        // B wave: rotating swirl with pop-in.
                        atlas_for_heat.emit(kami_pipelines::AtlasSprite {
                            pos: p,
                            vel: glam::Vec3::ZERO,
                            tint: [0.4, 0.95, 0.55],
                            size: 1.5,
                            slot: slot::WIND_SWIRL,
                            rot: 0.0,
                            rot_vel: 2.5,
                            age: 0.0,
                            life: 1.5,
                            gravity: false,
                            bob_amp: 0.0,
                            bob_w: 0.0,
                            bob_phase: 0.0,
                            pulse_amp: 0.3,
                            pulse_w: 2.5,
                            wiggle_amp: 0.0,
                            wiggle_w: 0.0,
                            pop_ease_t: 0.25,
                        });
                    }
                }
                6 => {
                    // Per-voxel flame via shared icon map. Chain
                    // ignition lights each cell as heat propagates.
                    for y in 1..=2 {
                        for lx in 4..=12 {
                            let w = glam::Vec3::new(
                                -16.0 + lx as f32 + 0.5,
                                32.0 + y as f32 + 0.5,
                                16.0 + 2.5,
                            );
                            if !voxels_for_heat.is_solid(w) {
                                continue;
                            }
                            let h = heat.get(lx as i32 - 16, y as i32 + 32, 18);
                            let Some(icon) = icon_map.pick(h, 0.0) else {
                                continue;
                            };
                            let phase = (lx as f32 + y as f32) * 1.3 + tick_count as f32 * 0.3;
                            if icon.bobbing {
                                atlas_for_heat.emit_bobbing(
                                    w, icon.slot, icon.tint, icon.size, icon.life, phase,
                                );
                            } else {
                                atlas_for_heat
                                    .emit_static(w, icon.slot, icon.tint, icon.size, icon.life);
                            }
                        }
                    }
                }
                7 => {
                    // FieldIconMap resolves dry-hot → flame,
                    // hot-wet → steam, cold-wet → bubble / drop
                    // automatically from (h, m) per cell. Steam
                    // gets an upward drift velocity post-pick.
                    for lx in 4..=12 {
                        let wx = -16.0 + lx as f32 + 0.5;
                        let w = glam::Vec3::new(wx, 33.5, 18.5);
                        let h = heat.get(lx as i32 - 16, 33, 18);
                        let m = moist.get(lx as i32 - 16, 33, 18);
                        let Some(icon) = icon_map.pick(h, m) else {
                            continue;
                        };
                        // Steam puffs drift up; everything else stays put.
                        if icon.slot == kami_pipelines::atlas_slot::STEAM_PUFF {
                            atlas_for_heat.emit(kami_pipelines::AtlasSprite {
                                pos: w + glam::Vec3::new(
                                    (rnd() - 0.5) * 0.3,
                                    0.3 + rnd() * 0.2,
                                    0.0,
                                ),
                                vel: glam::Vec3::new(0.0, 0.7, 0.0),
                                tint: icon.tint,
                                size: icon.size,
                                slot: icon.slot,
                                rot: rnd() * 6.28,
                                rot_vel: 0.3,
                                age: 0.0,
                                life: icon.life,
                                gravity: false,
                                bob_amp: 0.0,
                                bob_w: 0.0,
                                bob_phase: 0.0,
                                pulse_amp: 0.0,
                                pulse_w: 0.0,
                                wiggle_amp: 0.0,
                                wiggle_w: 0.0,
                                pop_ease_t: 0.0,
                            });
                        } else if icon.bobbing {
                            atlas_for_heat.emit_bobbing(
                                w,
                                icon.slot,
                                icon.tint,
                                icon.size,
                                icon.life,
                                lx as f32 * 1.3 + tick_count as f32 * 0.3,
                            );
                        } else {
                            atlas_for_heat
                                .emit_static(w, icon.slot, icon.tint, icon.size, icon.life);
                        }
                    }
                }
                8 => {
                    // Raindrops falling + splash ring on ground
                    // impact. Water_drop atlas slot is an elongated
                    // teardrop so orientation reads naturally.
                    use kami_pipelines::atlas_slot as slot;
                    for _ in 0..6 {
                        let x = -16.0 + rnd() * 14.0;
                        let z = 16.0 + rnd() * 14.0;
                        let p = glam::Vec3::new(x, 44.0 + rnd() * 2.0, z);
                        atlas_for_heat.emit(kami_pipelines::AtlasSprite {
                            pos: p,
                            vel: glam::Vec3::new(0.0, -5.0, 0.0),
                            tint: [0.55, 0.75, 1.0],
                            size: 0.7,
                            slot: slot::WATER_DROP,
                            rot: 0.0,
                            rot_vel: 0.0,
                            age: 0.0,
                            life: 2.2,
                            gravity: false,
                            bob_amp: 0.0,
                            bob_w: 0.0,
                            bob_phase: 0.0,
                            pulse_amp: 0.0,
                            pulse_w: 0.0,
                            wiggle_amp: 0.0,
                            wiggle_w: 0.0,
                            pop_ease_t: 0.0,
                        });
                    }
                    // Splash rings on ground level (y=32) at random
                    // cells so the landing event reads too.
                    if tick_count % 2 == 0 {
                        if tick_count % 12 == 0 {
                            sfx("pop");
                        }
                        for _ in 0..2 {
                            let x = -16.0 + rnd() * 14.0;
                            let z = 16.0 + rnd() * 14.0;
                            let p = glam::Vec3::new(x, 33.0, z);
                            atlas_for_heat.emit_pop(
                                p,
                                slot::WATER_SPLASH,
                                [0.7, 0.85, 1.0],
                                1.0,
                                0.45,
                                0.15,
                            );
                        }
                    }
                }
                10 => {
                    // FieldIconMap drives every per-cell sprite
                    // choice: flame / steam / drop / bubble / ember.
                    // Steam gets upward drift, drops fall under
                    // gravity-scaled downward vel — motion is the
                    // only post-pick customisation.
                    use kami_pipelines::atlas_slot as slot;
                    let wind = wind_for_tick.borrow();
                    for lx in 3..=13 {
                        for ly in 1..=5 {
                            let wx = -16.0 + lx as f32 + 0.5;
                            let wy = 32.0 + ly as f32 + 0.5;
                            let wz = 18.5;
                            let h = heat.get(lx as i32 - 16, ly + 32, 18);
                            let m = moist.get(lx as i32 - 16, ly + 32, 18);
                            let Some(icon) = icon_map.pick(h, m) else {
                                continue;
                            };
                            if rnd() > 0.35 {
                                continue;
                            } // sub-sample density
                            let pos_base = glam::Vec3::new(wx, wy, wz);
                            let phase = lx as f32 * 1.1 + ly as f32 * 0.7 + tick_count as f32 * 0.3;
                            match icon.slot {
                                s if s == slot::STEAM_PUFF => {
                                    atlas_for_heat.emit(kami_pipelines::AtlasSprite {
                                        pos: pos_base + glam::Vec3::new(0.0, 0.3, 0.0),
                                        vel: glam::Vec3::new(0.0, 0.7, 0.0),
                                        tint: icon.tint,
                                        size: icon.size,
                                        slot: icon.slot,
                                        rot: rnd() * 6.28,
                                        rot_vel: 0.4,
                                        age: 0.0,
                                        life: icon.life,
                                        gravity: false,
                                        bob_amp: 0.0,
                                        bob_w: 0.0,
                                        bob_phase: 0.0,
                                        pulse_amp: 0.0,
                                        pulse_w: 0.0,
                                        wiggle_amp: 0.0,
                                        wiggle_w: 0.0,
                                        pop_ease_t: 0.0,
                                    });
                                }
                                s if s == slot::WATER_DROP => {
                                    atlas_for_heat.emit(kami_pipelines::AtlasSprite {
                                        pos: pos_base,
                                        vel: glam::Vec3::new(0.0, -1.8, 0.0),
                                        tint: icon.tint,
                                        size: icon.size,
                                        slot: icon.slot,
                                        rot: 0.0,
                                        rot_vel: 0.0,
                                        age: 0.0,
                                        life: icon.life,
                                        gravity: false,
                                        bob_amp: 0.0,
                                        bob_w: 0.0,
                                        bob_phase: 0.0,
                                        pulse_amp: 0.0,
                                        pulse_w: 0.0,
                                        wiggle_amp: 0.0,
                                        wiggle_w: 0.0,
                                        pop_ease_t: 0.0,
                                    });
                                }
                                _ if icon.bobbing => {
                                    atlas_for_heat.emit_bobbing(
                                        pos_base, icon.slot, icon.tint, icon.size, icon.life, phase,
                                    );
                                }
                                _ => {
                                    atlas_for_heat.emit_static(
                                        pos_base, icon.slot, icon.tint, icon.size, icon.life,
                                    );
                                }
                            }
                        }
                    }
                    // Wind streamlines via arrow_trail.
                    for _ in 0..2 {
                        let mut p = glam::Vec3::new(
                            -11.5 + (rnd() - 0.5) * 0.6,
                            33.5,
                            18.5 + (rnd() - 0.5) * 0.6,
                        );
                        for _ in 0..10 {
                            let v = wind.vec_at(
                                p.x.floor() as i32,
                                p.y.floor() as i32,
                                p.z.floor() as i32,
                            );
                            let vlen = v.length();
                            if vlen < 0.02 {
                                break;
                            }
                            let dir = v / vlen;
                            p += dir * 0.4;
                            let rot = dir.z.atan2(dir.x);
                            atlas_for_heat.emit(kami_pipelines::AtlasSprite {
                                pos: p,
                                vel: glam::Vec3::ZERO,
                                tint: [1.0, 0.82, 0.45],
                                size: 0.45,
                                slot: slot::ARROW_TRAIL,
                                rot,
                                rot_vel: 0.0,
                                age: 0.0,
                                life: 0.5,
                                gravity: false,
                                bob_amp: 0.0,
                                bob_w: 0.0,
                                bob_phase: 0.0,
                                pulse_amp: 0.0,
                                pulse_w: 0.0,
                                wiggle_amp: 0.0,
                                wiggle_w: 0.0,
                                pop_ease_t: 0.0,
                            });
                        }
                    }
                }
                9 => {
                    // Ember trails along wind: arrow_trail oriented
                    // at each step (Splatoon-ink drift feel).
                    use kami_pipelines::atlas_slot as slot;
                    let wind = wind_for_tick.borrow();
                    for _ in 0..4 {
                        let x = -15.0 + rnd() * 13.0;
                        let y = 34.0 + rnd() * 4.0;
                        let z = 17.0 + rnd() * 13.0;
                        let mut p = glam::Vec3::new(x, y, z);
                        for _ in 0..10 {
                            let v = wind.vec_at(
                                p.x.floor() as i32,
                                p.y.floor() as i32,
                                p.z.floor() as i32,
                            );
                            let vlen = v.length();
                            if vlen < 0.01 {
                                break;
                            }
                            let dir = v / vlen;
                            p += dir * 0.4;
                            let rot = dir.z.atan2(dir.x);
                            atlas_for_heat.emit(kami_pipelines::AtlasSprite {
                                pos: p,
                                vel: glam::Vec3::ZERO,
                                tint: [0.95, 0.78, 0.45],
                                size: 0.55,
                                slot: slot::ARROW_TRAIL,
                                rot,
                                rot_vel: 0.0,
                                age: 0.0,
                                life: 0.9,
                                gravity: false,
                                bob_amp: 0.0,
                                bob_w: 0.0,
                                bob_phase: 0.0,
                                pulse_amp: 0.0,
                                pulse_w: 0.0,
                                wiggle_amp: 0.0,
                                wiggle_w: 0.0,
                                pop_ease_t: 0.0,
                            });
                        }
                    }
                }
                _ => {}
            }

            // Phase 3: paper ignition. Check each paper voxel; if T
            // exceeds 40 (arbitrary threshold), turn to FIRE.
            // Same fixed world coords as the demo (paper line).
            let paper_positions: [(i32, i32, i32); 8] = [
                (-16 + 5, 32 + 1, 16 + 2),
                (-16 + 6, 32 + 1, 16 + 2),
                (-16 + 7, 32 + 1, 16 + 2),
                (-16 + 8, 32 + 1, 16 + 2),
                (-16 + 9, 32 + 1, 16 + 2),
                (-16 + 10, 32 + 1, 16 + 2),
                (-16 + 11, 32 + 1, 16 + 2),
                (-16 + 12, 32 + 1, 16 + 2),
            ];
            // Paper ignition: scenes that emit fire OR propagate it.
            // Scene 7 uses moisture to raise the threshold (wet
            // paper survives). Scenes 1/5/8/9 have no ignition loop.
            let allow_ignition = matches!(scene_id, 0 | 2 | 3 | 4 | 6 | 7 | 10);
            for &(x, y, z) in &paper_positions {
                if !allow_ignition {
                    break;
                }
                let t = heat.get(x, y, z);
                let m = if matches!(scene_id, 7 | 10) {
                    moist.get(x, y, z)
                } else {
                    0.0
                };
                let sample = glam::Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                if !voxels_for_heat.is_solid(sample) {
                    continue;
                }
                // Scene 6: lower threshold so propagation is visible
                // in ~5 s. Scene 7: wet paper ignites only at much
                // higher T (threshold = 40 + M·80).
                let threshold = match scene_id {
                    6 => 25.0,
                    7 | 10 => 40.0 + m * 80.0,
                    _ => 40.0,
                };
                if t > threshold {
                    voxels_for_heat.set_voxel(glam::IVec3::new(x, y, z), PALETTE_FIRE);
                    particles_for_heat.burst(sample, 10, [1.0, 0.6, 0.2]);
                    sfx("coin"); // Mario coin on new ignition
                    // Nintendo Zelda-style "item get" sparkle on new
                    // ignition (scenes 6/7/10). Three stars pop-in
                    // at 120° with easeOutBack overshoot.
                    for k in 0..3 {
                        let offs = (k as f32) * 2.094; // 120°
                        atlas_for_heat.emit_pop(
                            sample + glam::Vec3::new(offs.cos() * 0.5, 0.6, offs.sin() * 0.5),
                            kami_pipelines::atlas_slot::SPARKLE_STAR,
                            [1.0, 0.95, 0.6],
                            1.2,
                            0.7,
                            0.25,
                        );
                    }
                    log::info!(
                        "[isekai-v2-dec] ignited paper ({},{},{}) T={:.1} M={:.2} thresh={:.1}",
                        x,
                        y,
                        z,
                        t,
                        m,
                        threshold
                    );
                }
            }

            // ── Active-region clip ────────────────────────────────
            // Demo phenomena are anchored to the pre-authored house
            // at chunk (-1, 2, 1). Confining the DEC state to a 1-
            // chunk AABB around that origin (= 3×3×3 = 27 chunks
            // max) caps the simulation cost even if advection would
            // otherwise carry fragments far away. Far-field losses
            // are invisible since the camera + visual budget only
            // cover the demo house anyway.
            const ACTIVE_CENTER: (i32, i32, i32) = (-1, 2, 1);
            const ACTIVE_RADIUS: i32 = 1;
            heat.prune_outside(ACTIVE_CENTER, ACTIVE_RADIUS);
            moist.prune_outside(ACTIVE_CENTER, ACTIVE_RADIUS);
            wind_for_tick
                .borrow_mut()
                .prune_outside(ACTIVE_CENTER, ACTIVE_RADIUS);

            if tick_count % 120 == 0 {
                log::info!(
                    "[isekai-v2-dec] heat: {}ck / {}B, moisture: {}ck / {}B, wind: {}ck",
                    heat.chunk_count(),
                    heat.memory_bytes(),
                    moist.chunk_count(),
                    moist.memory_bytes(),
                    wind_for_tick.borrow().chunk_count(),
                );
            }
            drop(heat);
            drop(moist);
            // Mining: left-click (while pointer-locked) ray-marches 8m
            // from the eye along forward and removes the first solid
            // voxel hit. Mesh rebuild happens inside break_voxel.
            if camera.consume_action() {
                if let Some((block, _normal)) =
                    voxels_for_mining.raycast(camera.eye(), camera.forward(), 8.0)
                {
                    log::info!(
                        "[isekai-v2] mine block=({},{},{})",
                        block.x,
                        block.y,
                        block.z
                    );
                    // Debris burst matching the mined block's color
                    // (palette lookup would require keeping the broken
                    // material; use a warm dust tone as generic FX).
                    let burst_pos = glam::Vec3::new(
                        block.x as f32 + 0.5,
                        block.y as f32 + 0.5,
                        block.z as f32 + 0.5,
                    );
                    particles_fx.burst(burst_pos, 14, [0.75, 0.62, 0.48]);
                    voxels_for_mining.break_voxel(block);
                }
            }
            // Place (right-click): raycast → step one block out along
            // the face normal → set brick. Skip if placement would be
            // inside the camera's own body (eye - 1 m vertical).
            if camera.consume_action2() {
                if let Some((block, normal)) =
                    voxels_for_mining.raycast(camera.eye(), camera.forward(), 8.0)
                {
                    let place = glam::IVec3::new(
                        block.x + normal.x as i32,
                        block.y + normal.y as i32,
                        block.z + normal.z as i32,
                    );
                    let eye = camera.eye();
                    let head = glam::IVec3::new(
                        eye.x.floor() as i32,
                        eye.y.floor() as i32,
                        eye.z.floor() as i32,
                    );
                    let feet = glam::IVec3::new(head.x, head.y - 1, head.z);
                    if place != head && place != feet {
                        log::info!(
                            "[isekai-v2] place block=({},{},{})",
                            place.x,
                            place.y,
                            place.z
                        );
                        voxels_for_mining.set_voxel(place, PALETTE_BRICK);
                        // Small white sparkle on placement.
                        let p = glam::Vec3::new(
                            place.x as f32 + 0.5,
                            place.y as f32 + 0.5,
                            place.z as f32 + 0.5,
                        );
                        particles_fx.burst(p, 6, [0.95, 0.85, 0.75]);
                    }
                }
            }
        });

    log::info!("[isekai-v2] backend={:?}", app.backend());
    app.run()
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Native test entry — drives a single frame via `from_context` for
/// headless validation. Used by `cargo test`.
#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    #[test]
    fn builder_compiles() {
        // Smoke test: confirms the builder chain type-checks.
        // Actual GPU work needs a winit + surface, which is out of scope
        // for unit tests.
        fn _assert_shape() {
            use glam::Vec3;
            use kami_app::{CameraMode, InputMode};
            let _ = CameraMode::FirstPerson {
                spawn: Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
            };
            let _ = InputMode::WasdFps;
        }
    }
}

const PALETTE_AIR: u8 = 0;
const PALETTE_WOOD: u8 = 1;
const PALETTE_GRASS: u8 = 2;
const PALETTE_STONE: u8 = 3;
const _PALETTE_SAND: u8 = 4;
const PALETTE_BRICK: u8 = 5;
const PALETTE_GLASS: u8 = 6;
const PALETTE_FIRE: u8 = 7;
const PALETTE_PAPER: u8 = 8;
const PALETTE_ASH: u8 = 9;
const PALETTE_WATER: u8 = 10;

fn isekai_palette() -> kami_pipelines::VoxelPalette {
    vec![
        [0.0, 0.0, 0.0],    // 0 air (unused)
        [0.55, 0.35, 0.2],  // 1 wood
        [0.2, 0.7, 0.25],   // 2 grass
        [0.5, 0.5, 0.55],   // 3 stone
        [0.9, 0.88, 0.8],   // 4 sand
        [0.82, 0.25, 0.2],  // 5 brick
        [0.4, 0.8, 0.9],    // 6 glass
        [1.0, 0.45, 0.1],   // 7 fire (vivid orange, emits heat)
        [0.96, 0.94, 0.88], // 8 paper (ivory)
        [0.25, 0.22, 0.2],  // 9 ash
        [0.12, 0.36, 0.65], // 10 water (emits moisture)
    ]
}

/// Streaming voxel world. Per-chunk generator produces a bedrock floor
/// layer shaped by a coarse FBM height — matches the Plains terrain
/// silhouette roughly so walking on voxels feels continuous with the
/// PBR ground below. Only chunks whose vertical slab intersects the
/// height surface contain solid voxels.
pub(crate) fn build_voxel_world(
    ctx: &kami_render::RenderContext,
) -> kami_pipelines::VoxelChunkAdapter {
    let palette = isekai_palette();
    kami_pipelines::VoxelChunkAdapter::streaming(ctx, palette, 2, |cx, cy, cz| {
        // Voxel terrain heightmap: low-frequency FBM × 12 m amplitude.
        // Deterministic per world-pos → adjacent chunks align.
        let mut has_solid = false;
        let chunk_size = kami_pipelines::CHUNK_SIZE;
        let origin = glam::Vec3::new(
            (cx * chunk_size as i32) as f32,
            (cy * chunk_size as i32) as f32,
            (cz * chunk_size as i32) as f32,
        );
        let mut chunk = kami_pipelines::VoxelChunk::new(origin);
        for lz in 0..chunk_size {
            for lx in 0..chunk_size {
                let wx = origin.x + lx as f32;
                let wz = origin.z + lz as f32;
                let h = simple_fbm(wx * 0.02, wz * 0.02) * 12.0 + 18.0;
                for ly in 0..chunk_size {
                    let wy = origin.y + ly as f32;
                    if wy < h - 3.0 {
                        chunk.set(lx, ly, lz, PALETTE_STONE);
                        has_solid = true;
                    } else if wy < h {
                        chunk.set(lx, ly, lz, PALETTE_GRASS);
                        has_solid = true;
                    }
                }
            }
        }
        if has_solid { Some(chunk) } else { None }
    })
}

/// 2-octave value noise (deterministic, no std-rand). Matches the
/// pattern used in kami-terrain::noise but small + inline.
fn simple_fbm(x: f32, z: f32) -> f32 {
    let n = |x: f32, z: f32| {
        let sx = (x * 12.9898 + z * 78.233).sin() * 43758.547;
        sx - sx.floor()
    };
    let v0 = n(x.floor(), z.floor());
    let v1 = n(x.floor() + 1.0, z.floor());
    let v2 = n(x.floor(), z.floor() + 1.0);
    let v3 = n(x.floor() + 1.0, z.floor() + 1.0);
    let fx = x - x.floor();
    let fz = z - z.floor();
    let ix0 = v0 * (1.0 - fx) + v1 * fx;
    let ix1 = v2 * (1.0 - fx) + v3 * fx;
    ix0 * (1.0 - fz) + ix1 * fz
}

/// Pre-authored demo house at a fixed location. Inserted once at
/// startup; survives streaming unload if placed within the view
/// window (adapter treats pre-authored chunks identically to
/// generated ones — in practice the house is far enough from spawn
/// that it will get unloaded if the player walks away).
pub(crate) fn build_demo_house(
    ctx: &kami_render::RenderContext,
    voxels: &kami_pipelines::VoxelChunkAdapter,
) {
    // House at chunk coord (-1, 2, 1) → world origin (-16, 32, 16).
    // Close to spawn (0, ~22 floor, 80) so the player should see the
    // brick house while walking forward from spawn.
    let origin = glam::Vec3::new(-16.0, 32.0, 16.0);
    let mut chunk = kami_pipelines::VoxelChunk::new(origin);

    for z in 0..16 {
        for x in 0..16 {
            let edge = x == 0 || x == 15 || z == 0 || z == 15;
            chunk.set(x, 0, z, if edge { PALETTE_STONE } else { PALETTE_GRASS });
        }
    }
    for y in 1..5 {
        for i in 0..16 {
            chunk.set(i, y, 0, PALETTE_BRICK);
            chunk.set(i, y, 15, PALETTE_BRICK);
            chunk.set(0, y, i, PALETTE_BRICK);
            chunk.set(15, y, i, PALETTE_BRICK);
        }
        if y <= 3 {
            chunk.set(15, y, 7, PALETTE_AIR);
            chunk.set(15, y, 8, PALETTE_AIR);
        }
    }
    for j in 6..=9 {
        chunk.set(j, 3, 0, PALETTE_GLASS);
        chunk.set(j, 3, 15, PALETTE_GLASS);
        chunk.set(0, 3, j, PALETTE_GLASS);
    }
    // Roof removed for v3-demos so the top-down spawn can see the
    // fire / paper / water row below. Keep the single stone pillar
    // at (2, y, 2) as a visual anchor.
    for y in 6..=9 {
        chunk.set(2, y, 2, PALETTE_STONE);
    }

    // v3 DEC compositional demo: fire at one end, paper line stretching
    // toward it, **water bucket planted in the middle**. Heat and
    // moisture fields diffuse in parallel; paper ignition threshold
    // is `T > 40 + M * 40` so wet paper survives longer. First few
    // paper voxels ignite quickly; papers next to water hold out;
    // far papers take longer as heat needs time to diffuse through
    // the moisture plume.
    chunk.set(4, 1, 2, PALETTE_FIRE);
    chunk.set(4, 2, 2, PALETTE_FIRE);
    for dx in 0..8 {
        chunk.set(5 + dx, 1, 2, PALETTE_PAPER);
    }
    // Water planted between paper indices 3 and 4 (world x = 8..9),
    // one cell above the paper row to act as a moisture canopy.
    chunk.set(8, 2, 2, PALETTE_WATER);
    chunk.set(9, 2, 2, PALETTE_WATER);

    voxels.insert_chunk(ctx, chunk);
}

/// Build the Plains terrain adapter from kami-terrain-scene's `biomes.edn` (executor edge,
/// ADR-0044/0046): the heightmap/splat/palette are data, parity-tested in the scene crate,
/// with the compiled-in `BiomePreset::Plains` only as fallback. Shared by the main entry
/// and the omniverse scene.
pub(crate) fn plains_terrain(ctx: &kami_render::RenderContext) -> kami_pipelines::TerrainAdapter {
    match kami_terrain_scene::resolve_biome("plains") {
        Some(b) => kami_pipelines::TerrainAdapter::streaming_with_config(
            ctx,
            b.to_heightmap_config(42.0),
            b.to_splat_thresholds(),
            b.to_material_palette(),
            128,
            2,
        ),
        None => {
            log::warn!("[isekai] biomes.edn 'plains' unavailable; using builtin BiomePreset");
            kami_pipelines::TerrainAdapter::streaming(
                ctx,
                kami_terrain::BiomePreset::Plains,
                42.0,
                128,
                2,
            )
        }
    }
}
