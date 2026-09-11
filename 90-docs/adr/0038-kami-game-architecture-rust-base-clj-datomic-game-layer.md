# ADR-0038: KAMI game architecture — Rust base + CLJ/EDN/Datomic game layer (canonical model)

**Date**: 2026-06-20
**Status**: Accepted — the canonical model all game work is organised around
**Author**: kami-engine team
**Related / consolidates**: ADR-0035 (kami-engine-clj — Clojure→WASM), ADR-0036
(kami-engine-sdk-clj — Datomic/wgpu SDK), ADR-0037 (cross-platform packaging),
ARCHITECTURE.md

---

## Context

The engine accreted **three ways to make a game**, and the boundaries were never
written down:

1. **Rust crates** (`kami-app` + `kami-app-{game}`) — fastest, but the game is Rust.
2. **CLJ-as-brain** (`kami-engine-sdk-clj`, JVM/ClojureScript) — Datomic is the source of
   truth and the sim loop runs in Clojure; the Rust/wgpu layer is a GPU service (ADR-0036).
   Cannot ship to iOS/console (no JVM, no general JS engine there).
3. **CLJ-compiled-to-WASM** (`kami-engine-clj` compiler + `kami-script-runtime` host) — the
   whole game compiles to one WASM module a Rust host drives; runs **everywhere**, including
   the no-JIT consoles via `wasmi` (ADR-0035, ADR-0037).

This created drift: which path is the *default*? where does Datomic actually live at
runtime? and an inconsistent crate name (`kami-clj`, since renamed — see §5).

The intended premise is simple and worth making canonical:

> **Rust is the per-platform-optimised low-level base (fastest). A game is authored as
> _data in Datomic_ + _behaviour in a Clojure subset_ — code-as-data — and ships as a
> write-once artifact the Rust base runs on every platform.**

This ADR makes that the canonical model and organises the crates, the two runtime models,
and the naming around it.

---

## Decision

### 1. Four canonical layers

```
┌ GAME (what you author) ───────────────────────────────────────────────┐
│  • behaviour : Clojure subset  (logic.clj — code-as-data, EDN forms)   │
│  • data/scene: Datomic / datalevin  →  EDN snapshot (scene.edn)        │
└───────────────────────────────────────────────────────────────────────┘
┌ LANGUAGE + DATA TIER ─────────────────────────────────────────────────┐
│  kami-engine-clj      CLJ-subset → WASM compiler (the language)         │
│  kami-engine-sdk-clj  Datomic schema + kami.{scene,ecs,sim,render,db}   │
│                       (authoring brain; emits snapshots + render-IR)    │
└───────────────────────────────────────────────────────────────────────┘
┌ RUNTIME / HOST TIER (Rust) ───────────────────────────────────────────┐
│  kami-script-runtime  drives game.wasm (wasmtime JIT | wasmi no-JIT)    │
│  kami-clj-host        decodes render-IR → wgpu  (Model-A GPU bridge)    │
│  kami-clj-play        native windowed player (winit) over the runtime   │
└───────────────────────────────────────────────────────────────────────┘
┌ LOW-LEVEL BASE (Rust, per-platform optimised, fastest) ───────────────┐
│  kami-render (wgpu: Metal/Vulkan/WebGPU/DX12) · kami-core (hecs ECS)    │
│  kami-input · kami-audio · kami-physics-* · kami-pipelines · …          │
└───────────────────────────────────────────────────────────────────────┘
```

**The split is the point:** everything hot (render, physics, skinning, audio mix) stays
native Rust, compiled per platform; only gameplay glue is CLJ. Measured (ADR-0037): a CLJ
game-step is ~0.15–0.19 ms even on the `wasmi` interpreter — gameplay is not the hot path,
so writing it in CLJ costs nothing visible.

### 2. Two runtime models, one shipping path

| | **Model A — brain-on-host** | **Model B — compiled-guest** |
|---|---|---|
| Sim loop runs in | JVM Clojure / browser CLJS | one `game.wasm` (`kami-engine-clj`) |
| Driven by | `kami-engine-sdk-clj` + `kami-clj-host` | `kami-script-runtime` (wasmtime/wasmi) |
| Datomic | **live** (`as-of` undo, Datalog at edit time) | **baked** snapshot (a frame never queries it) |
| Runs on | web, JVM desktop | **web, mac, iOS, Android, PS5, Switch** |
| Role | **authoring / dev** — fast iteration, REPL, live editing | **ship** — the universal runtime |

**Canonical pipeline:** author in **A** (Datomic is the live source of truth — transact,
query, time-travel) → **bake** an EDN scene snapshot + **compile** the logic with
`kami-engine-clj` → **ship** via **B** on every platform. `kami-clj-play/games/survivors`
is the reference (`author.clj` transacts datoms → Datalog query → `scene.edn`; the player
loads `logic.clj` + `scene.edn` and hardcodes no game content).

So Datomic *is* the source of truth — at authoring time. Per ADR-0036 a 60 fps frame never
touches the DB; it runs the dense in-memory ECS projected from the snapshot. "Live Datomic
in the loop" is a Model-A dev affordance, not a console-runtime requirement.

### 3. `kami-app` (Rust-direct) is the escape hatch, not the default

Hand-written Rust games (`kami-app-{game}`) remain supported for **engine systems and
perf-bespoke titles**, but the **default game-authoring path is CLJ/EDN/Datomic (Model B)**.
New gameplay should be CLJ unless it needs to live inside the Rust base.

### 4. Naming (renames + the family)

`kami-clj` → **`kami-engine-clj`** (this ADR) — it sat oddly next to `kami-engine-sdk-clj`;
it is the *language/compiler* tier of the CLJ game layer, so it earns the `kami-engine-`
prefix. The CLJ family is now:

| Crate | Tier | Lang |
|---|---|---|
| `kami-engine-clj` | CLJ-subset → WASM compiler (the language) | Rust |
| `kami-script-runtime` | WASM host (Model B) | Rust |
| `kami-clj-play` | native windowed player | Rust |
| `kami-clj-host` | render-IR → wgpu bridge for the SDK (Model A) | Rust |
| `kami-engine-sdk-clj` | Datomic brain SDK (Model A) | CLJ/CLJS |

*Proposed (not yet done, low-priority):* align `kami-clj-host` → `kami-engine-clj-host` and
`kami-clj-play` → `kami-engine-clj-play` for a fully consistent prefix. Deferred to avoid
churn while the hosts are still landing.

### 5. Invariants carried from prior ADRs

- **No-JIT targets use `wasmi`** (iOS/PS5/Switch); the deterministic all-i64 ABI makes
  wasmtime and wasmi runs bit-identical (ADR-0037). Codified in `kami-script-runtime::platform`.
- **GPU stays Rust**; CLJ never touches wgpu/rapier directly — only the `kami:engine/*` WIT
  surface and the render-IR contract.

---

## Consequences

- **One mental model.** "Rust = fast base; game = Datomic + CLJ, code-as-data; ship via
  compiled-guest WASM." The three historical paths now have explicit, non-overlapping roles.
- **Datomic's role is unambiguous:** live source of truth at authoring (Model A), baked
  snapshot at runtime (Model B). No "does a frame hit the DB?" confusion.
- **CLJ is the default**, Rust-direct (`kami-app`) is the escape hatch. Reverses the prior
  implicit default.
- **Cost:** two models to maintain. Justified — A gives REPL/live-Datomic dev ergonomics, B
  gives universal reach; neither alone covers both. The bake step (`kbb -M:kami bake`) is the seam
  that keeps them in sync.
- **Migration:** existing `kami-app-{game}` titles keep working; new games go CLJ. The
  `kami-engine-clj` rename is mechanical (done + verified: all tests green on both backends).

---

## Alternatives Considered

1. **One model only.** Drop A → lose live-Datomic/REPL authoring ergonomics. Drop B → can't
   ship to iOS/console at all. Rejected: keep both with explicit roles + a bake seam.
2. **Full Clojure (SCI/JVM) at runtime instead of a subset.** No shippable JVM/JS on
   consoles; SCI needs a JS/JVM host. Rejected — the `kami-engine-clj` subset compiles to
   portable WASM that `wasmi` runs with no codegen.
3. **Datomic queried live every frame.** Rejected (ADR-0036): a frame runs the projected
   dense ECS; the DB is an edit-time/authoring concern.
4. **Keep the `kami-clj` name.** Rejected — inconsistent with `kami-engine-sdk-clj`; the
   rename clarifies it is the language tier.

---

## References

- ADR-0035 — `kami-engine-clj` (née kami-clj) Clojure→WASM scripting
- ADR-0036 — `kami-engine-sdk-clj` Datomic/datalevin brain + render-IR + wgpu
- ADR-0037 — cross-platform packaging (no-JIT `wasmi`, platform matrix, determinism)
- `kami-clj-play/games/survivors/{logic.clj, scene.edn, author.clj}` — the canonical pipeline in one game
- ARCHITECTURE.md — crate ownership + authority rules
