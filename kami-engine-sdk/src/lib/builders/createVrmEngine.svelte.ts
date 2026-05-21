import {
  KAMI_ENGINE_SDK_DEFAULT_MAX_RAM_BYTES,
  type DualEngineState,
  type KamiWasmExports,
  type EngineCapabilities,
  type EngineMemoryBudget,
} from '../types/engine.js';
import { createMorphController, type MorphController } from './createMorphController.svelte.js';
import { createBoneController, type BoneController } from './createBoneController.svelte.js';
import { createMotionPlayer, type MotionPlayer } from './createMotionPlayer.svelte.js';
import { CHARACTER_PRESETS, applyCharacterColors, type CharacterPreset } from '../data/character-presets.js';

/** Options for creating a dual-engine VRM viewer. */
export interface CreateVrmEngineOpts {
  /** Canvas element ID for Three.js (WebGL). */
  canvasId: string;
  /** Canvas element ID for KAMI Engine (WebGPU). If omitted, uses `canvasId + '-kami'`. */
  kamiCanvasId?: string;
  /** VRM model URL (R2 or absolute). */
  vrmUrl: string;
  /** Which engines to initialize. KAMI (wgpu) is the only supported engine. */
  engines?: ('kami')[];
  /** KAMI WASM module base URL. */
  wasmUrl?: string;
  /** R2 base URL for part assets. */
  r2Base?: string;
  /** Enforce anatomical joint constraints. Default: true. */
  enforceConstraints?: boolean;
  /** Maximum RAM budget in bytes. Default: 2 GiB. */
  maxRamBytes?: number;
  /** Default character preset to apply after VRM load. Default: 'Sofia'. Set null to skip. */
  defaultCharacter?: CharacterPreset | null;
  /** Callback when engines are ready. */
  onReady?: (state: DualEngineState) => void;
  /** Callback on error. */
  onError?: (error: string) => void;
}

/**
 * Create a dual-engine VRM viewer (KAMI WebGPU + Three.js WebGL).
 *
 * Headless builder — no DOM dependency. Manages engine lifecycle, morph
 * targets, bone rotations, motion playback, and animation loop.
 *
 * Usage:
 * ```ts
 * const engine = createVrmEngine({ canvasId: 'vrm', vrmUrl: '...' });
 * await engine.init();
 * engine.morphs.applyPreset(EXPRESSION_PRESETS[1]); // Joy
 * engine.motion.play('dance');
 * ```
 */
export function createVrmEngine(opts: CreateVrmEngineOpts) {
  const engineList = opts.engines ?? ['kami'];
  const maxRamBytes = opts.maxRamBytes ?? KAMI_ENGINE_SDK_DEFAULT_MAX_RAM_BYTES;
  const memoryBudget: EngineMemoryBudget = {
    maxRamBytes,
    maxRamMiB: Math.floor(maxRamBytes / (1024 * 1024)),
  };

  let state = $state<DualEngineState>({
    kami: null,
    three: null,
    vrmUrl: opts.vrmUrl,
    loading: true,
    error: null,
    morphNames: [],
  });

  let animFrameId: number | null = null;
  let time = 0;

  const morphCtrl: MorphController = createMorphController({
    kami: null, three: null,
  });

  const boneCtrl: BoneController = createBoneController({
    kami: null, three: null,
    enforceConstraints: opts.enforceConstraints ?? true,
  });

  const motionCtrl: MotionPlayer = createMotionPlayer(boneCtrl);

  /** Detect WebGPU and WebGL support. */
  async function detectCapabilities(): Promise<EngineCapabilities> {
    const webgpu = typeof navigator !== 'undefined' && 'gpu' in navigator
      ? !!(await (navigator as any).gpu?.requestAdapter?.())
      : false;

    const webgl = (() => {
      try {
        const canvas = document.createElement('canvas');
        return !!(canvas.getContext('webgl2') || canvas.getContext('webgl'));
      } catch { return false; }
    })();

    return { webgpu, webgl };
  }

  /** Initialize KAMI Engine (WebGPU WASM). */
  async function initKami(): Promise<KamiWasmExports | null> {
    if (!opts.wasmUrl) return null;
    try {
      const mod = await import(/* @vite-ignore */ `${opts.wasmUrl}/kamiWeb.js`);
      await mod.default(`${opts.wasmUrl}/kamiWebBg.wasm`);
      const kamiCid = opts.kamiCanvasId ?? opts.canvasId + '-kami';
      await mod.runEmbedVrm(kamiCid, opts.vrmUrl);

      const morphNamesJson = mod.getVrmMorphNames();
      const morphNames: string[] = JSON.parse(morphNamesJson);
      state.morphNames = morphNames;

      return {
        runEmbedVrm: mod.runEmbedVrm,
        setVrmMorph: mod.setVrmMorph,
        setVrmMorphByName: mod.setVrmMorphByName,
        getVrmMorphNames: mod.getVrmMorphNames,
        resetVrmMorphs: mod.resetVrmMorphs,
        setVrmCamera: mod.setVrmCamera,
        evaluateMotion: mod.evaluateMotion,
        clampBone: mod.clampBone,
        getVrmBoneNames: mod.getVrmBoneNames,
        setVrmBoneRotation: mod.setVrmBoneRotation,
        resetVrmPose: mod.resetVrmPose,
        getVrmSkeletonInfo: mod.getVrmSkeletonInfo,
        getVrmMeshLabels: mod.getVrmMeshLabels,
        setVrmMeshVisibility: mod.setVrmMeshVisibility,
        composeVrmWithPreset: mod.composeVrmWithPreset,
      };
    } catch (e) {
      console.warn('[kami-engine-sdk] KAMI WASM init failed:', e);
      return null;
    }
  }

  // three.js path removed (see ADR: kami-engine VRM three.js-free, 2026-04-18).
  // KAMI wgpu handles rendering end-to-end; motion/morph are driven via the
  // WASM exports. The field `state.three` is retained for backwards type
  // compatibility but always null.

  let resizeObserver: ResizeObserver | null = null;

  /** Drive motion evaluation + auto-blink. KAMI handles its own RAF render
   * loop in Rust; this tick only advances JS-side motion state and morphs. */
  function startAnimLoop() {
    let lastBlink = 0;
    let lastT = performance.now();

    function tick() {
      animFrameId = requestAnimationFrame(tick);
      const now = performance.now();
      const dt = (now - lastT) / 1000;
      lastT = now;
      time += dt;

      // Auto-blink via blendshape name (KAMI morph export).
      if (state.kami && now - lastBlink > 3500 + Math.random() * 1000) {
        state.kami.setVrmMorphByName?.('blink', 1);
        setTimeout(() => state.kami?.setVrmMorphByName?.('blink', 0), 150);
        lastBlink = now;
      }

      // Motion evaluation writes bone rotations into KAMI via boneCtrl.
      motionCtrl.evaluate(time);
    }

    tick();
  }

  /** Initialize all engines and start rendering. */
  async function init() {
    state.loading = true;
    state.error = null;

    try {
      state.kami = engineList.includes('kami') ? await initKami() : null;
      state.three = null;
      state.loading = false;

      // Wire up controllers
      morphCtrl.updateEngines(state.kami, null);
      boneCtrl.updateEngines(state.kami, null);

      startAnimLoop();

      // Apply default character preset (Sofia by default)
      const charPreset = opts.defaultCharacter !== null
        ? (opts.defaultCharacter ?? CHARACTER_PRESETS[0])
        : null;
      if (charPreset) {
        applyCharacter(charPreset);
      }

      opts.onReady?.(state);
    } catch (e: any) {
      state.error = e.message ?? String(e);
      state.loading = false;
      opts.onError?.(state.error!);
    }
  }

  /**
   * Apply a character preset (colors + expression + pose).
   *
   * Sets VRM material colors, expression morphs, and bone rotations.
   */
  function applyCharacter(preset: CharacterPreset) {
    const vrm = (state.three as any)?.vrm;
    if (vrm) {
      applyCharacterColors(vrm.scene, preset.colors);
      // Apply expression via expressionManager (Bluesky VRM standard names)
      morphCtrl.resetAll();
      for (const [name, weight] of Object.entries(preset.expr)) {
        vrm.expressionManager?.setValue(name, weight);
      }
    }
    // Apply pose
    boneCtrl.resetAll();
    for (const [bone, axes] of Object.entries(preset.pose)) {
      for (const [axis, deg] of Object.entries(axes)) {
        boneCtrl.setBone(bone, axis as 'x' | 'y' | 'z', deg as number);
      }
    }
  }

  /** Dispose all resources. */
  function dispose() {
    if (animFrameId !== null) cancelAnimationFrame(animFrameId);
    resizeObserver?.disconnect();
    resizeObserver = null;
    (state.three as any)?.dispose?.();
    state.kami = null;
    state.three = null;
  }

  return {
    get state() { return state; },
    get memoryBudget(): EngineMemoryBudget { return memoryBudget; },
    get morphs(): MorphController { return morphCtrl; },
    get bones(): BoneController { return boneCtrl; },
    get motion(): MotionPlayer { return motionCtrl; },
    init,
    dispose,
    applyCharacter,
    detectCapabilities,
  };
}

export type VrmEngine = ReturnType<typeof createVrmEngine>;
