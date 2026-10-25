# kotoba-lang/kami-vegetation-scene

Zero-dep portable `.cljc` — restored from the legacy `kami-engine/kami-vegetation-scene` Rust
crate (deleted in kotoba-lang/kami-engine PR #82, "Remove Rust workspace from kami-engine") as
part of the **clj-wgsl migration** (ADR-2607010930, `com-junkawasaki/root`).

## Status

Restored. Ports the full original crate (`src/lib.rs`, 445 lines, recovered from commit
`a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa`) to zero-dep portable CLJC:

- `src/vegetation_scene.cljk` (namespace `vegetation-scene`) — the EDN authoring surface for
  `kotoba-lang/vegetation` taxonomic-profile config. Parses `:vegetation/profiles` EDN via
  `kotoba-lang/scene`'s tolerant accessors (`scene/mget` / `scene/num` / `scene/vec3` /
  `scene/root-map` / `scene/kw-key`) and merges each profile's fields onto
  `vegetation.taxonomy/moss` (the CLJC mirror of the original
  `kami_vegetation::taxonomy::TaxonomicProfile::default`-equivalent shape) — any key a profile
  omits keeps the `vegetation.taxonomy` default, never a hardcoded/transcribed value.
- `resources/vegetation.edn` — the canonical preset table (`grass` / `fern` / `palm` / `conifer`
  / `bush` / `cactus` / `moss`), byte-identical to the original crate's `data/vegetation.edn`.
  Also embedded as a literal string constant (`vegetation-scene/vegetation-edn`) directly in the
  source so the namespace loads identically on the JVM and in ClojureScript without
  resource-loading/`slurp` portability concerns.

## Dependency relationship

This crate is the data-tier counterpart of `kami-vehicle-scene` / `kami-atmosphere-scene` /
`kami-terrain-scene` for the vegetation taxonomy system:

- **`kotoba-lang/scene`** — tolerant EDN accessor primitives (`kw-key`/`mget`/`num`/`vec3`/
  `root-map`): missing keys fall back to defaults, namespaced keywords match on `ns/name`,
  numbers coerce int<->float.
- **`kotoba-lang/vegetation`** — the CLJC port of the native `kami-vegetation` engine (25 tests /
  180,986 assertions): provides the `taxonomy/{grass,fern,palm,conifer,bush,cactus,moss}` preset
  builders this crate's `builtin-profile` uses as the fallback/parity oracle, and
  `taxonomy/moss` as the merge base.

`vegetation-scene` depends on both; it does not modify or vendor either.

A `ProfileSpec` (this crate) and a `vegetation.taxonomy` TaxonomicProfile map are the exact same
flat map shape in CLJC — unlike the original Rust, which needed a distinct `ProfileSpec` struct
and a `common_name` -> `&'static str` resolution step (`TaxonomicProfile.common_name` required a
static lifetime; CLJC strings are always owned) — so `to-taxonomic-profile` is the identity
function here, kept only for API parity with the original.

## Public API

- `vegetation-edn` — the shipped EDN source (literal string constant).
- `canopy-from-id` / `id-from-canopy`, `division-from-id`, `habit-from-id`,
  `arrangement-from-id`, `leaf-shape-from-id` — map hyphenated EDN keyword ids to
  `vegetation.taxonomy` keywords (mirroring `kami-vegetation`'s private `parse_*` fns, keeping
  the engine namespace untouched).
- `profile-spec-defaults` — the default ProfileSpec, derived from `vegetation.taxonomy/moss`.
- `profile-spec-from-map` — build a ProfileSpec from one profile's parsed EDN map, merging onto
  `profile-spec-defaults`.
- `to-taxonomic-profile` — identity (see above); kept for API parity.
- `profiles-from-edn` / `profile-from-edn` — parse profiles (or one profile) from arbitrary EDN
  source, throwing `ex-info` (`:vegetation-scene/error` of `:not-a-map` / `:no-profiles` /
  `:profile-not-found`) on failure.
- `shipped-profiles` / `shipped-profile` — convenience loaders against the shipped
  `vegetation-edn`.
- `builtin-profile` — the compiled-in fallback/parity oracle
  (`vegetation.taxonomy/{grass,fern,palm,conifer,bush,cactus,moss}`), by name.
- `all-profile-names` — `["grass" "fern" "palm" "conifer" "bush" "cactus" "moss"]`.

All 9 original Rust `#[test]`s ported 1:1 to `test/vegetation_scene_test.cljk` (+1 smoke test) —
10 tests / 29 assertions, 0 failures.

## Develop

```bash
kbb -M:test
```
