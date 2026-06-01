# kami-engine

Reusable Rust game/robotics engine (wgpu renderer + physics + WASM) for the
etzhayyim project. Standalone reusable layer (L2) per ADR-2606011500.

- **Workspace**: a Cargo workspace of `kami-*` crates (core / render / genesis
  physics + control / articulated / sensor-sim / autodrive / pathfind / vehicle
  / terrain / atmosphere / …), generic robot fixtures under `fixtures/`, plus
  reference-game + product app crates (`kami-app-*`).
- **UI SDK**: `kami-engine-sdk/` is a **nested git-submodule** (the TS/Svelte L1
  UI layer, `etzhayyim/kami-engine-sdk`). Run `git submodule update --init` to
  populate it.
- **etzhayyim-specific robotics-actor apps** (shibuya / giemon / giemon-factory
  / tatekata) live OUTSIDE this repo, in the monorepo's `40-engine/kami-apps/`
  (ADR-2606011500 stage 3).

License: Apache 2.0 + etzhayyim Charter Compliance Rider (see `CHARTER-RIDER.md`).
