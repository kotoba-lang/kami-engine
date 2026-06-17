//! End-to-end: the gameka survivors gameSpec, authored in kami-clj, compiled
//! to WASM and driven on the kami-script-runtime host over many ticks.
//!
//! Proves the whole stack runs a real survivors loop — Clojure data/logic →
//! kami-clj compiler → wasmtime host imports → hecs world evolution.

use std::sync::{Arc, Mutex};

use kami_core::actor::components::Position;
use kami_script_runtime::{KamiScriptRuntime, Tag};

const SURVIVORS: &str = include_str!("survivors.clj");

fn world() -> Arc<Mutex<hecs::World>> {
    Arc::new(Mutex::new(hecs::World::new()))
}

fn count_tag(w: &hecs::World, tag: &str) -> usize {
    let mut q = w.query::<&Tag>();
    q.iter().filter(|(_, t)| t.0 == tag).count()
}

#[test]
fn survivors_core_loop_evolves() {
    let w = world();
    let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
    rt.set_seed(7); // deterministic run
    rt.load_clj("survivors", SURVIVORS).unwrap();
    rt.call_init("survivors").unwrap();

    // Run the full loop: every defsystem each tick, then integrate motion.
    for _ in 0..80 {
        rt.call_systems("survivors", 16).unwrap();
        rt.integrate(16);
    }

    let world = w.lock().unwrap();
    assert_eq!(count_tag(&world, "player"), 1, "exactly one player");

    // Wave-spawn produced enemies, bounded by the alive cap.
    let mut enemies = 0usize;
    let mut nearest = f32::MAX;
    {
        let mut q = world.query::<(&Tag, &Position)>();
        for (_, (t, p)) in q.iter() {
            if t.0 == "enemy" {
                enemies += 1;
                let d2 = p.0[0] * p.0[0] + p.0[1] * p.0[1];
                if d2 < nearest {
                    nearest = d2;
                }
            }
        }
    }
    assert!(enemies > 0, "wave spawning produced enemies");
    assert!(enemies < 200, "alive count stays under the cap (got {enemies})");
    // AI (move-toward over doseq-entities) + integration marched an enemy in
    // from the 300px spawn ring toward the player at the origin.
    assert!(
        nearest.sqrt() < 300.0,
        "an enemy advanced toward the player (nearest = {})",
        nearest.sqrt()
    );
}

#[test]
fn weapon_culls_enemies_in_range() {
    // Three enemies clustered on the player; the auto-fire system removes the
    // nearest in range each tick → population strictly drains to zero.
    let w = world();
    let mut rt = KamiScriptRuntime::new(w.clone()).unwrap();
    let src = r#"
        (defn init []
          (let [p (spawn-entity "player")]
            (set-position! p (f32 0.0) (f32 0.0) (f32 0.0)))
          (spawn-entity "enemy")
          (spawn-entity "enemy")
          (spawn-entity "enemy"))
        (defsystem weapon [dt]
          (let [hit (nearest-tagged "enemy" (f32 0.0) (f32 0.0) (f32 50.0))]
            (when (not= hit -1)
              (despawn-entity hit))))
    "#;
    rt.load_clj("g", src).unwrap();
    rt.call_init("g").unwrap();

    assert_eq!(count_tag(&w.lock().unwrap(), "enemy"), 3);
    rt.call_systems("g", 16).unwrap();
    assert_eq!(count_tag(&w.lock().unwrap(), "enemy"), 2, "one culled per fire");
    rt.call_systems("g", 16).unwrap();
    rt.call_systems("g", 16).unwrap();
    assert_eq!(count_tag(&w.lock().unwrap(), "enemy"), 0, "all enemies culled");
}
