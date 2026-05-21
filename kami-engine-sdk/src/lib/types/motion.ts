/** Procedural motion animation key. */
export type MotionKey =
  | 'idle' | 'breathe' | 'nod' | 'shake'
  | 'waveHi' | 'dance' | 'bounce' | 'sway'
  | 'lookAround' | 'excited' | 'sadSway'
  | 'lapDance' | 'twerk' | 'grinding' | 'cowgirl'
  | 'doggy' | 'missionary' | 'orgasm' | 'poleDance'
  | 'stripTease' | 'ahegao';

/** Named motion preset for UI display. */
export interface MotionPreset {
  name: string;
  key: MotionKey | null;
}
