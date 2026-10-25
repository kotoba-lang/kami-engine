# packages/

Imported `kotoba-lang/kami-*` repositories (ADR-0049). Each package is the source
repository's default-branch tip as of the commit in its `imports.json`, with
the deviations listed in `local-changes.json`. The old repositories are not archived by this change.

| Package | Source commit | What it is |
|---|---|---|
| [kami-scene-contracts](kami-scene-contracts) | `87918d6` | EDN authority for KAMI scene-domain data (catalog and per-domain contracts) |
| [kami-input-scene](kami-input-scene) | `3e840f5` | Default device-to-action input-binding maps (`fps` / `graph`) as portable CLJC |
| [kami-skeleton-scene](kami-skeleton-scene) | `58d4d3a` | Humanoid skeleton scene data with clip, PMX and VMD helpers as portable CLJC |
| [kami-app](kami-app) | `a5d45be` | builder SDK (KamiApp, RenderPipeline trait, Camera, RAF) CLJC — restored from legacy kami-engine Rust crate (deleted #259) as zero-dep portable CLJC. Part of the clj-wgsl migration (ADR-2607010930). |
| [kami-atmosphere-scene](kami-atmosphere-scene) | `a06aaf9` | EDN authoring surface for kami-atmosphere weather config, merges weather.edn presets onto kotoba-lang/atmosphere defaults. Restored from legacy kami-atmosphere-scene crate (deleted #82). ADR-2607010930. |
| [kami-postfx-scene](kami-postfx-scene) | `585f2f0` | EDN authoring surface for kami-postfx post-processing presets (nintendo/retro/final-fantasy/baminiku-character). Restored from legacy kami-postfx-scene crate (deleted #82). ADR-2607010930. |
| [kami-terrain-scene](kami-terrain-scene) | `1087f93` | EDN authoring surface for kami-terrain biome presets (heightmap/splat/palette) and default Gerstner ocean waves. Referenced by name (resolve_biome) from kami-app-isekai/kami-app-quarry-walk. Restored from legacy kami-terrain-scene crate (deleted #82). ADR-2607010930. |
| [kami-vegetation-scene](kami-vegetation-scene) | `d7ba32c` | EDN authoring surface for kami-vegetation TAXONOMIC PROFILE presets (grass/fern/palm/conifer/bush/cactus/moss). Restored from legacy kami-vegetation-scene crate (deleted #82). ADR-2607010930. |
| [kami-app-amenominaka](kami-app-amenominaka) | `ae215e9` | amenominaka (天之御中) — Omniverse Kit-equivalent app shell path reservation, kotoba .cljc |
| [kami-engine-animation](kami-engine-animation) | `33ac2ab` | Portable animation engine for the Kami family: timelines, tracks and keyframes as immutable EDN, with scalar interpolation and timeline evaluation as the first stable surface. |
| [kami-app-animator](kami-app-animator) | `bd00b69` | Imported `kami-app-animator` (no repository description) |
| [kami-app-bim-editor](kami-app-bim-editor) | `544cb5d` | Imported `kami-app-bim-editor` (no repository description) |
| [kami-app-cad](kami-app-cad) | `db874d8` | Browser NURBS/CAD authoring workspace: loft sections, a dimension-constrained sketch, extruded solids with limited Boolean ops, feature history and EDN projects. Geometry comes from kami-engine-cad, rendering from kotoba-lang/webgpu; requires WebGPU. |
| [kami-app-character-creator](kami-app-character-creator) | `8e776d9` | EDN-native VRM character creator (ADR-2607031200) — Phase 1: CharacterDoc -> real .vrm export, built on kotoba-lang/{vrm,character} |
| [kami-engine-cae-solver](kami-engine-cae-solver) | `92dcd10` | CAE solver contract — solve multimethod dispatch (:rom ↔ :lbm) |
| [kami-app-hygaccess-plant](kami-app-hygaccess-plant) | `a7451e7` | Mixing-tank CFD simulation (nagare + kami-engine-cae-solver) for the sodium-hypochlorite dilution step of cloud-itonami-hygiene-access's water-purification-drops |
| [kami-engine-modeling](kami-engine-modeling) | `c514bc8` | Imported `kami-engine-modeling` (no repository description) |
| [kami-app-modeler](kami-app-modeler) | `091b260` | Imported `kami-app-modeler` (no repository description) |
| [kami-app-sculpt](kami-app-sculpt) | `5f4a69c` | Imported `kami-app-sculpt` (no repository description) |
| [kami-mangaka-page](kami-mangaka-page) | `555e18a` | kami-mangaka-page domain logic, split out of kami-engine's kami-mangaka-page-clj (ADR-2607010930). |
| [kami-app-sip](kami-app-sip) | `54ae09a` | kami-app-sip (SIP game) domain logic, split out of kami-engine's kami-app-sip-clj (ADR-2607010930). |
| [kami-app-suji](kami-app-suji) | `d6347be` | 姿勢がつくる負荷を、ブラウザ内で解いて 3D で見せる single-page app（suji + WebGPU） |
| [kami-autodrive-scene](kami-autodrive-scene) | `8922de8` | EDN authoring surface for kami-autodrive per-vehicle-class GNC presets (limits + autopilot tuning). Restored from legacy kami-autodrive-scene crate (deleted #82). ADR-2607010930. |
| [kami-cam-scene](kami-cam-scene) | `c40307f` | EDN authoring surface for kami-cam (kotoba-lang/cnc) STOCK MATERIAL presets — tolerant EDN accessors from kotoba-lang/scene, parity-tested against kotoba.cam.stock/material-presets. Restored from legacy kami-cam-scene crate (deleted #82). ADR-2607010930. |
| [kami-character-scene](kami-character-scene) | `9dcf246` | EDN authoring surface for kami-character MetaHuman hair-style presets, FACS face-rig, and default animation blueprint. Restored from legacy kami-character-scene crate (deleted #82). ADR-2607010930. |
| [kami-eizo-timeline](kami-eizo-timeline) | `226e1c1` | Portable .cljc EDL/timeline data model (track/clip/transition/marker) + SMPTE drop-frame timecode — the missing NLE data model, ADR-2607121400 |
| [kami-ongaku-project](kami-ongaku-project) | `6dccb9e` | DAW-session data model (track/bus-graph/clip/automation/tempo-map) — L3 SSoT of the ongaku engine stack, ADR-2607121400 |
| [kami-creative-studio](kami-creative-studio) | `2652dd3` | Imported `kami-creative-studio` (no repository description) |
| [kami-eizo-compositor](kami-eizo-compositor) | `2e4382d` | VFX compositing node data model (chroma-key/roto/tracking/blend) — kotoba-lang eizo domain, ADR-2607121400 |
| [kami-eizo-grade](kami-eizo-grade) | `f960c23` | ASC CDL / monotone curves / .cube LUT / waveform+vectorscope color-grading data model (kami-eizo family, ADR-2607121400) |
| [kami-engine-vphysics](kami-engine-vphysics) | `7ff753e` | shared vehicle-physics primitives (road load, aero force, range) |
| [kami-engine-aero](kami-engine-aero) | `fabfd90` | reduced-order aero Cd solver (component build-up) + design range loop |
| [kami-engine-core](kami-engine-core) | `dcefa5a` | engine core — restored from legacy kami-engine Rust crate (deleted #259) as zero-dep portable CLJC. Part of the clj-wgsl migration (ADR-2607010930). |
| [kami-engine-crash](kami-engine-crash) | `849a2be` | reduced-order frontal-crash solver (:rom-crash) |
| [kami-engine-echem](kami-engine-echem) | `8b7b449` | reduced-order PEM fuel-cell solver (:rom-fc) |
| [kami-engine-engine](kami-engine-engine) | `66fd75a` | umbrella (World+Clock+Net) — restored from legacy kami-engine Rust crate (deleted #259) as zero-dep portable CLJC. Part of the clj-wgsl migration (ADR-2607010930). |
| [kami-engine-io](kami-engine-io) | `87868b4` | engine I/O — restored from legacy kami-engine Rust crate (deleted #259) as zero-dep portable CLJC. Part of the clj-wgsl migration (ADR-2607010930). |
| [kami-engine-motor](kami-engine-motor) | `c465afd` | reduced-order traction-motor solver (:rom-motor) |
| [kami-engine-pack](kami-engine-pack) | `770965c` | Reduced-order lithium pack solver (:rom-pack) — pack-level energy with Peukert rate derating and IR sag. Power+endurance -> pack mass. |
| [kami-engine-rotor](kami-engine-rotor) | `41ba842` | Reduced-order rotor solver (:rom-rotor) — momentum theory + figure of merit. Thrust → diameter, RPM, shaft power, torque, blade loading. |
| [kami-engine-multirotor](kami-engine-multirotor) | `97377fa` | The multirotor mass spiral: payload + endurance -> MTOW, or a refusal. Over kami-engine-rotor/-pack/-motor. |
| [kami-engine-render](kami-engine-render) | `87f0fd6` | engine render (wgpu bootstrap + scene pipelines CLJC IR) — restored from legacy kami-engine Rust crate (deleted #259) as zero-dep portable CLJC. Part of the clj-wgsl migration (ADR-2607010930). |
| [kami-engine-script-runtime](kami-engine-script-runtime) | `55aefca` | script runtime (wasmtime/wasmi dual backend host CLJC contracts) — wasmi no-JIT invariant. restored from legacy kami-engine Rust crate (deleted #259) as zero-dep portable CLJC. Part of the clj-wgsl migration (ADR-2607010930). |
| [kami-engine-vehicle-designer](kami-engine-vehicle-designer) | `84da8ae` | Clean-sheet BEV/FCEV vehicle design actor — concept proposer sealed behind a PhysicsGovernor, plus Isaac/CAE sim-verify and datafied BOM→CAM→assembly (langgraph StateGraph). |
| [kami-engine-web](kami-engine-web) | `1bc9526` | engine web entry — restored from legacy kami-engine Rust crate (deleted #259) as zero-dep portable CLJC. Part of the clj-wgsl migration (ADR-2607010930). |
| [kami-flow](kami-flow) | `9e8d356` | KAMI Flow — RTL->PnR->GDSII->Verify/Power/DFT/SI/Yield->STA->DRC/LVS->Signoff orchestration pipeline. Restored from legacy kami-flow crate (deleted #82). ADR-2607010930. |
| [kami-game-scene](kami-game-scene) | `f7f97c5` | EDN authoring surface for kami-game's ANIMATION-STATE presets, Godot game catalog, brainrot evolution/characters, Sabiotoshi item catalog, battle-royale storm/consumables/weapons, and Pokoa dex/items. Restored from legacy kami-game-scene crate (deleted #82). ADR-2607010930. |
| [kami-gen-hybrid](kami-gen-hybrid) | `4082aa2` | Imported `kami-gen-hybrid` (no repository description) |
| [kami-gen-ml3d](kami-gen-ml3d) | `e7388b6` | Imported `kami-gen-ml3d` (no repository description) |
| [kami-gen-procedural](kami-gen-procedural) | `ec78fba` | Imported `kami-gen-procedural` (no repository description) |
| [kami-genre-base-systems](kami-genre-base-systems) | `c9abd0e` | Base/foundation gameplay systems, one per PlayStation-Store-style genre category (generic archetypes, kami-clj subset) |
| [kami-mangaka-expression](kami-mangaka-expression) | `96fc3ce` | kami.mangaka.expression + patterns EDN (ADR-2607102200 addendum 12) |
| [kami-mangaka-reader](kami-mangaka-reader) | `01823ab` | kami-mangaka-reader domain logic (ClojureScript/shadow-cljs), split out of kami-engine's kami-mangaka-reader-clj (ADR-2607010930). |
| [kami-sensor-sim](kami-sensor-sim) | `caae678` | Camera / lidar / IMU / contact sensor synth, isaacsim.sensors API-compat target (analytic CPU sensor models: pinhole camera projection+depth, analytic ray-cast lidar VLP-16/toy, body-frame IMU, sphere-vs-primitive contact). Restored from legacy kami-sensor-sim crate (deleted #82). ADR-2607010930. |
| [kami-solar-helix-scene](kami-solar-helix-scene) | `fcc22d7` | Imported `kami-solar-helix-scene` (no repository description) |
| [kami-studio](kami-studio) | `5693307` | Imported `kami-studio` (no repository description) |
