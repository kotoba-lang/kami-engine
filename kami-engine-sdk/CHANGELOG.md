# Changelog

All notable changes to `@etzhayyim/kami-engine-sdk` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] — 2026-05-26 cutover

The 2026-05-26 cutover is a **breaking change** that retires three.js from every layer of the SDK. Consumers that depended on the three.js peer-dep surface (the `./spark` 3DGS demos, the `mountIncidentScene` WebVR renderer, the `ThreeVrmHandle` type) need to migrate before upgrading.

Anchor ADRs:

- [ADR-2605264300](../../../90-docs/adr/2605264300-kami-engine-sdk-three-free-cutover.md) — full-SDK three.js-free cutover
- [ADR-2605265200](../../../90-docs/adr/2605265200-kami-engine-sdk-20-actors-legacy-duplicate-retirement.md) — 20-actors duplicate retirement (3 phases)
- [ADR-0031](../../../90-docs/adr/0031-kami-vrm-three-free-topology.md) — parent decision (2026-04-18 VRM-only three-free)

### Removed (breaking)

- **`src/lib/spark/`** — the four sparkjs.dev-equivalent 3DGS demos (`mountSplatCloud`, `mountGaussianEllipsoid`, `mountTemporalSplat4D`, `mountDynoSample`) are removed. The `./spark` subpath export is gone. Canonical 3DGS rendering goes through `kami-pipelines::GsplatAdapter` (Rust + wgpu WGSL EWA + SH degree 0–3). The browser-side `./gsplat` XRPC + WASM bridge survives unchanged.

- **`src/lib/webvr/{webvr-scene,node-effects}.ts`** — the three.js-based scene renderer and node-effects registry (~1,800 LoC) are removed. The `mountIncidentScene`, `MountOpts`, and `SceneHandle` public exports are gone.

- **`ThreeVrmHandle` interface** — the `unknown`-typed stub representing the retired three-vrm renderer handle is removed from the public type surface. `DualEngineState.three` field is also removed.

- **`./spark` subpath export** in `package.json` — removed.

- **`declare module 'three'`** ambient declaration in `src/ambient.d.ts` — removed (was needed for the SDK to type-check without `@types/three`; no longer applicable).

### Changed (breaking)

- **`@langchain/langgraph` + `@langchain/core` are now MANDATORY peer dependencies** — they were previously declared `peerDependenciesMeta.optional: true`, but the SDK's own source has always imported them at top level (`webvr/incident-pregel.ts` + `genko/canvas-pregel.ts` both use `StateGraph` + `Annotation`). The "optional" flag was a soft-lie that caused vite-plugin-svelte's optional-peer-dep handler to stub them under empty exports, breaking downstream builds. Consumers that import `./webvr` or `./genko` must install both langchain peers.

- **`three` and `@pixiv/three-vrm` are NO LONGER peer dependencies** — they were optional peers before; now they're absent from `package.json` entirely. Consumers that need three.js for non-SDK reasons (vendor-private renderers per ADR-2605172400, e.g., cyber-drill) carry three as their own direct dependency.

- **`createIncidentVrEngine` is now renderer-pluggable** — instead of attaching to a canvas, the engine emits `SceneDescriptor` objects via a caller-supplied `onScene` callback. Consumers wire their own scene-rendering surface (a `kami-app-{game}` wgpu crate, a vendor three.js renderer, or anything else). The previous `attach(canvas)` + `detach()` API is removed; pass `onScene: (scene) => mySurface.update(scene)` instead.

- **`createMorphController` / `createBoneController`**: `updateEngines(kami, three)` is now `updateEngines(kami)` — the two-arg form referenced the retired three.js engine slot.

- **`createConversationController.smoothExpr` / `smoothPose` / `idleMicro`** — rewritten to drive KAMI WASM exports directly (`setVrmMorphByName`, `setVrmBoneRotation`). Per-emotion and per-bone cache mirrors (`exprCache` / `poseCache`) are maintained locally because the WASM surface is setter-only. Behaviour is functionally equivalent; the change is invisible to consumers unless they were reaching into `engine.state.three` directly (which was always returning `undefined` post-ADR-0031 anyway).

### Added

- **`@etzhayyim/kami-engine-sdk/webvr` headless engine** — the existing `./webvr` export now lacks a built-in renderer. The engine's surface is `createIncidentVrEngine({scenario, cineBridge, onScene, onOpLog})` + a reactive `state` + decision log. Pair with a caller-owned scene surface (see cyber-drill's `$lib/three-renderer` for a vendor-three.js example, or build a `kami-app-{game}` wgpu crate).

- **`NodeEffectKind`** type is now re-exported from `./webvr` (was missed in the initial cutover; restored in iter-2 / commit `ea0fd3ab8`).

- **CI regression workflow** — `.github/workflows/kami-engine-sdk.yml` in the monorepo runs SDK build + vitest + cyber-drill prod build on every relevant PR + push to main + manual `workflow_dispatch`. Mirrored by a `pre-push` block in `lefthook.yml` for local pre-flight.

### Fixed

- **cyber-drill prod build** previously failed with `__vite-optional-peer-dep` stub errors during SSR build because the SDK declared langchain as optional peers but always imported them. Resolved by removing the optional flag in the SDK + adding `build.rollupOptions.external` for `@langchain/langgraph` + `@langchain/core` in cyber-drill's `vite.config.ts` (commit `b638c27e0`).

- **`no-two-stage-gftd-domains` lint** (in the monorepo, not the SDK itself) was silently a no-op since the gftd → etzhayyim domain rename. Re-enabled by fixing the regex + filter mismatch + retiring 2 actual `did:web:iryo.gftd.ai` legacy references in `20-actors/karute/actor-manifest.jsonld` (commit `9910f542b`).

### Migration guide for consumers

1. **Remove `three` and `@pixiv/three-vrm` from your `dependencies`** if you were only importing them transitively via the SDK. If you use three directly (vendor renderer, Threlte app, etc.), keep them.

2. **Add `@langchain/langgraph` and `@langchain/core`** to your `dependencies` if you import from `@etzhayyim/kami-engine-sdk/webvr` or `/genko`. Without them, `import { INCIDENT_GRAPH }` or `genkoEmbedHTML` will fail at module-load time.

3. **Replace `engine.attach(canvas) / engine.detach()`** with `createIncidentVrEngine({ ..., onScene: (scene) => surface.update(scene) })` and a separate scene-surface mount lifecycle. See the cyber-drill `+page.svelte` route for a vendor-three.js reference implementation.

4. **Replace `(state.three as any).vrm.expressionManager.setValue(...)`** with `engine.state.kami?.setVrmMorphByName(...)`. The three-vrm read paths are gone; KAMI WASM exports are the only morph/bone surface.

5. **If you imported from `@etzhayyim/kami-engine-sdk/spark`**, that subpath is removed. Either inline the spark sample code into your own app's `src/lib/`, or wait for the same demos to ship as a `kami-pipelines/examples/` wgpu port (out of scope for this release; tracked as a follow-up).

6. **If you used `ThreeVrmHandle` or `DualEngineState.three` in your TypeScript types**, remove those references. The type stub was always returning `unknown` and the field was always `null` post-ADR-0031.

## [0.1.0] — pre-2026-05-26

Initial release. Svelte 5 VRM character viewer + headless builders + Genko manga editor + trackpad/document bridge helpers + manufacturing/robotics planning helpers. Dual-engine rendering (KAMI WebGPU + three.js WebGL) was the documented architecture, though the three.js path was already disabled at runtime per ADR-0031 (2026-04-18).
