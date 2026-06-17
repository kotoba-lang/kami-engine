//! WIT Component Model wrapping for compiled kami-clj core modules.
//!
//! Takes a raw WASM core module (output of `codegen::compile`) and wraps it
//! with the `kami:engine/kami-game` WIT world so it can be loaded as a
//! Component by `kami-script-runtime`.
//!
//! Currently returns the core module bytes unchanged; full wit-component
//! adapter encoding is a Phase-3 item (ADR-0035).

use crate::CljError;

/// Wrap a compiled core-module as a WIT component targeting `kami:engine/kami-game`.
///
/// Right now this is a pass-through that returns the core-module bytes as-is.
/// The `kami-script-runtime` wasmtime host can load core modules directly via
/// `Module::new`, so the component wrapping is optional for the current phase.
pub fn wrap_as_component(core_wasm: Vec<u8>) -> Result<Vec<u8>, CljError> {
    // TODO: use wit_component::ComponentEncoder to produce a real component
    // once the WIT adapter for kami:engine@1.0.0 is stabilised.
    Ok(core_wasm)
}

/// Return the WIT world source for `kami:engine/kami-game`.
pub fn kami_game_wit() -> &'static str {
    include_str!("../../wit/kami-game/world.wit")
}
