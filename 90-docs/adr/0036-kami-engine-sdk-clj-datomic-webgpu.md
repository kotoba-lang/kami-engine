# ADR-0036: kami-engine-sdk-clj — Clojure + Datomic SDK over a Rust/wgpu GPU arm

**Date**: 2026-06-13  
**Status**: Accepted — working core + GPU bridge verified (browser render confirmed)  
**Author**: kami-engine team  
**Related**: ADR-0035 (kami-clj — Clojure→WASM game scripting), `ARCHITECTURE.md`,
`kami-engine-sdk-clj/ARCHITECTURE.md`

---

## Context

`kami-engine` is ~91 Rust crates. Authoring a scene/game means writing Rust and
rebuilding WASM. We want to author **scenes, ECS, gameplay, and even shaders in
Clojure**, with **Datomic** (Datalog) as the source of truth — *without*
re-implementing the proven GPU layer.

Two adjacent pieces already exist but don't cover this:

- **ADR-0035 `kami-clj`** compiles *per-entity Clojure scripts* to guest WASM run
  under `kami-script-runtime`. It scripts behaviour inside a Rust-owned loop; it
  does not move the engine loop, scene model, or render orchestration to Clojure.
- **`kami-engine-sdk`** (submodule) is the TypeScript/Svelte integration SDK.

There was no Clojure-native SDK where **clj is the brain** (scene graph as datoms,
systems as pure fns, render-IR built by query) and the GPU is a service.

## Decision

Add **`kami-engine-sdk-clj`** (Clojure/ClojureScript) + **`kami-clj-host`** (Rust),
bridged by a small **render-IR** contract over the KAMI columnar format. clj is the
brain; `kami-render` (wgpu) stays the GPU arm, unchanged.

Three load-bearing decisions (forks resolved):

1. **GPU stays Rust; clj is everything above the GPU line.** Re-implementing wgpu
   in cljs would duplicate `kami-render`'s bootstrap, 7 pipelines, WGSL, and
   WebGL2-fallback parity. Instead the clj side emits **render-IR** and a new
   additive crate `kami-clj-host` executes it on `kami-render`. "Not Rust" is
   satisfied for scene/ECS/gameplay/shader-authoring; the GPU driver is reused.

2. **Datomic = ECS source of truth, two layers.** A component is a Datomic
   attribute, an entity is a Datomic entity. Editing is `transact`, undo is
   `as-of`, history is `history`, queries are Datalog. On load the scene is
   *projected* into a dense in-memory ECS for the 60 fps tick; on save the diff is
   committed back. **A frame never queries Datomic.** datalevin is the default
   store (OSS, embeddable, aligned with `root/`'s "no proprietary Datomic" note);
   the schema + `kami.db` API are Datalog-portable to Datomic Cloud/Peer.

3. **render-IR is retained-by-id, immediate-by-frame.** Meshes/materials/shaders
   are uploaded once keyed by string id; each frame submits a draw-list that
   references them. The **heavy** per-instance matrices travel zero-copy in the
   KAMI columnar buffer (`kami-core::ipc`, `Dtype::Mat4`); the **tiny** references
   (which mesh/material/pipeline per column) travel as a JSON sidecar.

### New artifacts

| Path | What |
|---|---|
| `kami-engine-sdk-clj/` | clj/cljs SDK: `kami.{scene,db,ecs,sim,render,math,wgsl,ipc,gpu}` + `kami.backend.{browser,host}` |
| `kami-clj-host/` | Rust: `frame.rs` (pure KAMI decoder) + `host.rs` (wasm-bindgen `KamiCljHost` + wgpu instanced pass) |
| `wit/kami-frame.wit` | `kami:engine/frame` spec: `register-mesh/material/shader` + `submit-frame(meta, ir-ptr, ir-len)` |

### Authority / boundaries

`kami-clj-host` is a **separate additive workspace crate** that *consumes*
`kami-render` (via `RenderContext::for_web_surface` — the sanctioned bootstrap
owner) and supplies its own wgpu pipeline. Per `ARCHITECTURE.md`'s change-approval
table, a new crate needs **no engine-owner review**; bootstrap/Backends/
`scene_pipelines` are untouched (no `kami-app-{game}` impact note). The only
engine-repo edits are additive: `wit/kami-frame.wit` and one `members` line.

## Verification

All headless gates green; the end-to-end browser render was confirmed on WebGPU.

| Gate | Command | Result |
|---|---|---|
| clj contract layer | `clojure -Sdeps … contract+runtime` | 16 tests / 61 assertions |
| Datomic two-layer (real datalevin) | `clj -M:roundtrip` | connect→tx→snapshot→ecs→render-IR→pack→commit ✅ |
| clj↔Rust byte contract | `cargo test -p kami-clj-host` | 4 tests (decodes exact `kami.ipc/pack` bytes) |
| Rust GPU host compiles (wasm) | `cargo check -p kami-clj-host --features host` | clean |
| **Browser end-to-end** | `wasm-pack build … && http.server` | **2 cubes rendered on cream via clj render-IR → Rust wgpu → WebGPU** |

The cross-language anchor is `kami-clj-host/tests/fixtures/frame.bin` — the literal
bytes `kami.ipc/pack` emits, decoded and asserted by the Rust test (regenerate via
`clj -M:gen -m gen-fixture`).

## Consequences

- New games can be authored in Clojure with Datomic-backed scenes; the GPU path is
  the existing, battle-tested `kami-render`.
- `kami-clj` (ADR-0035) and this SDK are **complementary**: a `kami-clj`-compiled
  `defsystem` could later register into `kami.sim` as a hot-path system; both
  target `kami:engine`.
- New maintenance surface: the render-IR / KAMI columnar contract must stay in
  sync across `kami.ipc` (clj) and `kami-clj-host::frame` (Rust). The fixture test
  is the guard.

## Open questions

1. **Datomic flavor** — datalevin (default) has no `as-of`/`history`; full
   time-travel undo needs Datomic Cloud/Peer. The API is store-agnostic.
2. **WGSL subset** scope for `kami.wgsl` before falling back to raw WGSL strings.
3. **Snapshot granularity** — whole-scene vs streaming sub-DAGs (align with
   `kami-pipelines` chunk streaming for open worlds).
4. Where `kami-clj`-compiled hot systems plug into `kami.sim` (guest WASM vs host).
