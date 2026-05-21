import type { MorphController } from './createMorphController.svelte.js';
import { KOKORO_VISEME_MAP, VISEME_CYCLE } from '../data/voice-catalog.js';
import { PROSODY_MODULATION } from '../data/voice-catalog.js';
import type { EmotionScores } from '../types/voice.js';

type HeadTTSMessage = {
  type: string;
  data?: {
    visemes?: string[];
    vtimes?: number[];
    vdurations?: number[];
    audio?: ArrayBuffer | Blob | string;
  } | string;
  audio?: ArrayBuffer | Blob | string;
};

/** Options for creating a voice synthesizer. */
export interface VoiceSynthOpts {
  morphController: MorphController;
}

/**
 * Headless voice synthesizer — Kokoro-82M (HeadTTS) with phoneme-level lip sync.
 *
 * Primary: HeadTTS (Kokoro-82M ONNX, WebGPU → WASM fallback, ~500MB cached in IndexedDB).
 * Fallback: Web Speech API with generic viseme cycling.
 *
 * HeadTTS returns per-phoneme Oculus viseme timestamps which are scheduled
 * to drive VRM morph targets for precise lip sync.
 */
export function createVoiceSynth(opts: VoiceSynthOpts) {
  let speaking = $state(false);
  let kokoroReady = $state(false);
  let kokoroLoading = $state(false);
  let kokoroError = $state('');
  let activeVoice = $state('afBella');
  let currentSpeed = $state(1.0);
  let mouthSyncInterval: ReturnType<typeof setInterval> | null = null;
  let visemeTimeouts: ReturnType<typeof setTimeout>[] = [];
  let headtts: any = null;
  let audioContext: AudioContext | null = null;

  /** Lazy-load and connect HeadTTS (Kokoro-82M). */
  async function initKokoro(): Promise<boolean> {
    if (kokoroReady) return true;
    if (kokoroLoading) return false;
    kokoroLoading = true;
    kokoroError = '';

    try {
      const mod = await import(
        /* @vite-ignore */ 'https://cdn.jsdelivr.net/npm/@met4citizen/headtts@1.2/+esm'
      );

      headtts = new mod.HeadTTS({
        endpoints: ['webgpu', 'wasm'],
        languages: ['en-us'],
      });

      await headtts.connect();
      headtts.setup({ voice: activeVoice, speed: currentSpeed });

      kokoroReady = true;
      kokoroLoading = false;
      return true;
    } catch (e: any) {
      kokoroError = e?.message ?? String(e);
      kokoroLoading = false;
      console.warn('[kami-engine-sdk] Kokoro TTS init failed:', e);
      return false;
    }
  }

  /** Clear all mouth morph targets (34-40). */
  function clearMouth() {
    for (let i = 34; i <= 40; i++) {
      opts.morphController.setMorph(i, 0);
    }
  }

  /** Apply an Oculus viseme to VRM morph targets. */
  function applyViseme(viseme: string) {
    clearMouth();
    const targets = KOKORO_VISEME_MAP[viseme];
    if (targets) {
      for (const { index, weight } of targets) {
        opts.morphController.setMorph(index, weight);
      }
    }
  }

  /** Schedule viseme animations from HeadTTS timing data. */
  function scheduleVisemes(
    visemes: string[],
    vtimes: number[],
    vdurations: number[],
    audioStartTime: number,
  ) {
    cancelVisemes();
    for (let i = 0; i < visemes.length; i++) {
      const delay = vtimes[i] - audioStartTime;
      const dur = vdurations[i];
      const v = visemes[i];

      // Apply viseme at start time
      const tOn = setTimeout(() => applyViseme(v), Math.max(0, delay));
      // Clear at end of duration
      const tOff = setTimeout(() => clearMouth(), Math.max(0, delay + dur));
      visemeTimeouts.push(tOn, tOff);
    }
  }

  /** Cancel all scheduled visemes. */
  function cancelVisemes() {
    for (const t of visemeTimeouts) clearTimeout(t);
    visemeTimeouts = [];
    clearMouth();
  }

  /** Play audio from ArrayBuffer/Blob using Web Audio API. */
  async function playAudio(audioData: any): Promise<void> {
    if (!audioContext) audioContext = new AudioContext();
    try {
      let buffer: ArrayBuffer;
      if (audioData instanceof Blob) {
        buffer = await audioData.arrayBuffer();
      } else if (audioData instanceof ArrayBuffer) {
        buffer = audioData;
      } else if (typeof audioData === 'string') {
        // Base64 WAV
        const binary = atob(audioData);
        const bytes = new Uint8Array(binary.length);
        for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
        buffer = bytes.buffer;
      } else {
        return;
      }
      const decoded = await audioContext.decodeAudioData(buffer);
      const source = audioContext.createBufferSource();
      source.buffer = decoded;
      source.connect(audioContext.destination);
      source.start();
      source.onended = () => {
        speaking = false;
        clearMouth();
      };
    } catch (e) {
      console.warn('[voice] audio playback error:', e);
      speaking = false;
      clearMouth();
    }
  }

  /**
   * Speak text using Kokoro TTS with phoneme-level viseme lip sync.
   *
   * Falls back to Web Speech API if Kokoro unavailable.
   */
  async function speak(text: string, emotions?: EmotionScores): Promise<void> {
    if (speaking) return;
    speaking = true;

    // Try Kokoro
    const ready = await initKokoro();
    if (ready && headtts) {
      try {
        headtts.setup({ voice: activeVoice, speed: currentSpeed });

        // Set up message handler before synthesize
        const audioPromise = new Promise<void>((resolve, reject) => {
          headtts.onmessage = (msg: HeadTTSMessage) => {
            const payload = typeof msg.data === 'object' && msg.data !== null ? msg.data : undefined;
            if (msg.type === 'audio' && payload) {
              const { visemes, vtimes, vdurations } = payload;
              if (visemes && vtimes && vdurations) {
                scheduleVisemes(visemes, vtimes, vdurations, vtimes[0] || 0);
              }
              // Audio data comes separately or embedded
              if (msg.audio || payload.audio) {
                playAudio(msg.audio || payload.audio).then(resolve).catch(reject);
              } else {
                resolve();
              }
            } else if (msg.type === 'error') {
              reject(new Error(typeof msg.data === 'string' ? msg.data : 'synthesis failed'));
            }
          };
        });

        await headtts.synthesize({ input: text });
        await audioPromise;
        return;
      } catch (e) {
        console.warn('[voice] Kokoro speak failed, falling back:', e);
        cancelVisemes();
      }
    }

    // Fallback: Web Speech API
    speakWebSpeech(text, emotions);
  }

  /** Start generic mouth sync cycling (Web Speech fallback). */
  function startFallbackMouthSync() {
    let i = 0;
    mouthSyncInterval = setInterval(() => {
      clearMouth();
      const weight = 0.4 + Math.random() * 0.4;
      opts.morphController.setMorph(VISEME_CYCLE[i % VISEME_CYCLE.length], weight);
      i++;
    }, 120);
  }

  /** Stop fallback mouth sync. */
  function stopFallbackMouthSync() {
    if (mouthSyncInterval) {
      clearInterval(mouthSyncInterval);
      mouthSyncInterval = null;
    }
    clearMouth();
  }

  /** Web Speech API fallback with generic mouth animation. */
  function speakWebSpeech(text: string, emotions?: EmotionScores): void {
    if (!('speechSynthesis' in window)) { speaking = false; return; }
    window.speechSynthesis.cancel();

    const utt = new SpeechSynthesisUtterance(text);
    utt.pitch = 1.0;
    utt.rate = currentSpeed;
    utt.volume = 0.9;

    if (emotions) {
      let pMod = 1, rMod = 1, vMod = 1;
      for (const [axis, score] of Object.entries(emotions)) {
        const mod = PROSODY_MODULATION[axis as keyof typeof PROSODY_MODULATION];
        if (mod && score > 0.1) {
          pMod *= 1 + (mod.pitch - 1) * score;
          rMod *= 1 + (mod.rate - 1) * score;
          vMod *= 1 + (mod.volume - 1) * score;
        }
      }
      utt.pitch = Math.max(0.1, Math.min(2, utt.pitch * pMod));
      utt.rate = Math.max(0.1, Math.min(10, utt.rate * rMod));
      utt.volume = Math.max(0, Math.min(1, utt.volume * vMod));
    }

    const voices = window.speechSynthesis.getVoices();
    const jaVoice = voices.find(v => v.lang.startsWith('ja'));
    if (jaVoice) utt.voice = jaVoice;

    utt.onstart = () => startFallbackMouthSync();
    utt.onend = () => { speaking = false; stopFallbackMouthSync(); };
    utt.onerror = () => { speaking = false; stopFallbackMouthSync(); };

    window.speechSynthesis.speak(utt);
  }

  /** Stop speaking and all lip sync. */
  function stop() {
    if (headtts?.stop) headtts.stop();
    window.speechSynthesis?.cancel();
    cancelVisemes();
    stopFallbackMouthSync();
    speaking = false;
  }

  /** Set Kokoro voice ID. */
  function setVoice(voiceKey: string) {
    activeVoice = voiceKey;
    if (headtts && kokoroReady) headtts.setup({ voice: voiceKey });
  }

  /** Set speech speed (1.0 = normal). */
  function setSpeed(speed: number) {
    currentSpeed = speed;
    if (headtts && kokoroReady) headtts.setup({ speed });
  }

  return {
    get speaking() { return speaking; },
    get kokoroReady() { return kokoroReady; },
    get kokoroLoading() { return kokoroLoading; },
    get kokoroError() { return kokoroError; },
    get activeVoice() { return activeVoice; },
    get speed() { return currentSpeed; },
    initKokoro,
    speak,
    stop,
    setVoice,
    setSpeed,
  };
}

export type VoiceSynth = ReturnType<typeof createVoiceSynth>;
