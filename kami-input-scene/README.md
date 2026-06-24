# kami-input-scene

Data-tier crate that makes `kami-input`'s **default input-binding maps** data-driven: the
named `InputMap` device→action tables live as **canonical EDN** here, loaded into the real
engine struct at startup.

It is the input sibling of [`kami-vehicle-scene`](../kami-vehicle-scene) /
[`kami-character-scene`](../kami-character-scene) — a thin `from_edn` layer over
[`kami-scene`](../kami-scene)'s tolerant EDN accessors. This realises the **ADR-0040**
"input: device→action maps as EDN" line (`90-docs/adr/0040-everything-describable-is-edn-datomic.md`).

## Why (ADR-0038 / ADR-0040)

The architecture rule: **hot per-frame work stays native Rust; only init-time CONFIG/DATA
moves to EDN.** `kami-input`'s `InputMap::resolve` (first-match key→action lookup), gesture
detection, and `FocusManager` routing run untouched in Rust. But the *binding table* itself —
which physical key codes map to which abstract actions — is read **once** when an app sets up
its input handler. That is config, so it moves out of the `default_fps()` / `default_graph()`
`fn` bodies into EDN that an author (or a fork, or Datomic) can edit without recompiling.

This crate is **additive**: `kami-input`'s compiled-in `InputMap::{default_fps,default_graph}()`
builders are *not* deleted. They remain the `builtin_input_map()` fallback **and** the parity
oracle — every binding in the shipped EDN is asserted equal (in order) to the hardcoded Rust it
mirrors, so the EDN is the source of truth while behaviour is provably unchanged.

```
kami-input  (hot resolve/gesture/focus, hardcoded preset fns = oracle/fallback)   ← unchanged
      ▲ path dep
kami-input-scene  (this crate: data/input.edn  +  from_edn loaders)               ← additive
```

## Data (the source of truth)

| File | Table | Mirrors |
|---|---|---|
| [`data/input.edn`](data/input.edn) | `:input/maps` (2 maps: `:fps` / `:graph`) | `InputMap::{default_fps,default_graph}()` |

Each map is an **ordered** vector of `[key-code action-keyword]` pairs — order matters, because
`InputMap::resolve` is first-match. `key-code` is the W3C `KeyboardEvent.code` string (`"KeyW"`,
`"ArrowUp"`, `"Escape"`); `action` is an `Action` keyword id (hyphenated: `:move-up` / `:zoom-in`
/ `:pause` / …). Both shipped maps have 12 bindings.

## API

```rust
use kami_input_scene as scene;

// Both default maps, straight from the shipped input.edn.
let maps = scene::shipped_input_maps()?;          // BTreeMap<String, InputMap>
let fps  = scene::shipped_input_map("fps")?;      // kami_input::InputMap

// Or parse arbitrary EDN (a fork, a Datomic snapshot, an author's rebind):
let custom = scene::input_map_from_edn(my_edn, "graph")?;
let table  = scene::input_maps_from_edn(my_edn)?;
```

Loaders: `input_maps_from_edn` / `input_map_from_edn` / `input_map_from_pairs` (one ordered
pair-vector → `InputMap`) / `shipped_input_maps` / `shipped_input_map`. `builtin_input_map(name)`
is the Rust-fn oracle/fallback. `action_from_id` / `id_from_action` translate the action
keyword. `INPUT_EDN` is the shipped string (`include_str!`) for baking into a wasm bundle.
`ALL_MAP_NAMES` is the iteration order. `Error` is `NotAMap` / `NoTable` / `MapNotFound` /
`UnknownAction`.

`InputMap` derives no `PartialEq` (it is just `pub bindings: Vec<(String, Action)>`), so parity
compares the `bindings` vecs element-by-element; `Action` derives `PartialEq, Eq, Copy`.

## Tests (parity = the correctness contract)

```bash
cargo test-native -p kami-input-scene
```

- `tests/input_parity.rs` — for each of the 2 maps, **every** `(key-code, action)` binding
  loaded from `input.edn`, **in order**, `==` the value from the real `InputMap::<X>()` fn; plus
  `resolve` first-match parity, binding counts (12 each), unknown action / unknown map / non-map
  root / missing table → error.

If any hardcoded binding drifts from the EDN, these fail — that is the point: the EDN is the
authoritative copy, pinned to the engine's behaviour.

## Note on the build target

The workspace `.cargo` config defaults `build.target` to `wasm32-unknown-unknown`. Run the
native test suite via the workspace alias: `cargo test-native -p kami-input-scene`.

## License

Apache-2.0 / MIT (workspace inherited).
