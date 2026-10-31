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
| [kami-app-animator](kami-app-animator) | `bd00b69` | Imported `kami-app-animator` (no repository description) |
| [kami-app-bim-editor](kami-app-bim-editor) | `544cb5d` | Imported `kami-app-bim-editor` (no repository description) |
| [kami-app-cad](kami-app-cad) | `db874d8` | Browser NURBS/CAD authoring workspace: loft sections, a dimension-constrained sketch, extruded solids with limited Boolean ops, feature history and EDN projects. Geometry comes from kami-engine-cad, rendering from kotoba-lang/webgpu; requires WebGPU. |
| [kami-app-character-creator](kami-app-character-creator) | `8e776d9` | EDN-native VRM character creator (ADR-2607031200) — Phase 1: CharacterDoc -> real .vrm export, built on kotoba-lang/{vrm,character} |
| [kami-app-hygaccess-plant](kami-app-hygaccess-plant) | `a7451e7` | Mixing-tank CFD simulation (nagare + kami-engine-cae-solver) for the sodium-hypochlorite dilution step of cloud-itonami-hygiene-access's water-purification-drops |
| [kami-app-modeler](kami-app-modeler) | `091b260` | Imported `kami-app-modeler` (no repository description) |
| [kami-app-sculpt](kami-app-sculpt) | `5f4a69c` | Imported `kami-app-sculpt` (no repository description) |
| [kami-app-sip](kami-app-sip) | `54ae09a` | kami-app-sip (SIP game) domain logic, split out of kami-engine's kami-app-sip-clj (ADR-2607010930). |
| [kami-app-suji](kami-app-suji) | `d6347be` | 姿勢がつくる負荷を、ブラウザ内で解いて 3D で見せる single-page app（suji + WebGPU） |
| [kami-autodrive-scene](kami-autodrive-scene) | `8922de8` | EDN authoring surface for kami-autodrive per-vehicle-class GNC presets (limits + autopilot tuning). Restored from legacy kami-autodrive-scene crate (deleted #82). ADR-2607010930. |
| [kami-cam-scene](kami-cam-scene) | `c40307f` | EDN authoring surface for kami-cam (kotoba-lang/cnc) STOCK MATERIAL presets — tolerant EDN accessors from kotoba-lang/scene, parity-tested against kotoba.cam.stock/material-presets. Restored from legacy kami-cam-scene crate (deleted #82). ADR-2607010930. |
| [kami-character-scene](kami-character-scene) | `9dcf246` | EDN authoring surface for kami-character MetaHuman hair-style presets, FACS face-rig, and default animation blueprint. Restored from legacy kami-character-scene crate (deleted #82). ADR-2607010930. |
| [kami-creative-studio](kami-creative-studio) | `2652dd3` | Imported `kami-creative-studio` (no repository description) |
| [kami-engine-core](kami-engine-core) | `dcefa5a` | engine core — restored from legacy kami-engine Rust crate (deleted #259) as zero-dep portable CLJC. Part of the clj-wgsl migration (ADR-2607010930). |
| [kami-engine-engine](kami-engine-engine) | `66fd75a` | umbrella (World+Clock+Net) — restored from legacy kami-engine Rust crate (deleted #259) as zero-dep portable CLJC. Part of the clj-wgsl migration (ADR-2607010930). |
| [kami-engine-io](kami-engine-io) | `87868b4` | engine I/O — restored from legacy kami-engine Rust crate (deleted #259) as zero-dep portable CLJC. Part of the clj-wgsl migration (ADR-2607010930). |
| [kami-engine-render](kami-engine-render) | `87f0fd6` | engine render (wgpu bootstrap + scene pipelines CLJC IR) — restored from legacy kami-engine Rust crate (deleted #259) as zero-dep portable CLJC. Part of the clj-wgsl migration (ADR-2607010930). |
| [kami-engine-script-runtime](kami-engine-script-runtime) | `55aefca` | script runtime (wasmtime/wasmi dual backend host CLJC contracts) — wasmi no-JIT invariant. restored from legacy kami-engine Rust crate (deleted #259) as zero-dep portable CLJC. Part of the clj-wgsl migration (ADR-2607010930). |
| [kami-engine-web](kami-engine-web) | `1bc9526` | engine web entry — restored from legacy kami-engine Rust crate (deleted #259) as zero-dep portable CLJC. Part of the clj-wgsl migration (ADR-2607010930). |
| [kami-game-scene](kami-game-scene) | `f7f97c5` | EDN authoring surface for kami-game's ANIMATION-STATE presets, Godot game catalog, brainrot evolution/characters, Sabiotoshi item catalog, battle-royale storm/consumables/weapons, and Pokoa dex/items. Restored from legacy kami-game-scene crate (deleted #82). ADR-2607010930. |
| [kami-genre-base-systems](kami-genre-base-systems) | `c9abd0e` | Base/foundation gameplay systems, one per PlayStation-Store-style genre category (generic archetypes, kami-clj subset) |
| [kami-solar-helix-scene](kami-solar-helix-scene) | `fcc22d7` | Imported `kami-solar-helix-scene` (no repository description) |
| [kami-studio](kami-studio) | `5693307` | Imported `kami-studio` (no repository description) |
| [kami-vehicle-scene](kami-vehicle-scene) | `da9a156` | EDN authoring surface for kami-vehicle ground/garage/powertrain CONFIG (surface grip table, demo-circuit zone map, engine/gearbox/tire presets, 6 garage SedanSpecs). Restored from legacy kami-vehicle-scene crate (kotoba-lang/kami-engine, deleted PR #82). ADR-2607010930. |

## Removed duplicates (JUN-649)

These packages were imported here and into the kotoba monorepo from the same
upstream commit, and their code was identical. The placement rule puts them in
kotoba `lib/` (`docs/migration-placement.json`), so the copies here were removed:

| package | upstream commit | source of truth |
|---|---|---|
| kami-ongaku-project | `6dccb9e` | kotoba `lib/kotoba.kamiOngakuProject` |
| kami-mangaka-expression | `96fc3ce` | kotoba `lib/kotoba.kamiMangakaExpression` |
| kami-sensor-sim | `caae678` | kotoba `lib/kotoba.kamiSensorSim` |

## Moved to kotoba lib/ (JUN-649)

Used outside a game engine, so placed in the kotoba monorepo (`docs/migration-placement.json`).
Moved with their provenance (`imports.json`) and local changes:

| package | upstream commit | now |
|---|---|---|
| cad | `161b152` | kotoba `lib/kotoba.cad` |
| kami-engine-cae-solver | `92dcd10` | kotoba `lib/kotoba.kamiEngineCaeSolver` |
| kami-engine-vphysics | `7ff753e` | kotoba `lib/kotoba.kamiEngineVphysics` |
| kami-engine-aero | `fabfd90` | kotoba `lib/kotoba.kamiEngineAero` |
| kami-engine-crash | `849a2be` | kotoba `lib/kotoba.kamiEngineCrash` |
| kami-engine-echem | `8b7b449` | kotoba `lib/kotoba.kamiEngineEchem` |
| kami-engine-motor | `c465afd` | kotoba `lib/kotoba.kamiEngineMotor` |
| kami-engine-pack | `770965c` | kotoba `lib/kotoba.kamiEnginePack` |
| kami-engine-rotor | `41ba842` | kotoba `lib/kotoba.kamiEngineRotor` |
| kami-engine-multirotor | `97377fa` | kotoba `lib/kotoba.kamiEngineMultirotor` |
| kami-engine-vehicle-designer | `84da8ae` | kotoba `lib/kotoba.kamiEngineVehicleDesigner` |
| anime | `f2eb7a9` | kotoba `lib/kotoba.anime` |
| douga | `93639cd` | kotoba `lib/kotoba.douga` |
| kami-eizo-timeline | `226e1c1` | kotoba `lib/kotoba.kamiEizoTimeline` |
| kami-eizo-compositor | `2e4382d` | kotoba `lib/kotoba.kamiEizoCompositor` |
| kami-eizo-grade | `f960c23` | kotoba `lib/kotoba.kamiEizoGrade` |
| kami-mangaka-page | `555e18a` | kotoba `lib/kotoba.kamiMangakaPage` |
| kami-mangaka-reader | `01823ab` | kotoba `lib/kotoba.kamiMangakaReader` |
| kami-vehicle | `50d5e29` | kotoba `lib/kotoba.kamiVehicle` |
| biomech | `c85ffb9` | kotoba `lib/kotoba.biomech` |
| freeboard | `11510d5` | kotoba `lib/kotoba.freeboard` |
| kami-engine-animation | `33ac2ab` | kotoba `lib/kotoba.kamiEngineAnimation` |
| kami-engine-modeling | `c514bc8` | kotoba `lib/kotoba.kamiEngineModeling` |
| kami-flow | `9e8d356` | kotoba `lib/kotoba.kamiFlow` |
| kami-gen-hybrid | `4082aa2` | kotoba `lib/kotoba.kamiGenHybrid` |
| kami-gen-ml3d | `e7388b6` | kotoba `lib/kotoba.kamiGenMl3d` |
| kami-gen-procedural | `ec78fba` | kotoba `lib/kotoba.kamiGenProcedural` |
| kami-web | `2c4064b` | kotoba `lib/kotoba.kamiWeb` |
| swarm-choreo | `fe69e71` | kotoba `lib/kotoba.swarmChoreo` |
