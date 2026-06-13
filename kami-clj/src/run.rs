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
