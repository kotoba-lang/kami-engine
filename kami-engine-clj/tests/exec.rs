//! Execution-grade compiler tests — compile an expression, RUN it, assert the value.
//!
//! Unlike `basic.rs` (which only checks that `\0asm` bytes came out), these run the
//! compiled module under wasmtime and assert what it actually computes. That is the
//! only kind of test that catches silent codegen bugs. Requires `--features run`.
#![cfg(feature = "run")]

use kami_engine_clj::run::{eval_f32, eval_i64};

fn eval(expr: &str) -> i64 {
    eval_i64(expr).unwrap_or_else(|e| panic!("eval `{expr}` failed: {e:?}"))
}

fn evalf(expr: &str) -> f32 {
    eval_f32(expr).unwrap_or_else(|e| panic!("evalf `{expr}` failed: {e:?}"))
}

#[test]
fn arithmetic_computes_correct_values() {
    assert_eq!(eval("(+ 2 3)"), 5);
    assert_eq!(eval("(+ 1 2 3 4)"), 10); // variadic add
    assert_eq!(eval("(- 10 3)"), 7);
    assert_eq!(eval("(- 10 3 2)"), 5); // variadic sub
    assert_eq!(eval("(* 4 5)"), 20);
    assert_eq!(eval("(* 2 3 4)"), 24); // variadic mul
    assert_eq!(eval("(quot 17 5)"), 3);
    assert_eq!(eval("(mod 17 5)"), 2);
    assert_eq!(eval("(inc 41)"), 42);
    assert_eq!(eval("(dec 1)"), 0);
}

#[test]
fn two_arg_comparisons_are_correct() {
    assert_eq!(eval("(= 3 3)"), 1);
    assert_eq!(eval("(= 3 4)"), 0);
    assert_eq!(eval("(< 1 2)"), 1);
    assert_eq!(eval("(< 2 1)"), 0);
    assert_eq!(eval("(> 5 2)"), 1);
    assert_eq!(eval("(<= 2 2)"), 1);
    assert_eq!(eval("(>= 2 3)"), 0);
}

/// REGRESSION GUARD: multi-arg `=` must mean "all equal", not fold the boolean
/// result back into the next comparison. `(= 5 5 5)` was returning 0 before the fix
/// (push 5; 5==5→1; then 1==5→0) — a silent unsoundness any chained equality hit.
#[test]
fn multi_arg_equality_means_all_equal() {
    assert_eq!(eval("(= 1 1 1)"), 1);
    assert_eq!(eval("(= 5 5 5)"), 1); // was 0 — the bug
    assert_eq!(eval("(= 7 7 7 7)"), 1);
    assert_eq!(eval("(= 5 5 6)"), 0);
    assert_eq!(eval("(= 1 2 1)"), 0);
}

/// REGRESSION GUARD: ordered comparisons with >2 args must check EVERY adjacent
/// pair. The old codegen only compared args[0] and args[1] and silently dropped the
/// rest — `(< 1 2 0)` returned 1 (true) when 2 < 0 is false.
#[test]
fn multi_arg_ordering_checks_every_pair() {
    assert_eq!(eval("(< 1 2 3)"), 1);
    assert_eq!(eval("(< 1 2 0)"), 0); // was 1 — the dropped-tail bug
    assert_eq!(eval("(> 3 2 1)"), 1);
    assert_eq!(eval("(> 3 2 5)"), 0);
    assert_eq!(eval("(<= 1 1 2)"), 1);
    assert_eq!(eval("(<= 1 2 2 1)"), 0);
    assert_eq!(eval("(>= 5 5 1)"), 1);
}

/// Guest f32 arithmetic computes REAL floats (unbox bits → float op → rebox), so games can
/// finally do `(set-velocity! p (*f (axis "MoveX") speed) …)` in CLJ instead of the host.
#[test]
fn guest_f32_arithmetic_computes_real_floats() {
    assert!((evalf("(+f (f32 1.5) (f32 2.25))") - 3.75).abs() < 1e-6);
    assert!((evalf("(-f (f32 5.0) (f32 1.5))") - 3.5).abs() < 1e-6);
    assert!((evalf("(*f (f32 3.0) (f32 2.5))") - 7.5).abs() < 1e-6);
    assert!((evalf("(/f (f32 7.0) (f32 2.0))") - 3.5).abs() < 1e-6);
    assert!((evalf("(+f (f32 1.0) (f32 2.0) (f32 3.0))") - 6.0).abs() < 1e-6); // variadic
    assert!((evalf("(*f (f32 -2.0) (f32 4.0))") - -8.0).abs() < 1e-6); // negative
}

/// The reason f32 comparison is a distinct op: it is SIGN-CORRECT. A signed integer compare of
/// the bit-patterns says -1.0 > 1.0 (its bit-pattern is numerically larger); the f32 compare is
/// right. This is the unsoundness the f32-reject was guarding against, now fixed by `<f`.
#[test]
fn guest_f32_comparison_is_sign_correct() {
    assert_eq!(eval("(<f (f32 -1.0) (f32 1.0))"), 1); // would be 0 with I64LtS on bits
    assert_eq!(eval("(<f (f32 1.0) (f32 -1.0))"), 0);
    assert_eq!(eval("(>f (f32 2.5) (f32 2.0))"), 1);
    assert_eq!(eval("(<=f (f32 2.0) (f32 2.0))"), 1);
    assert_eq!(eval("(=f (f32 3.5) (f32 3.5))"), 1);
    assert_eq!(eval("(<f (f32 1.0) (f32 2.0) (f32 3.0))"), 1); // chain
    assert_eq!(eval("(<f (f32 1.0) (f32 2.0) (f32 0.0))"), 0);
}

/// `defatom` gives the guest persistent mutable state (a WASM global), so a game holds
/// lives/score directly instead of counting off-map marker entities. The cell must accumulate
/// across separate tick calls — this drives 200 ticks and 2 hits and reads the values back.
#[test]
fn defatom_persists_state_across_ticks() {
    use kami_engine_clj::compile_str;
    let src = r#"
        (defatom score 0)
        (defatom lives 3)
        (defn init [] 0)
        (defn step [dt] (set-atom! score (+ (atom-val score) 1)))
        (defn hit  [dt] (set-atom! lives (- (atom-val lives) 1)))
        (defn getscore [] (atom-val score))
        (defn getlives [] (atom-val lives))
    "#;
    let wasm = compile_str(src).expect("compile");
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, &wasm).expect("module");
    let mut store: wasmtime::Store<()> = wasmtime::Store::new(&engine, ());
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");

    let step = instance.get_typed_func::<i64, i64>(&mut store, "step").unwrap();
    for _ in 0..200 {
        step.call(&mut store, 0).unwrap();
    }
    let hit = instance.get_typed_func::<i64, i64>(&mut store, "hit").unwrap();
    hit.call(&mut store, 0).unwrap();
    hit.call(&mut store, 0).unwrap();

    let getscore = instance.get_typed_func::<(), i64>(&mut store, "getscore").unwrap();
    let getlives = instance.get_typed_func::<(), i64>(&mut store, "getlives").unwrap();
    assert_eq!(getscore.call(&mut store, ()).unwrap(), 200, "score must accumulate across 200 ticks");
    assert_eq!(getlives.call(&mut store, ()).unwrap(), 1, "lives 3 - 2 hits = 1");
}

#[test]
fn conditionals_pick_the_right_branch() {
    assert_eq!(eval("(if (< 1 2) 100 200)"), 100);
    assert_eq!(eval("(if (< 2 1) 100 200)"), 200);
    assert_eq!(eval("(if (= 3 3 3) 1 0)"), 1);
    assert_eq!(eval("(let [a 10 b 20] (+ a b))"), 30);
}
