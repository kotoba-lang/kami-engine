import type { VrmEngine } from './createVrmEngine.svelte.js';
import type { VoiceSynth } from './createVoiceSynth.svelte.js';
import type { RotationAxis } from '../types/bone.js';
import {
  SOFIA_SYSTEM_PROMPT,
  EMOTION_EXPRESSION_MAP,
  EMOTION_POSE_FALLBACK,
  CONVERSATION_POSE_MAP,
  AUTO_PROMPTS,
  parseLLMResponse,
  type ConversationResponse,
  type ConversationEmotion,
  type ConversationPose,
} from '../data/conversation.js';

/** Chat message in the conversation history. */
export interface ChatMessage {
  role: 'user' | 'assistant';
  text: string;
  emotion?: ConversationEmotion;
  intensity?: number;
  pose?: ConversationPose;
  timestamp: number;
}

/** Options for creating a conversation controller. */
export interface ConversationControllerOpts {
  engine: VrmEngine;
  voice: VoiceSynth;
  /** LLM endpoint URL. Default: `/api/llm`. */
  llmEndpoint?: string;
  /** LLM model name. Default: `qwen3.5-4b`. */
  llmModel?: string;
  /** System prompt override. Default: Sofia prompt. */
  systemPrompt?: string;
  /** Enable autonomous idle behavior. Default: true. */
  autoIdle?: boolean;
  /** Idle timeout (ms) before autonomous action. Default: 15000. */
  idleTimeout?: number;
}

/**
 * Conversation controller — orchestrates LLM ↔ emotion ↔ expression ↔ pose ↔ TTS.
 *
 * Flow: User text → LLM (qwen3.5-4b) → JSON { text, emotion, intensity, pose, mouthMove }
 * → smoothExpr() (emotion→morph, eased 500ms) + smoothPose() (pose→bones, eased 600ms)
 * → Kokoro TTS (phoneme→viseme lip sync) simultaneously.
 *
 * Expression panel auto-reflects current morph state since VrmEngine.morphs
 * is the single source of truth for both manual sliders and conversation-driven changes.
 */
export function createConversationController(opts: ConversationControllerOpts) {
  const endpoint = opts.llmEndpoint ?? '/api/llm';
  const model = opts.llmModel ?? 'qwen3.5-4b';
  const systemPrompt = opts.systemPrompt ?? SOFIA_SYSTEM_PROMPT;

  let messages = $state<ChatMessage[]>([]);
  let busy = $state(false);
  let lastUserAction = $state(Date.now());
  let convoHistory: { role: string; content: string }[] = [];
  let idleIntervalId: ReturnType<typeof setInterval> | null = null;

  /** Smooth expression transition (ease-out-quad, 500ms). */
  function smoothExpr(target: Record<string, number>, duration = 500) {
    const vrm = (opts.engine.state.three as any)?.vrm;
    if (!vrm?.expressionManager) return;

    const names = ['happy', 'angry', 'sad', 'surprised', 'relaxed'];
    const start: Record<string, number> = {};
    for (const n of names) start[n] = vrm.expressionManager.getValue(n) ?? 0;

    const t0 = performance.now();
    function tick() {
      const p = Math.min(1, (performance.now() - t0) / duration);
      const e = p < 0.5 ? 2 * p * p : 1 - Math.pow(-2 * p + 2, 2) / 2;
      for (const n of names) {
        vrm.expressionManager.setValue(n, start[n] + ((target[n] ?? 0) - start[n]) * e);
      }
      if (p < 1) requestAnimationFrame(tick);
    }
    requestAnimationFrame(tick);
  }

  /** Smooth pose transition (ease-out-quad, 600ms). */
  function smoothPose(poseName: ConversationPose, duration = 600) {
    const vrm = (opts.engine.state.three as any)?.vrm;
    if (!vrm?.humanoid) return;

    const targetPose = CONVERSATION_POSE_MAP[poseName] ?? CONVERSATION_POSE_MAP.natural;
    const boneNames = ['head', 'neck', 'spine', 'chest', 'leftUpperArm', 'leftLowerArm', 'rightUpperArm', 'rightLowerArm', 'hips'];

    const startRot: Record<string, { x: number; y: number; z: number }> = {};
    for (const b of boneNames) {
      const node = vrm.humanoid.getNormalizedBoneNode(b);
      if (node) startRot[b] = { x: node.rotation.x, y: node.rotation.y, z: node.rotation.z };
    }

    const t0 = performance.now();
    function tick() {
      const p = Math.min(1, (performance.now() - t0) / duration);
      const e = p < 0.5 ? 2 * p * p : 1 - Math.pow(-2 * p + 2, 2) / 2;

      for (const b of boneNames) {
        const node = vrm.humanoid.getNormalizedBoneNode(b);
        if (!node || !startRot[b]) continue;
        const target = targetPose[b] ?? {};
        for (const axis of ['x', 'y', 'z'] as RotationAxis[]) {
          const targetRad = ((target[axis] ?? 0) * Math.PI) / 180;
          node.rotation[axis] = startRot[b][axis] + (targetRad - startRot[b][axis]) * e;
        }
      }
      if (p < 1) requestAnimationFrame(tick);
    }
    requestAnimationFrame(tick);
  }

  /**
   * Apply LLM response to character — emotion, pose, and mouth simultaneously.
   *
   * This is the core sync function. Expression panel auto-reflects changes
   * because VrmEngine.morphs.weights is the shared reactive state.
   */
  function applyResponse(resp: ConversationResponse) {
    const emotion = resp.emotion || 'neutral';
    const intensity = Math.max(0, Math.min(1, resp.intensity ?? 0.5));

    // 1. Expression: emotion × intensity → morph weights (eased 500ms)
    const exprMap = EMOTION_EXPRESSION_MAP[emotion] ?? {};
    const scaledExpr: Record<string, number> = {};
    for (const [k, v] of Object.entries(exprMap)) {
      scaledExpr[k] = v * intensity;
    }
    smoothExpr(scaledExpr, 500);

    // 2. Pose: explicit pose from LLM, fallback to emotion default (eased 600ms)
    const poseName = resp.pose || EMOTION_POSE_FALLBACK[emotion] || 'natural';
    smoothPose(poseName, 600);

    // 3. Voice: Kokoro TTS with phoneme viseme sync
    if (resp.mouthMove && resp.text) {
      opts.voice.speak(resp.text);
    }
  }

  /** Call LLM with conversation history. */
  async function callLLM(): Promise<ConversationResponse> {
    const resp = await fetch(endpoint, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        model,
        messages: [{ role: 'system', content: systemPrompt }, ...convoHistory],
        maxTokens: 512,
        temperature: 0.85,
      }),
    });
    if (!resp.ok) throw new Error(`LLM ${resp.status}`);
    const data = await resp.json();
    const raw = data?.choices?.[0]?.message?.content ?? '';
    return parseLLMResponse(raw);
  }

  /** Send a user message and get AI response with expression sync. */
  async function send(text: string) {
    if (!text.trim() || busy) return;
    busy = true;
    lastUserAction = Date.now();

    // Add user message
    messages = [...messages, { role: 'user', text, timestamp: Date.now() }];
    convoHistory.push({ role: 'user', content: text });

    try {
      const resp = await callLLM();

      // Add assistant message with emotion metadata
      messages = [...messages, {
        role: 'assistant',
        text: resp.text || '...',
        emotion: resp.emotion,
        intensity: resp.intensity,
        pose: resp.pose,
        timestamp: Date.now(),
      }];
      convoHistory.push({ role: 'assistant', content: JSON.stringify(resp) });

      // Apply to character (expression + pose + voice simultaneously)
      applyResponse(resp);
    } catch (e) {
      messages = [...messages, {
        role: 'assistant',
        text: 'ごめん、ちょっと調子悪いみたい...もう一回話しかけて！',
        emotion: 'sad',
        timestamp: Date.now(),
      }];
      console.error('[conversation]', e);
    }

    busy = false;
  }

  /** Trigger autonomous action (idle behavior). */
  async function autoAction() {
    if (busy) return;
    busy = true;
    const prompt = AUTO_PROMPTS[Math.floor(Math.random() * AUTO_PROMPTS.length)];
    convoHistory.push({ role: 'user', content: `[SYSTEM: ${prompt}]` });

    try {
      const resp = await callLLM();
      if (resp.text) {
        messages = [...messages, {
          role: 'assistant',
          text: resp.text,
          emotion: resp.emotion,
          intensity: resp.intensity,
          pose: resp.pose,
          timestamp: Date.now(),
        }];
        convoHistory.push({ role: 'assistant', content: JSON.stringify(resp) });
        applyResponse(resp);
      }
    } catch (e) {
      console.error('[auto-action]', e);
    }
    busy = false;
  }

  /** Micro idle expression (subtle movement without LLM call). */
  function idleMicro() {
    const vrm = (opts.engine.state.three as any)?.vrm;
    if (!vrm?.humanoid) return;
    const head = vrm.humanoid.getNormalizedBoneNode('head');
    if (head) {
      const t = performance.now() / 1000;
      head.rotation.z = Math.sin(t * 0.3) * 0.03;
    }
  }

  /** Start autonomous idle behavior loop. */
  function startIdleLoop() {
    if (idleIntervalId || opts.autoIdle === false) return;
    const timeout = opts.idleTimeout ?? 15000;

    idleIntervalId = setInterval(() => {
      const idle = Date.now() - lastUserAction;
      if (idle > timeout && !busy) {
        if (Math.random() < 0.4) autoAction();
        else idleMicro();
      } else if (idle > timeout * 0.5) {
        idleMicro();
      }
    }, 5000);
  }

  /** Stop idle loop. */
  function stopIdleLoop() {
    if (idleIntervalId) {
      clearInterval(idleIntervalId);
      idleIntervalId = null;
    }
  }

  /** Clear conversation history. */
  function clear() {
    messages = [];
    convoHistory = [];
  }

  // Auto-start idle loop
  startIdleLoop();

  return {
    get messages() { return messages; },
    get busy() { return busy; },
    send,
    autoAction,
    clear,
    startIdleLoop,
    stopIdleLoop,
    applyResponse,
  };
}

export type ConversationController = ReturnType<typeof createConversationController>;
