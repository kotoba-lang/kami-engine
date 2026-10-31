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

## Kotoba / Amu selfhost migration (owner decisions, 2026-10-31)

The kami family is moving to canonical `.kotoba` built, checked and tested by
Amu selfhost only (Linear project P-JUN-38, decisions recorded in JUN-640).
These decisions apply to all work in this repository:

- **Placement.** This repository is the game-engine monorepo: gameplay rules,
  scene/game contracts, the `kami:engine` WIT and guest vocabulary, the builder
  SDK, `kami-app-*` apps and the engine's host integration. Parts that are
  useful outside a game engine — 3D processing (geometry, meshes, transforms,
  render IR / WebGPU) and physics (rigid body, vehicle, CAE/CFD solvers) — go
  to the `kotoba-lang/kotoba` monorepo `lib/`. The test is whether anything
  other than the game engine uses it, or is expected to. Dependencies point
  from this repository to `kotoba` `lib/`, never back. Placement of each
  package is tracked in JUN-649.
- **CLJK stays as a reference oracle.** Existing `.cljk` sources are kept, not
  deleted, when their `.kotoba` port lands. The oracle is run by hand on a
  developer machine to produce reference values (KIR, Definition CIDs, golden
  vectors); record the command, tool versions, inputs and outputs with a date.
  It never goes into CI, gates, builds or a fallback path. A mismatch between
  the oracle and the native result is reported, not reconciled.
- **Rust is pruned.** Rust is not kept even as an oracle. Do not add Rust here
  or move Rust into an adapter repo; remaining kami Rust repos
  (`kami-engine-host-rs`, `kami-apps`) are handled in JUN-648.
- **`kami.host` is in scope.** The browser host in `kotoba-lang/host` is part
  of this migration (Amu browser Wasm, JUN-645) and is placed by the same rule.
- **Old repositories are archived after their move.** A standalone
  `kotoba-lang/kami-*` repository is archived once its default-branch tip is
  in its destination and it has no later commits or open PRs. Repositories
  are archived, never deleted.

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
| `packages/kami-game-scene/resources/game_scene/`, `kami-live/data/`, `fixtures/` | scene/game catalog EDN, URDF, CSV, YAML, WGSL fixtures for adapter conformance |
| `kami-genesis/` | WGSL step shaders for the Genesis/Isaac-compat physics target |
| `kami-text/` | fonts and the macOS emoji raster helper |
| `kami-app-sip-clj/`, `kami-mangaka-reader-clj/dist/`, `kami-clj-host/demo/` | built web artifacts and demos |
| `kami-flow/graph/` | graph-ingest assets for `graph.etzhayyim.com` |
| `kami-web/`, `kami-ui-sdk/`, `kami-engine-guest/`, `kami-mangaka-*-clj/` | pointer READMEs only — the code moved to its own repo; do not restore a nested tree. Exception: `kami-mangaka-reader-clj/` has no README and keeps only its built `dist/` (`reader.js`, `manifest.edn`) |
| `kami-engine-sdk` | gitlink (no `.gitmodules`); the SDK is a separate repo |
| `hermes/` | resident-bot profiles for this repo |
| `scripts/native/`, `scripts/native_gates.py` | the CI gates: Kotoba programs for the WIT, adapter-registry and guest-lint checks, and the Python stager that runs them with Amu |
| `scripts/` | CLJK reference oracle for those gates (`wit_test.cljk`, `check_adapter_registry.cljk`, `guest_lint.cljk`), task runner (`run-task.cljk` + `scripts/tasks.edn`) |
| `docs/migration-ledger.json`, `docs/migration-ledger.md` | the migration ledger (JUN-641): every tracked file classified, each unit's namespace dependencies and upstream kotoba libraries, the Amu blockers and the port waves; the `.md` is the reviewable summary |
| `docs/migration-placement.json` | where each unit goes (JUN-649): this repository or kotoba `lib/`, with the reason, duplicates and splits; hand-written, merged into the ledger |
| `scripts/migration_ledger.py`, `scripts/amu_capability_probe.py`, `scripts/placement_consumers.py` | generate or check the ledger; probe which CLJK constructs the pinned Amu CLI admits; survey who requires each unit across the local checkouts |
| `docs/evidence/` | dated oracle-vs-native measurements and Amu capability probes |
| `edn-datomize.cljk`, `schema.edn` | EDN → datom conversion; `schema.edn` is generated |
| `90-docs/adr/` | repo-local ADRs (0032–0048) |

## Verify

The gates CI runs (Apple Silicon, with an Amu checkout at the commit pinned in
`.github/workflows/ci.yml`):

```bash
python3 scripts/native_gates.py --amu ../amu/bin/amu             # all three gates + negative controls
python3 scripts/native_gates.py --amu ../amu/bin/amu --gate wit  # one gate
```

Each gate is a Kotoba program in `scripts/native/`; `native_gates.py` embeds the
files it reads, runs it with `amu run`, and decodes main's i64 result (0 is a
pass; the codes are listed at the top of each program). Every gate also runs
mutants of its inputs that must fail. `scripts/native/edn_read.kotoba` works
around the native EDN reader's handling of commas, newlines, comments and
whitespace runs; see its header before changing it.

The migration ledger must list exactly the tracked files. After adding,
removing or renaming a file, regenerate it (it reads kotoba `lib/` to resolve
upstream namespaces):

```bash
python3 scripts/migration_ledger.py --check                                         # what CI runs
python3 scripts/migration_ledger.py --kotoba ../kotoba --kotoba-rev origin/main     # regenerate
python3 scripts/amu_capability_probe.py ../amu/bin/amu docs/evidence/amu-capability-probe-YYYYMMDD.json
```

Re-run the probe when the Amu pin in `ci.yml` moves, and update `BLOCKERS` in
`scripts/migration_ledger.py` from its result.

Every ledger unit needs an entry in `docs/migration-placement.json`; the check
fails on a unit without one. Decide a new unit by the placement rule above and
record the reason. To refresh the consumer evidence the reasons cite:

```bash
python3 scripts/placement_consumers.py --forest ~/github --output docs/evidence/placement-consumers-YYYYMMDD.json
```

Reference oracle, run by hand only (see the migration decisions above; record
the result with a date under `docs/evidence/`). On main the root `deps.edn`
does not build a classpath (`kotoba.test` is reached through two gitlib roots),
so `docs/evidence/native-gates-oracle-20261031.json` records how it was run:

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

CI (`.github/workflows/ci.yml`) runs the native gates on a `macos-15` runner
against a pinned, digest-checked Amu checkout, a guard that fails on any
`Cargo.toml`, `Cargo.lock`, `*.rs`, `rust-toolchain*` or `.cargo/` file, and
the migration ledger check (`migration_ledger.py --check`: the ledger lists
exactly `git ls-files`; it does not re-derive classes, dependencies or
blockers).

- Removed from CI (JUN-642): `kotoba-lang/setup-kbb` and the three `kbb -M`
  gate runs. No kbb, nbb, JVM or Node runs in CI.
- Native gate: present, with negative controls for every gate.
- No longer covered by CI: regenerating `world.wit` (`wit_test.cljk --gen`
  stays with the oracle); the CLJK gates' exact behaviour on inputs the
  mutants do not exercise. The gameplay/render/physics suites were never in CI
  and still are not.
- The WIT gate searches the committed `world.wit` for every `:doc`, where the
  CLJK gate searched its own regenerated text, so it also fails when the
  committed file loses documentation.
- GitHub Actions is disabled for this repository (checked 2026-10-31; the last
  run was 2026-07-22), so none of this runs until it is turned back on.

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
  hand-edit `world.wit` alone. Write the regenerated file as UTF-8: the
  committed file once lost every `—` and `…` to `?`, which the native WIT gate
  now rejects.
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
