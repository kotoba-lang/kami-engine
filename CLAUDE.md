# kami-engine — KAMI Game Engine

See also: `ARCHITECTURE.md` for ownership boundaries and authority rules across `kami-web`, `kami-engine-sdk`, and `kami-ui-sdk`.

## Architecture

**wgpu 統一レンダラ (WebGPU → WebGL2 fallback, ~97% browser coverage) + Nintendo-style UI SDK。**

- Rendering: `Backends::BROWSER_WEBGPU | Backends::GL`。Desktop: Vulkan / Metal / DX12
- Layout + rendering: Rust → WASM。Go CLI はデータ収集のみ
- UI overlay: kami-ui-sdk (JS, DOM)。WebGPU canvas 上に重畳

## WIT Contracts

| Package | Path | Purpose |
|---|---|---|
| `gftd:kami-cine@1.0.0` | `wit/cine/package.wit` | 8-stage neural cinematic pipeline (world-model → usd-scene → neural-geom → temporal-field → neural-render → diffusion-pass → exr-seq → encode). Consumed by `60-apps/ai-gftd-project-{mangaka,animeka,dogaka}/`. Stage records live in shared `app.etzhayyim.apps.cine.*` lexicons (`00-contracts/lexicons/ai/gftd/apps/cine/`). Execution is pod-side per ADR-2605111200; CF Workers only dispatch. Rust crate impl deferred — `kami-cine-{world-model,usd,neural-geom,temporal-field,neural-render,diffusion-pass,exr,encode}` planned. |

## CLJ/EDN Game Layer (ADR-0035/0036/0037/0038)

**Premise (ADR-0038, canonical):** Rust is the per-platform-optimised low-level base
(fastest); a **game** is authored as **data in Datomic + behaviour in a Clojure subset**
(code-as-data) and ships as a write-once artifact (compiled-guest WASM + EDN scene) that the
Rust base runs on **web / mac / iOS / Android / PS5 / Switch**. Everything hot (render,
physics, audio) stays native Rust; only gameplay glue is CLJ (measured ~0.15–0.19 ms/step,
not the hot path).

### Crates

**Single entry points (ADR-0039):** the compiler is **`kototama`**
(`com-junkawasaki/kototama`, layering kotoba-clj core + kami-engine-clj game) — all
hosts depend on kototama, not on kotoba-clj/kami-engine-clj directly. The renderer entry
for **new web games is `kami-web::run_with_render_ir`** (data-driven EDN render-IR); the
per-demo `run_with_*` entries are legacy.

| Crate | Tier | Role |
|---|---|---|
| **kami-engine-clj** | language | Clojure/EDN-subset → **WASM compiler** (`wasm-encoder`, all-i64 ABI, f32, `defsystem`/`defentity`, vec/map prelude). WIT world `kami:engine/kami-game`. `--bin kamiclj` compiles `logic.clj` → `game.wasm`. |
| **kami-script-runtime** | host | Drives `game.wasm` over `hecs`, binding `kami:engine/*` (scene/physics/input/render/audio/time/random). **Two WASM backends, one binding codebase**: `backend-wasmtime` (JIT) and `backend-wasmi` (no-JIT → iOS/PS5/Switch). Deterministic: both backends produce **bit-identical** runs (golden-frame test). Also hosts `input_map`, `platform`, and the `kami` CLI bin. |
| **kami-scene** | data | Tolerant EDN accessors for `scene.edn` (shared by the players; unit-tested). |
| **kami-clj-play** / **kami-clj-play3d** | player | Native winit + wgpu/Metal players (2D survivors / stylized 3D battle-royale). The GPU arm; load `logic.clj` + `scene.edn`, hardcode no game content. |
| **kami-engine-sdk-clj** | brain (Model A) | Clojure/CLJS SDK: Datomic/datalevin source of truth → ECS → render-IR. |
| **kami-clj-host** | brain GPU bridge | Decodes the render-IR → `kami-render` (for the Model-A SDK). |

### Two runtime models (ADR-0038 §2)

- **Model A — brain-on-host** (`kami-engine-sdk-clj` + `kami-clj-host`): the sim loop runs in
  JVM Clojure / browser CLJS, **live Datomic** (`as-of` undo, Datalog). Web/desktop **authoring/dev** only.
- **Model B — compiled-guest** (`kami-engine-clj` → wasm, `kami-script-runtime`): the whole game
  is one wasm a Rust host drives. The **universal ship path** (incl. no-JIT consoles). Datomic is
  baked to an EDN snapshot a frame never queries.
- Canonical pipeline: author in A (live Datomic) → **bake** snapshot + **compile** logic → **ship** via B.
- `kami-app` (Rust-direct games) is the **escape hatch**, not the default — gameplay should be CLJ.

### Tooling & tests

- **`bb kami`** (root `bb.edn`): `targets` / `plan <t>` / `spec <t>` (packaging matrix, single
  source of truth in `kami-script-runtime::platform`) · `bake` (datalevin → scene.edn) · `compile`
  (logic.clj → game.wasm) · `host <t>` (cross-build, feature+triple from `kami spec`) · `package mac`
  (relocatable `.app`) · `play` · `test`.
- **`scripts/test-script-backends.sh`** — runs the suite under **both** wasmtime and wasmi and fails
  on any divergence. Reference game: `kami-clj-play/games/survivors/` (`author.clj` transacts datoms →
  Datalog query → `scene.edn`; player loads `logic.clj` + `scene.edn`).

## Crate Structure (Rust, 29 crates)

### Core

| Crate | 役割 |
|---|---|
| **kami-core** | ECS (hecs) + Actor model + KAMI IPC (columnar zero-copy) |
| **kami-render** | wgpu 統一レンダラ (PBR shader, mesh, camera, texture) |
| **kami-engine** | 統合 (World + Clock + Net) |
| **kami-input** | 統一入力 (keyboard, mouse, touch, gamepad, gesture, FocusManager) |
| **kami-scene-graph** | Scene DAG (parent-child transform hierarchy, hecs 統合) |

### Game Systems

| Crate | 役割 |
|---|---|
| **kami-game** | Game systems (25 modules: physics, NPC, inventory, scene) |
| **kami-physics-2d** | 2D 物理 (AABB, circle, impulse resolution, trigger) |
| **kami-vehicle** | BeamNG-grade soft-body vehicle physics (~80 mass nodes / ~220 beams / Pacejka tire / full powertrain — engine curve / clutch / 6-spd gearbox / open / locked / LSD diff / FWD-RWD-AWD). XPBD integrator with rigid-chassis projection (Müller 2005 shape match) + implicit-Euler+CG alternate. 8 SurfaceKind presets (Dry/Wet/Gravel/Sand/Snow/Ice/Mud/Grass) with grip + friction coefficients. JBeam-subset JSON loader. Garage: 6 vehicles (sedan / hatchback / SUV / sports / pickup / bus). 54 tests |
| **kami-tilemap** | 2D タイルマップ (tile layers, auto-tile, collision map) |
| **kami-pathfind** | A* grid pathfinding + NavMesh (NPC navigation) |
| **kami-skeleton** | Skeletal animation (bone hierarchy, GPU skinning, blend) |
| **kami-audio** | Spatial audio (HRTF, 3D panning, mixer, priority channels) |

### Rendering & Effects

| Crate | 役割 |
|---|---|
| **kami-text** | SDF text rendering (procedural glyph atlas, instanced quads) |
| **kami-ui-gpu** | GPU UI (rounded rect, circle, gradient, border, ToastStack — wgpu instanced) |
| **kami-postfx** | Post-processing (bloom, outline, CRT, vignette, pixelate) |
| **kami-gltf** | glTF 2.0 loader |
| **kami-vrm** | VRM spec compliance: parse / decompose / compose / export + spring bone simulator (verlet + sphere/capsule colliders) + node constraint solver (Rotation / Aim / Roll). Pure Rust, no gpu deps. Consumed by `kami-web::run_embed_vrm` for end-to-end wgpu VRM rendering (ADR-0031) |

### Geometry & Content

| Crate | 役割 |
|---|---|
| **kami-voxel** | Voxel storage (Dense/Sparse/Octree) |
| **kami-sdf** | SDF primitives + CSG tree |
| **kami-mesher** | Marching cubes mesh generation |
| **kami-scad** | OpenSCAD parser + CSG evaluator |
| **kami-nerf** | NeRF (Neural Radiance Field) |
| **kami-geo** | Geospatial (Web Mercator + H3) |

### Open-world rendering (Decima-style, 2026-04-14)

| Crate | 役割 | Tests |
|---|---|---|
| **kami-terrain** | FBM value noise heightmap + splatmap material blend + chunk mesh/LOD + Gerstner water with wind-driven waves + biome presets (Plains/Quarry/Desert/Tundra) | 14 |
| **kami-atmosphere** | Procedural sky (Rayleigh gradient + sun + cloud ray-march) + day/night cycle + wind (gust oscillation) + weather presets (overcast/clear) | 7 |
| **kami-atmosphere** | Sky/sun/clouds + day-night cycle + weather presets + **wind_field** (2-octave FBM, spatially-varying ripples mirrored in WGSL) | 11 |
| **kami-vegetation** | GPU-instanced vegetation. **Taxonomy-driven** `mesh_from_profile(&TaxonomicProfile)` switches on `CanopyShape` (7: Blade/Fan/Radial/Cone/Dome/Column/Carpet) parameterized by `leaf_count/leaf_size/stem_radius`. 7 profiles (grass/fern/palm/conifer/bush/cactus/moss) — adding a species = adding a profile (no new mesh fn). `OwnedTaxonomicProfile::from_json_str` bridges `app.etzhayyim.apps.seibutsu.renderProfile` XRPC. Poisson-disk biome-filtered placement, WASM-cached cull (flat for N<10k, patch-clustered for N≥10k), per-species batched `draw_indexed`, ground AO + wind field in shader | 23 |

Dependency chain: `kami-terrain → kami-vegetation`. Used via `kami-web` WASM exports for browser demos at `isekai.etzhayyim.com/{terrain-demo,quarry,quarry-walk}.htm`.

### WASM exports (kami-web open-world API)

| Export | Purpose |
|---|---|
| `generate_terrain_chunk(cfg)` | Heightmap + splatmap + mesh + palette (biome-aware) |
| `generate_water_mesh(cfg)` | Water plane + Gerstner waves |
| `compute_wind_waves(dir,speed,gust)` | 4 wind-driven Gerstner waves (Beaufort scale, deep-water dispersion) |
| `compute_weather_preset(time,game_time,preset)` | Sky + wind + cloud uniforms (overcast/clear/default) |
| `cache_vegetation(cfg)` / `cull_vegetation(cam_x,cam_z,budget)` | WASM-memory instance cache + partial-sort cull (zero-copy Float32Array) |
| `cache_heightmap(cfg)` / `sample_terrain_height(x,z)` | Heightmap cache + bilinear sample (pure Rust, per-frame) |
| **`run_with_quarry_walk(canvas_id)`** | **Full-Rust entry**: WebGPU setup + event listeners + RAF loop + 4 pipeline render. HTML needs only `<canvas>` + `await init(); run_with_quarry_walk('c')` |

**Archived 2026-04-14** (JS-hybrid prototypes, moved to `_archive/60-apps/kami-demos-js-hybrid-2026-04-14/`):
- `terrain-demo.htm` / `quarry.htm` / `quarry-walk.htm` (pre-v2) — 2,348 LOC of mixed JS + WGSL superseded by full-Rust entry.
- Dead WASM exports removed: `get_heightmap` (→ `cache_heightmap` + `sample_terrain_height`), `look_at` / `mul_mat4` (→ `view_projection` composite).

### Topology tiers

```
L0 leaves:
  kami-terrain   (14 tests)   — FBM noise, splatmap, chunk mesh, water, biome presets
  kami-atmosphere ( 7 tests)  — sky/wind/cloud, day-night, weather presets

L1 derived:
  kami-vegetation ( 9 tests)  → kami-terrain

L2 renderer + facade:
  kami-render::scene_pipelines  (4 pipeline structs, WGSL via include_str!)
  kami-game::quarry_scene       (4 tests — Player/physics/camera/mesh)
  kami-web                      → {terrain,atmosphere,vegetation,render,game}

L3 entry:
  kami-web::quarry_walk_entry::run_with_quarry_walk

L4 HTML (shell only):
  quarry-walk-v2.htm (~30 lines) — <canvas> + init + run call
```

### Visualization & Network

| Crate | 役割 |
|---|---|
| **kami-graph** | Graph layout (Merkle DAG PCB + Force-directed) |
| **kami-knp** | KAMI Network Protocol (UDP + WebTransport) |
| **kami-rtc** | WebRTC SDK (room/peer/media/spatial-audio/signaling, 15 tests) |
| **kami-bridge** | OS input capture/injection bridge (macOS CGEvent, Windows Win32, clipboard sync) |

### OS & Desktop

| Crate | 役割 |
|---|---|
| **kami-os** | OS compositor (wgpu window manager, taskbar, launcher, notifications, consent modal) |

### Builder SDK + Per-Game Crates (DEFAULT for new games, 2026-04)

| Crate | 役割 |
|---|---|
| **kami-app** | Builder SDK (`KamiApp::new_web/.with_*/.run`). `RenderPipeline` / `InputHandler` / `Scene` trait + `Camera` (yaw/pitch/time) + `DepthTarget` + RAF loop + DPR/resize/pointer-lock/HUD publish |
| **kami-pipelines** | Shared `RenderPipeline` adapters: `SkyAdapter`, `TerrainAdapter` (streaming chunks + multi-species vegetation), `WaterAdapter` (Gerstner + fresnel), `VoxelChunkAdapter` (streaming blocky chunks + DDA raycast + greedy meshing + mine/place + AABB collision + floor probe), `ParticleAdapter` (billboard particles, gravity + gravity-free `emit_flow`, burst API). v3-DEC adapters: `FieldVisAdapter` (Λ⁰), `EdgeVisAdapter` (Λ¹ arrows), `FaceVisAdapter` (Λ²) — all with camera-distance LOD (near=stride1 / mid=2 / far=4). Nintendo-style adapters: `AtlasVisAdapter` (procedural sprite atlas — 16 shape slots with 1D bob / scale pulse / rot wiggle / pop-in easeOutBack; distance LOD sparkle collapse + cull) and `FieldIconMap` (7-rule heat/moisture → icon map). `GsplatAdapter` (3D Gaussian Splat preview/QC, ADR-2605092800 — CPU sort + WGSL EWA falloff, ≤50k splats / cloud, multi-cloud HashMap, consumes `kami_render::splat`/`splat_loader` PLY + `.splat` parsers; **WGSL `evaluate_sh` for SH degree 0–3** with `sh_rest` storage buffer + Inria-convention band coefficients — view-dependent specular when `f_rest_*` is present, bit-exact identical degree-0 path otherwise). Game-specific pipelines live in game crate |
| **kami-dec** | v3 DEC physics primitives (see §v3 DEC below) |
| **kami-app-isekai** | ISEKAI game (Plains biome, voxel sandbox target). Live at `isekai.etzhayyim.com/v2.htm` + v3 physics demo `isekai.etzhayyim.com/v3-demos.htm` (11 scenes) |
| **kami-app-quarry-walk** | 2nd reference game (Quarry biome). Live at `isekai.etzhayyim.com/quarry-walk-v2.htm` |
| **kami-app-car-sim** | BeamNG-grade soft-body car simulator. Live at `driver.etzhayyim.com`. Garage picker (6 vehicles) + paint colour + 8-zone surface map (asphalt-dry/wet/gravel/sand/snow/ice/mud/grass) + parts detach UI. 3 wgpu pipelines (line wireframe / filled body Lambert / ground tiles with procedural surface texture in fragment shader — no PNG/JPG assets, all noise-based). XPBD + rigid-chassis projection + tire-as-PBD-constraint integrator. Drive with WASD, scroll to zoom, drag to orbit |

### v3 DEC Physics (kami-dec, 2026-04)

Discrete Exterior Calculus on the voxel cubical complex — replaces N hand-coded rules with a small algebraic vocabulary `∂_t φ = L(φ)` where `L = d / * / Δ` compositions.

| Type | Form | Storage |
|---|---|---|
| `ScalarField` | Λ⁰ (cell centre) | `HashMap<ChunkCoord, Box<[f32; 4096]>>` |
| `EdgeField` | Λ¹ (3 edges/cell +X/+Y/+Z) | `HashMap<ChunkCoord, Box<[[f32; 3]; 4096]>>` |
| `FaceField` | Λ² (3 face normals/cell +YZ/+ZX/+XY) | `HashMap<ChunkCoord, Box<[[f32; 3]; 4096]>>` |

Operators: `d_0: Λ⁰ → Λ¹` (grad) / `d_1: Λ¹ → Λ²` (curl) / `div: Λ¹ → Λ⁰` (codiff) / `solve_poisson_jacobi` / `solve_poisson_multigrid` (2-level V-cycle) / `project_divergence_free` / `project_divergence_free_mg` (Helmholtz) / `vorticity_confine` (Fedkiw) / `step_maxwell` (Yee E/B leapfrog) / `advect_field` (semi-Lagrangian) / `prune_outside` (active-region clip).

Demo URL: `isekai.etzhayyim.com/v3-demos.htm#scene=0..10`. 11 scenes isolate one phenomenon each (heat / moisture / wind / projection / walls+vorticity / Maxwell EM / fire propagation / water extinguish / gravity rain / wind drag / gravity·fire·water·wind coupled). HUD shows position / yaw / pitch / fps / backend via `window.__kami_hud_isekai`.

Perf optimisation stack (layered, each optional): (1) active-region clip (`prune_outside`, caps at 27 chunks), (2) 30 Hz fixed DEC tick (render 60 Hz), (3) multigrid projection, (4) projection iters 12 → 6, (5) streamline-particle budget cap, (6) camera-distance LOD on visualisers.

Tests (6): `point_source_diffuses_isotropically` / `decay_prunes_empty_chunks` / `cross_chunk_boundary_diffusion` / `d1_of_d0_is_zero` (Bianchi) / `multigrid_produces_bounded_residual` / `maxwell_energy_bounded`. Benchmark `cargo bench -p kami-dec --bench ns_vs_rule` compares M1 rule vs v3-NS fire propagation throughput.

### Nintendo-style Visual Layer (N1–N6, 2026-04-19)

Layered on top of the DEC physics for v3-demos.htm. Goal: replace abstract billboards with iconic Nintendo shapes (flame, water drop, sparkle, shock wave, wind swirl, arrow trail) that are cheaper to render and pair with Web Audio feedback.

| Path | What | Where |
|---|---|---|
| **N1 sprite-atlas** | 16-slot procedural WGSL shader; 1 pipeline + 1 adapter covers all iconic shapes (no PNG asset) | `kami-render/src/shaders/scene_atlas.wgsl` + `kami-render/src/scene_pipelines.rs::{AtlasPipeline, AtlasInstance, atlas_slot::*}` + `kami-pipelines/src/atlas_vis.rs::{AtlasVisAdapter, AtlasSprite}` |
| **N2 event-sprites** | Per-scene atlas slot routing (flame cluster / streamline chevrons / shock-wave rings / splash rings / ignition sparkles) | `kami-app-isekai/src/lib.rs` scene match |
| **N3 spring-anim** | AtlasSprite extras: `bob_amp/w/phase` (Y sin) + `pulse_amp/w` (scale throb) + `wiggle_amp/w` (rot) + `pop_ease_t` (easeOutBack 0→1.2→1.0 on emit) | `atlas_vis.rs` |
| **N4 sound-hooks** | Web Audio synth (no assets) — 27 presets; Rust `extern fn kami_play(name)` → JS `window.kamiPlay` with 60 ms same-name throttle + deferred pre-gesture queue | `kami-ui-sdk/kami-sound.js` (copied to isekai static) + v3-demos.htm bridge |
| **N5 field-icon-map** | `FieldIconMap::nintendo_default()` — 7 ordered rules mapping `(heat, moist) → FieldIcon { slot, tint, size, bobbing, life }` | `kami-pipelines/src/field_icon.rs` |
| **N6 atlas-lod** | 2-tier distance LOD on `AtlasVisAdapter`: near (<15 m) = full detail, mid-far (15–40 m) = collapse to `SPARKLE_STAR` (Animal Crossing twinkle), far (>40 m) = cull | `atlas_vis.rs::tick_and_upload` |

Event → sound mapping: scene boot → `select`, paper ignition → `coin` (Mario), Maxwell ring → `tick`, water splash → `pop` (Animal Crossing bubble), wall vortex stall → `whoosh` (Splatoon ink).

Atlas slot catalogue: 0–2 flame S/M/L, 3 ember, 4–5 smoke thin/thick, 6–7 ash/ash_fine, 8 water_drop, 9 water_splash, 10 steam_puff, 11 bubble, 12 sparkle_star, 13 shock_wave, 14 wind_swirl, 15 arrow_trail.

**New game = new `kami-app-{game}` crate, NOT a new `kami-web::run_with_*`**. See `ARCHITECTURE.md` for responsibility matrix + migration status.

### Entry Points (legacy)

| Crate | 役割 |
|---|---|
| **kami-web** | Legacy monolithic WASM entry (6567 LoC, 11 `run_with_*`). Frozen — new games use `kami-app-{game}` |
| **kami-map** | Map renderer WASM entry (maps.etzhayyim.com) |
| **kami-demo** | Desktop demo (winit) |

## kami-ui-sdk (JS, Nintendo-style)

```
kami-ui-sdk/
├── kami-ui.js       — UI コンポーネント
├── kami-motion.js   — モーション (spring physics + easing)
├── kami-sound.js    — 合成音 (Web Audio API, 音声ファイル不要)
└── kami-effect.js   — パーティクルエフェクト (DOM overlay)
```

ロード順: `kami-motion.js` → `kami-sound.js` → `kami-effect.js` → `kami-ui.js`。KamiUI は他 SDK を自動検出し統合 (motion → UI entrance animation, sound → Toast feedback, effect → confetti on load)。

### kami-ui.js — UI Components

| Component | API | 用途 |
|---|---|---|
| `KamiUI.init()` | `{font, bg}` | フォント読み込み + ベーススタイル |
| `KamiUI.StatusBar` | `{text, position}` → `{setText, remove}` | タイトル/ステータス |
| `KamiUI.ControlHint` | `{hints: [{key, action}]}` | 操作ヒント |
| `KamiUI.LabelOverlay` | `{nodes, canvasWidth, canvasHeight}` → `{setNodes, destroy}` | カメラ追従ラベル |
| `KamiUI.FileLoader` | `{accept, onLoad}` | ファイル読み込み |
| `KamiUI.Toast` | `(msg, {type, duration})` | 一時通知 (+ sound + popIn) |
| `KamiUI.Badge` | `(text, {color})` | カウント/タグ |
| `KamiUI.Legend` | `{items: [{color, label}]}` | 色凡例 (+ stagger fadeIn) |

`KamiUI.THEME` — Nintendo カラー/フォント/角丸/影の一括設定。

### kami-motion.js — Motion

| API | 説明 | Nintendo 参考 |
|---|---|---|
| `spring(el, props, opts)` | Spring physics | Splatoon UI bounce |
| `tween(opts)` | 数値アニメーション | — |
| `fadeIn(el)` / `fadeOut(el)` | 透明度 + Y shift | メニュー出現/退出 |
| `popIn(el)` / `popOut(el)` | Scale overshoot | Switch アイコン |
| `shake(el)` | 横揺れ | エラーフィードバック |
| `pulse(el)` | 呼吸 scale | セレクト強調 |
| `slideIn(el, {direction})` | 方向指定スライド | — |
| `stagger(selector, props, opts)` | 連鎖アニメーション | リスト出現 |
| `transition(el, styles, opts)` | CSS transition + Promise | — |

Easing: `linear`, `easeOut`, `easeIn`, `easeInOut`, `bounce` (Mario coin), `elastic` (Splatoon splat), `back`, `backOut`, `pop` (Switch pop)

### kami-sound.js — Sound

Web Audio API 合成。音声ファイル不要。`KamiSound.init()` → `KamiSound.play(name)`。

| Preset | 説明 | Nintendo 参考 |
|---|---|---|
| `click` | sine pip | Switch ボタン |
| `hover` | 高音 soft | カーソル移動 |
| `select` | 2 音 confirm | Mario メニュー |
| `success` | 上昇 triad (C-E-G) | 1-UP |
| `error` | 下降 buzz | wrong answer |
| `warning` | 2 低音 | 注意 |
| `coin` | B5-E6 | Mario コイン |
| `loaded` | D5-F5-A5-D6 | Zelda アイテム get |
| `whoosh` | sweep + noise | Splatoon ink |
| `navigate` | 軽い pip | pan/zoom |
| `zoomIn` / `zoomOut` | pitch sweep | ズーム操作 |
| `reset` | 下降 2 音 | Switch キャンセル |
| `tick` | ランダム高音 | タイプライター |

`KamiSound.register(name, fn)` でカスタム追加。`KamiSound.osc()` / `KamiSound.noise()` で低レベル合成。

### kami-effect.js — Visual Effects

DOM パーティクルシステム。WebGPU canvas 上にオーバーレイ。

| Effect | 説明 | Nintendo 参考 |
|---|---|---|
| `confetti(x, y, opts)` | 紙吹雪バースト (物理: 重力 + 回転) | Mario star / Splatoon win |
| `sparkle(el, opts)` | 周囲に星 (螺旋 + フェード) | Zelda fairy |
| `ripple(x, y, opts)` | 拡大リング | Splatoon ink impact |
| `floatText(text, x, y, opts)` | 浮き上がり + scale overshoot | ダメージ数値 / "+1" |
| `trail(opts)` → `{destroy}` | マウス追従ドット (慣性) | Kirby star trail |
| `flash(opts)` | 全画面フラッシュ | 被弾 / トランジション |

## UI/UX Style: Nintendo

kami-engine の全 UI/UX は Nintendo を参考にする。

| 要素 | 仕様 |
|---|---|
| 背景 | クリーム `#f0ead6`。ダークテーマ禁止 |
| ノード色 | Splatoon パステルパレット (20 色) |
| フォント | Nunito (丸ゴシック)、太字、白テキストシャドウ |
| HUD | 白背景 + 角丸 16px + soft shadow |
| 配線 | 明るい色 (gold/green/blue) |
| 音 | 合成 (Web Audio)。ファイル読み込み禁止 |
| アニメーション | Spring physics (bouncy)。CSS transition は最終手段 |
| エフェクト | DOM パーティクル。GPU パーティクルは kami-render 側 |

## kami-graph (System Visualization)

### Merkle DAG PCB Layout

```
Layer 0 (Y=0):   Writers (263 apps)   — write/invoke する apps
                       ↓ write
Bus Layer:        Collection bus lines — 共有 Merkle nodes (水平線)
                       ↓ subscribe/read
Layer 1 (Y=far): Readers (626 apps)   — subscribe/read する apps
```

### Graph Input (`setup_graph_input`)

| 操作 | Input | 動作 |
|---|---|---|
| Pan | Mouse drag / Touch / WASD | camera 移動 |
| Zoom | Scroll wheel / +/- | ortho extent 変更 |
| Select | Click (drag < 3px) | ノード選択 |
| Reset | Double-click | fit-to-view 復帰 |

### Data Flow

```
gftd haisen scan (Go) → JSON
  → kami-web run_with_graph (Rust WASM) → PcbLayout → wgpu rendering
  → window.__kami_nodes/cam (JS 共有) → KamiUI.LabelOverlay (DOM)
```

## SDK-Promoted Primitives (Reusable across KAMI apps)

### kami-input: FocusManager

Multi-panel/window focus routing for any KAMI app (OS, pptx, xlsx, maps).

```rust
use kami_input::{FocusManager, FocusTarget, PanelId};
let mut fm = FocusManager::new();
fm.set_focus(42);                           // Panel 42 receives input
fm.push_modal(99);                          // Modal 99 captures all input
assert_eq!(fm.resolve(), FocusTarget::Modal(99));
fm.set_global_overlay(true);                // Global overlay (launcher) > all
assert_eq!(fm.resolve(), FocusTarget::GlobalOverlay);
```

Priority: `GlobalOverlay` > `Modal(id)` stack top > `Panel(id)` > `None`

### kami-ui-gpu: ToastStack

Toast notification queue with entrance animation + auto-dismiss for any KAMI app.

```rust
use kami_ui_gpu::{ToastStack, ToastLevel, UiLayer};
let mut stack = ToastStack::new();
stack.push("Title".into(), "Body".into(), ToastLevel::Success, 3000);
stack.tick(16);                              // Advance animation + timer
stack.render(&mut layer);                    // Render to UiLayer (top-right)
```

`ToastLevel`: `Info` (blue) / `Success` (green) / `Warning` (yellow) / `Error` (red) — Nintendo pastel palette.

### kami-os: OS Compositor

Desktop environment crate consuming both SDK primitives above. See `60-apps/ai-gftd-project-os/CLAUDE.md`.

## Genko (原稿) — Manga Canvas Editor (kami-engine-sdk)

`kami-engine-sdk/src/lib/genko/genko-embed.ts` — Self-contained HTML manga editor。`embedMode: "draw"` の全 app で共有。

| 機能 | 実装 |
|---|---|
| Document model | `{pages: [{nodes: [{type, data}]}]}` — stroke/panel/ai-image/text/tone |
| AI image rendering | `_genImageUrl` (URL, preferred) or `_genImage` (base64 fallback) |
| Persistence | B2 primary (`mangaka/docs/{docId}.json`) + graph metadata |
| AT URI deep-link | `parseAtUriFromPath()` → `/at/{authority}/{collection}/{rkey}` → auto-load |
| Project management | B2 index (`mangaka/projects-index.json`) + graph fallback |

## Prohibitions (CRITICAL)

- **Canvas 2D 禁止 (CRITICAL)** — `<canvas>.getContext('2d')` による描画は全面禁止。wgpu (WebGPU + WebGL2 fallback) が唯一のレンダリングパス。ゲームロジックは `kami-game` Rust crate → `kami-web` WASM (`run_with_scene` / `run_with_game` / `run_with_sabiotoshi`) 経由。inline JS Canvas 2D ゲーム (`ketsu-game.htm` 形式) は新規作成禁止、既存は段階移行対象。`gftd code-quality` の `kami_canvas2d_prohibition` check で検出
- **JS による Rust ロジック再実装禁止** — wasm-bindgen で WASM 呼び出し。手動 JS port は Shannon 違反
- **Go での layout/rendering 禁止** — Go CLI はデータ収集のみ
- **音声ファイル禁止** — Web Audio API 合成のみ (`kami-sound.js` パターン)
- **ダークテーマ禁止** — Nintendo クリーム背景 `#f0ead6`
- **独自レンダラ禁止** — `kami-render` wgpu PBR pipeline が唯一

## Ownership & Authority (CRITICAL)

`kami-render` / `kami-app` / `kami-pipelines` / `kami-app-{game}` / `kami-web` / `kami-engine-sdk` / `kami-ui-sdk` の責任境界は `ARCHITECTURE.md` を正本とする。

- **`kami-render`** が GPU bootstrap (Backends + Limits policy) + low-level scene pipelines の正本
- **`kami-app`** が Builder SDK contract の正本 (`KamiApp::with_*`, `RenderPipeline` trait, Camera/Depth)
- **`kami-pipelines`** が shared pipeline adapter の正本 (Sky / Terrain+vegetation / Water)
- **`kami-app-{game}`** は per-game 独立 wasm bundle、engine contract に依存するが逆依存なし
- **`kami-web`** は **legacy monolithic entry**。`run_with_*` の新規追加はゲーム migration 経由のみ (ゲームは `kami-app-{game}` へ)。**例外: VRM viewer surface** は ADR-0031 (`90-docs/adr/0031-kami-vrm-three-free-topology.md`) に基づき kami-web に留まり、VRM spec (skinning / morph / spring / constraint / part composition) にマップされる additive export (`run_embed_vrm` / `set_vrm_*` / `get_vrm_*` / `compose_vrm_*`) は許容する。VRM locomotion (walk/run/jump/idle + third-person orbit) も `run_embed_vrm` 内の additive 拡張として許容 (2026-04-20, isekai.etzhayyim.com/v3-demos.htm#scene=12)
- `kami-engine-sdk` は contract 追従の統合層 (Svelte components/builders/types)
- `kami-ui-sdk` は汎用 DOM UI utility (engine contract を定義しない)

### Merge gate

- `kami-render::bootstrap` 変更は engine owner review + 全 kami-app-{game} への impact note
- `kami-app` Builder API 追加 (additive) は engine owner review
- `kami-app` API 削除・改名は engine owner review + migration plan
- `kami-pipelines` adapter behavior 変更は pipelines owner review
- 新規 `kami-app-{game}` crate は review 不要 (additive、per-game isolated)
- `kami-web` touch は **discouraged** — 新ゲームは `kami-app-{game}` に
- `kami-engine-sdk` contract touch は engine + sdk owner review
- `kami-ui-sdk` 見た目変更は ui-sdk owner review

## Build & Verify

```bash
# WASM
cd 40-engine/kami-engine
wasm-pack build --target web kami-web

# Graph test
gftd haisen scan --include-infra > kami-web/haisen.json
python3 -m http.server 8091
# → http://localhost:8091/kami-web/graph.html

# Headless visual test
node /tmp/visual-test-full-interact.mjs

# Rust tests
cargo test -p kami-graph
```

### Headless WASM tests (no browser)

Run the pure-Rust asset decoders (spz / EXT_meshopt_compression / KHR_mesh_quantization /
KHR_texture_basisu-UASTC) on `wasm32-wasip1` under `wasmtime` — no browser, no GPU:

```bash
brew install wasmtime          # one-time
./scripts/test-wasm.sh         # all kami-render decoder unit tests on wasm
./scripts/test-wasm.sh basisu  # filter (passes through to cargo test)
```

`.cargo/config.toml` wires `wasmtime` as the wasip1 test runner. The script drives the
**rustup** toolchain explicitly (`RUSTC`/`cargo`) because a Homebrew `rustc` on `PATH`
ships no wasm std (`can't find crate for core/std`). Tests use `--no-default-features`
to drop `wgpu-backend` (wgpu/GPU can't run under WASI — those paths still need a browser,
e.g. `wasm-pack build --target web` + headless Chrome).
