# Testing the CLJ/EDN game stack

How the kami-clj game layer (ADR-0035/0036/0037/0038) is tested, and how to run the
gates. See `CLAUDE.md` → "CLJ/EDN Game Layer" for the architecture.

## Setup caveats (read first)

- **Sibling `kotoba` checkout required.** `kami-engine-clj` and `kami-scene` path-depend
  on `../../kotoba/crates/kotoba-edn` (the shared EDN reader). The CLJ crates do **not**
  build from a lone `kami-engine` checkout — you need the `kotoba` repo at `../../kotoba`.
  (This is also why CI isn't wired yet: a workflow must check out `kotoba` into the sibling
  path before `cargo test`.)
- **Org config forces `wasm32`.** A parent `.cargo/config.toml` sets
  `build.target = wasm32-unknown-unknown` for the whole tree, so host-side crates are built
  via the repo's aliases: **`cargo test-native`** / **`cargo check-native`** (they add
  `--target aarch64-apple-darwin`). Use them, not bare `cargo test`.

## The gates

| Command | What it checks |
|---|---|
| `cargo test-native -p kami-engine-clj` | compiler: valid-WASM emission + **rejects malformed source with `Err`, never panics** |
| `cargo test-native -p kami-script-runtime` | host: language semantics, GAME_PRELUDE (timer/vec3), host-fn robustness on bad/edge inputs, golden-frame determinism, `on-event` lifecycle (wasmtime backend) |
| `cargo test-native -p kami-clj-host` | render-IR decoder: fixture round-trip + **rejects malformed bytes** (bad magic/version, truncation, out-of-bounds) |
| `cargo test-native -p kami-scene` | scene.edn EDN accessors: keyword/namespace match, int↔float coercion, defaults, malformed → `None` |
| **`scripts/test-script-backends.sh`** | the **dual-backend gate** — runs the kami-script-runtime suite under BOTH `wasmtime` (JIT) and `wasmi` (no-JIT, the iOS/PS5/Switch path); **both must pass**. Override the host triple with `HOST_TARGET=…`. |
| **`scripts/test-bb-pipeline.sh`** | the `bb kami` pipeline end-to-end: `bb spec` emits the expected matrix EDN per target, `bb compile` turns `logic.clj` → a real `game.wasm`. |

## The two load-bearing invariants

1. **Cross-backend determinism.** The all-i64 guest ABI + host-seeded RNG mean wasmtime and
   wasmi produce **bit-identical** world state. `golden_frame_determinism` and
   `golden_frame_with_despawn_determinism` pin a world-state hash GOLDEN that *both* backends
   must hit — the foundation for lockstep co-op / replay. (Writing the first of these caught a
   real system-ordering bug; see ADR-0037 Phase 1.)
2. **No-JIT parity.** Every behavioral test runs under `wasmi` too, so the console/iOS host
   path (no runtime codegen) is continuously exercised without a device.

## Quick run

```bash
# whole CLJ stack, native host
cargo test-native -p kami-engine-clj -p kami-script-runtime -p kami-clj-host -p kami-scene

# the no-JIT path must agree with the JIT path
scripts/test-script-backends.sh        # both backends, fails on any divergence

# the build/ship pipeline still wires up
scripts/test-bb-pipeline.sh
```

The GPU players (`kami-clj-play`, `kami-clj-play3d`) are exercised by running them
(`cargo run -p kami-clj-play[3d]`) — they need a window/GPU, so they aren't in the unit gates;
their data-loading path is hardened to fail with a clear message + exit code (not a panic) on
missing/malformed `scene.edn`.
