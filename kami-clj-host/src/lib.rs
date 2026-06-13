//! kami-clj-host — Rust host bridge for the Clojure SDK (kami-engine-sdk-clj).
//!
//! Concrete implementation of the `kami:engine/frame` WIT contract
//! (`../wit/kami-frame.wit`): decode the KAMI columnar render-IR that
//! `kami.ipc/pack` emits and drive `kami-render` (wgpu). clj is the brain,
//! this crate is the GPU arm.
//!
//! - [`frame`] — pure, GPU-free columnar decoder. Unit-tested headlessly against
//!   bytes emitted by the clj side (`tests/fixtures/frame.bin`).
//! - [`host`] — wasm-bindgen + wgpu browser host (behind the `host` feature):
//!   `register_mesh` / `register_material` / `register_shader` / `submit_frame`.

pub mod frame;

#[cfg(feature = "host")]
pub mod host;
