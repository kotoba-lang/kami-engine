import type { KamiWasmExports, ThreeVrmHandle } from '../types/engine.js';
import type { ExpressionPreset } from '../types/morph.js';

/** Options for creating a morph controller. */
export interface MorphControllerOpts {
  kami: KamiWasmExports | null;
  three: ThreeVrmHandle | null;
  targetCount?: number;
}

/**
 * Headless morph target controller for dual-engine VRM.
 *
 * Syncs morph weights to both KAMI (WebGPU WASM) and Three.js (WebGL)
 * engines simultaneously. Uses Svelte 5 `$state` for reactivity.
 */
export function createMorphController(opts: MorphControllerOpts) {
  const count = opts.targetCount ?? 57;
  let weights = $state(new Float32Array(count));

  /** Set a single morph target weight (0–1) on both engines. */
  function setMorph(index: number, weight: number) {
    if (index < 0 || index >= count) return;
    const w = Math.max(0, Math.min(1, weight));
    weights[index] = w;

    opts.kami?.setVrmMorph(index, w);

    const vrm = opts.three?.vrm as any;
    if (vrm) {
      vrm.scene?.traverse((o: any) => {
        if (o.isMesh && o.morphTargetInfluences && index < o.morphTargetInfluences.length) {
          o.morphTargetInfluences[index] = w;
        }
      });
    }
  }

  /** Apply a sparse weight map (index → weight). */
  function applyWeights(map: Record<number, number>) {
    for (const [idx, w] of Object.entries(map)) {
      setMorph(Number(idx), w);
    }
  }

  /** Apply an expression preset (reset + set preset morphs). */
  function applyPreset(preset: ExpressionPreset) {
    resetAll();
    applyWeights(preset.morphs);
  }

  /** Reset all morph weights to 0. */
  function resetAll() {
    opts.kami?.resetVrmMorphs();

    const vrm = opts.three?.vrm as any;
    if (vrm) {
      vrm.scene?.traverse((o: any) => {
        if (o.isMesh && o.morphTargetInfluences) {
          for (let i = 0; i < o.morphTargetInfluences.length; i++) {
            o.morphTargetInfluences[i] = 0;
          }
        }
      });
    }

    weights = new Float32Array(count);
  }

  /** Update engine references (after late init). */
  function updateEngines(kami: KamiWasmExports | null, three: ThreeVrmHandle | null) {
    opts.kami = kami;
    opts.three = three;
  }

  return {
    get weights() { return weights; },
    setMorph,
    applyWeights,
    applyPreset,
    resetAll,
    updateEngines,
  };
}

export type MorphController = ReturnType<typeof createMorphController>;
