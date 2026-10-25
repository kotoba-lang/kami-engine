# packages/

Imported `kotoba-lang/kami-*` repositories (ADR-0049). Each package is the source
repository's default-branch tip as of the commit in its `imports.json`, with
the deviations listed in `local-changes.json`. The old repositories are not archived by this change.

| Package | Source commit | What it is |
|---|---|---|
| [kami-scene-contracts](kami-scene-contracts) | `87918d6` | EDN authority for KAMI scene-domain data (catalog and per-domain contracts) |
| [kami-input-scene](kami-input-scene) | `3e840f5` | Default device-to-action input-binding maps (`fps` / `graph`) as portable CLJC |
| [kami-skeleton-scene](kami-skeleton-scene) | `58d4d3a` | Humanoid skeleton scene data with clip, PMX and VMD helpers as portable CLJC |
