/** Eight-axis emotion model (Hume-inspired). */
export type EmotionAxis = 'joy' | 'anger' | 'sadness' | 'surprise' | 'fear' | 'disgust' | 'contempt' | 'excitement';

/** Emotion score vector (0.0–1.0 per axis). */
export type EmotionScores = Record<EmotionAxis, number>;

/** Voice type preset for TTS prosody. */
export interface VoiceType {
  name: string;
  key: string;
  pitch: number;
  rate: number;
  volume: number;
}

/** Emotion-driven prosody modulation factors. */
export interface ProsodyModulation {
  pitch: number;
  rate: number;
  volume: number;
}
