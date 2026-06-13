# ADR-0035: kami-clj — Clojure/EDN → WASM Game Scripting

**Date**: 2026-06-13  
**Status**: Accepted — Phase 1 + Phase 2 complete  
**Author**: kami-engine team

---

## Context

`kami-engine` game logic is currently written entirely in Rust.  This is optimal for
performance-critical systems (physics, rendering, pathfinding) but creates friction for:

- **Rapid iteration** on game behaviour (every change requires a recompile + WASM rebuild).
- **Content creator scripting** — designers should be able to express game rules without
  touching the Rust layer.
- **Hot-reload** — scripts should be swappable at runtime without restarting the engine.

`kotoba-clj` in the `kotoba` repo already proves the pattern: a Clojure/EDN-subset
compiler that emits real WebAssembly bytes via `wasm-encoder`, then runs those bytes on
`wasmtime`.  The compiled Clojure is not interpreted — it *is* WASM.

We want the same for game scripting: write game behaviour in a Lisp dialect, compile it
to WASM, and plug it into the live engine via a `wasmtime` host that exposes all engine
subsystems as WIT imports.

---

## Decision

Introduce two new crates:

| Crate | Role |
|---|---|
| `kami-clj` | Clojure/EDN-subset → WASM compiler (extended from kotoba-clj) |
| `kami-script-runtime` | wasmtime host binding engine imports to live `hecs::World` etc. |

Plus WIT definitions in `wit/kami-game/world.wit` that describe the contract between
compiled Clojure scripts and the engine.

---

## Architecture

```
.clj source
   │
   ▼
kami-clj compiler (Rust)
   │  Clojure/EDN subset → ast.rs → codegen.rs → .wasm core module
   ▼
kami:engine kami-game Component
   │  (via wit-component wrapping — same approach as kotoba-clj component.rs)
   ▼
kami-script-runtime (wasmtime host)
   │
   ├── kami:engine/scene   ──► hecs::World (spawn/despawn/get-position/set-position)
   ├── kami:engine/physics ──► rapier3d impulses / raycasts
   ├── kami:engine/input   ──► kami-input (key-down / axis / pointer)
   ├── kami:engine/render  ──► wgpu draw queue (draw-mesh / spawn-particle)
   ├── kami:engine/audio   ──► kami-audio spatial mixer
   └── kami:engine/time    ──► GameClock (delta-ms / elapsed-ms / tick-n)
```

---

## Clojure Subset

### Value model

Identical to `kotoba-clj`: all values on the WASM operand stack are `i64`.

| Guest type | Encoding |
|---|---|
| Integer / Boolean | i64 directly (booleans = 1/0) |
| String literal | `(offset << 32) \| len` packed i64 handle into linear memory |
| F32 (position, velocity) | IEEE-754 bit-pattern zero-extended to i64 |
| Entity ID | i64 (hecs `Entity::id()` cast) |

### Language features

Inherits all of kotoba-clj Phase-A+B (loops, vectors, maps, CBOR) plus:

```
top-level    def  defn  ns  defsystem
control      if  when  cond  let  do  loop/recur
arithmetic   + - * / mod  inc dec abs
comparison   = != < > <= >=  zero? pos? neg?
logic        and or not
strings      str-len  byte-at  bytes-alloc  byte-append!  bytes-finish
memory       alloc  load64  store64!  load32  store32!
f32          (f32 1.5)  or bare float literals  f32->bits  bits->f32
```

### New game builtins

```clojure
;; Entity lifecycle
(spawn-entity "player")      ;; → entity-id (i64)
(despawn-entity eid)

;; Transform (f32 bit-patterns)
(get-x eid)  (get-y eid)  (get-z eid)
(set-position! eid x y z)
(get-vx eid) (get-vy eid) (get-vz eid)
(set-velocity! eid vx vy vz)
(get-rx eid) (get-ry eid) (get-rz eid) (get-rw eid)  ;; quaternion
(set-rotation! eid rx ry rz rw)

;; Physics
(apply-impulse! eid ix iy iz)
(apply-force!   eid fx fy fz)
(raycast ox oy oz dx dy dz)  ;; → entity-id or 0

;; Input
(key-down?    "ArrowRight")  ;; → 1/0
(key-pressed? "Space")
(axis "horizontal")          ;; → f32 bits
(pointer-x)  (pointer-y)

;; Render
(draw-mesh!      "player-mesh" x y z)
(spawn-particle! "coin-burst"  x y z)
(draw-line!      x0 y0 z0 x1 y1 z1 0xFFFF00FF)

;; Audio
(play-sound    "coin")
(stop-sound    "coin")
(play-sound-at "footstep" x y z)

;; Time
(delta-ms)    ;; → i64, frame delta in ms
(elapsed-ms)  ;; → i64, engine uptime
(tick-n)      ;; → i64, fixed-step tick counter
```

### defsystem

`(defsystem name [dt] body…)` is sugar for `(defn name-tick [dt] body…)` exported as
`name-tick`.  The runtime discovers all `*-tick` exports and calls them each frame.

---

## GAME_PRELUDE

Prepended automatically by `compile_str_with_prelude`:

```clojure
;; F32 constants (bit patterns)
(def F32-ZERO  0)
(def F32-ONE   1065353216)   ;; 0x3F800000 = 1.0f
(def F32-HALF  1056964608)   ;; 0x3F000000 = 0.5f
(def F32-TWO   1073741824)   ;; 0x40000000 = 2.0f
(def F32-NEG-ONE -1082130432);; 0xBF800000 = -1.0f

;; Vec3 on the heap
(defn vec3-make [x y z] …)
(defn vec3-x [v] …)  (defn vec3-y [v] …)  (defn vec3-z [v] …)
(defn entity-pos [eid] (vec3-make (get-x eid) (get-y eid) (get-z eid)))

;; Fixed-step timer
(defn timer-make [period-ms] …)
(defn timer-tick! [t dt] …)
(defn timer-fired? [t] …)   ;; returns 1 + resets if period elapsed
```

---

## Example: Player Controller

```clojure
(ns player.controller)

(def speed (f32 5.0))
(def player 0)

(defn init []
  (def player (spawn-entity "player")))

(defsystem player-move [dt]
  (let [vx (if (key-down? "ArrowRight") speed
               (if (key-down? "ArrowLeft") (- speed) F32-ZERO))
        vy (f32 0.0)
        vz (if (key-down? "ArrowDown") speed
               (if (key-down? "ArrowUp") (- speed) F32-ZERO))]
    (set-velocity! player vx vy vz))
  (when (key-pressed? "Space")
    (apply-impulse! player F32-ZERO (f32 8.0) F32-ZERO)
    (play-sound "jump")))
```

---

## Implementation Phases

### Phase 1 ✅ — Scaffold

- [x] `wit/kami-game/world.wit` — WIT interfaces
- [x] `kami-clj/src/ast.rs` — full AST + new builtins + f32 support
- [x] `kami-clj/src/codegen.rs` — WASM emitter with f32/host-import lowering
- [x] `kami-clj/src/lib.rs` — GAME_PRELUDE + public API
- [x] `kami-script-runtime/src/lib.rs` — wasmtime host stubs + HostState
- [x] `90-docs/adr/0035-*` (this doc)

### Phase 2 ✅ — Host binding completions

- [x] Entity ID registry in `HostState` (`entity_registry: HashMap<String, hecs::Entity>` + `entity_by_id: HashMap<u32, hecs::Entity>`)
- [x] Real `set-position!` / `set-velocity!` / `set-rotation!` via `hecs::World::get::<&mut T>`
- [x] `key-down?` / `key-pressed?` / `axis` / `pointer-x/y` wired to `HostState` input snapshot
- [x] `play-sound` / `play-sound-at` / `stop-sound` queued to `audio_queue: Vec<(String,[f32;3])>`
- [x] `draw-mesh!` / `spawn-particle!` / `draw-line!` queued to `draw_queue: Vec<DrawCommand>`
- [x] `delta-ms` / `elapsed-ms` / `tick-n` bound to `HostState` time counters
- [x] **Codegen bug fix**: `Alloc` / `BytesAlloc` / `ByteAppend` / `BytesFinish` locals were declared `i64` but filled with `i32` values → added `I64ExtendI32U` before every `LocalSet/LocalTee` and `I32WrapI64` before every i32-context use
- [x] **Arity fix**: `DeltaMs` / `ElapsedMs` / `TickN` / `PointerX` / `PointerY` corrected to `n == 0` (were wrongly `n == 1`)
- [x] **Typed-func fix**: `call_init` / `call_tick` updated to `(i64,)` return to match all-i64 codegen
- [x] **8 integration tests** passing end-to-end: `empty_script_init_tick`, `spawn_entity_returns_nonzero_id`, `key_down_*`, `audio_queue_filled_by_play_sound`, `draw_queue_filled_by_draw_mesh`, `delta_ms_accessible_in_script`, `defsystem_tick_called_by_runtime`
- [x] **Browser demo**: `kami-web/clj-demo.html` + `kami-clj/examples/compile_demo.rs` — full Clojure→WASM→JS-host pipeline confirmed in browser

### Codegen ABI (settled in Phase 2)

All guest values live on the all-i64 WASM operand stack.  At host-import call sites
the codegen lowers types as follows:

| Param kind | Guest side | WASM boundary | Host (Rust/JS) |
|---|---|---|---|
| `I64` | i64 | `i64` | `i64` / BigInt |
| `I32` | i64 | `i32.wrap_i64` → `i32` | `i32` / Number |
| `F32` | i64 bit-pattern | `i32.wrap_i64` + `f32.reinterpret_i32` → `f32` | `f32` / Number |
| `StringHandle` | packed i64 | split into ptr `i32` + len `i32` | `(i32, i32)` |

Return values are lifted back to i64: `I64ExtendI32U` for `I32`, `I32ReinterpretF32 + I64ExtendI32U` for `F32`.

Memory-operation locals (`Alloc`, `BytesAlloc`, `ByteAppend`, `BytesFinish`) are declared as i64 (all locals use the same declaration group) and bridge to/from i32 heap arithmetic via `I64ExtendI32U` / `I32WrapI64` at every boundary.

### Phase 3 — Integration in game loop

- [ ] `KamiApp` builder gets `.with_scripts(Vec<Script>)` hook
- [ ] Scripts discovered by scanning a `scripts/` directory
- [ ] Auto-export: every `-tick` fn called each frame in order
- [ ] Hot-reload: file watcher recompiles + swaps the module at runtime

### Phase 4 — Language growth

- [ ] `(defentity name [components…] body…)` — entity template DSL
- [ ] Vector / map prelude (from kotoba-clj PRELUDE) for state bags
- [ ] CBOR prelude for structured event payloads (on-event)
- [ ] `(query-entities pred?)` — ECS query → entity list

---

## WIT world (summary)

```
world kami-game {
    import kami:engine/scene@1.0.0;    // spawn/despawn/get|set-position/velocity/rotation
    import kami:engine/physics@1.0.0;  // apply-impulse/force, raycast
    import kami:engine/input@1.0.0;    // key-down, axis, pointer-x/y
    import kami:engine/render@1.0.0;   // draw-mesh, spawn-particle, draw-line
    import kami:engine/audio@1.0.0;    // play/stop/play-at
    import kami:engine/time@1.0.0;     // delta-ms, elapsed-ms, tick-n

    export memory;
    export cabi_realloc: func(…) -> s32;
    export init:     func();
    export tick:     func(dt-ms: s64);
    export on-event: func(kind: s32, payload-ptr: s32, payload-len: s32) -> s32;
}
```

---

## Tradeoffs

| | Clojure WASM scripting | Pure Rust game crates |
|---|---|---|
| Iteration speed | Fast (no recompile) | Slow (Rust compile) |
| Performance | ~95% native (WASM JIT) | 100% native |
| Type safety | Runtime (clj subset) | Compile-time (Rust) |
| Hot-reload | Yes (swap module) | No |
| Debuggability | WASM trap msgs | Full Rust backtraces |
| Good for | Game logic, AI, UX | Physics, rendering, ECS |

**Recommendation**: use Clojure scripts for game behaviour, entity controllers, UI logic,
and AI.  Keep physics, rendering pipelines, and ECS in Rust.

---

## Relation to kotoba-clj

`kami-clj` is a sibling compiler, not a dependency on `kotoba-clj`.  Both share the
same design principles (all-i64 stack, kotoba-edn reader, two-pass codegen,
wasm-encoder) and the same prelude pattern, but target different WIT worlds:

| | kotoba-clj | kami-clj |
|---|---|---|
| WIT world | `kotoba:kais/kotoba-node` | `kami:engine/kami-game` |
| Host imports | kqe, llm, auth, kse | scene, physics, input, render, audio, time |
| Use case | Datalog agent, LangGraph node | Game entity controller |
| F32 support | No | Yes (`Expr::Float`, f32 builtins) |
| defsystem | No | Yes |

---

## Alternatives Considered

1. **Lua scripting** — widely used in games (Love2D, Defold).  Rejected: requires an
   interpreter (~200KB), no WIT integration, Rust FFI is lossy.

2. **Rhai** — embeddable Rust scripting.  Rejected: not WASM-native, no Component Model
   story, adding another language would diverge from the kotoba ecosystem.

3. **Extend kotoba-clj** — add game builtins to kotoba-clj directly.  Rejected: kotoba-clj
   is scoped to the `kotoba:kais` world; mixing game + database concerns violates the
   single-responsibility of each WIT world.

4. **TypeScript via Javy** — compile TS to WASM via QuickJS.  Rejected: performance
   overhead from JS runtime, no meaningful reduction in author burden vs just using TS
   directly in kami-engine-sdk.
