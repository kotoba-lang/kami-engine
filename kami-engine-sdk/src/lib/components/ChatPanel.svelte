<script lang="ts">
  import type { VrmEngine } from '../builders/createVrmEngine.svelte.js';
  import { createVoiceSynth } from '../builders/createVoiceSynth.svelte.js';
  import { createConversationController, type ConversationController } from '../builders/createConversationController.svelte.js';

  interface Props {
    engine: VrmEngine;
    /** Bindable conversation controller for external access. */
    controller?: ConversationController;
    /** LLM endpoint. Default: `/api/llm`. */
    llmEndpoint?: string;
    class?: string;
  }

  let {
    engine,
    controller = $bindable(),
    llmEndpoint = '/api/llm',
    class: className,
  }: Props = $props();

  let inputText = $state('');
  let chatEl: HTMLDivElement;

  $effect(() => {
    const voice = createVoiceSynth({ morphController: engine.morphs });
    controller = createConversationController({ engine, voice, llmEndpoint });
  });

  function handleSend() {
    if (!inputText.trim()) return;
    controller!.send(inputText);
    inputText = '';
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  /** Auto-scroll to bottom when messages change. */
  $effect(() => {
    if (controller?.messages && chatEl) {
      requestAnimationFrame(() => {
        chatEl.scrollTop = chatEl.scrollHeight;
      });
    }
  });

  const EMOTION_COLORS: Record<string, string> = {
    happy: '#fbbf24',
    sad: '#60a5fa',
    angry: '#ef4444',
    surprised: '#f97316',
    relaxed: '#a78bfa',
    neutral: '#9ca3af',
  };
</script>

<div style="display:flex;flex-direction:column;height:100%;{className ? '' : ''}" class={className}>
  <!-- Messages -->
  <div bind:this={chatEl} style="flex:1;overflow-y:auto;padding:8px;display:flex;flex-direction:column;gap:6px">
    {#each controller?.messages ?? [] as msg (msg.timestamp)}
      <div style="display:flex;flex-direction:column;align-items:{msg.role === 'user' ? 'flex-end' : 'flex-start'}">
        <div style="
          max-width:85%;padding:8px 12px;border-radius:12px;font-size:13px;line-height:1.4;
          background:{msg.role === 'user' ? '#7c3aed' : 'rgba(255,255,255,0.08)'};
          color:{msg.role === 'user' ? '#fff' : 'rgba(255,255,255,0.9)'};
          border-bottom-{msg.role === 'user' ? 'right' : 'left'}-radius:4px;
        ">
          {msg.text}
        </div>
        {#if msg.emotion && msg.role === 'assistant'}
          <div style="display:flex;align-items:center;gap:4px;margin-top:2px;padding:0 4px">
            <span style="width:6px;height:6px;border-radius:50%;background:{EMOTION_COLORS[msg.emotion] ?? '#888'}"></span>
            <span style="font-size:9px;color:rgba(255,255,255,0.4)">
              {msg.emotion} {msg.intensity ? Math.round(msg.intensity * 100) + '%' : ''}
              {msg.pose ? '· ' + msg.pose : ''}
            </span>
          </div>
        {/if}
      </div>
    {/each}

    {#if controller?.busy}
      <div style="display:flex;align-items:flex-start">
        <div style="padding:8px 12px;border-radius:12px;font-size:13px;background:rgba(255,255,255,0.08);color:rgba(255,255,255,0.4)">
          <span style="animation:pulse 1.5s infinite">考え中...</span>
        </div>
      </div>
    {/if}
  </div>

  <!-- Input -->
  <div style="display:flex;gap:6px;padding:8px;border-top:1px solid rgba(255,255,255,0.08);flex-shrink:0">
    <input
      type="text"
      bind:value={inputText}
      onkeydown={handleKeydown}
      placeholder="ソフィアに話しかける..."
      disabled={controller?.busy}
      style="flex:1;padding:8px 12px;background:rgba(255,255,255,0.08);color:#fff;font-size:13px;border-radius:8px;border:1px solid rgba(255,255,255,0.1);outline:none"
    />
    <button
      onclick={handleSend}
      disabled={controller?.busy || !inputText.trim()}
      style="padding:8px 16px;font-size:13px;border-radius:8px;border:none;cursor:pointer;background:#7c3aed;color:#fff;opacity:{controller?.busy || !inputText.trim() ? '0.5' : '1'}"
    >
      送信
    </button>
  </div>
</div>

<style>
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }
</style>
