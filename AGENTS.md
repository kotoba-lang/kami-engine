# kami-engine — agent guide

`kami-engine` is the **Kotoba/CLJ/EDN/WIT asset and contract repository** for
the KAMI engine family. It holds:

- the `kami:engine` host interface (WIT, generated from an EDN IDL) and the
  kami-clj guest vocabulary;
- the adapter registry naming which native/backend targets live elsewhere;
- scene, fixture and shader data that adapters must conform to;
- portable CLJC packages: `kami-gameplay` (third-person shooter rules,
  walkable 3D worlds), `kami-render` (render-style/character contracts + WGSL
  shaders), `kami-physics` (fidelity router over the Kotoba physics contract).

Native render, physics, robotics, WebGPU and packaging runtimes live in
adapter repositories and consume these assets. User-facing summary: `README.md`.

## History (short)

This repo used to be a 29-crate Rust workspace (`kami-core`, `kami-render`
wgpu pipelines, `kami-app`/`kami-app-{game}`, `kami-web` wasm entries,
`kami-script-runtime`, `kami-engine-clj` compiler, …) plus a nested JS
`kami-ui-sdk`. All of that is gone: the Rust code was removed (ADR-0048,
`90-docs/adr/0048-global-format-standards-kotoba-clj-wasm-no-rust.md`) and its
logic ported to standalone `kotoba-lang/*` repos; nested sub-projects were
moved out (ADR-2607102200). The Rust-era guide is in git history
(`git show 8d68cf9:AGENTS.md`). `ARCHITECTURE.md` and `TESTING.md` still
describe that removed workspace and are historical. All Clojure-family sources
were renamed to `.cljk` on 2026-09-11 (`cljk-origin.edn`).

> **Update 2026-10-25 (ADR-0049):** this repository is now the monorepo for the `kami-*` family. Standalone `kotoba-lang/kami-*` repositories are imported under `packages/<repo>/` (index: `packages/README.md`); the "moved out / do not restore a nested tree" statements here apply to the top-level pointer directories only, which stay as they are.

## Layout

| path | contents |
|---|---|
| `wit/kami-interface.edn` | **the one source** of the `kami:engine@1.0.0` host interface (functions, params, docs) |
| `wit/kami-game/world.wit` | generated from the EDN IDL; must agree with it |
| `wit/guest-bindings.edn` | kami-clj name for each host import + allowed core forms |
| `wit/kami-frame.wit` | proposed additive frame-submit interface |
| `wit/cine/package.wit` | `gftd:kami-cine@1.0.0` neural cinematic pipeline contract |
| `docs/adapter-registry.edn` | contracts (`:host-interface`, `:scene-data`, `:clj-authoring`) and the adapters that implement them outside this repo |
| `kami-gameplay/` | CLJC gameplay rules (`src/kami/gameplay/*.cljk`), tests, `resources/kami/world3d/`, `bin/run_tests.cljk`, `bin/simulate.cljk`; see its `README.md` and `WORLD3D.md` |
| `kami-render/` | `src/kami/render/*.cljk` (style, character, equipment, capture lifecycle, …) + `src/shaders/*.wgsl`, tests |
| `kami-physics/` | `kami.physics.runtime` backend router (`rigid-2d`, `vehicle`, `cae`) over `kotoba-lang/physics` |
| `kami-clj-play/games/`, `kami-clj-play3d/games/` | kami-clj reference games (`logic.cljk`, `scene.edn`, `author.cljk`) |
| `kami-game-scene/data/`, `kami-live/data/`, `fixtures/` | scene/game catalog EDN, URDF, CSV, YAML, WGSL fixtures for adapter conformance |
| `kami-genesis/` | WGSL step shaders for the Genesis/Isaac-compat physics target |
| `kami-text/` | fonts and the macOS emoji raster helper |
| `kami-app-sip-clj/`, `kami-mangaka-reader-clj/dist/`, `kami-clj-host/demo/` | built web artifacts and demos |
| `kami-flow/graph/` | graph-ingest assets for `graph.etzhayyim.com` |
| `kami-web/`, `kami-ui-sdk/`, `kami-engine-guest/`, `kami-mangaka-*-clj/` | pointer READMEs only — the code moved to its own repo; do not restore a nested tree. Exception: `kami-mangaka-reader-clj/` has no README and keeps only its built `dist/` (`reader.js`, `manifest.edn`) |
| `kami-engine-sdk` | gitlink (no `.gitmodules`); the SDK is a separate repo |
| `hermes/` | resident-bot profiles for this repo |
| `scripts/` | gates (`wit_test.cljk`, `check_adapter_registry.cljk`, `guest_lint.cljk`), task runner (`run-task.cljk` + `scripts/tasks.edn`) |
| `edn-datomize.cljk`, `schema.edn` | EDN → datom conversion; `schema.edn` is generated |
| `90-docs/adr/` | repo-local ADRs (0032–0048) |

## Verify

```bash
kbb -M scripts/wit_test.cljk               # EDN IDL == world.wit, every host fn documented
kbb -M scripts/wit_test.cljk --gen         # print world.wit regenerated from the EDN
kbb -M scripts/check_adapter_registry.cljk # docs/adapter-registry.edn is well-formed
kbb -M scripts/guest_lint.cljk             # every kami-clj game stays inside wit/guest-bindings.edn
kbb -M:test                                # kami-gameplay suite from the root deps.edn
(cd kami-gameplay && kbb -M:test)          # same suite, JVM-shaped
(cd kami-gameplay && kbb --backend sci bin/run_tests.cljk)   # same suite under Node
(cd kami-render && kbb -M:test)
(cd kami-physics && kbb -M:test)
kbb --backend sci scripts/run-task.cljk wit-check   # run a registered task (also `npm run task wit-check`)
```

`kbb --backend sci scripts/run-task.cljk` with no task prints the registered
task names (`scripts/tasks.edn`) and exits 2.

CI (`.github/workflows/ci.yml`) installs kbb with `kotoba-lang/setup-kbb` and
runs the WIT, adapter-registry and guest-lint gates, plus a guard that fails on
any `Cargo.toml`, `Cargo.lock`, `*.rs`, `rust-toolchain*` or `.cargo/` file.
CI does not run the gameplay/render/physics suites.

`scripts/test-*.sh` drive the removed Rust crates and do not run.

## Rules

### Repository scope

- No Rust in this repo. Native backends go in adapter repos registered in
  `docs/adapter-registry.edn`.
- Moved sub-projects (`kami-web`, `kami-ui-sdk`, `kami-engine-guest`,
  `kami-mangaka-*`) keep only their pointer README here
  (`kami-mangaka-reader-clj` keeps only its built `dist/`). `kami-ui-sdk` JS is
  retired: CLJC lives in `kotoba-lang/kami-engine-app-sdk`, the game host
  surface in `kotoba-lang/host`.
- Root `deps.edn` keeps `:paths []`: consumers point `:local/root` here, and a
  default `["src"]` would add a missing directory to their classpath.
- The live browser host for compiled kami games is `kotoba-lang/host`
  (`kami.host`). Its 2D rigid-body system calls `kotoba-lang/physics-2d`
  directly rather than through this repo's `kami-physics` router, so browser
  bundles don't pull in the vehicle/CAE backends.

### Contracts and data

- `wit/kami-interface.edn` is the only source of the host interface. Change
  the EDN (including the `:doc` prose), then regenerate `world.wit`. Never
  hand-edit `world.wit` alone.
- Guest code has integer-only arithmetic and no math library (no sin, cos,
  sqrt, or float compare; `min`, `max` and `clamp` exist but only on
  integers). A host function must not expect the guest to compute a float. Add it in the `move-toward`/`spawn-ring` shape and
  record it in `wit/guest-bindings.edn`.
- `kami.render.capture-lifecycle` consumes WebGPU's
  `:kotoba.webgpu/capture-presence-evidence-v2`. Adapters must not rename or
  synthesize that value.
- `schema.edn` is generated by `edn-datomize.cljk`. Do not hand-edit it.
- Dependencies are immutable `:git/sha` pins. `nbb.edn` copies its shas from
  `deps.edn`, so change both together.
- `kami.gameplay.rng` must stay bit-identical across platforms. Run the
  gameplay suite on both the JVM-shaped and the Node path.

### UI / rendering style

- No Canvas 2D drawing (`getContext('2d')`). Scenes render through the EDN
  render-IR executor (WebGPU, falling back to WebGL2). Don't add a bespoke
  renderer.
- Keep shared logic single-sourced in CLJC/kami-clj. Don't hand-port it into
  parallel JS.
- No layout or rendering in Go. Go tooling only collects data.
- Audio is synthesized (Web Audio style), not loaded from audio files.
- UI follows the Nintendo-style guide: cream background `#f0ead6` (no dark
  theme), Splatoon pastel palette, Nunito rounded bold type, white rounded
  (16px) soft-shadow HUD, bright wiring colors, and springy motion (CSS
  transitions only as a last resort).
