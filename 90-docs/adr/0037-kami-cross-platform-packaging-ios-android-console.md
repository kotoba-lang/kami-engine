# ADR-0037: Cross-Platform Packaging — Ship CLJ/EDN Games to iOS, Android, PS5, Switch (+ existing Web/Desktop)

**Date**: 2026-06-20
**Status**: Proposed — Phase 1 (no-JIT backend) implemented + tested
**Author**: kami-engine team
**Related**: ADR-0035 (kami-engine-clj — Clojure→WASM scripting), ADR-0036 (kami-engine-sdk-clj — Datomic/wgpu SDK), `ARCHITECTURE.md`

---

## Context

A game in this engine is authored as **EDN data + Clojure logic**, and that pair is
*already* platform-independent:

- **EDN scene/ECS data** — Datomic/datalevin source of truth → `kami.scene/snapshot`
  (transit/edn), loaded into a dense in-memory ECS (ADR-0036).
- **Clojure game logic** — compiled by `kami-engine-clj` to a **real WASM module** against the
  `kami:engine/kami-game` WIT world (ADR-0035). The compiled `.wasm` is the same bytes on
  every target; it is not interpreted source.

What is per-platform today is only the **host**: the Rust binary that (1) runs the game
WASM, (2) renders via `kami-render`/wgpu, and (3) feeds input/time/audio. That host
currently ships for:

| Target | Logic host | Renderer | Status |
|---|---|---|---|
| Browser (WebGPU→WebGL2) | browser's own wasm engine + `kami-clj-host` (wasm-bindgen) | wgpu | ✅ shipped |
| macOS (Metal) / Linux (Vulkan) / Windows (DX12) | `kami-script-runtime` (**wasmtime**) | wgpu | ✅ shipped |

The four targets the product needs — **iPhone (iOS), Android, PS5, Nintendo Switch** —
have no host. The blocker is **not** the game (CLJ/EDN/WASM is done); it is three host
seams that each console/handheld platform constrains differently:

1. **JIT is forbidden on iOS, PS5, and Switch.** `kami-script-runtime` binds the game
   WASM with **wasmtime**, which JIT-compiles. App Store and both console SDKs prohibit
   runtime code generation (W^X). So the wasmtime host cannot ship there as-is.
2. **wgpu has no PS5 (GNM/AGC) or Switch (NVN) backend.** These are NDA console graphics
   APIs. wgpu *does* cover iOS (Metal) and Android (Vulkan), but `kami-render::bootstrap`
   has no surface-creation path for them yet.
3. **No app shell, input mapping, or compressed-asset variants** exist for mobile/console
   (touch / DualSense / Joy-Con / MFi; ASTC vs BCn).

A further constraint shapes the runtime model: the **CLJ-as-brain** path (ADR-0036, the
sim loop running as JVM Clojure or browser ClojureScript) **cannot run on iOS/PS5/Switch**
— there is no JVM and no general JS engine to ship. On those targets the **entire** game
(sim loop included) must take the **compiled-WASM path** (`kami-engine-clj` → wasm, driven by the
Rust host). Authoring stays on the JVM offline; only baked artifacts ship.

This ADR decides how to extend the existing stack to all six targets **without changing
the game-facing contract** — the same `.clj` + EDN snapshot runs everywhere.

---

## Decision

Keep the game artifact write-once. Port only the host, along three seams. Introduce a
**runtime-model split**, a **no-JIT WASM backend**, a **renderer backend matrix with an
explicit console seam**, and a **per-platform shell/asset bake**.

### 1. Two runtime models, selected per target (not per game)

The game's `.clj` is identical; what differs is *where the sim loop lives*.

| Model | Sim loop | Targets | Mechanism |
|---|---|---|---|
| **A. Brain-on-host** | JVM Clojure / browser CLJS drives `kami.sim` | Web, Desktop (dev) | ADR-0036, unchanged |
| **B. Compiled-guest** | whole game (incl. systems) compiled by `kami-engine-clj` to one wasm; Rust host drives `init/tick/on-event` | **Web, Desktop, iOS, Android, PS5, Switch** | ADR-0035, extended |

Model **B is the universal path** and the only one available on iOS/console. Web and
Desktop support both (B is what unifies them with mobile/console). The product targets
ship Model B exclusively. This makes "implement a game in CLJ/EDN for each platform" mean:
*author once on JVM, compile the logic to one wasm, bake the EDN scene to one snapshot,
and link them against the per-platform host.*

**Prerequisite**: finish `kami-engine-clj` Phase 4 language growth (ADR-0035 §"Phase 4") —
vector/map prelude, `(query-entities pred?)`, `(defentity …)` — so a full game (not just a
per-entity controller) fits the subset. Until then Model B is limited to logic already
expressible (the `survivors.clj` shape).

### 2. No-JIT WASM backend: add `wasmi` behind `kami-script-runtime`

Abstract the WASM execution backend in `kami-script-runtime` behind a trait
(`ScriptBackend`) with two implementations, selected by cargo feature:

| Backend | Feature | JIT? | Targets | Notes |
|---|---|---|---|---|
| `wasmtime` | `backend-wasmtime` (default) | yes | macOS, Linux, Windows, Android | fastest; allowed where W^X is not enforced |
| `wasmi` | `backend-wasmi` | **no** (pure interpreter, no codegen) | **iOS, PS5, Switch** + Android fallback | console/App-Store-legal; ~5–15× slower, acceptable for gameplay (not the hot path) |
| (browser) | n/a | host JS engine | Web | the browser executes the guest wasm; neither runtime is linked |

Both implement the **same `kami:engine/*` import binding** over the same `HostState`
(the `Linker`/`Store` logic is backend-agnostic — only module instantiation and typed-call
differ). Because the guest ABI is the all-i64 deterministic model (ADR-0035) and RNG is
host-seeded, **wasmtime and wasmi produce bit-identical runs** — lockstep netcode, replay,
and cross-platform co-op hold across heterogeneous hosts.

This is the single most important new decision: it is what makes iOS and consoles
reachable at all.

### 3. Renderer backend matrix — extend wgpu, isolate the console seam

`kami-render::bootstrap` is the sole owner of `Backends`/surface creation (ARCHITECTURE.md
authority rule 1). Add surface-creation paths; keep the console APIs behind a seam.

| Target | GPU API | Path | Effort |
|---|---|---|---|
| iOS | Metal | **wgpu (existing Metal backend)** + `for_ios_surface(CAMetalLayer)` | low — wgpu supports iOS Metal; needs surface wiring + iOS build target |
| Android | Vulkan | **wgpu (existing Vulkan backend)** + `for_android_surface(ANativeWindow)` | low — wgpu supports Android Vulkan |
| PS5 | GNM/AGC | **`RenderBackend::Console` seam** behind `RenderContext`; NDA impl out-of-repo | high — NDA SDK, separate private crate |
| Switch | NVN (or Vulkan subset where SDK permits) | same `Console` seam | high — NDA SDK |

The seam is the honest boundary: **everything above the GPU line (EDN, CLJ, render-IR,
input, audio, physics, the wasm host) is fully portable; only the PS5/Switch GPU backend
is platform-proprietary** and lives in a private crate that implements the existing
`RenderContext` contract. We add `for_console_surface(handle)` to `bootstrap` now so the
ABI seam exists even though the impl ships separately under NDA.

### 4. Per-platform shell, input, and asset bake

Only these diverge per target; the game never sees them.

- **App shell** (thin, native):
  - iOS: Swift + `CAMetalLayer` + UIKit lifecycle; links the Rust host as a static lib.
  - Android: `android-activity` (NativeActivity) + JNI surface + Vulkan; `.so` + APK/AAB.
  - PS5/Switch: console SDK entry shell linking the host static lib (private repo).
- **Input mapping** → existing `kami:engine/input`. The host maps device → the abstract
  surface the game already uses (`(axis "MoveX")`, `(key-down? …)`, `(pointer-x)`):
  touch sticks (iOS/Android), DualSense (PS5), Joy-Con/Pro (Switch), MFi (iOS). The `.clj`
  is unchanged — it only ever asks for named axes/actions.
- **Asset variants**: bake `kami.scene` assets to KTX2 with **ASTC** (iOS/Android/Switch)
  or **BCn** (desktop/PS5). Content-addressed; the snapshot references ids, the bake picks
  the variant per target.

### 5. Build / packaging tooling (`bb` + cargo cross)

Add a babashka task layer (`tools/kami`) orchestrating the write-once → per-target flow:

```
bb kami bake     <game>            ; Datomic snapshot (transit/edn) + KTX2 asset variants per target
bb kami compile  <game>            ; kami-engine-clj: game .clj → game.wasm  (one artifact, all targets)
bb kami host     --target ios|android|ps5|switch|web|mac
                                  ; cargo build the per-target host (backend-wasmi for ios/ps5/switch)
bb kami package  --target …        ; .app / .apk(.aab) / console package / web bundle / .app
bb kami run      --target mac      ; dev loop (wasmtime + hot-reload, kami-engine-clj Phase 3)
bb kami test                       ; headless golden-frame: run game.wasm under wasmi, hash ECS state
```

`game.wasm` and the snapshot are built **once**; `host`/`package` are the only per-target
steps. `kami test` runs on the no-JIT (`wasmi`) path in CI so the console/iOS code path is
continuously exercised without a device.

✅ **`bb.edn` orchestrator implemented** (root `bb.edn`): `bb {targets,plan,spec,bake,compile,
host,play,test}` wire the existing pieces — `bb bake survivors` runs `author.clj` (datalevin →
scene.edn), `bb compile survivors` runs `kamiclj` (logic.clj → game.wasm, verified 3 KB out),
`bb host <target>` reads the feature+triple from `kami spec <target>` (EDN — single source of
truth, no re-encoding) and cross-builds, reporting "NDA console SDK" for PS5/Switch. `bb test`
is the dual-backend gate. The per-platform decisions live only in `kami-script-runtime::platform`.
✅ **`bb package mac` produces a relocatable `.app`** (`dist/<Game>.app`): release host binary in
`Contents/MacOS/`, the game's `logic.clj` + `scene.edn` in `Contents/Resources/game/`, and an
`Info.plist`. The player resolves its game dir relocatably (`$KAMI_GAME_DIR` → `<exe>/../Resources/game`
→ dev `CARGO_MANIFEST_DIR`), so the bundle is self-contained — verified: the bundled binary loads the
game from `Resources/game`, not the source tree. iOS/Android/console mirror this layout behind their
native shells (Swift / NativeActivity / console SDK), which `bb package <target>` flags as the
remaining native-shell step.

---

## Architecture

```
            AUTHOR (offline, JVM — any dev machine)
            ┌───────────────────────────────────────────┐
            │ kami-engine-sdk-clj  (Datomic/datalevin)   │
            │   scene/ECS as datoms · systems as fns     │
            └───────────────┬───────────────────────────┘
                            │  bb kami bake / compile
        ┌───────────────────┴────────────────────┐
        ▼                                         ▼
  snapshot.edn (scene data)              game.wasm  (kami-engine-clj → kami:engine/kami-game)
  + KTX2 assets (ASTC | BCn)                 ── platform-independent, write-once ──
        │                                         │
        └─────────────────┬───────────────────────┘
                          │  linked into per-target host
   ┌──────────────────────┼──────────────────────────────────────────────┐
   ▼            ▼          ▼            ▼              ▼                    ▼
  Web        macOS      Android       iOS            PS5                Switch
 browser    wasmtime   wasmtime/    **wasmi**      **wasmi**           **wasmi**
 wasm eng   (JIT)       wasmi        (no JIT)       (no JIT)            (no JIT)
   │          │          │            │              │                    │
 wgpu       wgpu       wgpu(Vk)     wgpu(Metal)   Console seam        Console seam
 WebGPU     Metal       Android      iOS           (GNM/AGC, NDA)      (NVN, NDA)
   └──────────┴──────────┴────────────┴── kami-render (RenderContext contract) ──┘
                          host = kami-script-runtime + kami-render + kami-clj-host
                          imports bound identically: scene/physics/input/render/audio/time/random
```

---

## Consequences

**Gained**
- One `.clj` + one EDN snapshot run on all six targets. The game author writes Clojure/EDN
  and never touches Rust, Metal, Vulkan, or a console SDK.
- iOS and consoles become reachable purely by adding a `wasmi` backend + surface wiring —
  no second language, no re-port of game logic.
- Determinism (host-seeded RNG, all-i64 ABI) makes wasmtime↔wasmi runs identical →
  cross-platform lockstep co-op, replays, ghosts, and headless golden-frame CI for free.
- The existing Web/Desktop stack (ADR-0035/0036) is unchanged; this is additive.

**Costs / risks**
- `wasmi` is an interpreter: ~5–15× slower than wasmtime. Acceptable because gameplay is
  not the hot path (physics/render/skinning stay native in Rust). Discipline required:
  anything heavy must live in a Rust `kami:engine/*` host fn, not in guest Clojure.
- **PS5/Switch GPU backends are out of this repo's scope** and require NDA SDK access in a
  private crate implementing `RenderContext`. "Console support" = "every layer portable
  except the GPU backend." State this precisely; do not imply a turnkey console build.
- Model B requires `kami-engine-clj` Phase 4 (whole-game subset). Until done, console/iOS games
  are limited to the currently-expressible subset (`survivors.clj` complexity).
- iOS/console cannot use the CLJ-as-brain (JVM/CLJS) path; those targets are Model B only.
  Web/Desktop keep both, but should prefer B to stay on the unified path.

**Phased rollout**
1. ✅ **Backend split** — `kami-script-runtime` now compiles its one host-binding codebase
   against **either** wasmtime (default) or **wasmi** (`--no-default-features --features
   backend-wasmi`), selected by cargo feature via cfg-aliased engine types. All 14 tests
   — including the survivors core loop and the seeded-RNG determinism test — **pass
   identically on both backends**, confirming the no-JIT path executes kami-engine-clj-compiled
   game logic with the same results. A pure `backend-wasmi` build links no wasmtime /
   cranelift (no codegen), which is what iOS/PS5/Switch require. Implementation note: the
   only API divergences are module instantiation (wasmi `instantiate_and_start` vs
   wasmtime `instantiate`) and the error/linker types — both cfg-gated.
   CI gate: `scripts/test-script-backends.sh` runs the suite under both backends and
   fails if either diverges. (A single-binary cross-backend test is intentionally
   precluded — the cfg-alias makes the two engines mutually exclusive in one build — so
   parity is asserted by running the identical suite under each feature instead.)
   ✅ **Golden-frame determinism test** (`golden_frame_determinism`): runs a seeded sim for
   a fixed number of ticks and asserts a single world-state hash GOLDEN both backends must
   hit — the cross-backend determinism guard. Authoring it **found and fixed a real bug**:
   system execution order was engine-dependent (`Module`/`Instance::exports()` iterate in
   section order on wasmtime but **alphabetically on wasmi**), silently reordering
   `spawn`/`ai` and shifting a just-spawned entity by one tick. Fixed at the source —
   `kami-script-runtime` now reads the `-tick` export order from the WASM **export section**
   of the module bytes (CLJ definition order), engine-independent. After the fix both
   backends produce **bit-identical** world state (same GOLDEN): the "deterministic across
   backends" claim is now verified, not assumed.
2. ✅ **kami-engine-clj Phase 4** — language growth so a full game compiles to one guest wasm.
   ✅ **vector / state-bag prelude** (`vec-make` / `vec-push!` / `vec-get` / `vec-set!` /
   `vec-len` / `vec-clear!`) — fixed-capacity i64 array for state ECS components don't
   cover (spawn queues, wave lists, cooldown tables). ✅ **map / assoc prelude** (`map-make`
   / `map-put!` insert+update / `map-get` / `map-get-or` / `map-has?` / `map-len` /
   `map-clear!`) — fixed-capacity i64→i64 linear-scan store for sparse state (cooldowns by
   entity id, tag→count tallies). Both pure-prelude (no codegen change). ✅ **`defentity`**
   (`ast.rs`) — `(defentity name [params…] body…)` desugars to a constructor that spawns a
   fresh entity tagged `name`, binds it to `self` for the body to initialize, and returns
   the id (the prefab DSL). ✅ `query-entities` covered by existing `doseq-entities` /
   `nearest-tagged` / `count-tagged`. All compile-tested in `kami-engine-clj` and runtime-tested in
   `kami-script-runtime`, executing on **both** backends via the gate (17 tests green each).
3. **iOS** — *In progress:* ✅ **input seam #3 complete** (`kami-script-runtime::input_map`)
   — the device-neutral mapping every non-keyboard target shares (so it also advances Steps
   4/5). Axes: `VirtualStick` (touch → clamped, dead-zoned `[-1,1]` pair, y-up) +
   `apply_dead_zone` (physical sticks: DualSense/Joy-Con/MFi) → `feed_stick` → `(axis …)`.
   Buttons: `ButtonEdges` computes the press/release edge host-side so `(key-down? …)` reads
   as a level and `(key-pressed? …)` as a down-edge identically on every device →
   `feed_buttons`. Pure Rust; 9 unit tests + 2 end-to-end (touch→velocity, button→level/edge
   spawns) passing on both backends. Also exercised live on Mac headless — see
   `examples/mac_demo.rs` (same trace under wasmtime and wasmi). *Remaining (need a
   device/Xcode):* `for_ios_surface` (Metal/CAMetalLayer in `kami-render::bootstrap`) +
   Swift shell linking the `backend-wasmi` host + ASTC asset bake.
   **No-JIT host de-risked on desktop:** the native player (`kami-clj-play`) now forwards the
   backend feature, so the *same rendered game* runs under wasmtime **and** wasmi on macOS.
   Measured side by side (survivors, ~tens of enemies, Metal, vsync): both hold 60 fps with a
   CLJ game-step of ~0.15–0.19 ms — interpreter overhead is in the noise because gameplay
   isn't the hot path. This is the exact host code path iOS/PS5/Switch use, minus the surface.
4. **Android** — `for_android_surface` + NativeActivity shell + Vulkan + touch → AAB.
5. **Console seam** — `for_console_surface` + private NDA backend crate (PS5, then Switch).

**Shared across Steps 3–5 — packaging matrix as code (done):** `kami-script-runtime::platform`
turns the §4 matrix into executable data: `Target::{Web,Mac,Linux,Windows,Ios,Android,Ps5,
Switch}::spec()` returns the `jit_allowed` / `LogicHost` (wasmi vs wasmtime vs browser) /
`TexFmt` (ASTC vs BCn vs auto) / `RenderBackend` (incl. the `Console` NDA seam) / input
default for each, plus `host_feature()` (the cargo feature the host links). 5 tests pin the
invariants — iOS/PS5/Switch are no-JIT⇒wasmi, only consoles need the seam, mobile/Switch get
ASTC — so the per-platform decisions can't silently regress as the host crates land. The
`bb kami host/package` tooling and CI consume this instead of re-encoding the matrix in prose.
A `kami` CLI (`cargo run -p kami-script-runtime --bin kami -- targets | plan <target>`) makes
it actionable: it prints the full matrix and, per target, the JIT/host/texfmt/render/input
decision + rustc triple + the exact `cargo build` command for the host (or "NDA console SDK"
for PS5/Switch). This is the seam `bb kami host/package` shells out to.

---

## Alternatives Considered

1. **Ship wasmtime everywhere (incl. iOS/console).** Rejected: JIT is prohibited by the
   App Store and both console SDKs (W^X). wasmtime's interpreter (Winch) is not a supported
   no-codegen config across these SDKs; `wasmi` is purpose-built for this.

2. **AOT the guest wasm to native per console (no runtime wasm).** Rejected: loses
   hot-reload, needs a per-target wasm→native toolchain inside NDA SDKs, and forks the
   artifact per platform — defeating write-once. `wasmi` keeps one `game.wasm`.

3. **Re-author console games in Rust (`kami-app-{game}`).** Rejected: violates the whole
   premise (CLJ/EDN authoring) and doubles maintenance. Rust crates remain the path for
   engine systems, not gameplay.

4. **Run the CLJ-as-brain sim via a portable JS/JVM runtime on console.** Rejected: no
   shippable general JS engine or JVM on PS5/Switch; GraalVM native-image doesn't target
   these. The compiled-guest path (Model B) is the only portable sim model.

5. **Replace wgpu with a console-first middleware (e.g. bgfx/sokol).** Rejected: would
   discard `kami-render`'s shipped WebGPU/WebGL2/Metal/Vulkan/DX12 parity, 9 pipelines, and
   WGSL. The `RenderContext` seam lets console backends slot in without that loss.

---

## References

- ADR-0035 — `kami-engine-clj` Clojure→WASM scripting (`kami:engine/kami-game` WIT, all-i64 ABI, defsystem)
- ADR-0036 — `kami-engine-sdk-clj` Datomic/datalevin brain + render-IR + wgpu GPU arm
- `wit/kami-game/world.wit` — host imports (scene/physics/input/render/audio/time/random), guest exports (init/tick/on-event)
- `kami-script-runtime/tests/survivors.clj` — reference Model-B game (twin-stick survivors)
- `kami-render/src/bootstrap.rs` — `RenderContext` / `Backends` ownership (seam for `for_{ios,android,console}_surface`)
- `kami-clj-host/src/frame.rs` — KAMI columnar render-IR decoder (platform-independent)
- ARCHITECTURE.md — crate ownership + authority rules (render backend changes need engine owner)
