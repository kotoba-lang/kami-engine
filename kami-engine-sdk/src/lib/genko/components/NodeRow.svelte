<script lang="ts">
  import { agentColor, agentInitials, getSelectedIdx, getContextNodes } from '../stores/doc.svelte';
  import type { TreeNode } from '../stores/doc.svelte';

  let { node, depth = 0, onselect, ontogglevis, ontogglecollapse, ondelete, oncontext, onaddprompt }: {
    node: TreeNode;
    depth?: number;
    onselect: (gi: number) => void;
    ontogglevis: (kind: 's' | 'o', idx: number) => void;
    ontogglecollapse: (nid: string) => void;
    ondelete: (nid: string) => void;
    oncontext: (nid: string) => void;
    onaddprompt?: (nid: string) => void;
  } = $props();

  const selectedIdx = $derived(getSelectedIdx());
  const contextNodes = $derived(getContextNodes());
  const isSelected = $derived(node.gi === selectedIdx);
  const isAI = $derived(node.ref?.type === 'ai-image' || node.ref?.type === 'ai-desc' || node.ref?._genImage || node.ref?._genDesc);
  const isLink = $derived(node.ref?.type === 'link');
  const isPanel = $derived(node.ref?.type === 'panel');
  const inCtx = $derived(contextNodes.has(node.nid));
  const collapsed = $derived(!node.hasChildren ? false : undefined); // managed by parent

  function handleClick(ev: MouseEvent) {
    if ((ev.target as HTMLElement).closest('[data-action]')) return;
    if (isLink && node.ref?._href) {
      location.href = node.ref._href as string;
      return;
    }
    onselect(node.gi);
  }

  function handleKeydown(ev: KeyboardEvent) {
    if (ev.target !== ev.currentTarget) return;
    if (ev.key !== 'Enter' && ev.key !== ' ') return;
    ev.preventDefault();
    handleClick(ev as unknown as MouseEvent);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  class="nt-nd"
  class:sel={isSelected}
  class:ai={isAI}
  class:link={isLink}
  role={isLink ? 'link' : 'button'}
  aria-label={isLink ? `Open ${node.nm}` : `Select ${node.nm}`}
  style:padding-left="{8 + depth * 14}px"
  onclick={handleClick}
  onkeydown={handleKeydown}
  tabindex="0"
  draggable="true"
>
  {#if node.hasChildren}
    <button type="button" class="eye toggle btn-reset" data-action="toggle" onclick={() => ontogglecollapse(node.nid)}>
      {#if node.hasChildren}&#9660;{/if}
    </button>
  {:else}
    <span class="spacer"></span>
  {/if}

  <button
    type="button"
    class="eye"
    class:off={!node.vis}
    class:btn-reset={true}
    data-action="vis"
    onclick={() => ontogglevis(node.kind, node.idx)}
  >&#9673;</button>

  {#if node.agent}
    <span class="agent-badge" style:background={agentColor(node.agent)} title={node.agent}>
      {agentInitials(node.agent)}
    </span>
  {/if}

  {#if isLink}
    <span class="link-arrow">➜</span>
  {/if}

  <span class="nm" class:link-text={isLink}>{node.nm}</span>

  {#if isLink && node.ref?._subtitle}
    <span class="subtitle">{node.ref._subtitle}</span>
  {/if}

  <button
    type="button"
    class="ndel ctx"
    class:active={inCtx}
    class:btn-reset={true}
    data-action="ctx"
    onclick={() => oncontext(node.nid)}
    title={inCtx ? 'Remove from context' : 'Add to context'}
  >{inCtx ? '★' : '☆'}</button>

  {#if isPanel && onaddprompt}
    <button type="button" class="ndel prompt-btn btn-reset" data-action="addprompt" onclick={() => onaddprompt(node.nid)}>+P</button>
  {/if}

  <button type="button" class="ndel del-btn btn-reset" data-action="del" onclick={() => ondelete(node.nid)}>x</button>
</div>

<style>
  .nt-nd { display:flex; align-items:center; gap:1px; height:22px; font-size:11px; cursor:default; user-select:none; border:1px solid transparent; }
  .nt-nd:hover { background:rgba(0,0,0,0.04); }
  .nt-nd.sel { background:#ffe0e8; }
  .nt-nd.ai { background:rgba(96,144,224,0.06); }
  .nt-nd.link { background:rgba(64,160,96,0.08); cursor:pointer; }
  .spacer { width:12px; display:inline-block; }
  .btn-reset { background:none; border:none; padding:0; font:inherit; color:inherit; }
  .toggle { cursor:pointer; font-size:9px; width:12px; text-align:center; }
  .eye { font-size:10px; cursor:pointer; flex-shrink:0; color:#666; }
  .eye.off { opacity:0.3; }
  .agent-badge { display:inline-block; width:14px; height:14px; border-radius:50%; color:#fff; font-size:7px; text-align:center; line-height:14px; flex-shrink:0; margin-right:2px; }
  .link-arrow { font-size:10px; margin-right:2px; color:#409060; }
  .nm { flex:1; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:11px; }
  .nm.link-text { color:#307050; }
  .subtitle { font-size:9px; color:#888; margin-left:4px; }
  .ndel { font-size:10px; opacity:0; cursor:pointer; flex-shrink:0; padding:0 2px; }
  .nt-nd:hover .ndel { opacity:0.6; }
  .ndel:hover { opacity:1 !important; }
  .ctx { color:#aaa; }
  .ctx.active { color:#e06090; opacity:1; }
  .prompt-btn { color:#c0a020; }
  .del-btn { color:#c06060; }
</style>
