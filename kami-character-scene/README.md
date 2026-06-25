# kami-character-scene

Data-tier crate that makes `kami-character`'s **hair-style presets** data-driven: the five
named `HairStyle` parameter sets live as **canonical EDN** here, loaded into the real engine
struct at startup.

It is the character sibling of [`kami-vehicle-scene`](../kami-vehicle-scene) /
[`kami-postfx-scene`](../kami-postfx-scene) / [`kami-autodrive-scene`](../kami-autodrive-scene)
— a thin `from_edn` layer over [`kami-scene`](../kami-scene)'s tolerant EDN accessors.

## Why (ADR-0038)

The architecture rule: **hot geometry generation stays native Rust; only init-time
CONFIG/DATA moves to EDN.** `kami-character::hair_gen`'s strand / hair-card / polygon-mesh
generators (`generate_groom` / `generate_hair_cards` / `generate_hair_mesh`) run untouched in
Rust. But the *numbers* they run on — a `HairStyle`'s length / density / curl / colour /
head geometry — are read **once** when geometry is generated. Those are config, so they move
out of the preset `fn` bodies into EDN that an author (or a fork, or Datomic) can edit without
recompiling.

This crate is **additive**: `kami-character`'s compiled-in
`HairStyle::{blonde_long,dark_short,red_wavy,brown_curly,afro}()` builders are *not* deleted.
They remain the `builtin_hair_style()` fallback **and** the parity oracle — every value in the
shipped EDN is asserted equal to the hardcoded Rust it mirrors, so the EDN is the source of
truth while behaviour is provably unchanged.

```
kami-character  (hot hair_gen, hardcoded preset fns = oracle/fallback)   ← unchanged
      ▲ path dep
kami-character-scene  (this crate: data/hair.edn  +  from_edn loaders)   ← additive
```

## Data (the source of truth)

| File | Table | Mirrors |
|---|---|---|
| [`data/hair.edn`](data/hair.edn) | `:character/hair-styles` (5 presets: `:blonde-long` / `:dark-short` / `:red-wavy` / `:brown-curly` / `:afro`) | `HairStyle::{blonde_long,dark_short,red_wavy,brown_curly,afro}()` |

Each style is a map of the 14 `HairStyle` fields, hyphenated. `:style` is a `HairType`
keyword id (`:straight` / `:wavy` / `:curly` / `:afro` / `:braided`); colours are `[r g b]`.
Because the preset fns use `..Self::default()`, the EDN ships the **resolved** values (the
fields a builder omits are the `HairStyle::default()` values — `:head-radius 0.09` /
`:head-center-y 1.43` for all five).

## API

```rust
use kami_character_scene as scene;

// All five hair styles, straight from the shipped hair.edn.
let styles = scene::shipped_hair_styles()?;        // BTreeMap<String, HairStyle>
let afro   = scene::shipped_hair_style("afro")?;   // kami_character::HairStyle

// Or parse arbitrary EDN (a fork, a Datomic snapshot, an author's tweak):
let custom = scene::hair_style_from_edn(my_edn, "blonde-long")?;
let table  = scene::hair_styles_from_edn(my_edn)?;
```

Loaders: `hair_styles_from_edn` / `hair_style_from_edn` / `to_hair_style` (one map →
`HairStyle`) / `shipped_hair_styles` / `shipped_hair_style`. `builtin_hair_style(name)` is the
Rust-fn oracle/fallback. `hair_type_from_id` / `id_from_hair_type` translate the `:style`
keyword. `HairStyleSpec` is a `PartialEq` mirror of `HairStyle` (the engine struct derives no
`PartialEq`). `HAIR_EDN` is the shipped string (`include_str!`) for baking into a wasm bundle.
`ALL_HAIR_STYLE_NAMES` is the iteration order.

## Tests (parity = the correctness contract)

```bash
cargo test-native -p kami-character-scene
```

- `tests/hair_parity.rs` — for each of the 5 styles, **every** `HairStyle` field loaded from
  `hair.edn` `==` the value from the real `HairStyle::<X>()` fn (exact f32 equality; `HairType`
  exact), via the `HairStyleSpec` mirror; plus `blonde-long == HairStyle::default()`,
  missing-key → default, unknown style / non-map root / missing table → error.

If any hardcoded value drifts from the EDN, these fail — that is the point: the EDN is the
authoritative copy, pinned to the engine's behaviour.

## Note on the build target

The workspace `.cargo` config defaults `build.target` to `wasm32-unknown-unknown`. Run the
native test suite via the workspace alias: `cargo test-native -p kami-character-scene`.

## License

Apache-2.0 / MIT (workspace inherited).
