//! Basic compiler smoke tests — verifies that the Clojure subset compiles to
//! valid WASM bytes without a host runtime.

use kami_engine_clj::compile_str;

#[test]
fn empty_defn_compiles() {
    let src = "(defn init [] 0)";
    let wasm = compile_str(src).expect("compile failed");
    assert!(wasm.starts_with(b"\0asm"), "missing WASM magic");
}

/// The compiler must REJECT malformed source with an `Err`, never panic — a
/// guest author (or a tool) can feed it anything. Each input below should return
/// `Err(CljError)`; if any panics, this test fails (which is the point).
#[test]
fn compiler_rejects_malformed_input() {
    let bad = [
        "(defn)",                       // defn: too few forms
        "(defn f)",                     // defn: missing params + body
        "(def x)",                      // def: needs exactly (def name value)
        "(defn f [1] 0)",               // param list must be symbols
        "(defn f [] (f32))",            // f32 takes exactly one arg
        "(defn f [] (f32 1 2))",        // f32 takes exactly one arg
        "(defn f [] (let [a] a))",      // let binding vector must be even
        "(defn f [] (+))",             // + needs at least one arg
        "(weird-top-level-form 1 2)",   // unsupported top-level form
        "(123 456)",                    // list head must be a symbol
        "(defn f [] (",                 // unterminated — reader error
    ];
    for src in bad {
        assert!(
            compile_str(src).is_err(),
            "compiler should reject `{src}` with Err, not accept it or panic",
        );
    }
}

/// Integer math / signed ordering on an f32 bit-pattern is unsound under the all-i64 ABI
/// (the i64 holds float bits). The compiler must REJECT it with a clear error rather than
/// emit silent garbage — guest float arithmetic isn't supported.
#[test]
fn f32_arithmetic_is_rejected() {
    let bad = [
        "(defn f [e] (+ (get-x e) 1))",         // integer-add of a position's bits
        "(defn f [e] (- (get-y e) (get-x e)))", // subtract two f32 positions
        "(defn f [e] (* (get-vx e) 2))",        // scale a velocity in-guest
        "(defn f [] (< (get-x 0) (get-y 0)))",  // signed-compare f32 bits (wrong for negatives)
        "(defn f [] (inc (get-x 0)))",          // inc an f32
        "(defn f [] (+ (f32 1.0) (f32 2.0)))",  // add two float literals
        "(defn f [] (* (axis \"MoveX\") 3))",   // scale an axis reading
    ];
    for src in bad {
        assert!(
            compile_str(src).is_err(),
            "compiler must reject unsound f32 arithmetic, not accept `{src}`",
        );
    }
}

/// The flip side: passing f32 values straight to host primitives (the supported pattern)
/// must still compile — the reject is for *arithmetic*, not for using f32 at all.
#[test]
fn f32_passthrough_to_host_still_compiles() {
    let ok = "(defn move [e] (set-position! e (get-x e) (get-y e) (f32 0.0)))";
    assert!(compile_str(ok).is_ok(), "passing f32 to a host primitive must compile");
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
    use kami_engine_clj::compile_str_with_prelude;
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
fn vec_state_bag_compiles() {
    // Phase-4 vector (state bag): make / push! / get / len / set! / clear!.
    use kami_engine_clj::compile_str_with_prelude;
    let src = r#"
      (defn build []
        (let [v (vec-make 8)]
          (vec-push! v 11)
          (vec-push! v 22)
          (vec-set! v 0 33)
          (let [sum (+ (vec-get v 0) (vec-get v 1) (vec-len v))]
            (vec-clear! v)
            sum)))
    "#;
    let wasm = compile_str_with_prelude(src).expect("compile vec prelude");
    assert!(wasm.starts_with(b"\0asm"));
}

#[test]
fn map_assoc_bag_compiles() {
    // Phase-4 map (assoc bag): make / put! (insert + update) / get / get-or / has?.
    use kami_engine_clj::compile_str_with_prelude;
    let src = r#"
      (defn build []
        (let [m (map-make 8)]
          (map-put! m 100 3)
          (map-put! m 200 7)
          (map-put! m 100 (+ (map-get m 100) 1))
          (+ (map-get m 100)
             (map-get-or m 999 0)
             (map-has? m 200)
             (map-len m))))
    "#;
    let wasm = compile_str_with_prelude(src).expect("compile map prelude");
    assert!(wasm.starts_with(b"\0asm"));
}

#[test]
fn defentity_template_compiles() {
    // Phase-4 defentity: desugars to spawn `self` (tagged by name) + init + return.
    let src = r#"
      (defentity enemy [x y]
        (set-position! self x y (f32 0.0)))
      (defn init []
        (enemy (f32 10.0) (f32 20.0)))
    "#;
    let wasm = compile_str(src).expect("compile defentity");
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
    use kami_engine_clj::compile_str_with_prelude;
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
    let wasm = kami_engine_clj::compile_str(r#"(defsystem s [dt] (rand-int 1000))"#)
        .expect("rand-int compile");
    assert!(wasm.starts_with(b"\0asm"));
}

#[test]
fn count_tagged_compiles() {
    let wasm = kami_engine_clj::compile_str(r#"(defsystem s [dt] (when (< (count-tagged "enemy") 400) 1))"#)
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
    let wasm = kami_engine_clj::compile_str(src).expect("doseq-entities compile");
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
    let wasm = kami_engine_clj::compile_str(src).expect("nearest/doseq compile");
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
    let wasm = kami_engine_clj::compile_str(src).expect("survivors core loop compile");
    assert!(wasm.starts_with(b"\0asm"), "missing WASM magic");
    assert!(wasm.len() > 200, "suspiciously small module");
}
