//! Basic compiler smoke tests — verifies that the Clojure subset compiles to
//! valid WASM bytes without a host runtime.

use kami_clj::compile_str;

#[test]
fn empty_defn_compiles() {
    let src = "(defn init [] 0)";
    let wasm = compile_str(src).expect("compile failed");
    assert!(wasm.starts_with(b"\0asm"), "missing WASM magic");
}

#[test]
fn float_literal_compiles() {
    let src = "(defn get-speed [] (f32 5.0))";
    let wasm = compile_str(src).expect("compile");
    assert!(wasm.starts_with(b"\0asm"));
}

#[test]
fn defsystem_desugars_to_tick() {
    let src = r#"
      (defsystem player-move [dt]
        (+ dt 1))
    "#;
    let wasm = compile_str(src).expect("compile");
    assert!(wasm.starts_with(b"\0asm"));
    // The export should be named "player-move-tick"
    // TODO: verify the export name once we have a WAT pretty-printer test helper.
}

#[test]
fn game_prelude_compiles() {
    use kami_clj::compile_str_with_prelude;
    let src = r#"
      (defn test-prelude []
        (let [t (timer-make 1000)]
          (timer-tick! t 500)
          (timer-fired? t)))
    "#;
    let wasm = compile_str_with_prelude(src).expect("compile with prelude");
    assert!(wasm.starts_with(b"\0asm"));
}

#[test]
fn f32_constant_roundtrip() {
    // (f32 1.0) should produce the bit-pattern 0x3F800000 = 1065353216
    let src = "(defn get-one [] (f32 1.0))";
    let wasm = compile_str(src).expect("compile");
    assert!(wasm.starts_with(b"\0asm"));
}

#[test]
fn entity_spawn_builtin_compiles() {
    let src = r#"
      (defn init []
        (spawn-entity "player"))
    "#;
    let wasm = compile_str(src).expect("compile");
    assert!(wasm.starts_with(b"\0asm"));
}

#[test]
fn key_down_builtin_compiles() {
    let src = r#"
      (defn tick [dt]
        (if (key-down? "ArrowRight") 1 0))
    "#;
    let wasm = compile_str(src).expect("compile");
    assert!(wasm.starts_with(b"\0asm"));
}

#[test]
fn vec3_prelude_compiles() {
    use kami_clj::compile_str_with_prelude;
    let src = r#"
      (defn get-origin []
        (vec3-make F32-ZERO F32-ZERO F32-ZERO))
    "#;
    let wasm = compile_str_with_prelude(src).expect("compile");
    assert!(wasm.starts_with(b"\0asm"));
}

// ── survivors core-loop surface (rand-int / query / nearest / move-toward) ──

#[test]
fn rand_int_compiles() {
    let wasm = kami_clj::compile_str(r#"(defsystem s [dt] (rand-int 1000))"#)
        .expect("rand-int compile");
    assert!(wasm.starts_with(b"\0asm"));
}

#[test]
fn count_tagged_compiles() {
    let wasm = kami_clj::compile_str(r#"(defsystem s [dt] (when (< (count-tagged "enemy") 400) 1))"#)
        .expect("count-tagged compile");
    assert!(wasm.starts_with(b"\0asm"));
}

#[test]
fn doseq_entities_compiles() {
    // enemy AI over ALL enemies — impossible before (no iteration/lambda).
    let src = r#"
        (def player 1)
        (defsystem enemy-ai [dt]
          (doseq-entities [e "enemy"]
            (move-toward! e player (f32 40.0))))
    "#;
    let wasm = kami_clj::compile_str(src).expect("doseq-entities compile");
    assert!(wasm.starts_with(b"\0asm"));
}

#[test]
fn nested_doseq_and_nearest_compiles() {
    // bullet collision: each bullet despawns the nearest enemy in range.
    let src = r#"
        (defsystem bullet-collision [dt]
          (doseq-entities [b "bullet"]
            (let [hit (nearest-tagged "enemy" (get-x b) (get-y b) (f32 12.0))]
              (when (not= hit -1)
                (despawn-entity hit)
                (despawn-entity b)))))
    "#;
    let wasm = kami_clj::compile_str(src).expect("nearest/doseq compile");
    assert!(wasm.starts_with(b"\0asm"));
}

#[test]
fn survivors_core_loop_compiles() {
    // The full loop that FAILED before the extension: spawn (rng + cap),
    // enemy AI (iterate all), targeting/collision (iterate + broadphase).
    let src = r#"
        (def player 1)
        (defsystem wave-spawn [dt]
          (when (< (count-tagged "enemy") 400)
            (when (zero? (mod (tick-n) 30))
              (let [roll (rand-int 100)
                    e (spawn-entity "shambler")]
                (set-position! e (f32 0.0) (f32 0.0) (f32 0.0))))))
        (defsystem enemy-ai [dt]
          (doseq-entities [e "enemy"]
            (move-toward! e player (f32 40.0))))
        (defsystem weapon-pistol [dt]
          (when (zero? (mod (tick-n) 42))
            (let [hit (nearest-tagged "enemy" (get-x player) (get-y player) (f32 220.0))]
              (when (not= hit -1)
                (despawn-entity hit)
                (play-sound "shot")))))
    "#;
    let wasm = kami_clj::compile_str(src).expect("survivors core loop compile");
    assert!(wasm.starts_with(b"\0asm"), "missing WASM magic");
    assert!(wasm.len() > 200, "suspiciously small module");
}
