# kami-cam-scene

EDN authoring surface for `kami-cam` STOCK MATERIAL presets. Restored as a
zero-dependency portable `.cljc` namespace (`cam-scene`, `src/cam_scene.cljk`,
157 lines) from the legacy `kami-cam-scene` Rust crate in
`kotoba-lang/kami-engine` (deleted in PR #82 "Remove Rust workspace from
kami-engine"; source recoverable at commit
`a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa`), per ADR-2607010930.

## What it does

Turns a canonical `:cam/materials` EDN table into `CamMaterial`-shaped maps
(`{:name :density :hardness}`), the same way a kami-clj game parses
`scene.edn`: missing keys fall back to defaults, ids are hyphenated
keywords (`:aluminum-6061`), and integers coerce to doubles.

A material preset (density g/cm^3, Brinell hardness HB) is init-time
CONFIG — a feed/speed lookup read once when a CAM job seeds its `Stock` —
so it is safe to author as data (ADR-0046 / ADR-0038). The domain crate
itself stays EDN-free; the EDN dependency lives only in this data-tier
crate, mirroring the original Rust split.

## Dependency relationships

- [`kotoba-lang/scene`](https://github.com/kotoba-lang/scene) — the
  tolerant EDN accessors (`kw-key`, `mget`, `root-map`, `num`) used to
  parse the materials table. Pinned at commit
  `b0ca0ba9134dc8e57ebcb1d82d51829456d8b703`.
- [`kotoba-lang/cnc`](https://github.com/kotoba-lang/cnc) — the CAM domain
  logic this crate's data pairs with. `kami-cam-scene`'s five stock
  materials (aluminum-6061, steel-1045, titanium-ti6al4v, abs-plastic,
  wood-oak) are the data-tier counterpart of `kotoba.cam.stock`'s
  compiled-in `aluminum-6061` / `steel-1045` / `titanium-ti6al4v` /
  `abs-plastic` / `wood-oak` builders and `material-presets` table — those
  functions serve as the parity oracle this namespace's shipped EDN is
  tested against. Pinned at commit
  `aa11b30c3fa23d9fe3756e1ebed4d938afe74590`.

  Note: `kotoba-lang/cam` (an *already-restored* repo with the same short
  name) is a **different** crate — camera rigs (follow/look-constraint/
  shake), not CAD/CAM. The actual pairing target for stock-material
  presets is `kotoba-lang/cnc`'s `kotoba.cam.stock` namespace, verified by
  reading the original Rust's `use kami_cam::CamMaterial;` import and
  cross-checking which restored repo actually defines the matching
  `CamMaterial`-shaped preset builders.

## Shipped data

`resources/kami_cam_scene/materials.edn` holds the canonical
`:cam/materials` table (byte-for-byte the same content as the original
crate's `data/materials.edn`). The same text is embedded as the
`cam-scene/materials-edn` string constant in `src/cam_scene.cljk` — the
CLJC analogue of the original's `include_str!("../data/materials.edn")` —
so the namespace does no runtime file IO and stays portable to cljs/wasm.

## Tests

`test/cam_scene_test.cljk` (98 lines) ports all 6 original `#[test]`s from
`src/lib.rs` and both parity tests from `tests/materials_parity.rs`, plus
one namespace-loads smoke test: 10 tests / 50 assertions, 0 failures.

Run with:

```
kbb -M:test
```
