//! Model-B composition: the **native** kami-live `LiveShow` (choreography authored
//! as `:dance/*` EDN data) and the **compiled-CLJ** interactive logic (the shipped
//! `dance/logic.clj`, audience + performer) run in ONE host `hecs::World` and tick
//! loop — the VRM dance as a compiled-CLJ Model-B game.
//!
//! This closes the gap noted in the Model-B assessment: until now the dance show
//! ran natively and the dance `logic.clj` was loaded but never *composed* with the
//! show in one loop. Both shipped artifacts are used verbatim (`include_str!`), so
//! this proves the real dance scene + the real dance logic compose end-to-end.

use std::sync::{Arc, Mutex};

use kami_script_runtime::{KamiScriptRuntime, Tag};

/// The shipped dance artifacts — choreography as data, behaviour as compiled CLJ.
const DANCE_SCENE: &str = include_str!("../../kami-clj-play3d/games/dance/scene.edn");
const DANCE_LOGIC: &str = include_str!("../../kami-clj-play3d/games/dance/logic.clj");

fn count_tag(w: &hecs::World, tag: &str) -> usize {
    w.query::<&Tag>().iter().filter(|(_, t)| t.0 == tag).count()
}

#[test]
fn dance_show_and_clj_logic_compose() {
    // 1. Native choreography: kami-live parses the shipped scene.edn into a LiveShow.
    let mut show =
        kami_live::scene::DanceScene::from_edn(DANCE_SCENE).expect("dance scene.edn parses into a LiveShow");

    // 2. Compiled-CLJ interactive logic: the shipped dance logic.clj on the script host.
    let world = Arc::new(Mutex::new(hecs::World::new()));
    let mut rt = KamiScriptRuntime::new(world.clone()).expect("runtime");
    rt.set_seed(1); // deterministic
    rt.load_clj("dance", DANCE_LOGIC).expect("dance logic.clj compiles to WASM + loads");
    rt.call_init("dance").expect("init: spawn the performer");

    // 3. ONE host loop ticks BOTH each frame — native show + compiled-CLJ logic.
    let mut show_frames = 0usize;
    for _ in 0..120 {
        // native beat-grid + setlist + performer pose → render-IR
        let frame = show.frame(0.016);
        if frame.render_ir.as_map().map_or(false, |m| !m.is_empty()) {
            show_frames += 1;
        }
        // compiled-CLJ audience/performer systems, mutating the shared world
        rt.call_systems("dance", 16).expect("clj systems tick");
        rt.integrate(16);
    }

    // The native LiveShow produced a render-IR every frame...
    assert_eq!(show_frames, 120, "the native LiveShow drew the show each frame");

    // ...and the compiled-CLJ logic populated the SAME world: one performer + a
    // seated audience ring. Both halves of the Model-B dance ran, composed.
    let w = world.lock().unwrap();
    assert_eq!(count_tag(&w, "performer"), 1, "CLJ init spawned exactly one performer");
    assert!(count_tag(&w, "fan") > 0, "CLJ seat-audience spawned audience fans");
}
