<script lang="ts">
  import type { VrmEngine } from '../builders/createVrmEngine.svelte.js';
  import type { ExpressionPreset, MorphCategory } from '../types/morph.js';
  import { EXPRESSION_PRESETS } from '../data/expression-presets.js';
  import { MORPH_TARGETS } from '../data/morph-targets.js';

  interface Props {
    engine: VrmEngine;
    presets?: ExpressionPreset[];
    showSliders?: boolean;
    sliderCategories?: MorphCategory[];
    onchange?: (morphWeights: Record<number, number>) => void;
    class?: string;
  }

  let {
    engine,
    presets = EXPRESSION_PRESETS,
    showSliders = false,
    sliderCategories = ['ALL', 'BRW', 'EYE', 'MTH'],
    onchange,
    class: className,
  }: Props = $props();

  let activeKey = $state<string | null>(null);

  function applyPreset(preset: ExpressionPreset) {
    activeKey = preset.key;
    engine.morphs.applyPreset(preset);
    onchange?.(preset.morphs);
  }

  function handleSlider(index: number, e: Event) {
    const value = (e.target as HTMLInputElement).valueAsNumber / 100;
    engine.morphs.setMorph(index, value);
    activeKey = null;
  }

  const filteredTargets = $derived(
    MORPH_TARGETS.filter(t => sliderCategories.includes(t.category))
  );
</script>

<div style="display:flex;flex-direction:column;gap:12px" class={className}>
  <div style="display:flex;flex-wrap:wrap;gap:6px">
    {#each presets as preset (preset.key)}
      <button
        style="padding:5px 10px;font-size:11px;border-radius:4px;border:none;cursor:pointer;
          background:{activeKey === preset.key ? '#7c3aed' : 'rgba(255,255,255,0.1)'};
          color:{activeKey === preset.key ? '#fff' : 'rgba(255,255,255,0.7)'}"
        onclick={() => applyPreset(preset)}
      >
        {preset.name}
      </button>
    {/each}
  </div>

  {#if showSliders}
    <div style="display:flex;flex-direction:column;gap:4px">
      {#each filteredTargets as target (target.index)}
        <label style="display:flex;align-items:center;gap:8px;font-size:11px;color:rgba(255,255,255,0.6)">
          <span style="width:80px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{target.displayName}</span>
          <input
            type="range"
            min="0"
            max="100"
            value={Math.round(engine.morphs.weights[target.index] * 100)}
            oninput={(e) => handleSlider(target.index, e)}
            style="flex:1;height:4px;accent-color:#7c3aed"
          />
          <span style="width:28px;text-align:right">{Math.round(engine.morphs.weights[target.index] * 100)}</span>
        </label>
      {/each}
    </div>
  {/if}
</div>
