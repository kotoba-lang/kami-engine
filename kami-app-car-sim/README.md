# kami-app-car-sim

Public car-sim demo at <https://driver.gftd.ai>.

A thin per-game crate (per the kami-app builder convention) that:

1. instantiates a `kami_vehicle::Vehicle` from `models::garage::build()`,
2. constructs three custom `wgpu` pipelines for rendering,
3. drives the simulation from JS-side `window.__carsim_*` globals
   (throttle / brake / steer / handbrake / gear / detach / repair),
4. publishes telemetry to `window.__carsim_hud` each tick.

## Pipelines

| Pipeline | Topology | Purpose |
|---|---|---|
| `line_pipeline` | LineList | Beam wireframe (chassis, suspension, tire ring), ground grid, stress colouring |
| `tri_pipeline` | TriangleList, alpha-blend, double-sided | Filled body panels (paint / window glass / underbody) + tire side-walls + tread bands |
| `ground_pipeline` | TriangleList | Multi-zone surface map with surface-id-driven procedural texture (asphalt grain + lane markings, sand dunes, ice cracks, mud ruts, grass tufts, etc.) — **all procedural in WGSL, no PNG/JPG assets** |

Single uniform buffer (`Uniforms { view_proj, cam_pos, color, light_dir }`)
shared across all pipelines.

## Map (`MapGround::demo_circuit`)

```
        +z (forward)
   ┌─────────────────────────┐
   │ Snow ─[asphalt]─ Mud    │
   │      ──[snow]──         │
   │      ──[ice]───         │
   │      ──[wet]───         │
   │      ──[dry]───         │   default = grass
   │ Mud ─[asphalt]─ Sand    │
   └─────────────────────────┘
        -z (back)
```

`map.surface_at(car.com.x, car.com.z)` is queried every frame and
published to `window.__carsim_current_surface` for the HUD.

## Build + deploy

```bash
cd 40-engine/kami-engine
wasm-pack build --target web --out-dir /tmp/kami-app-car-sim-pkg kami-app-car-sim
# → /tmp/kami-app-car-sim-pkg/{kami_app_car_sim.js, _bg.wasm, .d.ts}

cp /tmp/kami-app-car-sim-pkg/kami_app_car_sim* \
   ../../60-apps/ai-gftd-project-car-sim/appview/ai-gftd-wasm-car-sim-c4r51m00/svelte/build/

cd ../../60-apps/ai-gftd-project-car-sim/appview/ai-gftd-wasm-car-sim-c4r51m00
gftd deploy
```

WASM size: ~255 KB (with all 3 pipelines + procedural shaders).

## Controls (HTML side)

| Input | Action |
|---|---|
| `W` / `↑` | throttle |
| `S` / `↓` | brake |
| `A` / `↓` / `D` / `→` | steer (auto-centring) |
| `Space` | handbrake |
| `R` | reverse gear |
| `1`–`6` | manual gear |
| Mouse drag on canvas | orbit camera |
| Mouse wheel | zoom |

URL parameters: `?vehicle=sports&paint=%23ffd400` for a yellow turbo RWD.

## Reference

* Crate (physics): `40-engine/kami-engine/kami-vehicle/`
* Worker scaffolding: `60-apps/ai-gftd-project-car-sim/appview/ai-gftd-wasm-car-sim-c4r51m00/`
* Project entry in `deps.toml`: `[[projects]] name = "car-sim"`
