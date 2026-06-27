//! End-to-end test for the new RT + binaural host imports (ADR-0045 phase 2).
//!
//! Compiles the rt-audio-demo's actual `logic.clj` → WASM, instantiates it in
//! the runtime, runs init + the defsystems, and asserts the calls landed in
//! HostState: `rt-enable!` set the active RT recipe and `set-listener!` moved
//! the binaural listener pose. This proves the whole vertical resolves —
//! clj builtin → WASM import → host `func_wrap` binding → HostState accessor.

use std::sync::{Arc, Mutex};

use kami_script_runtime::KamiScriptRuntime;

fn make_runtime() -> KamiScriptRuntime {
    let world = Arc::new(Mutex::new(hecs::World::new()));
    KamiScriptRuntime::new(world).expect("runtime init")
}

/// The shipped reference game — compiled here so the test breaks if the demo
/// or the host imports drift.
const DEMO: &str = include_str!("../../kami-clj-play3d/games/rt-audio-demo/logic.clj");

#[test]
fn rt_enable_sets_active_recipe() {
    let mut rt = make_runtime();
    rt.load_clj("demo", DEMO)
        .expect("compile + load demo logic.clj");
    rt.call_init("demo").expect("init");
    // init calls (rt-enable! "gi")
    assert_eq!(rt.rt_recipe().as_deref(), Some("gi"));
}

#[test]
fn set_listener_tracks_the_player() {
    let mut rt = make_runtime();
    rt.load_clj("demo", DEMO)
        .expect("compile + load demo logic.clj");
    rt.call_init("demo").expect("init"); // spawns player at origin
    rt.call_systems("demo", 16).expect("systems"); // `listen` sets the listener

    let l = rt.listener();
    // Player spawned at origin → listener position is origin, forward is -z.
    assert_eq!(&l[0..3], &[0.0, 0.0, 0.0], "listener position = player");
    assert_eq!(&l[3..6], &[0.0, 0.0, -1.0], "listener forward = -z");
}

#[test]
fn inline_calls_round_trip_through_host_state() {
    // A minimal script (no game scaffolding) pinning the raw import semantics.
    let mut rt = make_runtime();
    let src = r#"
      (defn init []
        (rt-enable! "shadows")
        (set-listener! (f32 1.5) (f32 2.0) (f32 -3.0) (f32 1.0) (f32 0.0) (f32 0.0)))
      (defn tick [dt] 0)
    "#;
    rt.load_clj("inline", src).expect("compile + load");
    rt.call_init("inline").expect("init");

    assert_eq!(rt.rt_recipe().as_deref(), Some("shadows"));
    assert_eq!(rt.listener(), [1.5, 2.0, -3.0, 1.0, 0.0, 0.0]);
}

#[test]
fn empty_recipe_disables_rt() {
    let mut rt = make_runtime();
    let src = r#"
      (defn init [] (rt-enable! "gi"))
      (defn tick [dt] (rt-enable! ""))
    "#;
    rt.load_clj("toggle", src).expect("compile + load");
    rt.call_init("toggle").expect("init");
    assert_eq!(rt.rt_recipe().as_deref(), Some("gi"));
    rt.call_tick("toggle", 16).expect("tick");
    assert_eq!(rt.rt_recipe(), None, "empty recipe falls back to raster");
}
