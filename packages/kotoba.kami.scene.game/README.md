# kotoba-lang/kami-game-scene

Zero-dep portable `.cljc` — restored from the legacy `kami-engine/kami-game-scene` Rust crate
(deleted in kotoba-lang/kami-engine PR #82, "Remove Rust workspace from kami-engine", recovered
at commit `a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa`) as part of the **clj-wgsl migration**
(ADR-2607010930, `com-junkawasaki/root`).

## Status

Restored. Ports the full original crate (`src/lib.rs` + 6 sibling modules, 1985 Rust lines
across `src/` + `tests/`) to zero-dep portable CLJC across 7 namespaces:

| Namespace                    | Lines | Restored from | Pairs with (`kotoba-lang/game`)               |
|-------------------------------|------:|----------------|------------------------------------------------|
| `game-scene`                  |   346 | `src/lib.rs`   | `game.animation` (ANIMATION-STATE presets)      |
| `game-scene.catalog`           |   143 | `src/catalog.rs` | `game.island-gen/godot-game-catalog`          |
| `game-scene.brainrot`          |   175 | `src/brainrot.rs` | `game.island-gen/brainrot-evolution-chains` |
| `game-scene.character`         |   130 | `src/character.rs` | `game.island-gen/brainrot-characters`      |
| `game-scene.item-catalog`      |   203 | `src/item_catalog.rs` | `game.sabiotoshi/default-item-catalog`  |
| `game-scene.battle-royale`     |   223 | `src/battle_royale.rs` | `game.battle-royale` (storm/consumables/weapons) |
| `game-scene.pokoa`             |   206 | `src/pokoa.rs` | `game.pokoa/pokoa-dex`, `game.pokoa/pokoa-items` |

Each namespace turns a canonical, hand-authorable EDN table (shipped in `resources/game_scene/`
and also embedded as a literal string constant, for JVM/CLJS parity) into the real
`kotoba-lang/game` data shape, and is parity-tested `=` against `kotoba-lang/game`'s compiled-in
oracle (`game.animation/skibidi-idle` et al, `game.island-gen/godot-game-catalog`,
`game.island-gen/brainrot-evolution-chains`, `game.island-gen/brainrot-characters`,
`game.sabiotoshi/default-item-catalog`, `game.battle-royale/{default-storm-phases,
consumable-pool,weapon-pool}`, `game.pokoa/{pokoa-dex,pokoa-items}`).

## Why this is safe (ADR-0038 / ADR-0046)

The hot per-frame/per-battle logic (animation easing, storm shrink, ballistics, rust-restoration
scoring) stays in `kotoba-lang/game`. Everything ported here is **init-time CONFIG/DATA** — read
once to seed a `kotoba-lang/game` struct/list — so moving it to EDN is behaviour-preserving. The
compiled-in factories/tables in `kotoba-lang/game` remain the fallback and the parity oracle;
`kotoba-lang/game` itself is never modified by this crate.

## Simplification vs the original Rust

The original Rust types (`AnimationClip`, `GameDef`, `BrainrotEvolution`/`EvolutionStage`,
`RestorableItem`/`RustZone3D`/`DisassemblyStep`) derived no `PartialEq` (some also held
`glam::Vec3`), so the crate carried hand-written `PartialEq` "Spec" mirror types (`ClipSpec`,
`GameDefSpec`, `EvolutionChainSpec`, `ItemSpec`, ...) purely to make parity assertions
compile. Clojure maps/keywords/vectors have structural equality for free, so this port skips
those mirror types entirely: the EDN-loaded values are already shaped exactly like
`kotoba-lang/game`'s data, and parity is asserted with plain `=`. The `*-spec-from-map`
/ `spec-to-*` function names are kept (some as `identity`) purely for 1:1 API parity with the
original Rust — see each namespace's docstring for details.

## Dependency relationship

- **`kotoba-lang/scene`** — tolerant EDN accessor primitives (`scene/kw-key` / `scene/mget` /
  `scene/num` / `scene/vec3` / `scene/root-map`): missing keys fall back to defaults,
  namespaced keywords match on `ns/name`, numbers coerce int<->float.
- **`kotoba-lang/game`** — the 30-module `game` crate (the CLJC port of the native `kami-game`
  engine), restored earlier in this migration. Specifically: `game.animation`,
  `game.island-gen`, `game.sabiotoshi`, `game.pokoa`, `game.battle-royale`.

`game-scene` depends on both; it does not modify or vendor either.

## Public API (per namespace)

- `game-scene` — `animations-edn`, `all-animation-names`, `clip-from-map`, `clip-id`,
  `head-bob-phase-from-id`/`head-bob-phase-id`, `animation-specs`, `builtin-animation`,
  `animations-from-edn`/`animation-from-edn`, `shipped-animations`/`shipped-animation`. Errors
  throw `ex-info` with `:game-scene/error` of `:not-a-map` / `:no-table` /
  `:animation-not-found` / `:unknown-clip`.
- `game-scene.catalog` — `genre-id`/`genre-from-id`, `game-def-from-map`, `spec-to-game-def`,
  `catalog-specs-from-edn`/`catalog-from-edn`, `builtin-catalog-specs`, `shipped-catalog`.
- `game-scene.brainrot` — `character-id`/`character-from-id`, `evolution-stage-from-map`,
  `evolution-chain-from-map`, `spec-to-chain`, `chain-specs-from-edn`/`chains-from-edn`,
  `builtin-chain-specs`, `shipped-chains`.
- `game-scene.character` — `appearance-from-map`, `character-from-map`, `characters-from-edn`,
  `builtin-characters`, `shipped-characters`.
- `game-scene.item-catalog` — `rust-type-id`/`rust-type-from-id`, `tool-kind-id`/
  `tool-kind-from-id`, `rust-zone-from-map`, `disassembly-step-from-map`, `item-from-map`,
  `spec-to-item`, `item-specs-from-edn`/`items-from-edn`, `builtin-item-specs`, `shipped-items`.
- `game-scene.battle-royale` — `rarity-id`/`rarity-from-id`, `consumable-type-id`/
  `consumable-type-from-id`, `weapon-type-id`/`weapon-type-from-id`, `storm-phase-from-map`,
  `consumable-from-map`, `weapon-from-map`, `storm-phases-from-edn`/`consumables-from-edn`/
  `weapons-from-edn`, `builtin-storm-phases`/`builtin-consumables`/`builtin-weapons`,
  `shipped-storm-phases`/`shipped-consumables`/`shipped-weapons`.
- `game-scene.pokoa` — `pokoa-type-id`, `species-from-map`, `dex-specs-from-edn`,
  `builtin-dex-specs`, `shipped-dex-specs`, `item-def-from-map`, `item-specs-from-edn`,
  `builtin-item-specs`, `shipped-item-specs`.

Every parsing entry point throws `ex-info` with a namespace-scoped `:error` key
(`:game-scene.<ns>/error`) of `:not-a-map` / `:no-table` (and `:no-catalog` for `catalog`) on
malformed EDN, mirroring the original Rust `Error`/`*Error` enums.

## Known upstream data quirk (documented, not "fixed")

`game.pokoa/pokoa-dex` (in the read-only `kotoba-lang/game` dependency) transcribed species #7
Ohiolet's description with a plain ASCII `--` where the canonical `pokoa_dex.edn` (byte-identical
to the deleted Rust crate's shipped data) has an em dash `—`. `test/game_scene/pokoa_test.cljk`
documents and normalizes this one field for its parity assertion; the library code
(`game-scene.pokoa/dex-specs-from-edn`) is unaffected and keeps the em dash verbatim, since
`kotoba-lang/game` is not modified by this crate.

## Tests

Every applicable original Rust `#[test]` (in `src/*.rs`) AND every test in the 7
`tests/*_parity.rs` integration-test files ported 1:1, plus a namespace-loads smoke test per
namespace — 56 tests / 595 assertions, 0 failures.

## Develop

```bash
kbb -M:test
```
