import type { PartCategory, PartEntry, PartComposerState } from '../types/part.js';
import type { KamiWasmExports } from '../types/engine.js';

/** Options for creating a part composer (three.js-free, wgpu path). */
export interface PartComposerOpts {
  kami: KamiWasmExports | null;
  r2Base: string;
  /** Canvas element id. Required for hot preset swap (canvas remount). */
  canvasId?: string;
  /** Current base VRM URL (used as the body/skeleton source during preset swap). */
  baseVrmUrl?: string;
}

/** Classify a mesh/material label into a part category by name heuristics. */
function classifyMesh(label: string): PartCategory {
  const s = label.toLowerCase();
  if (s.includes('hair') || s.includes('bangs')) return 'Hair';
  if (s.includes('face') || s.includes('eye') || s.includes('mouth') || s.includes('brow') || s.includes('eyelash') || s.includes('eyeline')) return 'Face';
  if (s.includes('body') || s.includes('skin')) return 'Body';
  if (s.includes('cloth') || s.includes('outfit') || s.includes('shirt') || s.includes('pants') || s.includes('dress') || s.includes('shoe') || s.includes('tops') || s.includes('bottom')) return 'Outfit';
  if (s.includes('accessory') || s.includes('hat') || s.includes('glass') || s.includes('ribbon')) return 'Accessory';
  return 'Body';
}

/**
 * Headless VRM part composition manager (wgpu / KAMI-native path).
 *
 * Enumerates the currently-loaded VRM's draw-batch labels via KAMI wasm
 * exports and classifies them into part categories. Visibility toggles are
 * forwarded to `setVrmMeshVisibility`, which filters the render loop.
 *
 * **Preset swap (hair/outfit exchange across GLB sources) is not yet
 * supported on the wgpu path** — that requires recomposing the VRM via
 * `kami_vrm::compose` and reloading the resulting GLB. Preset-loading
 * functions currently log a warning and no-op; the base VRM's parts
 * remain the only selectable units.
 */
export function createPartComposer(opts: PartComposerOpts) {
  let composerState = $state<PartComposerState>({
    registry: { Body: [], Hair: [], Face: [], Outfit: [], Accessory: [] },
    activeHairStyle: 'longStraight',
    activeHairColor: 'blonde',
    activeOutfitStyle: 'tshirt',
    activeOutfitColor: 'white',
  });

  /** Scan the loaded VRM for parts via KAMI batch labels. */
  function scanBaseParts() {
    if (!opts.kami?.getVrmMeshLabels) return;
    let labels: string[];
    try {
      labels = JSON.parse(opts.kami.getVrmMeshLabels());
    } catch {
      return;
    }
    const reg: Record<PartCategory, PartEntry[]> = {
      Body: [], Hair: [], Face: [], Outfit: [], Accessory: [],
    };
    for (const label of labels) {
      const cat = classifyMesh(label);
      reg[cat].push({
        name: label,
        category: cat,
        source: 'base',
        visible: true,
        object: null,
      });
    }
    composerState.registry = reg;
  }

  /** Toggle visibility of a specific part. */
  function togglePart(category: PartCategory, index: number) {
    const part = composerState.registry[category][index];
    if (!part || !opts.kami?.setVrmMeshVisibility) return;
    const next = !part.visible;
    opts.kami.setVrmMeshVisibility(part.name, next);
    part.visible = next;
  }

  /**
   * Preset swap (wgpu path):
   * 1. Fetch current base VRM bytes + preset GLB bytes.
   * 2. `composeVrmWithPreset(base, preset, category)` → merged GLB bytes.
   * 3. Create a Blob URL and replace the canvas element in-place (old RAF
   *    loop self-terminates on texture acquire failure).
   * 4. `runEmbedVrm(canvasId, blobUrl)` on the cloned canvas.
   * 5. Rescan parts.
   */
  async function loadPresetPart(key: string, category: PartCategory) {
    const kami = opts.kami;
    if (!kami?.composeVrmWithPreset || !kami?.runEmbedVrm) {
      console.warn(`[kami-engine-sdk] loadPresetPart('${key}'): KAMI compose/reload not available`);
      return;
    }
    if (!opts.canvasId || !opts.baseVrmUrl) {
      console.warn(`[kami-engine-sdk] loadPresetPart('${key}'): canvasId + baseVrmUrl required for hot swap`);
      return;
    }
    const presetUrl = `${opts.r2Base}/avatar/parts/${key}.glb`;
    try {
      const [baseResp, presetResp] = await Promise.all([
        fetch(opts.baseVrmUrl),
        fetch(presetUrl),
      ]);
      if (!baseResp.ok) throw new Error(`base fetch ${baseResp.status}`);
      if (!presetResp.ok) throw new Error(`preset fetch ${presetResp.status}`);
      const [baseBuf, presetBuf] = await Promise.all([
        baseResp.arrayBuffer(),
        presetResp.arrayBuffer(),
      ]);
      const composed = kami.composeVrmWithPreset(
        new Uint8Array(baseBuf),
        new Uint8Array(presetBuf),
        category,
      );
      const blobBytes = new Uint8Array(composed.byteLength);
      blobBytes.set(composed);
      const blobUrl = URL.createObjectURL(new Blob([blobBytes], { type: 'model/vnd.vrm' }));

      // Remount canvas (old WebGPU surface becomes orphan; RAF loop dies on
      // next configure_surface failure).
      const oldCanvas = document.getElementById(opts.canvasId) as HTMLCanvasElement | null;
      if (!oldCanvas || !oldCanvas.parentElement) {
        console.warn(`[kami-engine-sdk] loadPresetPart: canvas #${opts.canvasId} not found`);
        return;
      }
      const newCanvas = oldCanvas.cloneNode(false) as HTMLCanvasElement;
      oldCanvas.parentElement.replaceChild(newCanvas, oldCanvas);

      await kami.runEmbedVrm(opts.canvasId, blobUrl);
      scanBaseParts();
    } catch (e) {
      console.warn(`[kami-engine-sdk] loadPresetPart failed:`, e);
    }
  }

  function setHairStyle(style: string) { composerState.activeHairStyle = style; loadPresetPart(`hair_${style}_${composerState.activeHairColor}`, 'Hair'); }
  function setHairColor(color: string) { composerState.activeHairColor = color; loadPresetPart(`hair_${composerState.activeHairStyle}_${color}`, 'Hair'); }
  function setOutfitStyle(style: string) { composerState.activeOutfitStyle = style; loadPresetPart(`outfit_${style}_${composerState.activeOutfitColor}`, 'Outfit'); }
  function setOutfitColor(color: string) { composerState.activeOutfitColor = color; loadPresetPart(`outfit_${composerState.activeOutfitStyle}_${color}`, 'Outfit'); }

  /** Update engine reference after late KAMI init. */
  function updateEngine(kami: KamiWasmExports | null) {
    opts.kami = kami;
    if (kami) scanBaseParts();
  }

  return {
    get state() { return composerState; },
    scanBaseParts,
    loadPresetPart,
    setHairStyle,
    setHairColor,
    setOutfitStyle,
    setOutfitColor,
    togglePart,
    updateEngine,
  };
}

export type PartComposer = ReturnType<typeof createPartComposer>;
