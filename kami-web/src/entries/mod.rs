//! Per-game WASM entry modules.
//!
//! Each sibling module hosts a single `run_with_*` / `run_embed_*` family
//! and owns its scene construction + render loop. Engine-wide helpers
//! (GPU bootstrap via kami-render, motion eval, VRM morph, RTC) stay in
//! `lib.rs`.
//!
//! Scope (2026-04-17): first move (`quarry_walk`) establishes the pattern.
//! Remaining entries (`run_with_scene`/`run_with_game`/`run_with_graph`/
//! `run_with_sabiotoshi`/`run_embed_scad`/`run_embed_sdf*`/`run_embed_nerf`/
//! `run_with_character`/`run_embed_vrm`) migrate here as they are touched.

pub mod quarry_walk;

pub mod render_ir;
