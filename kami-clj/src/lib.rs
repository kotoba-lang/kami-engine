//! # kami-clj — Clojure-subset → WebAssembly game-script compiler
//!
//! `kami-clj` reads a Clojure/EDN-subset source file and compiles it to a
//! **real WebAssembly module** that targets the `kami:engine@1.0.0` WIT world
//! (see `wit/kami-game/world.wit`).  The compiled module plugs into
//! `kami-script-runtime`, which binds every `kami:engine/*` import to the live
//! Rust game-engine state (`hecs::World`, input, audio, wgpu draw queue).
//!
//! ## Architecture diagram
//!
//! ```text
//! .clj source  ──► kami-clj compiler ──► .wasm (core module)
//!                  (this crate)                │
//!                                      wit-component wrapping
//!                                              │
//!                                    kami-game Component
//!                                              │
//!                                    kami-script-runtime
//!                                    (wasmtime host, Rust)
//!                                              │
//!                          ┌───────────────────┼──────────────────┐
//!                    hecs::World         kami-input          kami-audio
//!                    (ECS entities)     (keyboard/gamepad)  (spatial audio)
//! ```
//!
//! ## Clojure subset supported
//!
//! Inherits everything from `kotoba-clj` plus game-specific extensions:
//!
//! - **F32 literals**: `(f32 1.5)` or bare `1.5` — compiled to
//!   `f32.const` → `i32.reinterpret_f32` → `i64.extend_i32_u` so the
//!   value stays on the all-i64 stack.
//! - **`defsystem`**: top-level tick-handler form — see below.
//! - **Scene / ECS builtins**: `spawn-entity`, `despawn-entity`, `get-x`,
//!   `set-position!`, `get-vx`, `set-velocity!`, … (full list in `ast.rs`).
//! - **Input builtins**: `key-down?`, `key-pressed?`, `axis`, `pointer-x/y`.
//! - **Render builtins**: `draw-mesh!`, `spawn-particle!`, `draw-line!`.
//! - **Audio builtins**: `play-sound`, `stop-sound`, `play-sound-at`.
//! - **Time builtins**: `delta-ms`, `elapsed-ms`, `tick-n`.
//!
//! ## defsystem
//!
//! ```clojure
//! (defsystem player-controller [dt]
//!   ;; runs every tick; dt is the delta time in milliseconds (i64)
//!   (when (key-down? "ArrowRight")
//!     (let [vx (f32 2.0)]
//!       (set-velocity! player vx (f32 0.0) (f32 0.0))))
//!   (when (key-down? "ArrowLeft")
//!     (set-velocity! player (f32 -2.0) (f32 0.0) (f32 0.0))))
//! ```
//!
//! `defsystem` desugars to `(defn player-controller-tick [dt] …)` and is
//! exported as `player-controller-tick` from the WASM module.  The host calls
//! all registered `-tick` exports each engine tick.
//!
//! ## Game PRELUDE
//!
//! Every module compiled with [`compile_str_with_prelude`] gets the
//! [`GAME_PRELUDE`] prepended, which provides:
//! - Vec3 heap helpers (`vec3-make`, `vec3-x`, `vec3-y`, `vec3-z`, …)
//! - Entity-position sugar (`entity-x`, `entity-y`, `entity-z`)
//! - Fixed-step timer utilities (`timer-make`, `timer-tick!`, `timer-fired?`)
//! - Common f32 constants (`F32-ZERO`, `F32-ONE`, `F32-HALF`, `F32-NEG-ONE`)
//!
//! ## Quick example
//!
//! ```rust
//! let src = r#"
//!   (def player-eid 1)
//!   (defn init [] player-eid)
//!   (defsystem move [dt]
//!     (when (key-down? "ArrowRight")
//!       (set-velocity! player-eid (f32 2.0) (f32 0.0) (f32 0.0))))
//! "#;
//! let wasm = kami_clj::compile_str(src).unwrap();
//! assert!(wasm.starts_with(b"\0asm"));
//! ```

pub mod ast;
pub mod codegen;
#[cfg(feature = "component")]
pub mod component;
#[cfg(feature = "run")]
pub mod run;

use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CljError {
    #[error("read error: {0}")]
    Read(String),
    #[error("lowering error: {0}")]
    Lower(String),
    #[error("codegen error: {0}")]
    Codegen(String),
    #[error("runtime error: {0}")]
    Run(String),
}

/// Compile Clojure-subset source into WebAssembly bytes (core module).
pub fn compile_str(src: &str) -> Result<Vec<u8>, CljError> {
    let program = ast::parse_program(src)?;
    codegen::compile(&program)
}

/// Compile a `.kami` or `.clj` game-script file.
///
/// A leading Unix shebang (`#!...`) is stripped so scripts can be executable:
/// ```text
/// #!/usr/bin/env kami-clj
/// ```
pub fn compile_file(path: impl AsRef<Path>) -> Result<Vec<u8>, CljError> {
    let src = std::fs::read_to_string(path.as_ref())
        .map_err(|e| CljError::Read(format!("read {}: {e}", path.as_ref().display())))?;
    compile_str(strip_shebang(&src))
}

/// Compile with the [`GAME_PRELUDE`] prepended.
pub fn compile_str_with_prelude(src: &str) -> Result<Vec<u8>, CljError> {
    compile_str(&format!("{GAME_PRELUDE}\n{src}"))
}

fn strip_shebang(src: &str) -> &str {
    if let Some(rest) = src.strip_prefix("#!") {
        match rest.find('\n') {
            Some(i) => &rest[i + 1..],
            None    => "",
        }
    } else {
        src
    }
}

// ---------------------------------------------------------------------------
// GAME_PRELUDE — convenience helpers written in the language itself
// ---------------------------------------------------------------------------

/// Common f32 constants and Vec3/timer utilities prepended when using
/// [`compile_str_with_prelude`].
///
/// ## Vec3
///
/// A Vec3 is a heap-allocated triple `[x:i32@0, y:i32@8, z:i32@8]`
/// (3 × 4 bytes = 12 bytes; stored as f32 bit-patterns).
///
/// ## Timer
///
/// A timer is a heap cell `[period-ms:i64@0, elapsed-ms:i64@8]`.
/// `(timer-tick! t dt)` advances the elapsed counter; `(timer-fired? t)` returns
/// 1 if elapsed ≥ period (and resets elapsed).
pub const GAME_PRELUDE: &str = r#"
;; ---- Common f32 bit-pattern constants (IEEE-754) --------------------------
(def F32-ZERO     0)           ;; 0.0f  = 0x00000000
(def F32-ONE      1065353216)  ;; 1.0f  = 0x3F800000
(def F32-HALF     1056964608)  ;; 0.5f  = 0x3F000000
(def F32-NEG-ONE -1082130432)  ;; -1.0f = 0xBF800000
(def F32-TWO      1073741824)  ;; 2.0f  = 0x40000000

;; ---- Vec3 helpers ----------------------------------------------------------
;; Allocate a Vec3 (3 × i32 f32-bit-pattern words)
(defn vec3-make [x y z]
  (let [p (alloc 12)]
    (store32! p x)
    (store32! (+ p 4) y)
    (store32! (+ p 8) z)
    p))

(defn vec3-x [v] (load32 v))
(defn vec3-y [v] (load32 (+ v 4)))
(defn vec3-z [v] (load32 (+ v 8)))

(defn vec3-set-x! [v x] (store32! v x) v)
(defn vec3-set-y! [v y] (store32! (+ v 4) y) v)
(defn vec3-set-z! [v z] (store32! (+ v 8) z) v)

;; Read entity position into a fresh Vec3 heap cell.
(defn entity-pos [eid]
  (vec3-make (get-x eid) (get-y eid) (get-z eid)))

;; Convenience single-component reads.
(defn entity-x [eid] (get-x eid))
(defn entity-y [eid] (get-y eid))
(defn entity-z [eid] (get-z eid))

;; ---- Fixed-step timer ------------------------------------------------------
;; Timer layout: [period-ms:i64@0, elapsed-ms:i64@8]
(defn timer-make [period-ms]
  (let [t (alloc 16)]
    (store64! t period-ms)
    (store64! (+ t 8) 0)
    t))

(defn timer-tick! [t dt]
  (let [elapsed (+ (load64 (+ t 8)) dt)]
    (store64! (+ t 8) elapsed)
    t))

(defn timer-fired? [t]
  (let [period  (load64 t)
        elapsed (load64 (+ t 8))]
    (if (>= elapsed period)
      (do (store64! (+ t 8) 0) 1)
      0)))
"#;

/// The prelude text (for callers that need the raw string, e.g. component path).
pub fn game_prelude() -> &'static str {
    GAME_PRELUDE
}
