import type { MotionKey } from '../types/motion.js';

/**
 * Canonical motion key mapping from SDK (camelCase) to kami-web WASM (snake_case).
 *
 * Only keys present in this map are considered supported by the current
 * canonical Rust `evaluate_motion` implementation.
 */
export const MOTION_KEY_TO_WASM: Partial<Record<MotionKey, string>> = {
  idle: 'idle',
  breathe: 'breathe',
  nod: 'nod',
  shake: 'shake',
  waveHi: 'wave_hi',
  dance: 'dance',
  bounce: 'bounce',
  sway: 'sway',
  lookAround: 'look_around',
  excited: 'excited',
  sadSway: 'sad_sway',
};

/** Returns canonical wasm key if supported by current Rust engine motion set. */
export function toWasmMotionKey(key: MotionKey): string | null {
  return MOTION_KEY_TO_WASM[key] ?? null;
}

