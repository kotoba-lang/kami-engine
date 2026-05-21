# KAMI Engine SDK

Svelte 5 SDK for KAMI Engine applications.

This package contains reusable UI components, headless builders, data presets, and embed helpers used by KAMI Engine apps:

- VRM character viewer components for Svelte
- headless builders for morph, bone, motion, part, voice, and emotion control
- Genko manga editor components and stores
- trackpad and document bridge helpers
- manufacturing and robotics planning helper types/functions

## Install

```bash
pnpm add @gftdcojp/kami-engine-sdk svelte three
```

`three` and `@pixiv/three-vrm` are peer dependencies. Install `@pixiv/three-vrm` when using VRM-specific viewer features.

## Usage

```svelte
<script lang="ts">
  import { VrmViewer, createVrmEngine } from '@gftdcojp/kami-engine-sdk';

  const engine = createVrmEngine();
</script>

<VrmViewer {engine} />
```

Import narrower modules when you only need one surface:

```ts
import { createVrmEngine } from '@gftdcojp/kami-engine-sdk/builders';
import { genkoEmbedHTML } from '@gftdcojp/kami-engine-sdk/genko';
import { kamiTrackpadHTML } from '@gftdcojp/kami-engine-sdk/trackpad';
import type { Document } from '@gftdcojp/kami-engine-sdk/document';
```

## Development

```bash
pnpm install
pnpm run check
pnpm run test
pnpm run build
```

The build uses `svelte-package` and writes `dist/`.

## License

Apache-2.0
