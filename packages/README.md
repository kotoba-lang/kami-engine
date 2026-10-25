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
