import type { ExpressionPreset } from '../types/morph.js';

/**
 * 24 expression presets mapping to VRM morph target indices.
 *
 * Morph index reference (VRoid bodyV1.vrm, 57 targets):
 *   0=Neutral 1=Angry 2=Fun 3=Joy 4=Sorrow 5=Surprised
 *   6-10=Brow(Angry/Fun/Joy/Sorrow/Surprised) 11=EyeNatural
 *   12-24=Eye variants 25-43=Mouth variants 44-56=Teeth variants
 */
export const EXPRESSION_PRESETS: ExpressionPreset[] = [
  { name: 'Neutral', key: 'neutral', morphs: {} },
  { name: 'Joy', key: 'joy', morphs: { 3: 0.8, 8: 0.6 } },
  { name: 'Smile', key: 'smile', morphs: { 3: 0.4, 8: 0.3 } },
  { name: 'Grin', key: 'grin', morphs: { 3: 1.0, 8: 0.8, 36: 0.5 } },
  { name: 'Angry', key: 'angry', morphs: { 1: 0.8, 6: 0.8 } },
  { name: 'Furious', key: 'furious', morphs: { 1: 1.0, 6: 1.0, 34: 0.3 } },
  { name: 'Sad', key: 'sad', morphs: { 4: 0.7, 9: 0.6 } },
  { name: 'Cry', key: 'cry', morphs: { 4: 1.0, 9: 0.8, 36: 0.3 } },
  { name: 'Surprised', key: 'surprised', morphs: { 5: 0.9, 10: 0.7, 24: 0.5 } },
  { name: 'Shocked', key: 'shocked', morphs: { 5: 1.0, 10: 1.0, 24: 0.8, 36: 0.7 } },
  { name: 'Fun', key: 'fun', morphs: { 2: 0.8, 7: 0.6 } },
  { name: 'Wink', key: 'wink', morphs: { 13: 1.0, 3: 0.3 } },
  { name: 'Wink R', key: 'winkR', morphs: { 14: 1.0, 3: 0.3 } },
  { name: 'Sleepy', key: 'sleepy', morphs: { 12: 0.6, 4: 0.2 } },
  { name: 'Smug', key: 'smug', morphs: { 2: 0.4, 7: 0.3, 34: 0.2 } },
  { name: 'Aa', key: 'aa', morphs: { 36: 1.0 } },
  { name: 'Ih', key: 'ih', morphs: { 37: 1.0 } },
  { name: 'Uu', key: 'uu', morphs: { 38: 1.0 } },
  { name: 'Ee', key: 'ee', morphs: { 39: 1.0 } },
  { name: 'Oh', key: 'oh', morphs: { 40: 1.0 } },
  { name: 'Pout', key: 'pout', morphs: { 38: 0.6 } },
  { name: 'Kissy', key: 'kissy', morphs: { 34: 0.5, 38: 0.4 } },
  { name: 'Pensive', key: 'pensive', morphs: { 4: 0.3, 9: 0.2, 12: 0.3 } },
  { name: 'Confident', key: 'confident', morphs: { 2: 0.5, 7: 0.3, 11: 0.4 } },
];
