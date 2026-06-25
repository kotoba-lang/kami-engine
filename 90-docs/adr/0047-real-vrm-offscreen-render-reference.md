# ADR-0047 — Real-VRM offscreen render reference (clj/edn-driven)

- Status: Accepted
- Date: 2026-06-25
- Relates: ADR-0031 (kami-vrm three-free topology / `run_embed_vrm`),
  ADR-0043 (VRM dance scene clj/edn), ADR-0044 (EDN render-IR three.js/VRM parity)

## Context

ADR-0043/0044 made the *whole* VRM dance authorable as `:dance/*` EDN, projected
to the render-IR. But "the data drives a real VRM" was only **asserted at the data
layer** (parity tests on the realizers). There was no end-to-end proof that a real
`.vrm` — actual geometry, skin weights, textures, morph targets, spring bones —
renders to pixels from that EDN. ADR-0031 keeps production VRM rendering on the
engine-owner-gated `kami-web::run_embed_vrm` surface, so we could not put the proof
there without crossing the gate.

## Decision

Ship a **self-contained offscreen wgpu reference renderer** for the real-VRM path as
a kami-live example, not on the gated surface. It is the headless proof + the
algorithm reference that `run_embed_vrm` (or a future `kami-vrm-render` crate) can
adopt.

- Reusable core lives in `kami-live/examples/common/vrm.rs` (included via
  `#[path]`, so it is shared, not duplicated, and is not itself an example binary):
  - `VrmDance::load` — real VRM → geometry + `JOINTS_0`/`WEIGHTS_0` + textures +
    morph targets + skeleton (parent/topo-order/inverse-bind) + `SpringSimulator`,
    all via existing `kami_vrm` (`parse_vrm`, `convert::extract_primitive_mesh`,
    `convert::read_accessor_f32`, `ExpressionManager`, `spring::SpringSimulator`).
  - `VrmDance::frame` — per-frame CPU: expression morph → humanoid FK from
    `DancePose` → spring bones → joint palette.
  - `GpuRenderer` — offscreen wgpu: GPU skinning (storage-buffer palette) + MToon
    toon-shade + rim + render-IR multi-light + textured per-material draws.
- The canonical example `vrm_edn.rs` is **clj/edn-driven**: VRM path / spring-bones
  toggle / scale come from `:dance/avatar`; per-frame `:lights`/`:env` from the
  render-IR. Change the EDN → change the render.
- three.js / three-vrm parity covered: real geometry, GPU `SkinnedMesh`, baseColor
  textures (UV + alpha-cutout), MToon, beat-synced multi-light, expression morph
  (blink/aa/happy), `VRMC_springBone`.

## Consequences

- The real-VRM render path is now reproducible headlessly (offscreen → PNG/GIF) and
  has a single reusable implementation, not copy-pasted per feature.
- A `.vrm` asset is required to run it (gitignored; not committed). The VRM
  Consortium `Seed-san.vrm` (VRM Public License 1.0) is the reference asset.
- Production VRM rendering stays gated on `run_embed_vrm` (ADR-0031). Promoting
  `common/vrm.rs` to a library crate (`kami-vrm-render`) or wiring it into
  `run_embed_vrm` is a follow-up; the example is the contract it would satisfy.
- No new dependency in kami-live's non-dev build: wgpu/image are **dev**-deps only,
  so the GPU code never enters the pure-data crate.
