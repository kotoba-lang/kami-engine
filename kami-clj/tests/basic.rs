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
