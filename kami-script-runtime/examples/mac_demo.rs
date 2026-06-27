//! mac_demo — run a CLJ/EDN game on this Mac, end to end.
//!
//! This is the "mac" target of ADR-0037's six, exercised for real: a game
//! written in the kami-clj subset is compiled to WASM, loaded into the host,
//! and ticked while we feed it touch input through the device-neutral
//! `input_map` seam — exactly the path iOS/Android/console will use, minus the
//! GPU (headless, so it runs anywhere with no window).
//!
//! Run under each backend to see the SAME game produce the SAME trace:
//!   cargo run --target aarch64-apple-darwin --example mac_demo
//!   cargo run --target aarch64-apple-darwin --example mac_demo \
//!       --no-default-features --features backend-wasmi

use std::sync::{Arc, Mutex};

use kami_core::actor::components::{Position, Velocity};
use kami_script_runtime::{BACKEND, KamiScriptRuntime, Tag, VirtualStick};

// A whole little game in the kami-clj subset: a player entity whose velocity
// each tick is driven by the abstract "MoveX"/"MoveY" axes the host feeds.
const GAME: &str = r#"
    (defentity player []
      (set-position! self (f32 0.0) (f32 0.0) (f32 0.0)))
    (defn init [] (player))
    (defsystem drive [dt]
      (doseq-entities [e "player"]
        (set-velocity! e (axis "MoveX") (axis "MoveY") (f32 0.0))))
"#;

fn player_pos(world: &Arc<Mutex<hecs::World>>) -> [f32; 3] {
    let w = world.lock().unwrap();
    let mut q = w.query::<(&Tag, &Position)>();
    q.iter()
        .find(|(_, (t, _))| t.0 == "player")
        .map(|(_, (_, p))| p.0)
        .unwrap_or([0.0; 3])
}

fn main() {
    println!("=== kami CLJ/EDN game on Mac — backend: {BACKEND} ===\n");

    let world = Arc::new(Mutex::new(hecs::World::new()));
    let mut rt = KamiScriptRuntime::new(world.clone()).expect("runtime");
    rt.set_seed(42); // deterministic: both backends trace identically

    rt.load_clj("demo", GAME).expect("compile+load CLJ game");
    rt.call_init("demo").expect("init");
    println!("spawned player at {:?}", player_pos(&world));

    // A virtual thumbstick centred at (100,100), radius 50 — what an iOS/Android
    // touch HUD would own. We script a touch path: hold right, then up-right.
    let stick = VirtualStick::new([100.0, 100.0], 50.0);
    let touches = [
        [150.0, 100.0], // full right
        [150.0, 100.0],
        [135.0, 65.0], // up-right
        [135.0, 65.0],
    ];

    let dt_ms = 100; // 10 Hz for a readable trace
    for (frame, touch) in touches.iter().enumerate() {
        let axes = stick.axes(*touch);
        rt.feed_stick("MoveX", "MoveY", axes); // device → abstract axes
        rt.call_systems("demo", dt_ms).expect("systems"); // guest sets velocity
        rt.integrate(dt_ms); // host Euler step

        let p = player_pos(&world);
        let v = {
            let w = world.lock().unwrap();
            let mut q = w.query::<(&Tag, &Velocity)>();
            q.iter()
                .find(|(_, (t, _))| t.0 == "player")
                .map(|(_, (_, vel))| vel.0)
                .unwrap_or([0.0; 3])
        };
        println!(
            "frame {frame}: touch {touch:?} → axes [{:+.2}, {:+.2}]  vel [{:+.2}, {:+.2}]  pos [{:+.3}, {:+.3}]",
            axes[0], axes[1], v[0], v[1], p[0], p[1],
        );
    }

    let p = player_pos(&world);
    println!("\nfinal player pos: [{:+.3}, {:+.3}]", p[0], p[1]);
    println!("✓ a CLJ game compiled to WASM and ran on this Mac via the {BACKEND} host.");
}
