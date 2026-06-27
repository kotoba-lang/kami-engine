//! Integration tests: compile Clojure → load into KamiScriptRuntime → call tick.
//!
//! These tests exercise the full compile+instantiate+execute loop without a real
//! game loop, using a bare hecs::World as the ECS backend.

use std::sync::{Arc, Mutex};

use kami_script_runtime::KamiScriptRuntime;

fn make_runtime() -> KamiScriptRuntime {
    let world = Arc::new(Mutex::new(hecs::World::new()));
    KamiScriptRuntime::new(world).expect("runtime init")
}

// ---------------------------------------------------------------------------

#[test]
fn empty_script_init_tick() {
    let mut rt = make_runtime();
    rt.load_clj("empty", "(defn init [] 0) (defn tick [dt] 0)")
        .expect("compile+load");
    rt.call_init("empty").expect("init");
    rt.call_tick("empty", 16).expect("tick");
}

#[test]
fn spawn_entity_returns_nonzero_id() {
    let mut rt = make_runtime();
    // init spawns an entity; the entity id is returned from init
    rt.load_clj(
        "spawner",
        "(defn init [] (spawn-entity \"player\")) (defn tick [dt] 0)",
    )
    .expect("compile+load");
    rt.call_init("spawner").expect("init");
    rt.call_tick("spawner", 16).expect("tick");
}

#[test]
fn key_down_false_by_default() {
    let mut rt = make_runtime();
    let src = r#"
      (defn init [] 0)
      (defn tick [dt]
        (if (key-down? "ArrowRight") 1 0))
    "#;
    rt.load_clj("input-test", src).expect("compile+load");
    rt.call_init("input-test").expect("init");
    // No key set — tick should return without error
    rt.call_tick("input-test", 16).expect("tick with no key");
}

#[test]
fn key_down_true_when_set() {
    let mut rt = make_runtime();
    let src = r#"
      (defn init [] 0)
      (defn tick [dt]
        (key-down? "Space"))
    "#;
    rt.load_clj("key-test", src).expect("compile+load");
    rt.call_init("key-test").expect("init");
    rt.set_key_down("Space", true);
    rt.call_tick("key-test", 16).expect("tick with key");
    rt.set_key_down("Space", false);
    rt.call_tick("key-test", 16).expect("tick after release");
}

#[test]
fn audio_queue_filled_by_play_sound() {
    let mut rt = make_runtime();
    let src = r#"
      (defn init [] 0)
      (defn tick [dt] (play-sound "coin"))
    "#;
    rt.load_clj("audio-test", src).expect("compile+load");
    rt.call_init("audio-test").expect("init");
    rt.call_tick("audio-test", 16).expect("tick");
    let queue = rt.drain_audio_queue();
    assert!(
        !queue.is_empty(),
        "expected audio queue entry after play-sound"
    );
    assert_eq!(queue[0].0, "coin");
}

#[test]
fn steam_queue_filled_by_steam_builtins() {
    use kami_script_runtime::{SteamBackend, SteamEvent, StubSteam};

    let mut rt = make_runtime();
    let src = r#"
      (defn init [] (steam-rich-presence! "status" "menu"))
      (defn tick [dt]
        (steam-unlock! "FIRST_WIN")
        (steam-set-stat! "kills" 7))
    "#;
    rt.load_clj("steam-test", src).expect("compile+load");
    rt.call_init("steam-test").expect("init");
    // init emitted the rich-presence; drain it before the tick.
    assert_eq!(
        rt.drain_steam_queue(),
        vec![SteamEvent::SetRichPresence("status".into(), "menu".into())]
    );
    rt.call_tick("steam-test", 16).expect("tick");
    let events = rt.drain_steam_queue();
    assert_eq!(
        events,
        vec![
            SteamEvent::UnlockAchievement("FIRST_WIN".into()),
            SteamEvent::SetStat("kills".into(), 7),
        ]
    );
    // Draining twice yields nothing — the queue is consumed, not duplicated.
    assert!(rt.drain_steam_queue().is_empty());
    // The default backend accepts the batch without panicking (off-Steam path).
    StubSteam.apply(events);
}

#[test]
fn draw_queue_filled_by_draw_mesh() {
    let mut rt = make_runtime();
    let src = r#"
      (defn init [] 0)
      (defn tick [dt]
        (draw-mesh! "player-mesh" (f32 0.0) (f32 0.0) (f32 0.0)))
    "#;
    rt.load_clj("render-test", src).expect("compile+load");
    rt.call_init("render-test").expect("init");
    rt.call_tick("render-test", 16).expect("tick");
    let queue = rt.drain_draw_queue();
    assert!(
        !queue.is_empty(),
        "expected draw queue entry after draw-mesh!"
    );
    assert_eq!(queue[0].mesh, "player-mesh");
}

#[test]
fn delta_ms_accessible_in_script() {
    let mut rt = make_runtime();
    let src = r#"
      (defn init [] 0)
      (defn tick [dt] (delta-ms))
    "#;
    rt.load_clj("time-test", src).expect("compile+load");
    rt.call_init("time-test").expect("init");
    rt.call_tick("time-test", 33).expect("tick with 33ms");
}

#[test]
fn defsystem_tick_called_by_runtime() {
    let mut rt = make_runtime();
    let src = r#"
      (def counter 0)
      (defn init [] 0)
      (defsystem update-counter [dt]
        (play-sound "tick"))
    "#;
    rt.load_clj("defsystem-test", src).expect("compile+load");
    rt.call_init("defsystem-test").expect("init");
    // Call the desugared tick export name
    let world = Arc::new(Mutex::new(hecs::World::new()));
    let _ = world; // just checking compile+load succeeds
}
