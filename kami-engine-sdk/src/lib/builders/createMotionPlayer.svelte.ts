import type { MotionKey } from '../types/motion.js';
import type { KamiWasmExports } from '../types/engine.js';
import type { BoneController } from './createBoneController.svelte.js';
import { toWasmMotionKey } from '../data/motion-key-map.js';

/**
 * Headless procedural motion animation player.
 *
 * When a Rust WASM `evaluateMotion()` is available, delegates the entire
 * motion evaluation to Rust (sin/cos + joint clamping in WASM). Otherwise
 * falls back to TypeScript evaluation.
 */
export function createMotionPlayer(boneCtrl: BoneController, kami?: KamiWasmExports | null) {
  let active = $state<MotionKey | null>(null);
  let playing = $state(false);

  /** Start playing a motion animation. */
  function play(key: MotionKey) {
    active = key;
    playing = true;
  }

  /** Stop the current motion and reset bones. */
  function stop() {
    active = null;
    playing = false;
    boneCtrl.resetAll();
  }

  /**
   * Evaluate current motion at time t. Called each frame from the animation loop.
   *
   * Prefers Rust WASM evaluation when available (joint-clamped sin/cos in WASM).
   * Falls back to TypeScript for portability.
   */
  function evaluate(t: number) {
    if (!active) return;

    // Try Rust WASM motion evaluator
    if (kami?.evaluateMotion) {
      const wasmKey = toWasmMotionKey(active);
      if (wasmKey) {
        const json = kami.evaluateMotion(wasmKey, t);
        const bones: Record<string, Record<string, number>> = JSON.parse(json);
        for (const [bone, axes] of Object.entries(bones)) {
          for (const [axis, deg] of Object.entries(axes)) {
            boneCtrl.setBone(bone, axis as 'x' | 'y' | 'z', deg);
          }
        }
        return;
      }
    }

    // TypeScript fallback
    evaluateTS(t, active, boneCtrl);
  }

  return {
    get active() { return active; },
    get playing() { return playing; },
    play,
    stop,
    evaluate,
  };
}

/** TypeScript procedural motion evaluation (fallback when WASM unavailable). */
function evaluateTS(t: number, key: MotionKey, ctrl: BoneController) {
  const s = Math.sin;
  const abs = Math.abs;

  switch (key) {
    case 'idle':
      ctrl.setBone('head', 'x', 5 + s(t * 0.8) * 2);
      ctrl.setBone('spine', 'x', s(t * 1.2) * 1);
      break;
    case 'breathe':
      ctrl.setBone('chest', 'x', s(t * 1.5) * 3);
      ctrl.setBone('spine', 'x', s(t * 1.5) * 1.5);
      break;
    case 'nod':
      ctrl.setBone('head', 'x', s(t * 3) * 15);
      break;
    case 'shake':
      ctrl.setBone('head', 'y', s(t * 4) * 20);
      break;
    case 'waveHi':
      ctrl.setBone('rightUpperArm', 'z', -70 + s(t * 4) * 10);
      ctrl.setBone('rightLowerArm', 'y', -100 + s(t * 6) * 30);
      ctrl.setBone('head', 'z', s(t * 2) * 5);
      break;
    case 'dance':
      ctrl.setBone('hips', 'y', s(t * 3) * 10);
      ctrl.setBone('hips', 'x', s(t * 6) * 3);
      ctrl.setBone('leftUpperArm', 'z', 50 + s(t * 3) * 20);
      ctrl.setBone('rightUpperArm', 'z', -50 + s(t * 3 + 1) * 20);
      ctrl.setBone('head', 'z', s(t * 3) * 8);
      ctrl.setBone('spine', 'y', s(t * 3) * 5);
      break;
    case 'bounce':
      ctrl.setBone('hips', 'x', abs(s(t * 4)) * 5);
      ctrl.setBone('leftUpperArm', 'z', 60 + s(t * 4) * 10);
      ctrl.setBone('rightUpperArm', 'z', -60 + s(t * 4) * 10);
      break;
    case 'sway':
      ctrl.setBone('spine', 'z', s(t * 1.5) * 8);
      ctrl.setBone('head', 'z', s(t * 1.5 + 0.5) * 5);
      ctrl.setBone('hips', 'z', s(t * 1.5) * 3);
      break;
    case 'lookAround':
      ctrl.setBone('head', 'y', s(t * 0.8) * 35);
      ctrl.setBone('head', 'x', s(t * 1.2) * 10);
      break;
    case 'excited':
      ctrl.setBone('hips', 'x', abs(s(t * 6)) * 4);
      ctrl.setBone('leftUpperArm', 'z', 40 + s(t * 5) * 25);
      ctrl.setBone('rightUpperArm', 'z', -40 + s(t * 5 + 1) * 25);
      ctrl.setBone('leftLowerArm', 'y', 70 + s(t * 5) * 30);
      ctrl.setBone('rightLowerArm', 'y', -70 + s(t * 5 + 1) * 30);
      ctrl.setBone('head', 'x', -5 + s(t * 3) * 5);
      break;
    case 'sadSway':
      ctrl.setBone('head', 'x', 15 + s(t * 0.6) * 3);
      ctrl.setBone('head', 'z', s(t * 0.8) * 5);
      ctrl.setBone('spine', 'x', 8 + s(t * 0.6) * 2);
      break;
    case 'lapDance':
      ctrl.setBone('hips', 'x', s(t * 2.0) * 10);
      ctrl.setBone('hips', 'z', s(t * 1.0) * 12);
      ctrl.setBone('hips', 'y', s(t * 2.0) * 8);
      ctrl.setBone('spine', 'x', s(t * 2.0 + 0.8) * -8);
      ctrl.setBone('spine', 'z', s(t * 1.0 + 0.5) * -6);
      ctrl.setBone('chest', 'x', -8 + s(t * 2.0 + 1.2) * 10);
      ctrl.setBone('head', 'x', -5 + s(t * 1.0) * 5);
      ctrl.setBone('head', 'z', s(t * 1.0 + 0.3) * 4);
      ctrl.setBone('leftUpperArm', 'z', 40 + s(t * 0.8) * 10);
      ctrl.setBone('rightUpperArm', 'z', -40 + s(t * 0.8 + 1) * 10);
      ctrl.setBone('leftUpperLeg', 'z', 15);
      ctrl.setBone('rightUpperLeg', 'z', -15);
      break;
    case 'twerk':
      ctrl.setBone('hips', 'x', 25 + s(t * 8) * 15);
      ctrl.setBone('hips', 'z', s(t * 4) * 5);
      ctrl.setBone('spine', 'x', -10 + s(t * 8 + 0.3) * -5);
      ctrl.setBone('chest', 'x', -15);
      ctrl.setBone('head', 'x', -10);
      ctrl.setBone('leftUpperLeg', 'x', -20);
      ctrl.setBone('rightUpperLeg', 'x', -20);
      ctrl.setBone('leftLowerLeg', 'x', -40);
      ctrl.setBone('rightLowerLeg', 'x', -40);
      ctrl.setBone('leftUpperArm', 'z', 60);
      ctrl.setBone('rightUpperArm', 'z', -60);
      ctrl.setBone('leftLowerArm', 'y', 90);
      ctrl.setBone('rightLowerArm', 'y', -90);
      break;
    case 'grinding':
      ctrl.setBone('hips', 'x', s(t * 1.5) * 10);
      ctrl.setBone('hips', 'z', s(t * 1.5 + 1.57) * 10);
      ctrl.setBone('hips', 'y', s(t * 1.5 + 0.78) * 8);
      ctrl.setBone('spine', 'x', s(t * 1.5 + 0.5) * -6);
      ctrl.setBone('spine', 'z', s(t * 1.5 + 2.07) * -5);
      ctrl.setBone('chest', 'x', -5 + s(t * 0.75) * 4);
      ctrl.setBone('head', 'x', -8 + s(t * 0.75) * 3);
      ctrl.setBone('head', 'z', s(t * 1.5) * 4);
      ctrl.setBone('leftUpperArm', 'z', 50 + s(t * 0.75) * 5);
      ctrl.setBone('rightUpperArm', 'z', -50 + s(t * 0.75 + 1) * 5);
      break;
    case 'cowgirl':
      ctrl.setBone('hips', 'x', s(t * 3.0) * 12);
      ctrl.setBone('hips', 'z', s(t * 1.5) * 6);
      ctrl.setBone('spine', 'x', s(t * 3.0 + 0.5) * -8);
      ctrl.setBone('chest', 'x', -10 + s(t * 3.0 + 1.0) * 10);
      ctrl.setBone('head', 'x', -10 + s(t * 3.0 + 1.2) * 8);
      ctrl.setBone('neck', 'x', s(t * 3.0 + 0.8) * 5);
      ctrl.setBone('leftUpperLeg', 'z', 25);
      ctrl.setBone('rightUpperLeg', 'z', -25);
      ctrl.setBone('leftLowerLeg', 'x', -60);
      ctrl.setBone('rightLowerLeg', 'x', -60);
      ctrl.setBone('leftUpperArm', 'z', 30 + s(t * 1.5) * 10);
      ctrl.setBone('rightUpperArm', 'z', -30 + s(t * 1.5) * 10);
      break;
    case 'doggy':
      ctrl.setBone('spine', 'x', 45 + s(t * 3.5) * 8);
      ctrl.setBone('chest', 'x', 10 + s(t * 3.5 + 0.3) * 5);
      ctrl.setBone('hips', 'x', s(t * 3.5) * 10);
      ctrl.setBone('hips', 'z', s(t * 1.75) * 4);
      ctrl.setBone('head', 'x', -15 + s(t * 3.5 + 0.8) * 10);
      ctrl.setBone('neck', 'x', -5 + s(t * 3.5 + 0.5) * 5);
      ctrl.setBone('leftUpperArm', 'x', 60);
      ctrl.setBone('rightUpperArm', 'x', 60);
      ctrl.setBone('leftLowerArm', 'x', 30);
      ctrl.setBone('rightLowerArm', 'x', 30);
      ctrl.setBone('leftUpperLeg', 'x', -30);
      ctrl.setBone('rightUpperLeg', 'x', -30);
      ctrl.setBone('leftLowerLeg', 'x', -70);
      ctrl.setBone('rightLowerLeg', 'x', -70);
      break;
    case 'missionary':
      ctrl.setBone('spine', 'x', -60);
      ctrl.setBone('chest', 'x', -10 + s(t * 2.5) * 5);
      ctrl.setBone('hips', 'x', s(t * 2.5) * 8);
      ctrl.setBone('head', 'x', 15 + s(t * 2.5 + 0.5) * 8);
      ctrl.setBone('neck', 'x', 10);
      ctrl.setBone('leftUpperLeg', 'x', -40 + s(t * 1.25) * 10);
      ctrl.setBone('rightUpperLeg', 'x', -40 + s(t * 1.25 + 0.5) * 10);
      ctrl.setBone('leftUpperLeg', 'z', 20);
      ctrl.setBone('rightUpperLeg', 'z', -20);
      ctrl.setBone('leftLowerLeg', 'x', -50);
      ctrl.setBone('rightLowerLeg', 'x', -50);
      ctrl.setBone('leftUpperArm', 'z', 80);
      ctrl.setBone('rightUpperArm', 'z', -80);
      break;
    case 'orgasm':
      ctrl.setBone('spine', 'x', -10 + s(t * 6) * 12);
      ctrl.setBone('chest', 'x', -15 + s(t * 6 + 0.3) * 10);
      ctrl.setBone('hips', 'x', s(t * 6) * 15);
      ctrl.setBone('hips', 'z', s(t * 3) * 5);
      ctrl.setBone('head', 'x', -20 + s(t * 5) * 12);
      ctrl.setBone('head', 'z', s(t * 4) * 8);
      ctrl.setBone('neck', 'x', -10 + s(t * 5) * 6);
      ctrl.setBone('leftUpperArm', 'z', 50 + s(t * 4) * 15);
      ctrl.setBone('rightUpperArm', 'z', -50 + s(t * 4 + 1) * 15);
      ctrl.setBone('leftLowerArm', 'y', 60 + s(t * 5) * 20);
      ctrl.setBone('rightLowerArm', 'y', -60 + s(t * 5 + 1) * 20);
      ctrl.setBone('leftUpperLeg', 'z', 15 + s(t * 3) * 5);
      ctrl.setBone('rightUpperLeg', 'z', -15 + s(t * 3) * 5);
      break;
    case 'poleDance':
      ctrl.setBone('hips', 'y', t * 20 % 360);
      ctrl.setBone('hips', 'z', s(t * 1.2) * 10);
      ctrl.setBone('spine', 'x', -10 + s(t * 1.5) * 8);
      ctrl.setBone('spine', 'z', s(t * 1.2 + 0.5) * -6);
      ctrl.setBone('chest', 'x', -15 + s(t * 1.5 + 0.5) * 10);
      ctrl.setBone('head', 'x', -10 + s(t * 0.8) * 5);
      ctrl.setBone('rightUpperArm', 'z', -170 + s(t * 0.5) * 10);
      ctrl.setBone('rightLowerArm', 'y', s(t * 0.8) * 10);
      ctrl.setBone('leftUpperArm', 'z', 60 + s(t * 1.2) * 20);
      ctrl.setBone('leftLowerArm', 'y', 40 + s(t * 1.2) * 15);
      ctrl.setBone('leftUpperLeg', 'x', s(t * 1.2) * 30);
      ctrl.setBone('rightUpperLeg', 'x', s(t * 1.2 + 3.14) * 20);
      ctrl.setBone('leftLowerLeg', 'x', -30 + abs(s(t * 1.2)) * -20);
      break;
    case 'stripTease':
      ctrl.setBone('hips', 'z', s(t * 1.2) * 10);
      ctrl.setBone('hips', 'y', s(t * 0.6) * 8);
      ctrl.setBone('spine', 'z', s(t * 1.2 + 0.5) * -5);
      ctrl.setBone('chest', 'x', -8 + s(t * 0.8) * 6);
      ctrl.setBone('head', 'x', -5 + s(t * 0.6) * 4);
      ctrl.setBone('head', 'y', s(t * 0.8) * 12);
      ctrl.setBone('head', 'z', s(t * 0.6 + 0.5) * 6);
      ctrl.setBone('rightUpperArm', 'z', -30 + s(t * 0.6) * 50);
      ctrl.setBone('rightLowerArm', 'y', -40 + s(t * 0.8) * 30);
      ctrl.setBone('leftUpperArm', 'z', 30 + s(t * 0.6 + 3.14) * 50);
      ctrl.setBone('leftLowerArm', 'y', 40 + s(t * 0.8 + 3.14) * 30);
      break;
    case 'ahegao':
      ctrl.setBone('head', 'x', -15 + s(t * 4) * 8);
      ctrl.setBone('head', 'z', s(t * 2.5) * 10);
      ctrl.setBone('head', 'y', s(t * 1.5) * 6);
      ctrl.setBone('neck', 'x', -8 + s(t * 3) * 5);
      ctrl.setBone('spine', 'x', -5 + s(t * 4) * 6);
      ctrl.setBone('chest', 'x', -10 + s(t * 4 + 0.3) * 8);
      ctrl.setBone('hips', 'x', s(t * 4) * 5);
      ctrl.setBone('leftUpperArm', 'z', 40 + s(t * 2) * 10);
      ctrl.setBone('rightUpperArm', 'z', -40 + s(t * 2 + 1) * 10);
      ctrl.setBone('leftUpperLeg', 'z', 10 + s(t * 2) * 5);
      ctrl.setBone('rightUpperLeg', 'z', -10 + s(t * 2) * 5);
      break;
  }
}

export type MotionPlayer = ReturnType<typeof createMotionPlayer>;
