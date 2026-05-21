<script lang="ts">
  import type { VrmEngine } from '../builders/createVrmEngine.svelte.js';
  import type { MotionKey } from '../types/motion.js';
  import { MOTION_PRESETS } from '../data/motion-presets.js';

  interface Props {
    engine: VrmEngine;
    onchange?: (key: MotionKey | null) => void;
    class?: string;
  }

  let { engine, onchange, class: className }: Props = $props();

  function selectMotion(key: MotionKey | null) {
    if (key) engine.motion.play(key);
    else engine.motion.stop();
    onchange?.(key);
  }
</script>

<div style="display:flex;flex-wrap:wrap;gap:6px" class={className}>
  {#each MOTION_PRESETS as preset (preset.name)}
    <button
      style="padding:5px 10px;font-size:11px;border-radius:4px;border:none;cursor:pointer;
        background:{engine.motion.active === preset.key ? '#7c3aed' : 'rgba(255,255,255,0.1)'};
        color:{engine.motion.active === preset.key ? '#fff' : 'rgba(255,255,255,0.7)'}"
      onclick={() => selectMotion(preset.key)}
    >
      {preset.name}
    </button>
  {/each}
</div>
