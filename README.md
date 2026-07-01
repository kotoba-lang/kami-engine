# kami-engine

`kami-engine` is now a Kotoba/CLJ/EDN/WIT asset and contract repository for the
Kami engine family.

The former in-repository Rust workspace has been removed. Native render,
physics, robotics, WebGPU, and packaging runtimes should live in adapter
repositories and consume the data, WIT, scene, fixture, and CLJ/CLJC assets kept
here.

## Current Scope

- `wit/` defines the host interface contract.
- `scripts/wit_test.clj` checks the EDN IDL, generated WIT, and kami-clj builtin
  host import map agree.
- `kami-*-scene/data/` and `fixtures/` retain EDN, YAML, CSV, URDF, and scene
  assets for adapter conformance.
- `kami-*-clj/` projects retain CLJ/CLJC authoring, manga, SIP, and web
  surfaces.
- `kami-web/`, `kami-web-modelb/`, and shader/data assets remain as web-facing
  non-Rust fixtures.

## Verify

```bash
bb wit-check
bb test
```

The default path should not contain `Cargo.toml`, `Cargo.lock`, `.rs`,
`rust-toolchain*`, or `.cargo/` files.
