import type { EmotionAxis, EmotionScores } from '../types/voice.js';

/** Keyword patterns for emotion detection (Japanese + English). */
const PATTERNS: Record<EmotionAxis, RegExp> = {
  joy:        /嬉し|楽し|happy|glad|love|好き|素敵|最高|wonderful|great|awesome|やった|よかった|❤|😊|😄|🥰/i,
  anger:      /怒|腹立|angry|hate|嫌い|ムカ|fury|furious|許さ|💢|😡|🤬/i,
  sadness:    /悲し|寂し|sad|lonely|泣|つら|辛い|切な|😢|😭|💔/i,
  surprise:   /驚|びっくり|wow|すごい|amazing|まさか|えっ|信じ|😲|😮|🤯/i,
  fear:       /怖|恐|fear|scared|危な|やば|不安|😨|😱|😰/i,
  disgust:    /気持ち悪|gross|きもい|汚|disgusting|🤢|🤮/i,
  contempt:   /馬鹿|くだらない|whatever|しょうもない|つまら|😒|🙄/i,
  excitement: /興奮|excited|amazing|キター|ワクワク|すげ|やばい|🔥|🎉|✨/i,
};

/**
 * Analyze text for 8-axis emotion scores using keyword heuristics.
 *
 * No LLM required — instant keyword matching with Japanese + English patterns.
 * Returns normalized scores in [0, 1].
 */
export function analyzeTextEmotion(text: string): EmotionScores {
  const scores: EmotionScores = {
    joy: 0, anger: 0, sadness: 0, surprise: 0,
    fear: 0, disgust: 0, contempt: 0, excitement: 0,
  };
  if (!text) return scores;

  for (const [axis, pattern] of Object.entries(PATTERNS) as [EmotionAxis, RegExp][]) {
    const matches = text.match(new RegExp(pattern.source, 'gi'));
    if (matches) scores[axis] = Math.min(1.0, matches.length * 0.4);
  }

  const sum = Object.values(scores).reduce((a, b) => a + b, 0);
  if (sum < 0.1) return scores;
  if (sum > 1.5) {
    const scale = 1.5 / sum;
    for (const axis of Object.keys(scores) as EmotionAxis[]) {
      scores[axis] *= scale;
    }
  }
  return scores;
}

/**
 * Map 8-axis emotion scores to VRM morph target weights.
 *
 * Returns a sparse morph weight map (index → weight) suitable for
 * passing to `MorphController.applyWeights()`.
 */
export function emotionToMorphWeights(emotions: EmotionScores): Record<number, number> {
  const w: Record<number, number> = {};

  // ALL morph targets
  w[3] = Math.min(1, emotions.joy * 1.2 + emotions.excitement * 0.5);  // Joy
  w[1] = Math.min(1, emotions.anger * 1.2 + emotions.contempt * 0.5);  // Angry
  w[4] = Math.min(1, emotions.sadness * 1.2 + emotions.fear * 0.3);    // Sorrow
  w[5] = Math.min(1, emotions.surprise * 1.3 + emotions.fear * 0.4);   // Surprised
  w[2] = Math.min(1, emotions.excitement * 0.8 + emotions.joy * 0.3);  // Fun

  // Brow targets
  w[6] = emotions.anger * 0.8;      // Brow Angry
  w[8] = emotions.joy * 0.6;        // Brow Joy
  w[9] = emotions.sadness * 0.6;    // Brow Sorrow
  w[10] = emotions.surprise * 0.7;  // Brow Surprised

  // Eye targets
  w[24] = emotions.surprise * 0.5;  // Eye Spread
  w[12] = emotions.sadness * 0.2;   // Partial blink (drowsy)

  // Mouth targets
  w[36] = emotions.surprise * 0.5;  // A (open mouth)
  w[34] = Math.min(1, emotions.contempt * 0.3 + emotions.disgust * 0.3); // Small mouth

  // Filter zero weights
  for (const k of Object.keys(w)) {
    if (w[Number(k)] < 0.01) delete w[Number(k)];
  }
  return w;
}
