import type { EmotionScores, EmotionAxis } from '../types/voice.js';
import type { MorphController } from './createMorphController.svelte.js';
import { analyzeTextEmotion, emotionToMorphWeights } from '../data/emotion-patterns.js';

/** Options for creating an emotion analyzer. */
export interface EmotionAnalyzerOpts {
  /** Morph controller for applying emotion → expression mapping. */
  morphController?: MorphController;
  /** Auto-sync detected emotions to VRM morphs. Default: true. */
  syncToVrm?: boolean;
}

/**
 * Headless 8-axis emotion analyzer with VRM expression mapping.
 *
 * Analyzes text for emotional content using keyword heuristics (no LLM).
 * Optionally syncs detected emotions to VRM morph targets in real-time.
 */
export function createEmotionAnalyzer(opts?: EmotionAnalyzerOpts) {
  let scores = $state<EmotionScores>({
    joy: 0, anger: 0, sadness: 0, surprise: 0,
    fear: 0, disgust: 0, contempt: 0, excitement: 0,
  });
  let syncEnabled = $state(opts?.syncToVrm ?? true);

  /** Analyze text and return emotion scores. */
  function analyze(text: string): EmotionScores {
    const result = analyzeTextEmotion(text);
    setScores(result);
    return result;
  }

  /** Apply emotion scores to VRM morph targets. */
  function applyToCharacter(emotions: EmotionScores) {
    if (!opts?.morphController) return;
    const weights = emotionToMorphWeights(emotions);
    opts.morphController.applyWeights(weights);
  }

  /** Set emotion scores directly. Syncs to VRM if enabled. */
  function setScores(newScores: EmotionScores) {
    scores = { ...newScores };
    if (syncEnabled) applyToCharacter(scores);
  }

  /** Reset all emotion scores to zero. */
  function reset() {
    setScores({
      joy: 0, anger: 0, sadness: 0, surprise: 0,
      fear: 0, disgust: 0, contempt: 0, excitement: 0,
    });
  }

  return {
    get scores() { return scores; },
    get syncEnabled() { return syncEnabled; },
    set syncEnabled(v: boolean) { syncEnabled = v; },
    analyze,
    setScores,
    applyToCharacter,
    reset,
  };
}

export type EmotionAnalyzer = ReturnType<typeof createEmotionAnalyzer>;
