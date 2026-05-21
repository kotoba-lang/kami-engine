<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { DualEngineState } from '../types/engine.js';
  import type { VrmEngine } from '../builders/createVrmEngine.svelte.js';
  import VrmCanvas from './VrmCanvas.svelte';
  import ExpressionPanel from './ExpressionPanel.svelte';
  import PosePanel from './PosePanel.svelte';
  import MotionPanel from './MotionPanel.svelte';
  import PartPicker from './PartPicker.svelte';
  import VoicePanel from './VoicePanel.svelte';
  import ChatPanel from './ChatPanel.svelte';

  type PanelName = 'expression' | 'pose' | 'motion' | 'parts' | 'voice' | 'chat';

  interface Props {
    vrmUrl: string;
    r2Base?: string;
    wasmUrl?: string;
    engines?: ('kami')[];
    embed?: boolean;
    panels?: PanelName[];
    activeTab?: PanelName;
    engine?: VrmEngine;
    extraTabs?: Snippet<[{ engine: VrmEngine }]>;
    onready?: (state: DualEngineState) => void;
    class?: string;
  }

  let {
    vrmUrl,
    r2Base = '',
    wasmUrl,
    engines = ['kami'],
    embed = false,
    panels = ['chat', 'expression', 'pose', 'motion', 'parts', 'voice'],
    activeTab = $bindable('chat'),
    engine = $bindable(),
    extraTabs,
    onready,
    class: className,
  }: Props = $props();

  const TAB_LABELS: Record<PanelName, string> = {
    chat: 'Chat',
    expression: 'Expression',
    pose: 'Pose',
    motion: 'Motion',
    parts: 'Parts',
    voice: 'Voice',
  };
</script>

<div style="display:flex;height:100%;min-height:0;{className ? '' : ''}" class={className}>
  <!-- Canvas area -->
  <div style="flex:1;min-height:0;min-width:0;display:flex;position:relative">
    <VrmCanvas {vrmUrl} {r2Base} {wasmUrl} {engines} {onready} bind:engine />
  </div>

  <!-- Side panel -->
  {#if !embed && engine}
    <div style="width:280px;background:rgba(16,14,24,0.98);border-left:1px solid rgba(255,255,255,0.06);display:flex;flex-direction:column;flex-shrink:0;overflow:hidden">
      <!-- Tabs -->
      <div style="display:flex;border-bottom:1px solid rgba(255,255,255,0.08);flex-shrink:0">
        {#each panels as tab (tab)}
          <button
            style="flex:1;padding:8px 2px;border:none;background:transparent;color:{activeTab === tab ? '#fff' : 'rgba(255,255,255,0.4)'};font-size:9px;cursor:pointer;border-bottom:2px solid {activeTab === tab ? '#7c3aed' : 'transparent'}"
            onclick={() => activeTab = tab}
          >
            {TAB_LABELS[tab]}
          </button>
        {/each}
      </div>

      <!-- Panel content -->
      <div style="flex:1;overflow:{activeTab === 'chat' ? 'hidden' : 'auto'};padding:{activeTab === 'chat' ? '0' : '12px'};display:flex;flex-direction:column">
        {#if activeTab === 'chat' && panels.includes('chat')}
          <ChatPanel {engine} />
        {:else if activeTab === 'expression' && panels.includes('expression')}
          <ExpressionPanel {engine} showSliders />
        {:else if activeTab === 'pose' && panels.includes('pose')}
          <PosePanel {engine} />
        {:else if activeTab === 'motion' && panels.includes('motion')}
          <MotionPanel {engine} />
        {:else if activeTab === 'parts' && panels.includes('parts')}
          <PartPicker {engine} {r2Base} />
        {:else if activeTab === 'voice' && panels.includes('voice')}
          <VoicePanel {engine} />
        {/if}

        {#if extraTabs && engine}
          {@render extraTabs({ engine })}
        {/if}
      </div>
    </div>
  {/if}
</div>
