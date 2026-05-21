<script lang="ts">
  import type { VrmEngine } from '../builders/createVrmEngine.svelte.js';
  import type { PosePreset, RotationAxis } from '../types/bone.js';
  import { POSE_PRESETS } from '../data/pose-presets.js';

  interface Props {
    engine: VrmEngine;
    presets?: PosePreset[];
    showSliders?: boolean;
    onchange?: (preset: PosePreset) => void;
    class?: string;
  }

  let {
    engine,
    presets = POSE_PRESETS,
    showSliders = true,
    onchange,
    class: className,
  }: Props = $props();

  let activeKey = $state<string | null>(null);

  function applyPose(preset: PosePreset) {
    activeKey = preset.key;
    engine.bones.applyPose(preset);
    onchange?.(preset);
  }

  const BONE_SLIDERS: { label: string; bone: string; axis: RotationAxis; min: number; max: number }[] = [
    { label: 'Nod', bone: 'head', axis: 'x', min: -60, max: 60 },
    { label: 'Turn', bone: 'head', axis: 'y', min: -80, max: 80 },
    { label: 'Tilt', bone: 'head', axis: 'z', min: -30, max: 30 },
    { label: 'L Shoulder', bone: 'leftUpperArm', axis: 'z', min: -30, max: 180 },
    { label: 'L Elbow', bone: 'leftLowerArm', axis: 'y', min: 0, max: 145 },
    { label: 'R Shoulder', bone: 'rightUpperArm', axis: 'z', min: -180, max: 30 },
    { label: 'R Elbow', bone: 'rightLowerArm', axis: 'y', min: -145, max: 0 },
    { label: 'Spine', bone: 'spine', axis: 'x', min: -30, max: 30 },
  ];

  function handleSlider(bone: string, axis: RotationAxis, e: Event) {
    const deg = (e.target as HTMLInputElement).valueAsNumber;
    engine.bones.setBone(bone, axis, deg);
    activeKey = null;
  }

  function getSliderValue(bone: string, axis: RotationAxis): number {
    return engine.bones.rotations.get(bone)?.[axis] ?? 0;
  }
</script>

<div style="display:flex;flex-direction:column;gap:12px" class={className}>
  <div style="display:flex;flex-wrap:wrap;gap:6px">
    {#each presets as preset (preset.key)}
      <button
        style="padding:5px 10px;font-size:11px;border-radius:4px;border:none;cursor:pointer;
          background:{activeKey === preset.key ? '#7c3aed' : 'rgba(255,255,255,0.1)'};
          color:{activeKey === preset.key ? '#fff' : 'rgba(255,255,255,0.7)'}"
        onclick={() => applyPose(preset)}
      >
        {preset.name}
      </button>
    {/each}
  </div>

  {#if showSliders}
    <div style="display:flex;flex-direction:column;gap:4px">
      {#each BONE_SLIDERS as slider (slider.label)}
        <label style="display:flex;align-items:center;gap:8px;font-size:11px;color:rgba(255,255,255,0.6)">
          <span style="width:80px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{slider.label}</span>
          <input
            type="range"
            min={slider.min}
            max={slider.max}
            value={getSliderValue(slider.bone, slider.axis)}
            oninput={(e) => handleSlider(slider.bone, slider.axis, e)}
            style="flex:1;height:4px;accent-color:#7c3aed"
          />
          <span style="width:28px;text-align:right">{Math.round(getSliderValue(slider.bone, slider.axis))}</span>
        </label>
      {/each}
    </div>
  {/if}
</div>
