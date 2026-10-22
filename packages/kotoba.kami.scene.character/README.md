# kami-character-scene (CLJC)

EDN authoring surface for [`kotoba-lang/character`](https://github.com/kotoba-lang/character)'s
hair-style presets, MetaHuman FACS face-rig, and default animation blueprint.

Restored from the legacy `kami-engine/kami-character-scene` Rust crate
(`kotoba-lang/kami-engine`, deleted in PR #82 "Remove Rust workspace") as
zero-dependency portable `.cljc`, per ADR-2607010930
(`com-junkawasaki/root`). Rust source recovered at commit
`a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa`.

## Modules

| Namespace | Lines | Ported from | Pairs with |
|---|---|---|---|
| `character-scene` | 254 | `src/lib.rs` | self-contained (see below) |
| `character-scene.face-rig` | 142 | `src/face_rig.rs` | `character.control-rig/metahuman-face-rig` |
| `character-scene.anim-blueprint` | 219 | `src/anim_blueprint.rs` | `character.anim-blueprint/metahuman-default` |

All three depend on [`kotoba-lang/scene`](https://github.com/kotoba-lang/scene)
(`scene/kw-key` `scene/mget` `scene/num` `scene/vec3` `scene/root-map`) for
tolerant EDN accessors — the same way native `kami-clj` players parse
`scene.edn`: missing keys fall back to defaults, namespaced keywords match
on `ns/name`, numbers coerce int↔float.

### `character-scene` (hair-style presets)

Turns canonical `:character/hair-styles` EDN (a table of named hair-style
parameter maps, 14 fields each: `style`/`length`/`density`/`volume`/`curl`/
`part-side`/`bangs-length`/`bangs-width`/`color`/`highlight-color`/
`highlight-ratio`/`root-darken`/`head-radius`/`head-center-y`) into plain
CLJC maps, mirroring the original engine's `kami_character::HairStyle`
struct.

**Deviation from the original crate**: the original Rust merged EDN fields
onto `kami_character::HairStyle::default()` and parity-tested against
`HairStyle::{blonde_long,dark_short,red_wavy,brown_curly,afro}()`. When
`kotoba-lang/character` was restored earlier in this migration wave, its
`character.hair` / `character.params` used a simpler preset-keyword model
instead of porting that 14-field struct, so there is no `character`-side
default to merge onto anymore. This namespace therefore carries its own
self-contained `default-hair-style` and `builtin-hair-styles` table (ported
1:1 from the original crate's resolved values — exactly what the shipped
`hair.edn` already encodes) as the local compiled-in oracle, preserving the
original parity-test *structure* without a cross-crate dependency that no
longer exists.

### `character-scene.face-rig`

Turns a `:character/face-rig` → `:au-bones` EDN table (19 rows, one per
MetaHuman FACS Action-Unit → skeleton-bone mapping) into a
`character.control-rig`-shaped rig map (control-input → multiply →
rotation-axis node triple per row), replaying the same node-index scheme
as `character.control-rig/metahuman-face-rig` so the two are node-for-node
`=`. Rig **evaluation** stays in `character.control-rig/evaluate`; only the
init-time AU→bone table description lives here.

### `character-scene.anim-blueprint`

Turns a `:character/anim-blueprint` EDN table (11 parameters, 2 layers —
`body` locomotion + `face` FACS — with states, blend-space-1d entries,
transitions and conditions, plus 1 blend profile) into a
`character.anim-blueprint`-shaped blueprint map, component-for-component
`=` `character.anim-blueprint/metahuman-default`. `:param-type` is derived
(`:bool` when the parameter name starts with `is_`, else `:float`). State
machine **evaluation** (`update`/transitions/blend) stays in
`character.anim-blueprint`; only the init-time description lives here.

## Data

`resources/character_scene/{hair,face_rig,anim_blueprint}.edn` — copies of
the shipped EDN fixtures (also embedded as string constants in the
corresponding `.cljc` namespaces, `include_str!`-style, for zero-dependency
portability).

## Tests

24 tests / 89 assertions, 0 failures, 0 errors (`kbb -M:test`).

Every applicable original Rust `#[test]` (from `src/lib.rs`,
`src/face_rig.rs`, `src/anim_blueprint.rs`, `tests/face_rig_parity.rs`,
`tests/anim_blueprint_parity.rs`) is ported 1:1, plus a namespace-loads
smoke test for the root namespace. `hair_parity.rs`'s tests are ported
against the local `builtin-hair-styles` oracle (see the Deviation note
above) rather than against `kami-character`, since the corresponding Rust
`HairStyle` struct no longer exists in the restored `character` CLJC.

Face-rig and anim-blueprint parity tests call the real
`kotoba-lang/character` functions (`character.control-rig/metahuman-face-rig`,
`character.anim-blueprint/metahuman-default`) as the oracle — not
transcribed — so drift between the EDN and the engine default fails loudly.

## Dependencies

- [`kotoba-lang/scene`](https://github.com/kotoba-lang/scene) — tolerant EDN accessors.
- [`kotoba-lang/character`](https://github.com/kotoba-lang/character) — read-only parity oracle for `face-rig` and `anim-blueprint` (untouched by this crate).
