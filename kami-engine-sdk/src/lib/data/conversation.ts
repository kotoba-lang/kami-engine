import type { RotationAxis } from '../types/bone.js';

/** LLM response format for character-driven conversation. */
export interface ConversationResponse {
  text: string;
  emotion: ConversationEmotion;
  intensity: number;
  pose: ConversationPose;
  mouthMove: boolean;
}

export type ConversationEmotion = 'happy' | 'sad' | 'angry' | 'surprised' | 'relaxed' | 'neutral';
export type ConversationPose = 'natural' | 'wave' | 'shy' | 'tilt' | 'chinRest' | 'hip' | 'nod' | 'dance';

/** Sofia system prompt — instructs LLM to respond with emotion/pose JSON. */
export const SOFIA_SYSTEM_PROMPT = `あなたは image2vrm の 3D VRM キャラクター「ソフィア」です。明るくて好奇心旺盛な性格で、ユーザーと楽しく会話します。

重要なルール:
1. 必ず日本語で返答してください
2. 返答は JSON 形式で返してください:
{
  "text": "会話テキスト（自然な日本語）",
  "emotion": "happy|sad|angry|surprised|relaxed|neutral",
  "intensity": 0.0〜1.0,
  "pose": "natural|wave|shy|tilt|chinRest|hip|nod|dance",
  "mouthMove": true
}
3. 会話テキストは2〜3文で簡潔に
4. 感情は会話内容に合わせて自然に変化させて
5. ポーズも感情に合わせて選んで（例：嬉しい→wave、恥ずかしい→shy、考え中→chinRest）
6. あなたはバーチャルキャラクターとして自分の存在を認識しています`;

/** Emotion → VRM expression morph mapping. */
export const EMOTION_EXPRESSION_MAP: Record<ConversationEmotion, Record<string, number>> = {
  happy:     { happy: 0.8 },
  sad:       { sad: 0.7 },
  angry:     { angry: 0.6 },
  surprised: { surprised: 0.9 },
  relaxed:   { relaxed: 0.6 },
  neutral:   {},
};

/** Emotion → default pose fallback. */
export const EMOTION_POSE_FALLBACK: Record<ConversationEmotion, ConversationPose> = {
  happy: 'natural',
  sad: 'shy',
  angry: 'hip',
  surprised: 'tilt',
  relaxed: 'chinRest',
  neutral: 'natural',
};

/** Pose name → bone rotation map (degrees). */
export const CONVERSATION_POSE_MAP: Record<ConversationPose, Record<string, Partial<Record<RotationAxis, number>>>> = {
  natural:   { leftUpperArm: { z: 65 }, rightUpperArm: { z: -65 }, leftLowerArm: { y: 10 }, rightLowerArm: { y: -10 }, head: { x: 5, z: -3 } },
  wave:      { rightUpperArm: { z: -70 }, rightLowerArm: { y: -120 } },
  shy:       { head: { x: 10, z: -5 }, spine: { x: 5 } },
  tilt:      { head: { x: 10, z: -8 } },
  chinRest: { leftUpperArm: { z: 50 }, leftLowerArm: { y: 60 }, rightUpperArm: { z: -50 }, rightLowerArm: { y: -60 }, head: { x: 8 }, spine: { x: 5 } },
  hip:       { leftUpperArm: { z: 50 }, leftLowerArm: { y: 80 }, rightUpperArm: { z: -50 }, rightLowerArm: { y: -80 } },
  nod:       { head: { x: 5 } },
  dance:     {},
};

/** Autonomous idle prompts (Japanese). */
export const AUTO_PROMPTS = [
  '（暇そうにしている…何か話しかけようかな）ユーザーに自分から話しかけてください。日常の話題や質問をしてみて。',
  '（ふと思いついたことがある）ユーザーに面白い話題を振ってください。',
  '（少し退屈になってきた…）ユーザーの注意を引くような面白いことを言ってください。',
  '（何か気になることがある）ユーザーに質問してみてください。趣味や好きなことについて。',
  '（楽しい気分になってきた）ユーザーと一緒に楽しめる話題を提案してください。',
  '（ちょっとポーズを変えたい気分）新しいポーズをとりながら、そのポーズについてコメントしてください。',
];

/**
 * Parse LLM response text to ConversationResponse.
 *
 * Extracts JSON from raw LLM output (handles markdown code blocks, etc.).
 * Falls back to neutral if parsing fails.
 */
export function parseLLMResponse(raw: string): ConversationResponse {
  try {
    const match = raw.match(/\{[\s\S]*\}/);
    if (match) return JSON.parse(match[0]);
  } catch { /* ignore parse errors */ }
  return { text: raw, emotion: 'neutral', intensity: 0.5, pose: 'natural', mouthMove: false };
}
