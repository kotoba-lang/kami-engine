<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { DualEngineState } from '../types/engine.js';
  import { createVrmEngine, type VrmEngine } from '../builders/createVrmEngine.svelte.js';

  interface Props {
    /** VRM model URL. */
    vrmUrl: string;
    /** R2 base URL for CDN assets. */
    r2Base?: string;
    /** KAMI WASM module URL. */
    wasmUrl?: string;
    /** Which engines to initialize. Default: `['kami']`. */
    engines?: ('kami')[];
    /** Canvas background color. */
    bgColor?: string;
    /** Enforce anatomical joint limits. Default: true. */
    enforceConstraints?: boolean;
    /** Bindable engine handle for external control. */
    engine?: VrmEngine;
    /** Loading overlay snippet. */
    loading?: Snippet<[{ progress: number }]>;
    /** Error overlay snippet. */
    error?: Snippet<[{ message: string; retry: () => void }]>;
    /** Fired when engines are ready. */
    onready?: (state: DualEngineState) => void;
    /** Additional CSS class. */
    class?: string;
  }

  let {
    vrmUrl,
    r2Base = '',
    wasmUrl,
    engines = ['kami'],
    bgColor,
    enforceConstraints = true,
    engine = $bindable(),
    loading: loadingSnippet,
    error: errorSnippet,
    onready,
    class: className,
  }: Props = $props();

  const uid = Math.random().toString(36).slice(2, 8);
  const threeCanvasId = `kami-vrm-three-${uid}`;
  const kamiCanvasId = `kami-vrm-kami-${uid}`;
  const hasKami = $derived(engines.includes('kami'));
  const hasThree = $derived(false);
  const dualPane = $derived(hasKami && hasThree);

  let initError = $state('');

  $effect(() => {
    const nextEngine = createVrmEngine({
      canvasId: threeCanvasId,
      kamiCanvasId,
      vrmUrl,
      engines,
      wasmUrl,
      r2Base,
      enforceConstraints,
      onReady: onready,
    });
    engine = nextEngine;
    initError = '';
    nextEngine.init().catch((e: any) => {
      initError = String(e?.message ?? e);
      console.error('[VrmCanvas] init error:', e);
    });
    return () => nextEngine.dispose();
  });
</script>

<div class="{className ?? ''}" style="position:relative;width:100%;height:100%;display:flex;flex:1 1 0%;min-height:0;min-width:0">
  {#if hasKami}
    <div style="flex:1 1 0%;position:relative;overflow:hidden;{dualPane ? 'border-right:1px solid rgba(255,255,255,0.06)' : ''}">
      <span style="position:absolute;top:6px;left:8px;font-size:11px;font-weight:600;z-index:5;color:#ff9060;text-shadow:0 1px 3px rgba(0,0,0,0.8)">KAMI Engine (MToon wgpu)</span>
      <canvas
        id={kamiCanvasId}
        width="640"
        height="480"
        style="width:100%;height:100%;display:block;{bgColor ? `background:${bgColor}` : ''}"
      ></canvas>
    </div>
  {/if}

  {#if hasThree}
    <div style="flex:1 1 0%;position:relative;overflow:hidden">
      <span style="position:absolute;top:6px;left:8px;font-size:11px;font-weight:600;z-index:5;color:#60ff90;text-shadow:0 1px 3px rgba(0,0,0,0.8)">Three.js + VRM (MToon)</span>
      <canvas
        id={threeCanvasId}
        width="640"
        height="480"
        style="width:100%;height:100%;display:block;{bgColor ? `background:${bgColor}` : ''}"
      ></canvas>
    </div>
  {/if}

  {#if engine?.state.loading}
    {#if loadingSnippet}
      {@render loadingSnippet({ progress: 0 })}
    {:else}
      <div style="position:absolute;inset:0;display:flex;align-items:center;justify-content:center;background:rgba(0,0,0,0.5)">
        <div style="color:rgba(255,255,255,0.6);font-size:14px">Loading VRM...</div>
      </div>
    {/if}
  {/if}

  {#if engine?.state.error || initError}
    <div style="position:absolute;inset:0;display:flex;align-items:center;justify-content:center;background:rgba(0,0,0,0.5)">
      <div style="color:#f66;font-size:12px;max-width:90%;word-break:break-all;padding:12px">{engine?.state.error || initError}</div>
    </div>
  {/if}
</div>
