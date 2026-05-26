# KAMI Engine SDK

Svelte 5 SDK for KAMI Engine applications.

This package contains reusable UI components, headless builders, data presets, and embed helpers used by KAMI Engine apps:

- VRM character viewer components for Svelte (rendered Rust-side via the KAMI Engine wgpu WASM)
- headless builders for morph, bone, motion, part, voice, and emotion control
- 3D Gaussian Splat preview bridge (`./gsplat`) — XRPC + WASM glue to `kami-pipelines::GsplatAdapter`
- headless incident-response engine (`./webvr`) — choice-based scenario runner with `onScene` callback for pluggable scene rendering
- Genko manga editor components and stores
- trackpad and document bridge helpers
- manufacturing and robotics planning helper types/functions

The SDK is **three.js-free** at every layer (runtime, types, declared deps, build output) as of 2026-05-26. The VRM viewer is end-to-end wgpu via the Rust+WASM `KamiWasmExports` surface; 3DGS rendering goes through `kami-pipelines::GsplatAdapter` (Rust+wgpu WGSL EWA + SH degree 0–3). See ADR-0031 (2026-04-18 VRM-only) + ADR-2605264300 (2026-05-26 full-SDK cutover).

## Install

```bash
pnpm add @etzhayyim/kami-engine-sdk svelte
```

Required peer dependencies installed alongside the SDK:

- `svelte` (^5.0.0) — Svelte 5 runes are used throughout the builders
- `@langchain/langgraph` (>=1.0.0) — used by `./webvr` (incident-response Pregel graph) and `./genko` (canvas Pregel pipelines)
- `@langchain/core` (>=1.0.0) — transitively required by `@langchain/langgraph`

No `three` / `@pixiv/three-vrm` peer dependencies — the SDK does not use three.js at runtime or in its types. Apps that need a three.js scene surface (e.g., cyber-drill's vendor-private renderer per ADR-2605172400) carry three directly as their own dependency.

## Usage

```svelte
<script lang="ts">
  import { VrmViewer, createVrmEngine } from '@etzhayyim/kami-engine-sdk';

  const engine = createVrmEngine({
    canvasId: 'vrm',
    vrmUrl: '...',
    wasmUrl: 'https://cdn.example.com/kami-web/',
  });
</script>

<VrmViewer {engine} />
```

The KAMI WASM module (`kamiWeb.js` + `kamiWebBg.wasm`) must be served from `wasmUrl`. The viewer's morph / bone / pose / motion controls all flow through the WASM exports — no DOM-side three.js needed.

Import narrower modules when you only need one surface:

```ts
import { createVrmEngine } from '@etzhayyim/kami-engine-sdk/builders';
import { genkoEmbedHTML } from '@etzhayyim/kami-engine-sdk/genko';
import { kamiTrackpadHTML } from '@etzhayyim/kami-engine-sdk/trackpad';
import type { Document } from '@etzhayyim/kami-engine-sdk/document';

// 3D Gaussian Splat preview/QC bridge (loads splat assets from maps.etzhayyim.com,
// pushes them into a kami-app-maps3d WASM module)
import { loadGsplatAsset, pushToWasm } from '@etzhayyim/kami-engine-sdk/gsplat';

// Headless incident-response engine (renderer is caller-supplied via `onScene`)
import { createIncidentVrEngine } from '@etzhayyim/kami-engine-sdk/webvr';
```

## Development

```bash
pnpm install
pnpm run check
pnpm run test
pnpm run build
```

The build uses `svelte-package` and writes `dist/`.

## Regression coverage

A path-triggered GitHub Actions workflow (`.github/workflows/kami-engine-sdk.yml` in the monorepo, see ADR-2605264300 "CI regression-test addendum") exercises the SDK build + vitest + cyber-drill consumer build on every PR + push to main + manual `workflow_dispatch`. A matching `pre-push` block in `lefthook.yml` runs the same three checks locally before push (<10s wall-clock).

## License

Apache-2.0
