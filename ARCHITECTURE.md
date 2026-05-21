# KAMI Engine Package Boundaries

## Scope

Defines responsibility and authority boundaries across:

- `kami-render` — wgpu bootstrap + low-level render pipelines (shaders, uniforms, swapchain)
- `kami-app` — Builder SDK + lifecycle (RAF loop, tick hooks, ECS world, camera, input, depth)
- `kami-pipelines` — shared `RenderPipeline` adapters (Sky, Terrain + vegetation streaming)
- `kami-app-{game}` — per-game crates composing engine primitives via the Builder API
- `kami-web` — legacy monolithic WASM entries (`run_with_*`); being retired in favor of per-game crates
- `kami-engine-sdk` — TypeScript / Svelte integration SDK
- `kami-ui-sdk` — framework-agnostic DOM UI utilities

## Topology (post-2026-04 migration)

```
┌──────────────────────────────────────────────────────────────┐
│ L4 game crates: kami-app-{isekai, quarry-walk, ...}           │
│   each is a thin composition (~30-60 LoC) of KamiApp builder  │
│   calls + pipeline choice + spawn/biome params.               │
└────────────┬─────────────────────────────────────────────────┘
             │
             ▼
┌──────────────────────────────────────────────────────────────┐
│ L3 kami-pipelines: shared RenderPipeline adapters             │
│   SkyAdapter, TerrainAdapter (streaming + vegetation)         │
│   sun/fog helpers. Depends on kami-app, kami-render,          │
│   kami-terrain, kami-vegetation.                              │
└────────────┬─────────────────────────────────────────────────┘
             │
             ▼
┌──────────────────────────────────────────────────────────────┐
│ L2 kami-app: Builder SDK                                      │
│   KamiApp, trait RenderPipeline, InputHandler, Scene;         │
│   Camera (yaw/pitch/time), DepthTarget, RAF loop, resize      │
│   observer, HUD publish. wasm-bindgen canvas glue.            │
└────────────┬─────────────────────────────────────────────────┘
             │
             ▼
┌──────────────────────────────────────────────────────────────┐
│ L1 kami-render: GPU bootstrap + low-level pipelines           │
│   RenderContext::for_web_surface (unified Backends + Limits), │
│   scene_pipelines::{Sky, Terrain, Vegetation, Character}.     │
└────────────┬─────────────────────────────────────────────────┘
             │
             ▼
  L0 leaves: kami-core, kami-voxel, kami-sdf, kami-terrain,
             kami-vegetation, kami-atmosphere, etc.
```

## Responsibility Matrix

| Package | Owns | Must Not Own |
|---|---|---|
| `kami-render` | Backends + Limits policy (`for_web_surface` — `BROWSER_WEBGPU \| GL`, `downlevel_webgl2_defaults`). Low-level wgpu pipeline structs (Sky / Terrain / Vegetation / Character / Water / Voxel / Particle — 7 primitives as of 2026-04-18) + WGSL shaders. `RenderContext::resize`. | Scene semantics, game loop, canvas/DOM glue, builder API |
| `kami-app` | Builder API (`KamiApp::new_web/.with_*/.run`), RAF loop, `DepthTarget`, Camera (yaw/pitch/time/move_world/vel_y/grounded/action_edge×2), input/scene/pipeline traits, HUD publish, resize observer, floor probe + 3-axis AABB collider sweep + gravity integration + jump | Game-specific logic, render pipeline impls, scene generators |
| `kami-pipelines` | `RenderPipeline` trait impls wrapping kami-render's pipelines. Streaming chunk logic for terrain + vegetation + voxel. Voxel `raycast` (Amanatides & Woo DDA) + `break_voxel` / `set_voxel` with neighbor rebuild + `aabb_solid` / `sample_floor` collision probes. Billboard particle system (emit / burst, gravity integration, camera-facing quads). Sun/fog/time helpers. Greedy voxel meshing (Lysenko 2012, same-material quad merge). `GsplatAdapter` (3DGS preview/QC, ADR-2605092800 — multi-cloud, CPU sort, WGSL EWA covariance, ≤50k splats per cloud, consumes `kami_render::splat`/`splat_loader` PLY + `.splat` parsers; runtime delivery on `maps.gftd.ai` stays on baked static meshes per 260416-maps-kami-street-asset-pipeline). | Game-specific scenes, input schemes, bespoke shaders |
| `kami-app-{game}` | Per-game composition: biome/seed/spawn choice, `wasm_bindgen` entry, game-specific pipeline authored in-crate (e.g. voxel sandbox) | Engine primitives, shared pipelines (import them) |
| `kami-web` (legacy) | Retained WASM exports (`run_with_*`, VRM morph, RTC bindings) for endpoints not yet migrated. Being phased out as kami-app-{game} crates are added. | **New game entries must go to kami-app-{game}, NOT here** |
| `kami-engine-sdk` | Svelte components / builders / types; contract adapters for `kami-web` or `kami-app-{game}` wasm exports | Defining engine semantics |
| `kami-ui-sdk` | Generic DOM UI utilities (`KamiUI`, `KamiMotion`, `KamiSound`, `KamiEffect`) | VRM engine control contracts, wasm API definitions |

## Authority Rules

1. **GPU bootstrap policy** (Backends + Limits) has exactly one owner: `kami-render::bootstrap`. All callers must go through `RenderContext::for_web_surface`/`for_native_surface`. Direct `wgpu::Instance::new` in other crates is forbidden.
2. **Builder API surface** (`KamiApp::with_*`) is owned by `kami-app` and is a soft contract — additive changes are free, removing/renaming requires downstream impact note.
3. **Shared pipelines** (`SkyAdapter`, `TerrainAdapter`) are owned by `kami-pipelines`. Game crates MAY author their own `impl RenderPipeline` for game-specific content (voxel, SDF character, particles). These do not need to move to kami-pipelines unless reused.
4. **Legacy `kami-web` contract** (`run_with_*`, VRM morphs, RTC) is frozen — no new exports. Migrations happen only outward (to `kami-app-{game}`).
5. **Motion semantics** (`evaluate_motion`, `clamp_bone`) remain in `kami-web` for backward compatibility with `kami-engine-sdk`; migration to a new crate is a separate decision.

## Naming and Contract Conventions

1. `kami-web` wasm exports use snake_case (`run_with_scene`).
2. `kami-app-{game}` wasm exports use snake_case with a `v2` suffix during the migration (`run_isekai_v2`, `run_quarry_walk_v2`). The `v2` suffix can be dropped once legacy is retired.
3. `kami-engine-sdk` may expose camelCase wrappers; mapping centralized in one place.
4. New wasm exports from `kami-app-{game}` crates do NOT require engine owner review (they're per-game, isolated bundles).
5. New method on `KamiApp` builder (engine contract) DOES require engine owner review.

## Change Approval Policy

| Change | Review required |
|---|---|
| New pipeline in game crate | None (game-scoped) |
| New `kami-app-{game}` crate | None (additive to workspace) |
| Modify `kami-pipelines` adapter behavior | pipelines owner review |
| Add primitive to `kami-render::scene_pipelines` | engine owner review |
| Modify `RenderContext` / Backends / Limits | engine owner review **+** downstream impact note (every kami-app game) |
| Modify `KamiApp` builder signatures | engine owner review **+** migration plan for all `kami-app-{game}` crates |
| Touch legacy `kami-web::run_with_*` | kami-web owner review (avoid — prefer migrating to a new game crate) |

## Drift Prevention Checklist

For PRs touching `kami-render`, `kami-app`, or `kami-pipelines`:

1. Verify downstream `kami-app-{game}` crates still compile (`cargo check --workspace --target wasm32-unknown-unknown`).
2. Verify legacy `kami-web::run_with_*` still compiles (until retired).
3. Verify the 1m/sample invariant in `Heightmap::generate` if chunk streaming is touched.
4. Verify `kami-engine-sdk` motion key mapping if `evaluate_motion` is touched.

## Migration Status (2026-04-18)

| Game | Entry | Status |
|---|---|---|
| ISEKAI | `kami-app-isekai::run_isekai_v2` → `/v2.htm` | **migrated**; legacy `kami-web::run_with_scene` still live at `/` |
| Quarry walk | `kami-app-quarry-walk::run_quarry_walk_v2` → `/quarry-walk-v2.htm` | **migrated** (2nd reference game validating multi-game topology) |
| Car sim | `kami-app-car-sim::run_car_sim` → `driver.gftd.ai/` | **shipped 2026-05** (3rd reference game; BeamNG-grade soft-body sedan demo backed by `kami-vehicle` physics crate; 3 custom wgpu pipelines — line wireframe / filled body Lambert / ground tiles with procedural WGSL textures; 8 surface zones on a single map) |
| PPTX renderer | `kami-web::render_document_frame` | legacy; no migration planned yet |
| Graph viz | `kami-web::run_with_graph` | legacy; candidate for `kami-app-graph` |
| VRM viewer | `kami-web::run_embed_vrm` | legacy; candidate for `kami-app-vrm-viewer` |
| SCAD / SDF / NeRF embeds | `kami-web::run_embed_*` | legacy; low priority |
| Sabiotoshi | `kami-web::run_with_sabiotoshi` | legacy; candidate for `kami-app-sabiotoshi` |
| Character | `kami-web::run_with_character` | legacy |

## Known Risks

1. **Legacy `kami-web` ownership is split across 11 entries**; incremental migration to `kami-app-{game}` is ongoing. Don't add new entries to `kami-web`.
2. `kami-engine-sdk` includes TypeScript fallback motions that can diverge from `kami-web::evaluate_motion` if not audited (pre-existing).
3. Streaming terrain depends on `Heightmap::generate`'s hard-coded 1m/sample convention (`kami-terrain/src/heightmap.rs:53`). Changing that invalidates seamless chunk tiling.
