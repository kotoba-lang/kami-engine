# kami-input-scene

EDN authoring surface for [`kotoba-lang/input`](https://github.com/kotoba-lang/input)'s
default device→action input-binding maps (`fps` / `graph`). Restored as
zero-dependency portable CLJC from the legacy `kami-input-scene` Rust crate
(`kotoba-lang/kami-engine`, deleted in PR #82 "Remove Rust workspace from
kami-engine"), per ADR-2607010930.

## What this is

`kami-input-scene` is the data-tier counterpart of `kami-input` for the input
system — the same pattern as `kami-scene` is to a whole `kami-clj` game's
`scene.edn`. It turns canonical `:input/maps` EDN (named, *ordered* tables of
`[key-code action]` pairs) into the real `input/input-map` binding-map
structure, the same shape `input`'s hardcoded `default-fps-map` /
`default-graph-map` presets build. It re-uses `scene`'s tolerant EDN accessors
(`kw-key` / `mget` / `root-map`) the same way games parse `scene.edn`:
namespaced keywords match on `ns/name`, malformed entries are skipped, and an
unrecognised action keyword is a hard error (so a typo never silently drops a
binding).

Per ADR-0038: hot per-frame input resolution (`input/resolve-action`, gesture
detection, focus routing) stays pure CLJC in `kotoba-lang/input`, untouched. A
default binding map is init-time CONFIG, so it's safe to author as EDN here.
The compiled-in `input/default-fps-map` / `input/default-graph-map` builders
remain the `builtin-input-map` fallback and are parity-tested against the
shipped EDN.

## Dependencies

- [`kotoba-lang/scene`](https://github.com/kotoba-lang/scene) — tolerant EDN
  accessors (`kw-key`, `mget`, `root-map`).
- [`kotoba-lang/input`](https://github.com/kotoba-lang/input) — the `actions`
  vocabulary and the `default-fps-map` / `default-graph-map` parity oracle.

No other runtime dependencies.

## Source

- `src/input_scene.cljk` — namespace `input-scene`, ~200 lines.
- `resources/input.edn` — the shipped EDN config, also inlined as the
  `input-edn` string constant in the source (the CLJC-portable equivalent of
  the original's `include_str!`).
- `test/input_scene_test.cljk` — every original Rust `#[test]` (from both
  `src/lib.rs`'s `#[cfg(test)] mod tests` and `tests/input_parity.rs`) ported
  1:1, plus a namespace-loads smoke test: 12 tests / 56 assertions, 0
  failures.

## Run tests

```
kbb -M:test
```
