import type { VoiceType, ProsodyModulation, EmotionAxis } from '../types/voice.js';

/** Kokoro-82M voice presets (ONNX WASM, @met4citizen/headtts). */
export interface KokoroVoice {
  name: string;
  key: string;
  gender: 'F' | 'M';
  accent: 'US' | 'UK';
}

/** 18 Kokoro TTS voice presets. afHeart is the Sofia default. */
export const KOKORO_VOICES: KokoroVoice[] = [
  { name: 'Heart', key: 'afHeart', gender: 'F', accent: 'US' },
  { name: 'Bella', key: 'afBella', gender: 'F', accent: 'US' },
  { name: 'Nicole', key: 'afNicole', gender: 'F', accent: 'US' },
  { name: 'Sarah', key: 'afSarah', gender: 'F', accent: 'US' },
  { name: 'Alloy', key: 'afAlloy', gender: 'F', accent: 'US' },
  { name: 'Nova', key: 'afNova', gender: 'F', accent: 'US' },
  { name: 'Sky', key: 'afSky', gender: 'F', accent: 'US' },
  { name: 'Jessica', key: 'afJessica', gender: 'F', accent: 'US' },
  { name: 'Fenrir', key: 'amFenrir', gender: 'M', accent: 'US' },
  { name: 'Michael', key: 'amMichael', gender: 'M', accent: 'US' },
  { name: 'Puck', key: 'amPuck', gender: 'M', accent: 'US' },
  { name: 'Echo', key: 'amEcho', gender: 'M', accent: 'US' },
  { name: 'Eric', key: 'amEric', gender: 'M', accent: 'US' },
  { name: 'Liam', key: 'amLiam', gender: 'M', accent: 'US' },
  { name: 'Emma', key: 'bfEmma', gender: 'F', accent: 'UK' },
  { name: 'Isabella', key: 'bfIsabella', gender: 'F', accent: 'UK' },
  { name: 'George', key: 'bmGeorge', gender: 'M', accent: 'UK' },
  { name: 'Daniel', key: 'bmDaniel', gender: 'M', accent: 'UK' },
];

/**
 * Oculus viseme → VRM morph target mapping for Kokoro phoneme-level lip sync.
 *
 * Kokoro-82M outputs phoneme timestamps with Oculus viseme IDs.
 * Each viseme maps to one or more VRM morph targets with weights.
 */
export const KOKORO_VISEME_MAP: Record<string, { index: number; weight: number }[]> = {
  sil: [],
  aa:  [{ index: 36, weight: 1.0 }],
  E:   [{ index: 39, weight: 1.0 }],
  I:   [{ index: 37, weight: 1.0 }],
  O:   [{ index: 40, weight: 1.0 }],
  U:   [{ index: 38, weight: 1.0 }],
  PP:  [],
  FF:  [{ index: 37, weight: 0.4 }],
  TH:  [{ index: 37, weight: 0.3 }],
  DD:  [{ index: 36, weight: 0.3 }],
  kk:  [{ index: 40, weight: 0.3 }],
  CH:  [{ index: 37, weight: 0.5 }],
  SS:  [{ index: 37, weight: 0.3 }],
  nn:  [{ index: 36, weight: 0.2 }],
  RR:  [{ index: 40, weight: 0.4 }],
};

/** 8 Web Speech API voice type presets (fallback when Kokoro unavailable). */
export const VOICE_TYPES: VoiceType[] = [
  { name: 'Bright', key: 'bright', pitch: 1.3, rate: 1.1, volume: 0.9 },
  { name: 'Calm', key: 'calm', pitch: 0.9, rate: 0.9, volume: 0.7 },
  { name: 'Soft', key: 'soft', pitch: 1.1, rate: 0.95, volume: 0.65 },
  { name: 'Cool', key: 'cool', pitch: 0.85, rate: 1.0, volume: 0.8 },
  { name: 'Deep', key: 'deep', pitch: 0.7, rate: 0.85, volume: 0.85 },
  { name: 'Child', key: 'child', pitch: 1.5, rate: 1.2, volume: 0.8 },
  { name: 'Robot', key: 'robot', pitch: 0.6, rate: 0.7, volume: 0.9 },
  { name: 'Whisper', key: 'whisper', pitch: 1.0, rate: 0.8, volume: 0.4 },
];

/** Emotion-driven prosody modulation factors per axis. */
export const PROSODY_MODULATION: Record<EmotionAxis, ProsodyModulation> = {
  joy:        { pitch: 1.10, rate: 1.05, volume: 1.05 },
  anger:      { pitch: 1.05, rate: 1.20, volume: 1.10 },
  sadness:    { pitch: 0.90, rate: 0.85, volume: 0.90 },
  surprise:   { pitch: 1.15, rate: 1.15, volume: 1.10 },
  fear:       { pitch: 1.10, rate: 1.10, volume: 0.85 },
  disgust:    { pitch: 0.95, rate: 0.90, volume: 0.95 },
  contempt:   { pitch: 0.90, rate: 0.85, volume: 0.90 },
  excitement: { pitch: 1.20, rate: 1.15, volume: 1.10 },
};

/** Viseme morph target indices for mouth sync during TTS. */
export const VISEME_INDICES = {
  a: 36,
  i: 37,
  u: 38,
  e: 39,
  o: 40,
} as const;

/** Viseme cycle order for lip sync animation. */
export const VISEME_CYCLE = [36, 40, 39, 38, 37] as const;
