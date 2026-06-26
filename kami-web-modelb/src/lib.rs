//! Web Model-B — the VRM dance as a **compiled-CLJ game running in the browser**.
//!
//! Both halves of the dance run in the page's wasm32:
//! - **Native choreography**: `kami_live::scene::DanceScene` parses the shipped
//!   `:dance/*` EDN → a `LiveShow` whose `frame()` emits the render-IR.
//! - **Compiled-CLJ logic**: the shipped `dance/logic.clj` is compiled to WASM
//!   *in-page* and driven by `KamiScriptRuntime` on the **wasmi** (no-JIT)
//!   backend — i.e. the game's WASM runs *wasm-in-wasm* inside the browser's wasm.
//!
//! `WebDance::tick(dt)` advances both and returns the render-IR EDN string for the
//! web GPU executor (kami-webgpu) to draw. `fan_count()` proves the compiled
//! Clojure is live (its `seat-audience` system populates the world).
//!
//! Build: `wasm-pack build --target web kami-web-modelb` → load from the shell
//! HTML (see `web/index.html`).

use std::sync::{Arc, Mutex};

use wasm_bindgen::prelude::*;

use kami_script_runtime::{KamiScriptRuntime, Tag};

/// The shipped dance artifacts — choreography as data, behaviour as Clojure.
const DANCE_SCENE: &str = include_str!("../../kami-clj-play3d/games/dance/scene.edn");
const DANCE_LOGIC: &str = include_str!("../../kami-clj-play3d/games/dance/logic.clj");

/// A running web Model-B dance: native LiveShow + compiled-CLJ logic, both in-page.
#[wasm_bindgen]
pub struct WebDance {
    show: kami_live::scene::DanceScene,
    rt: KamiScriptRuntime,
    world: Arc<Mutex<hecs::World>>,
}

#[wasm_bindgen]
impl WebDance {
    /// Parse the dance scene.edn into a LiveShow, compile the dance logic.clj to
    /// WASM in-page, and run its `init`. Errors surface to JS.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<WebDance, JsValue> {
        let show = kami_live::scene::DanceScene::from_edn(DANCE_SCENE)
            .ok_or_else(|| JsValue::from_str("dance scene.edn failed to parse"))?;
        let world = Arc::new(Mutex::new(hecs::World::new()));
        let mut rt = KamiScriptRuntime::new(world.clone())
            .map_err(|e| JsValue::from_str(&format!("runtime init: {e:?}")))?;
        rt.set_seed(1);
        rt.load_clj("dance", DANCE_LOGIC)
            .map_err(|e| JsValue::from_str(&format!("compile dance logic.clj: {e:?}")))?;
        rt.call_init("dance")
            .map_err(|e| JsValue::from_str(&format!("init: {e:?}")))?;
        Ok(WebDance { show, rt, world })
    }

    /// Advance one frame (seconds): native LiveShow choreography + compiled-CLJ
    /// systems, in the browser's wasm. Returns the render-IR EDN to draw.
    pub fn tick(&mut self, dt: f32) -> String {
        let frame = self.show.frame(dt);
        let dt_ms = (dt * 1000.0).max(0.0) as i64;
        // the compiled-CLJ audience/performer systems, mutating the shared world
        let _ = self.rt.call_systems("dance", dt_ms);
        self.rt.integrate(dt_ms);
        frame.render_ir_edn()
    }

    /// Audience fans the compiled-CLJ `seat-audience` system has spawned — proof the
    /// compiled Clojure gameplay is running in-page.
    pub fn fan_count(&self) -> usize {
        let w = self.world.lock().unwrap();
        let mut q = w.query::<&Tag>();
        q.iter().filter(|(_, t)| t.0 == "fan").count()
    }
}
