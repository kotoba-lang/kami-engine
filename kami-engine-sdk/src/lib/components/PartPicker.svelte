<script lang="ts">
  import type { VrmEngine } from '../builders/createVrmEngine.svelte.js';
  import { createPartComposer, type PartComposer } from '../builders/createPartComposer.svelte.js';
  import { HAIR_STYLES, HAIR_COLORS, OUTFIT_STYLES, OUTFIT_COLORS } from '../data/part-catalog.js';
  import type { PartCategory } from '../types/part.js';

  interface Props {
    /** Engine handle (from VrmCanvas bind:engine). */
    engine: VrmEngine;
    /** R2 base URL for part assets. */
    r2Base: string;
    /** Bindable composer handle. */
    composer?: PartComposer;
    /** Called when a part changes. */
    onpartchange?: (category: PartCategory, key: string) => void;
    /** Additional CSS class. */
    class?: string;
  }

  let {
    engine,
    r2Base,
    composer = $bindable(),
    onpartchange,
    class: className,
  }: Props = $props();

  $effect(() => {
    composer = createPartComposer({ kami: engine.state.kami, r2Base });
  });

  $effect(() => {
    if (engine.state.kami && !engine.state.loading) {
      composer!.updateEngine(engine.state.kami);
      composer!.scanBaseParts();
    }
  });
</script>

<div class="flex flex-col gap-4 {className ?? ''}">
  <!-- Hair Style -->
  <div>
    <h4 class="text-xs font-semibold text-white/80 mb-1.5">Hair Style</h4>
    <div class="flex flex-wrap gap-1">
      {#each HAIR_STYLES as style (style.key)}
        <button
          class="px-2 py-1 text-[10px] rounded
            {composer?.state.activeHairStyle === style.key
              ? 'bg-purple-600 text-white'
              : 'bg-white/10 text-white/60 hover:bg-white/15'}"
          onclick={() => { composer?.setHairStyle(style.key); onpartchange?.('Hair', style.key); }}
        >
          {style.name}
        </button>
      {/each}
    </div>
  </div>

  <!-- Hair Color -->
  <div>
    <h4 class="text-xs font-semibold text-white/80 mb-1.5">Hair Color</h4>
    <div class="flex gap-2">
      {#each HAIR_COLORS as color (color.key)}
        <button
          class="w-6 h-6 rounded-full border-2 transition-transform
            {composer?.state.activeHairColor === color.key ? 'border-white scale-110' : 'border-transparent'}"
          style="background: {color.hex}"
          title={color.name}
          onclick={() => { composer?.setHairColor(color.key); onpartchange?.('Hair', color.key); }}
        ></button>
      {/each}
    </div>
  </div>

  <!-- Outfit Style -->
  <div>
    <h4 class="text-xs font-semibold text-white/80 mb-1.5">Outfit Style</h4>
    <div class="flex flex-wrap gap-1">
      {#each OUTFIT_STYLES as style (style.key)}
        <button
          class="px-2 py-1 text-[10px] rounded
            {composer?.state.activeOutfitStyle === style.key
              ? 'bg-purple-600 text-white'
              : 'bg-white/10 text-white/60 hover:bg-white/15'}"
          onclick={() => { composer?.setOutfitStyle(style.key); onpartchange?.('Outfit', style.key); }}
        >
          {style.name}
        </button>
      {/each}
    </div>
  </div>

  <!-- Outfit Color -->
  <div>
    <h4 class="text-xs font-semibold text-white/80 mb-1.5">Outfit Color</h4>
    <div class="flex gap-2">
      {#each OUTFIT_COLORS as color (color.key)}
        <button
          class="w-6 h-6 rounded-full border-2 transition-transform
            {composer?.state.activeOutfitColor === color.key ? 'border-white scale-110' : 'border-transparent'}"
          style="background: {color.hex}"
          title={color.name}
          onclick={() => { composer?.setOutfitColor(color.key); onpartchange?.('Outfit', color.key); }}
        ></button>
      {/each}
    </div>
  </div>
</div>
