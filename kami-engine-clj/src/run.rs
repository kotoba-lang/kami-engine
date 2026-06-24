//! Thin wasmtime runner for compiled kami-clj modules.
//!
//! The full host binding lives in `kami-script-runtime`.  This module provides
//! a standalone runner for tests and CLI tools that just want to instantiate a
//! compiled module and call `init` / `tick` without the full engine stack.

use wasmtime::{Engine, Module, Store};

use crate::CljError;

/// Run the `init` export of a compiled module with no host imports.
/// Useful for headless unit-testing that the module instantiates cleanly.
pub fn run_init_headless(wasm: &[u8]) -> Result<(), CljError> {
    let engine = Engine::default();
    let module = Module::new(&engine, wasm)
        .map_err(|e| CljError::Run(format!("module load: {e}")))?;
    let mut store: Store<()> = Store::new(&engine, ());
    // Instantiate with no linker — modules that call host imports will trap here,
    // which is expected for modules that need the full kami-script-runtime host.
    let instance = wasmtime::Instance::new(&mut store, &module, &[])
        .map_err(|e| CljError::Run(format!("instantiate: {e}")))?;
    if let Some(init_fn) = instance.get_func(&mut store, "init") {
        init_fn
            .call(&mut store, &[], &mut [])
            .map_err(|e| CljError::Run(format!("init trap: {e}")))?;
    }
    Ok(())
}

/// Execution-grade test seam: compile a single expression as `(defn __probe [] <expr>)`,
/// run it headless (no host imports), and return the i64 it actually computes.
///
/// This is the difference between "the compiler emitted some WASM bytes" and "the
/// compiled program computes the right value" — the latter is what catches codegen bugs
/// like an unsound multi-arg `=` or a comparison chain that drops its tail operands.
pub fn eval_i64(expr: &str) -> Result<i64, CljError> {
    let wasm = crate::compile_str(&format!("(defn __probe [] {expr})"))?;
    let engine = Engine::default();
    let module =
        Module::new(&engine, &wasm).map_err(|e| CljError::Run(format!("module load: {e}")))?;
    let mut store: Store<()> = Store::new(&engine, ());
    let instance = wasmtime::Instance::new(&mut store, &module, &[])
        .map_err(|e| CljError::Run(format!("instantiate: {e}")))?;
    let probe = instance
        .get_func(&mut store, "__probe")
        .ok_or_else(|| CljError::Run("no __probe export".into()))?;
    let mut results = [wasmtime::Val::I64(0)];
    probe
        .call(&mut store, &[], &mut results)
        .map_err(|e| CljError::Run(format!("trap: {e}")))?;
    match results[0] {
        wasmtime::Val::I64(v) => Ok(v),
        ref other => Err(CljError::Run(format!("expected i64 result, got {other:?}"))),
    }
}

/// Like [`eval_i64`], but decode the result as the f32 bit-pattern it boxes — for asserting
/// the value of guest float expressions (`+f`/`-f`/`*f`/`/f`).
pub fn eval_f32(expr: &str) -> Result<f32, CljError> {
    Ok(f32::from_bits(eval_i64(expr)? as u32))
}
