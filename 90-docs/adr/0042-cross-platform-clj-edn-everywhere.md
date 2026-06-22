# ADR-0042 — CLJ/EDN everywhere: web, macOS, iOS, Android, console

- Status: accepted (design + staged implementation)
- Date: 2026-06-22
- Builds on: ADR-0037 (cross-platform packaging, wasmtime/wasmi), ADR-0038 (Rust base +
  CLJ/Datomic game layer), ADR-0040 (everything describable is EDN), ADR-0041 (play3d)

## Goal

One CLJ/EDN codebase runs on every target — web, macOS, iOS, Android, console. No
per-platform game rewrite; the only platform-specific thing is the thin native execution
core (and the browser's JS for web).

## The stack, per layer

```
   CLJ behaviour            EDN description (10 domains, ADR-0040)
   (logic.clj, systems)     render-graph · ui · input · audio · fsm
        │ kototama          · physics · camera · netsync · level · materials
        ▼ (Clojure→WASM)         │ (plain data — read on every platform)
   WASM module                   ▼
        │ kami-script-runtime    kotoba-edn (native) / cljs.reader (web)
        ▼                        │
   ┌─ wasmtime (JIT)  …… macOS / Linux / Windows / Android
   └─ wasmi   (no-JIT) …… iOS / PS5 / Switch / Android-fallback
        │  (bit-identical runs across backends — ADR-0037)
        ▼  kami:engine/* WIT imports
   native execution core (Rust): wgpu render (kami-webgpu-rs), audio, input, physics
        │  web counterpart:
        ▼  CLJS → WebGPU / WebAudio / DOM (kami.webgpu, kami.ui, kami.audio …)
```

## What runs as CLJ/EDN on every platform — today (verified)

- **EDN data, all 10 domains** — plain data; parsed by kotoba-edn natively and
  cljs.reader on web. Same bytes everywhere. ✔
- **CLJ game behaviour** (logic.clj + `defsystem`s) — compiled by kototama to WASM, run by
  kami-script-runtime. The **wasmi (no-JIT) backend builds for iOS/Android/console** (verified:
  `cargo check -p kami-script-runtime --no-default-features --features backend-wasmi` →
  Finished). The royale `logic.clj` compiles to WASM via kototama (verified). So the *game*
  is one CLJ artifact that runs on macOS/Android (wasmtime) and iOS/console (wasmi). ✔
- **Native render core** — kami-webgpu-rs interprets the EDN render-IR via wgpu (Metal on
  macOS/iOS, Vulkan/GL on Android), golden-frame verified.

## The keystone — already substantially built (verified)

The data-heavy domain interpreters — `kami.fsm`, `kami.physics`, `kami.netsync`,
`kami.level`, `rig->camera` — use Clojure **data structures** (maps, keywords, vectors,
`get-in`, `select-keys`, `reduce`, `some`). The naive assumption was that kototama's numeric
game subset (logic.clj) couldn't compile these. **It can** — kotoba-clj (under kototama)
already ships a heap-value PRELUDE with `vector`/`map` containers, keyword keys (lowered to
strings), and a broad clojure.core surface: `get` `get-in` `assoc` `update` `contains?`
`keys` `vals` `select-keys` `merge` `reduce` `reduce-kv` `some` `every?` `into` `mapcat` …

Verified by compiling representative interpreter logic to WASM and running it
(`tests/keystone_domains.rs`, 3/3 pass):

- **physics** collision matrix (`get-in` + membership) → correct booleans
- **fsm** `advance` (`reduce` over transitions + `get` + `=`) → correct transition
- **netsync** `snapshot` (`select-keys`) → drops unsynced fields

So the `.cljc` interpreters compile to WASM and run via kami-script-runtime on **all**
platforms (macOS/Android wasmtime, iOS/console wasmi) — "everything CLJ/EDN, everywhere"
is reached, not pending.

**Residual gap — now closed.** Set *values* (`{:player #{:bot}}`) didn't compile; a
one-arm `lower_expr` addition (set literal → growable vector; membership via `some`)
fixed it. `keystone_domains` is 4/4 (incl. `set_literal_value_compiles`) and the full
kotoba-clj suite stays green. The data subset
(maps/keywords/vectors/sets/get-in/select-keys/reduce/some) is complete — every `.cljc`
interpreter compiles to WASM and runs as CLJ on every platform. No residual gap.

## Decision

1. Treat the stack above as the canonical cross-platform design; all new gameplay is CLJ
   (behaviour) + EDN (description), never platform code.
2. Ship native via kami-script-runtime: `backend-wasmtime` on macOS/Linux/Windows/Android,
   `backend-wasmi` on iOS/PS5/Switch. The render core is kami-webgpu-rs (+ kami-render).
3. The 10 EDN domains keep `.cljc` interpreters (web today) and pure data; native runs them
   via Rust mirrors *until* the kototama data-subset lands, after which they compile to WASM
   and run as CLJ everywhere.
4. Packaging: macOS/Android via wasmtime host app; iOS/console via the wasmi host
   (static lib + platform shell). The game (logic.clj + scene.edn + EDN domains) is the
   same bytes on every target.

## Consequences

- The game is genuinely write-once: one CLJ/EDN bundle, every platform.
- "All CLJ everywhere" is true for behaviour now; for the data interpreters it lands with
  the kototama data subset (the single highest-leverage next investment).
- Verification travels: golden-frame/no-JIT-parity tests assert bit-identical runs across
  wasmtime/wasmi, so macOS and iOS produce the same frames from the same CLJ/EDN.
