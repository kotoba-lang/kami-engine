# ADR-0039 — kototama + render-IR are the single entry points

Status: accepted (2026-06-21)
Relates to: kototama ADR-0001, network-isekai ADR-0002, ADR-0038 (Rust base + CLJ layer)

## Context

Two duplications grew up as the CLJ/EDN game stack matured:

1. **Two Clojure→WASM compilers** — `kotoba-clj` (general core) and `kami-engine-clj`
   (game prelude + `kami:engine` ABI). Hosts depended on them directly.
2. **Per-demo renderers** — `kami-web` exposes one Rust `run_with_*` entry per game
   (`run_with_scene` / `run_with_game` / `run_with_graph` / `run_with_sabiotoshi` /
   `run_with_character` / `run_with_quarry_walk`), each with the game's look hardcoded
   in Rust.

`kototama` (the unified runtime) and `run_with_render_ir` (the data-driven renderer)
now exist. This ADR makes them the **single entry points** and marks the rest legacy.

## Decision

### 1. The compiler entry is `kototama`

`kototama` is the one Clojure→WASM toolchain (`compile_clj` / `compile_game` /
`compile_game_typed`), layering `kotoba-clj` (core) under `kami-engine-clj` (game). All
hosts depend on **kototama only**:

- ✅ `kami-script-runtime` (native) compiles via `kototama::compile_game_typed`.
- ✅ `network-isekai` (browser) compiles via the kototama wasm (`compile_game`).
- No other crate depends on `kotoba-clj` / `kami-engine-clj` directly — they are now
  **internal layers reached through kototama**. New consumers must not add a direct
  dependency on either; depend on `kototama`.

### 2. The renderer entry for new games is `run_with_render_ir`

`run_with_render_ir(canvas, ir_json)` interprets a CLJ-authored EDN render-IR (ADR-0002)
— no game look in Rust. **New web games use render-IR, not a new `run_with_*` entry.**

The existing per-demo entries are **legacy/deprecated**:
`run_with_scene`, `run_with_game`, `run_with_graph`, `run_with_sabiotoshi`,
`run_with_character`, `run_with_quarry_walk`.

- They are **not removed** — `app-aozora` and other live surfaces still call them. The
  `run_embed_*` family (VRM/SDF/SCAD/NeRF viewers, ADR-0031) is a separate viewer
  surface and is **out of scope** here.
- No new `run_with_*` game entry may be added (the `entries/mod.rs` migration note is
  updated accordingly). Migrate a demo by emitting its scene as render-IR and deleting
  its bespoke entry once no surface references it.

## Consequences

- One compiler, one renderer entry for new work; native + web share both.
- `kotoba-clj` / `kami-engine-clj` keep their repos + tests but are private layers of
  kototama in practice (single public surface).
- Legacy `run_with_*` shrink over time as demos move to render-IR; no big-bang removal.

## Migration ledger

| Surface | Entry | Move to render-IR |
|---|---|---|
| network-isekai / isekai.network | `run_with_render_ir` | ✅ done |
| quarry-walk / isekai voxel | `run_with_quarry_walk` | ▶ when touched |
| graph viewer | `run_with_graph` | ▶ when touched |
| sabiotoshi / scene / game / character | `run_with_*` | ▶ when touched |
