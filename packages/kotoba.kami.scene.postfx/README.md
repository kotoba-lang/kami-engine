# kami-postfx-scene

EDN authoring surface for [`kotoba-lang/postfx`](https://github.com/kotoba-lang/postfx)
post-processing pipeline presets (`nintendo`, `retro`, `final-fantasy`,
`baminiku-character`).

Restored from the legacy `kami-postfx-scene` Rust crate
(`kotoba-lang/kami-engine`, deleted in PR #82 "Remove Rust workspace from
kami-engine", recovered at commit
`a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa`) as zero-dependency portable CLJC,
per ADR-2607010930 (`com-junkawasaki/root`).

## What this is

The data-tier counterpart of `kami-atmosphere-scene` for the post-processing
system: it turns canonical `:postfx/presets` EDN (an ordered vector of
`:effect`-tagged effect maps per preset) into a real `postfx` pipeline
(`{:effects [...] :enabled true}`), rebuilt the same way the hardcoded
presets are (`postfx/new-pipeline` + `postfx/add` in order). It reuses the
tolerant `kotoba-lang/scene` accessors (`mget` / `num` / `vec3` / `kw-key` /
`root-map`) the same way games parse `scene.edn`: missing keys fall back to
`0`/`0.0`, namespaced keywords match on `ns/name`, ints coerce to floats.

Per ADR-0038, hot fullscreen passes / GPU-uniform packing stay in
`kotoba-lang/postfx`, untouched. A post-fx preset is init-time CONFIG, read
once when the pipeline is assembled at boot, so it is safe to move to EDN.
The compiled-in `postfx/{nintendo,retro,final-fantasy,baminiku-character}`
builders remain as the `builtin-preset` fallback and are parity-tested
against the shipped EDN (`postfx-edn` / `resources/postfx.edn`).

## Dependency relationships

- `kotoba-lang/scene` — tolerant EDN accessors (`mget`, `num`, `vec3`,
  `kw-key`, `root-map`).
- `kotoba-lang/postfx` — the real effect constructors (`bloom`, `outline`,
  …), `new-pipeline`/`add`, and the hardcoded preset builders used as the
  parity oracle.

`kami-postfx` (the engine crate the original Rust paired with) itself was
already restored earlier in this migration wave as `kotoba-lang/postfx`; this
crate is its EDN authoring surface, exactly mirroring the original's
`kami-postfx = { path = "../kami-postfx" }` / `kami-scene = { path =
"../kami-scene" }` Cargo dependencies.

### Adaptation note (EffectSpec / effect_id)

The original Rust needed a hand-rolled `EffectSpec` `PartialEq` mirror of
`PostEffect` (which derives only `Debug, Clone`) purely so two pipelines
could be compared field-by-field without touching `kami-postfx`. In
`kotoba-lang/postfx`, effects are already plain Clojure maps (`{:type :bloom
:threshold .. }`), which compare structurally via `=` for free, so no
projection type is needed here. `effect-id` and `pipeline-specs` are kept as
thin, 1:1-named helpers for API/test parity with the Rust surface, but are
trivial (`effect-id` reads `:type`; `pipeline-specs` reads `:effects`).

## Size / tests

- `src/postfx_scene.cljk`: 386 lines.
- `test/postfx_scene_test.cljk`: 174 lines — 15 tests / 145 assertions, 0
  failures. Ports every original Rust `#[test]` from
  `kami-postfx-scene/src/lib.rs`'s `mod tests` (9 tests) and
  `kami-postfx-scene/tests/postfx_parity.rs` (4 tests) 1:1, plus a
  render-IR-chain coverage test and a namespace-loads smoke test.

Run tests:

```sh
kbb -M:test
```
