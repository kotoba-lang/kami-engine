# kami-engine

`kami-physics` integrates the shared Kotoba physics scene contract with
realtime rigid bodies, vehicle reduced-order models and high-fidelity CAE.
Game and spatial-authoring callers use one CLJC router while fidelity remains
explicit and fail-closed.

> **Note (ADR-2607102200 addendum 11):** nested `kami-ui-sdk` JS retired → `kami-engine-app-sdk` (CLJC) + `kami-web/vendor/kami-ui-sdk` (demo only).

`kami-engine` is now a Kotoba/CLJ/EDN/WIT asset and contract repository for the
Kami engine family.

The former in-repository Rust workspace has been removed. Native render,
physics, robotics, WebGPU, and packaging runtimes should live in adapter
repositories and consume the data, WIT, scene, fixture, and CLJ/CLJC assets kept
here.

## Current Scope

- `wit/` defines the host interface contract.
- `docs/adapter-registry.edn` names adapter-owned native/backend targets and
  keeps them outside this default repository.
- `scripts/wit_test.cljk` checks the EDN IDL, generated WIT, and kami-clj builtin
  host import map agree.
- `kami-*-scene/data/` and `fixtures/` retain EDN, YAML, CSV, URDF, and scene
  assets for adapter conformance.
- `kami-*-clj/` projects retain CLJ/CLJC authoring, manga, SIP, and web
  surfaces.
- `kami-web/`, `kami-web-modelb/`, and shader/data assets remain as web-facing
  non-Rust fixtures.
- `kami.render.capture-lifecycle` consumes WebGPU's authoritative
  `:kotoba.webgpu/capture-presence-evidence-v2` queue-submit evidence schema
  (WebGPU merge `247ae4b`); adapters must not rename or synthesize this value.

## Verify

```bash
kbb -M scripts/wit_test.cljk            # wit/kami-interface.edn is the ONE source; world.wit must agree
kbb -M scripts/check_adapter_registry.cljk  # docs/adapter-registry.edn is well-formed
kbb -M scripts/guest_lint.cljk          # every kami-clj game stays inside wit/guest-bindings.edn
cd kami-gameplay && kbb -M:test        # gameplay rules, JVM
cd kami-gameplay && npx --yes kbb --backend sci bin/run_tests.cljk   # the same suite under Node
```

`wit-check` also asserts that every host function carries documentation and that
the generator preserves it. It did not before: `world.wit` is generated, all of
this interface's prose lived only in that file, and every `--gen` deleted it. The
prose now lives in the EDN.

`guest-lint` is new (see `wit/guest-bindings.edn`). The kami-clj vocabulary a
game may use was previously recorded only in a paragraph of another repository's
README, so a game reaching for a name that does not exist found out at compile
time at best and at link time in the browser at worst.

These were `bb scripts/…` until 2026-08-13. babashka was retired as this
workspace's script host by ADR-2607173000 and that conversion left
`scripts/tasks.edn` a literal empty registry, so both gates were documented but
unrunnable for four weeks (ADR-2608131600). They now run under JVM Clojure —
measured 2026-08-13 as "EDN IDL: 40 host functions across 7 interfaces / ✓ EDN
IDL and world.wit agree" and "ok docs/adapter-registry.edn contracts 3".

The default path should not contain `Cargo.toml`, `Cargo.lock`, `.rs`,
`rust-toolchain*`, or `.cargo/` files.

## Nested monorepo cleanup (ADR-2607102200)

- Removed nested `kami-engine-sdk-clj/` (standalone `kotoba-lang/kami-engine-sdk-clj` is SSoT).
- Moved `kami-webgpu-rs` WGSL assets to `fixtures/webgpu-rs-shaders/` (runtime is `webgpu`).
- Removed 1–2 file README stubs that duplicate standalone scene/app repos.
- Kept: `kami-engine-clj/`, `kami-render/` shaders, `kami-ui-sdk/` JS, scene data that is still asset-authoritative here, `wit/`, `fixtures/`.
