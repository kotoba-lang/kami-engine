# kami-skeleton-scene

Zero-dependency portable CLJC restoration of the legacy `kami-skeleton-scene`
Rust crate (`kotoba-lang/kami-engine`, deleted in PR #82 "Remove Rust workspace
from kami-engine"), per ADR-2607010930.

It is the data/import-tier counterpart of [`kotoba-lang/skeleton`](https://github.com/kotoba-lang/skeleton)
for the skeletal-animation system — the skeleton sibling of `kami-vehicle-scene`
/ `kami-input-scene`:

- **EDN authoring surface** — `:skeleton/humanoid-constraints` EDN (an
  *ordered* table of `[joint-name {:min-deg [..] :max-deg [..]}]` pairs, authored
  in degrees) → the real `skeleton/joint-constraint` engine data, re-using the
  tolerant `kami-scene`/`scene` EDN accessors (`kw-key`/`mget`/`num`/`vec3`/
  `root-map`) the same way games parse `scene.edn`.
- **EDN-authored animation clips** — a dance/idle/gesture clip authored as
  plain EDN → `skeleton/animation-clip`, bone names retargeting via a
  caller-supplied name→index map.
- **MMD `.vmd`/`.pmx` import** — binary MikuMikuDance motion (`.vmd`) and mesh
  (`.pmx`) formats parsed into `skeleton/animation-clip` and a renderable
  `pmx-model` (+ `skeleton/skeleton` rig), so an MMD asset can play on any
  humanoid rig headless.

`kotoba-lang/skeleton` (hot per-frame evaluate/IK/clamp) stays untouched — the
EDN/import dependency lives only here.

## Modules restored

| Namespace | Source (deleted PR #82) | Lines | Pairs with |
|---|---|---|---|
| `skeleton-scene` | `src/lib.rs` | 232 | `skeleton/joint-constraint`, `skeleton/default-humanoid-constraints` |
| `skeleton-scene.clip` | `src/clip.rs` | 89 | `skeleton/animation-clip`, `skeleton/bone-track`, `skeleton/keyframe` |
| `skeleton-scene.vmd` | `src/vmd.rs` | 111 | `skeleton/animation-clip` (MMD `.vmd` import) |
| `skeleton-scene.pmx` | `src/pmx.rs` | 315 | `skeleton/skeleton`, `skeleton/bone` (MMD `.pmx` import) |

## Dependency relationships

- [`kotoba-lang/scene`](https://github.com/kotoba-lang/scene) — tolerant EDN
  accessors (`kw-key`/`mget`/`num`/`vec3`/`root-map`); used by `skeleton-scene`
  and `skeleton-scene.clip`.
- [`kotoba-lang/skeleton`](https://github.com/kotoba-lang/skeleton) — the
  engine data shapes this crate builds (`joint-constraint`,
  `default-humanoid-constraints`, `animation-clip`/`bone-track`/`keyframe`,
  `skeleton`/`bone`) and the `skeleton.math` Vec3/Quat/Mat4 primitives used by
  `skeleton-scene.pmx`'s PMX→skeleton conversion and skinning math.

Per ADR-0038: hot skeletal evaluation (`skeleton/evaluate`,
`evaluate-constrained`, `solve-ik-ccd`, `constraint-clamp`) stays pure CLJC in
`kotoba-lang/skeleton`, untouched by this crate. The humanoid constraint table
is init-time CONFIG (read once when an app builds its constraint index), so
it is safe to author as EDN here; the compiled-in
`skeleton/default-humanoid-constraints` remains the
`builtin-humanoid-constraints` fallback and is parity-tested against the
shipped EDN (`humanoid-edn`, byte-identical to `resources/humanoid.edn`).

## Notes on the port

- **EDN embedding**: the original's `include_str!("../data/humanoid.edn")` is
  ported as an inlined string constant (`humanoid-edn`), the CLJC-portable
  equivalent — works identically under Clojure and ClojureScript with no
  filesystem/classpath resource loading. The byte-identical source also ships
  at `resources/humanoid.edn` for provenance/tooling.
- **Byte cursor (`.pmx`)**: the original's mutable `struct Cur` (byte offset +
  the PMX per-section index-byte-size globals) is ported as an atom-backed
  cursor map, preserving the original's "a field only advances once its bytes
  are fully read" semantics exactly (`take!` doesn't mutate the position on a
  bounds failure). The original's `&mut PmxModel` incremental push (so a
  truncated v2 section still returns whatever earlier sections parsed) is
  ported via an atom-backed accumulator in `parse-rest!`.
- **Shift-JIS / UTF-16LE**: the original used the `encoding_rs` crate for `.vmd`
  bone-name decoding. This port uses platform charset support instead — the
  JVM `Shift_JIS`/`UTF-16LE` `java.nio.charset` names on Clojure, and the
  WHATWG `shift_jis`/`utf-16le` legacy `TextDecoder` labels on ClojureScript —
  keeping the crate additional-dependency-free on both platforms.
- **f32 vs. double**: the original's `[f32;3]` engine values and bit-for-bit
  `f32` parity tests don't apply verbatim in CLJC (there is no `f32` type in
  Clojure's default numeric tower). The constraint-table parity test instead
  compares two independently-computed CLJC values (EDN-loaded vs. the
  `skeleton/default-humanoid-constraints` oracle) using the *same*
  `(/ Math/PI 180.0)` conversion expression, so `=` equality still gives a
  meaningful, bit-for-bit-in-double parity contract.

## Tests

Every applicable original Rust `#[test]` is ported 1:1, across `src/lib.rs`
(`#[cfg(test)] mod tests`), `tests/humanoid_parity.rs`, `src/clip.rs`,
`src/vmd.rs`, and `src/pmx.rs`, plus a namespace-loads smoke test:

```
kbb -M:test
```

**19 tests / 213 assertions, 0 failures, 0 errors.**

## License

Apache-2.0 / MIT (workspace inherited from the original kami-engine crate).
