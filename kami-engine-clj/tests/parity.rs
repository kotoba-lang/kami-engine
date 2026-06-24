//! In-process two-backend parity: the SAME compiled module, run under wasmtime (JIT) and
//! wasmi (no-JIT interpreter — the console/iOS ship path), must produce bit-identical i64
//! results. This is the only test that proves the compiler's *per-op semantics* agree across
//! the two backends the engine actually ships on — not a synthetic game replayed through a
//! shell script, but the real codegen, both backends, one process. Requires `--features run`.
#![cfg(feature = "run")]

use kami_engine_clj::{compile_str, run::eval_i64};

/// Compile `(defn __probe [] <expr>)` and run it under the wasmi interpreter, returning the
/// i64 it computes — the mirror of `run::eval_i64` (which uses wasmtime).
fn eval_wasmi(expr: &str) -> i64 {
    let wasm = compile_str(&format!("(defn __probe [] {expr})")).expect("compile");
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, &wasm[..]).expect("wasmi module load");
    let mut store = wasmi::Store::new(&engine, ());
    let linker = wasmi::Linker::<()>::new(&engine);
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .expect("wasmi instantiate");
    let probe = instance
        .get_typed_func::<(), i64>(&store, "__probe")
        .expect("wasmi __probe export");
    probe.call(&mut store, ()).expect("wasmi call")
}

/// Every expression must compute the identical i64 under both backends. Cases deliberately
/// include the paths most likely to diverge across a JIT/interpreter boundary: signed
/// division/remainder, negatives, the multi-arg comparison chains we just fixed, and control
/// flow (if / let / abs).
#[test]
fn backends_agree_bit_for_bit() {
    let cases = [
        "(+ 2 3)",
        "(- 10 3 2)",
        "(* 2 3 4)",
        "(quot 17 5)",
        "(quot -17 5)",
        "(mod 17 5)",
        "(mod -17 5)",
        "(- 8)",
        "(abs -8)",
        "(= 5 5 5)",
        "(= 1 2 1)",
        "(< 1 2 3)",
        "(< 1 2 0)",
        "(> 9 3 3)",
        "(<= 1 1 2)",
        "(>= 5 5 1)",
        "(if (< 1 2) 100 200)",
        "(if (= 3 3 3) 42 0)",
        "(let [a 7 b 6] (* a b))",
        // guest f32 math — the boxed bit-patterns must round-trip identically through both
        // the JIT and the interpreter (f32 reinterpret + arithmetic + compare).
        "(+f (f32 1.5) (f32 2.25))",
        "(*f (f32 -2.0) (f32 4.0))",
        "(/f (f32 7.0) (f32 2.0))",
        "(<f (f32 -1.0) (f32 1.0))",
        "(if (<f (f32 0.5) (f32 0.25)) 1 0)",
    ];
    for expr in cases {
        let wt = eval_i64(expr).unwrap_or_else(|e| panic!("wasmtime `{expr}`: {e:?}"));
        let wi = eval_wasmi(expr);
        assert_eq!(
            wt, wi,
            "two-backend divergence on `{expr}`: wasmtime={wt}, wasmi={wi}"
        );
    }
}
