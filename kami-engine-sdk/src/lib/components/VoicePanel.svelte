<script lang="ts">
  import type { VrmEngine } from '../builders/createVrmEngine.svelte.js';
  import { createVoiceSynth, type VoiceSynth } from '../builders/createVoiceSynth.svelte.js';
  import { createEmotionAnalyzer, type EmotionAnalyzer } from '../builders/createEmotionAnalyzer.svelte.js';
  import { KOKORO_VOICES } from '../data/voice-catalog.js';
  import type { EmotionScores } from '../types/voice.js';

  interface Props {
    engine: VrmEngine;
    emotions?: EmotionScores;
    emotionSync?: boolean;
    class?: string;
  }

  let {
    engine,
    emotions = $bindable({ joy: 0, anger: 0, sadness: 0, surprise: 0, fear: 0, disgust: 0, contempt: 0, excitement: 0 }),
    emotionSync = true,
    class: className,
  }: Props = $props();

  let text = $state('Hello! How are you today?');
  let speed = $state(1.0);

  let voice = $state<VoiceSynth>();
  let emotionAnalyzer = $state<EmotionAnalyzer>();

  $effect(() => {
    voice = createVoiceSynth({ morphController: engine.morphs });
    emotionAnalyzer = createEmotionAnalyzer({
      morphController: engine.morphs,
      syncToVrm: emotionSync,
    });
  });

  function selectVoice(key: string) { voice?.setVoice(key); }
  function speak() { voice?.speak(text, emotions); }
  function stop() { voice?.stop(); }
  function onSpeedChange(e: Event) {
    speed = (e.target as HTMLInputElement).valueAsNumber / 100;
    voice?.setSpeed(speed);
  }

  let analyzeTimeout: ReturnType<typeof setTimeout>;
  function onTextInput() {
    clearTimeout(analyzeTimeout);
    analyzeTimeout = setTimeout(() => {
      if (emotionAnalyzer) emotions = emotionAnalyzer.analyze(text);
    }, 300);
  }

  /** Pre-init Kokoro on voice tab open. */
  $effect(() => { voice?.initKokoro(); });

  const femaleVoices = KOKORO_VOICES.filter(v => v.gender === 'F');
  const maleVoices = KOKORO_VOICES.filter(v => v.gender === 'M');
</script>

<div style="display:flex;flex-direction:column;gap:12px" class={className}>
  <!-- Kokoro status -->
  <div style="font-size:10px;padding:6px 8px;border-radius:4px;background:rgba(255,255,255,0.05)">
    {#if voice?.kokoroReady}
      <span style="color:#4ade80">● Kokoro TTS ready</span>
    {:else if voice?.kokoroLoading}
      <span style="color:#facc15">● Loading Kokoro model (~500MB)...</span>
    {:else if voice?.kokoroError}
      <span style="color:#f87171">● Kokoro failed: {voice.kokoroError}</span>
      <br/><span style="color:rgba(255,255,255,0.4)">Falling back to Web Speech API</span>
    {:else}
      <span style="color:rgba(255,255,255,0.4)">● Initializing...</span>
    {/if}
  </div>

  <!-- Voice selection -->
  <div>
    <h4 style="font-size:11px;font-weight:600;color:rgba(255,255,255,0.8);margin-bottom:6px">Voice</h4>
    <div style="font-size:9px;color:rgba(255,255,255,0.4);margin-bottom:4px">Female</div>
    <div style="display:flex;flex-wrap:wrap;gap:4px;margin-bottom:6px">
      {#each femaleVoices as v (v.key)}
        <button
          style="padding:3px 8px;font-size:10px;border-radius:4px;border:none;cursor:pointer;
            background:{voice?.activeVoice === v.key ? '#7c3aed' : 'rgba(255,255,255,0.1)'};
            color:{voice?.activeVoice === v.key ? '#fff' : 'rgba(255,255,255,0.6)'}"
          onclick={() => selectVoice(v.key)}
        >{v.name}</button>
      {/each}
    </div>
    <div style="font-size:9px;color:rgba(255,255,255,0.4);margin-bottom:4px">Male</div>
    <div style="display:flex;flex-wrap:wrap;gap:4px">
      {#each maleVoices as v (v.key)}
        <button
          style="padding:3px 8px;font-size:10px;border-radius:4px;border:none;cursor:pointer;
            background:{voice?.activeVoice === v.key ? '#7c3aed' : 'rgba(255,255,255,0.1)'};
            color:{voice?.activeVoice === v.key ? '#fff' : 'rgba(255,255,255,0.6)'}"
          onclick={() => selectVoice(v.key)}
        >{v.name}</button>
      {/each}
    </div>
  </div>

  <!-- Speed -->
  <div>
    <h4 style="font-size:11px;font-weight:600;color:rgba(255,255,255,0.8);margin-bottom:4px">Speed: {speed.toFixed(1)}x</h4>
    <input type="range" min="50" max="200" value={Math.round(speed * 100)} oninput={onSpeedChange}
      style="width:100%;accent-color:#7c3aed" />
  </div>

  <!-- Text + speak -->
  <div style="display:flex;gap:8px">
    <input type="text" bind:value={text} oninput={onTextInput}
      style="flex:1;padding:6px 8px;background:rgba(255,255,255,0.1);color:#fff;font-size:12px;border-radius:4px;border:1px solid rgba(255,255,255,0.1);outline:none"
      placeholder="Text to speak..." />
    <button onclick={speak} disabled={voice?.speaking}
      style="padding:6px 12px;font-size:12px;border-radius:4px;border:none;cursor:pointer;background:#7c3aed;color:#fff;opacity:{voice?.speaking ? '0.5' : '1'}">
      {voice?.speaking ? '...' : 'Speak'}
    </button>
    {#if voice?.speaking}
      <button onclick={stop}
        style="padding:6px 8px;font-size:12px;border-radius:4px;border:none;cursor:pointer;background:#dc2626;color:#fff">
        Stop
      </button>
    {/if}
  </div>

  <!-- Emotion -->
  <div>
    <h4 style="font-size:11px;font-weight:600;color:rgba(255,255,255,0.8);margin-bottom:6px">Emotion</h4>
    {#each Object.entries(emotions) as [axis, score] (axis)}
      <div style="display:flex;align-items:center;gap:8px;font-size:11px;margin-bottom:2px">
        <span style="width:56px;color:rgba(255,255,255,0.5);text-transform:capitalize">{axis}</span>
        <div style="flex:1;height:6px;background:rgba(255,255,255,0.1);border-radius:3px;overflow:hidden">
          <div style="background:#a855f7;height:100%;border-radius:3px;width:{Math.round(score * 100)}%;transition:width 0.2s"></div>
        </div>
        <span style="width:24px;text-align:right;color:rgba(255,255,255,0.4)">{Math.round(score * 100)}</span>
      </div>
    {/each}
  </div>
</div>
