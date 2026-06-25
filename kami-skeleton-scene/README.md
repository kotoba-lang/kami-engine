# kami-skeleton-scene

Data-tier crate that makes `kami-skeleton`'s **default humanoid joint-constraint table**
data-driven: the per-bone anatomical Euler-angle rotation limits live as **canonical EDN**
here, loaded into the real engine struct at startup.

It is the skeleton sibling of [`kami-vehicle-scene`](../kami-vehicle-scene) /
[`kami-input-scene`](../kami-input-scene) — a thin `from_edn` layer over
[`kami-scene`](../kami-scene)'s tolerant EDN accessors. This realises the **ADR-0040**
"animation: ... constraints / retarget maps as EDN" line
(`90-docs/adr/0040-everything-describable-is-edn-datomic.md`).

## Why (ADR-0038 / ADR-0040)

The architecture rule: **hot per-frame work stays native Rust; only init-time CONFIG/DATA
moves to EDN.** `kami-skeleton`'s `evaluate` / `evaluate_constrained` / `solve_ik_ccd` and
`JointConstraint::clamp` run untouched in Rust. But the *constraint table* itself — which
bone has which Euler-angle limit — is read **once** when an app builds its constraint index
(`Skeleton::build_humanoid_constraints`). That is config, so it moves out of the
`default_humanoid_constraints()` `fn` body into EDN that an author (or a fork, or Datomic)
can edit without recompiling.

This crate is **additive**: `kami-skeleton`'s compiled-in `default_humanoid_constraints()`
table is *not* deleted. It remains the `builtin_humanoid_constraints()` fallback **and** the
parity oracle — every limit in the shipped EDN is asserted **bit-for-bit f32** equal (in
order, every joint) to the hardcoded Rust it mirrors, so the EDN is the source of truth while
behaviour is provably unchanged.

```
kami-skeleton  (hot evaluate/IK/clamp, hardcoded default table = oracle/fallback)   ← unchanged
      ▲ path dep
kami-skeleton-scene  (this crate: data/humanoid.edn  +  from_edn loaders)           ← additive
```

## Data (the source of truth)

| File | Table | Mirrors |
|---|---|---|
| [`data/humanoid.edn`](data/humanoid.edn) | `:skeleton/humanoid-constraints` (13 joints) | `default_humanoid_constraints()` |

The table is an **ordered** vector of `[joint-name {:min-deg [x y z] :max-deg [x y z]}]`
pairs — order matters (it mirrors the declaration order: `head` → `neck` → `spine` → `chest`
→ `hips` → `leftUpperArm` → `rightUpperArm` → `leftLowerArm` → `rightLowerArm` →
`leftUpperLeg` → `rightUpperLeg` → `leftLowerLeg` → `rightLowerLeg`). Joint names round-trip
exactly (camelCase preserved, e.g. `"leftUpperArm"`).

### Degrees in, radians out

Limits are **authored in degrees** (readable, and matching the Rust source which writes e.g.
`60.0 * d`). The loader converts each degree value to **radians at load** by multiplying by
`std::f32::consts::PI / 180.0` — the *identical* `f32` factor and the *same* `<deg> * d`
arithmetic the Rust source uses — so every loaded angle is **bit-for-bit equal** to
`default_humanoid_constraints()`. The axis order is `[x, y, z]`.

## API

```rust
use kami_skeleton_scene as scene;

// The default table, straight from the shipped humanoid.edn (radians).
let table = scene::shipped_humanoid_constraints()?;   // Vec<(String, JointConstraint)>

// Or parse arbitrary EDN (a fork, a Datomic snapshot, an author's edit):
let custom = scene::humanoid_constraints_from_edn(my_edn)?;
```

Loaders: `humanoid_constraints_from_edn` (EDN → ordered `Vec<(String, JointConstraint)>`) /
`constraint_from_map` (one `{:min-deg .. :max-deg ..}` map → `JointConstraint`) /
`shipped_humanoid_constraints`. `builtin_humanoid_constraints()` is the Rust-fn oracle/fallback.
`HUMANOID_EDN` is the shipped string (`include_str!`) for baking into a wasm bundle.
`DEG_TO_RAD` is the conversion factor. `ALL_JOINT_NAMES` is the joint iteration order.
`limits_eq` compares two `[f32;3]` arrays exactly. `Error` is `NotAMap` / `NoTable`.

`JointConstraint` derives no `PartialEq` (it is just `pub min: [f32;3]`, `pub max: [f32;3]`),
so parity compares the `[f32;3]` arrays directly with exact `f32` `==`.

## Tests (parity = the correctness contract)

```bash
cargo test-native -p kami-skeleton-scene
```

- `tests/humanoid_parity.rs` — the table loaded from `humanoid.edn` `==` the real
  `default_humanoid_constraints()`: same joint count (13), same names in order, every min/max
  `[f32;3]` **exactly** (bit-for-bit f32) equal; plus the exact-f32 degree→radian reproduction
  of the `0.0` and `145.0 * d` boundary cases; plus tolerant-parse errors (non-map root →
  error, missing/ non-vector table → error).

If any hardcoded limit drifts from the EDN, these fail — that is the point: the EDN is the
authoritative copy, pinned to the engine's behaviour.

## Note on the build target

The workspace `.cargo` config defaults `build.target` to `wasm32-unknown-unknown`. Run the
native test suite via the workspace alias: `cargo test-native -p kami-skeleton-scene`.

## License

Apache-2.0 / MIT (workspace inherited).
