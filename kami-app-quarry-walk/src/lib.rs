//! kami-app-quarry-walk — second reference game validating the
//! `kami-app` Builder + `kami-pipelines` topology for multi-game reuse.
//!
//! ```js
//! import init, { run_quarry_walk_v2 } from './kami_app_quarry_walk.js';
//! await init(); await run_quarry_walk_v2('canvas');
//! ```
//!
//! No pipeline code here: game logic = biome selection + spawn +
//! camera/input wiring. All rendering reuses `kami_pipelines`.

use kami_app::{CameraMode, InputMode, KamiApp, Position};
use log::Level;

#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub async fn run_quarry_walk_v2(canvas_id: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(Level::Info);

    // Quarry biome peaks ≈ 120 m. Spawn 150 m up on the central ridge
    // so the player looks down over the formation.
    let spawn = Position::new(0.0, 140.0, 120.0);

    let app = KamiApp::new_web(canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?
        .with_label("quarry-walk")
        .with_hud_publish(true)
        .with_camera(CameraMode::FirstPerson {
            spawn,
            yaw: 0.0,
            pitch: -0.4,
        })
        .with_input(InputMode::WasdFps);

    // Shared pipelines from kami-pipelines. Swap biome → different
    // terrain palette + placement tolerances; no rendering code change.
    let sky = kami_pipelines::SkyAdapter::new(app.render_context());
    // Executor edge (ADR-0044/0046): build the terrain from kami-terrain-scene's
    // biomes.edn, falling back to the compiled-in BiomePreset::Quarry if the EDN fails.
    let terrain = match kami_terrain_scene::resolve_biome("quarry") {
        Some(b) => kami_pipelines::TerrainAdapter::streaming_with_config(
            app.render_context(),
            b.to_heightmap_config(77.0),
            b.to_splat_thresholds(),
            b.to_material_palette(),
            128,
            2,
        ),
        None => {
            log::warn!("[quarry-walk] biomes.edn 'quarry' unavailable; using builtin BiomePreset");
            kami_pipelines::TerrainAdapter::streaming(
                app.render_context(),
                kami_terrain::BiomePreset::Quarry,
                77.0,
                128,
                2,
            )
        }
    };

    // Quarry biome sand_line=5, so sea level ≈ 4 keeps water in valleys.
    let water = kami_pipelines::WaterAdapter::new(app.render_context(), 1024.0, 4.0);

    log::info!("[quarry-walk-v2] backend={:?}", app.backend());
    app.with_pipeline(sky)
        .with_pipeline(terrain)
        .with_pipeline(water)
        .run()
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))
}
