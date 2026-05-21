<script lang="ts">
  import NodeRow from './NodeRow.svelte';
  import {
    getDoc, activePage, getCollapsedNodes, computeTreeNodes,
    loadPage, saveCurrentPage, toggleNodeVisibility, deleteNode,
    toggleCollapsed, toggleContext, addOverlay, setSelectedIdx,
    nid, pid, requestRedraw,
    type TreeNode
  } from '../stores/doc.svelte';

  let { nanoid = '', onchange }: { nanoid?: string; onchange?: () => void } = $props();

  const doc = $derived(getDoc());
  const collapsedNodes = $derived(getCollapsedNodes());
  const treeNodes = $derived(computeTreeNodes());

  let collapsed = $state(false);

  function fire() { requestRedraw(); onchange?.(); }

  function handlePageClick(idx: number) {
    if (idx !== doc.activePageIdx) { loadPage(idx); fire(); }
  }

  function handleDeletePage(idx: number) {
    if (doc.pages.length <= 1) return;
    doc.pages.splice(idx, 1);
    if (doc.activePageIdx >= doc.pages.length) doc.activePageIdx = doc.pages.length - 1;
    loadPage(doc.activePageIdx);
    fire();
  }

  function handleAddPage() {
    saveCurrentPage();
    doc.pages.push({ id: pid(), name: 'Page ' + (doc.pages.length + 1), youshi: { id: nid(), type: 'b4manga', visible: true }, nodes: [] });
    loadPage(doc.pages.length - 1);
    fire();
  }

  function handleAddGroup() {
    const gn = prompt('Group name:', 'Group ' + (treeNodes.filter(n => n.ref?.type === 'group').length + 1));
    if (!gn) return;
    addOverlay({ type: 'group', groupName: gn, _nid: nid(), _visible: true, _parent: '' });
    fire();
  }

  function handleToggleVis(kind: 's' | 'o', idx: number) {
    toggleNodeVisibility(kind, idx);
    fire();
  }

  function handleDelete(dnid: string) {
    deleteNode(dnid);
    fire();
  }

  function handleSelect(gi: number) {
    setSelectedIdx(gi);
    requestRedraw();
  }

  function handleContext(dnid: string) {
    toggleContext(dnid);
  }

  function handleAddPrompt(panelNid: string) {
    const txt = prompt('Panel prompt (scene description):');
    if (!txt) return;
    addOverlay({ type: 'prompt', prompt: txt, _nid: nid(), _visible: true, _parent: panelNid, _agent: 'director' });
    fire();
  }

  function handleToggleYoushi() {
    activePage().youshi.visible = !activePage().youshi.visible;
    fire();
  }

  function childrenOf(parentNid: string): TreeNode[] {
    return treeNodes.filter(n => n.par === parentNid);
  }
</script>

<div class="nt-panel" class:collapsed>
  <div class="nt-hdr">
    <button class="nt-toggle" onclick={() => { collapsed = !collapsed }}>
      {collapsed ? '▶' : '◀'}
    </button>
    <span class="nt-title">Mangaka</span>
  </div>

  {#if !collapsed}
    <div class="nt-body">
      {#each doc.pages as pg, pi}
        <div class="nt-pg">
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="nt-pg-hdr" class:act={pi === doc.activePageIdx} onclick={() => handlePageClick(pi)}>
            <span>{pg.name}</span>
            {#if doc.pages.length > 1}
              <span class="del" onclick={(event) => { event.stopPropagation(); handleDeletePage(pi); }}>x</span>
            {/if}
          </div>

          {#if pi === doc.activePageIdx}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="nt-nd youshi" onclick={handleToggleYoushi}>
              <span class="eye" class:off={!pg.youshi.visible}>◉</span>
              <span class="nm">genkouyoushi ({pg.youshi.type})</span>
            </div>

            {#snippet renderNodes(parentNid: string, depth: number)}
              {#each childrenOf(parentNid) as node (node.nid || node.gi)}
                <NodeRow
                  {node}
                  {depth}
                  onselect={handleSelect}
                  ontogglevis={handleToggleVis}
                  ontogglecollapse={toggleCollapsed}
                  ondelete={handleDelete}
                  oncontext={handleContext}
                  onaddprompt={handleAddPrompt}
                />
                {#if node.hasChildren && !collapsedNodes.has(node.nid)}
                  {@render renderNodes(node.nid, depth + 1)}
                {/if}
              {/each}
            {/snippet}

            {@render renderNodes('', 0)}
          {/if}
        </div>
      {/each}

      <div class="nt-add">
        <button onclick={handleAddPage}>+ Page</button>
        <button onclick={handleAddGroup}>+ Group</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .nt-panel { width:var(--nt-w,220px); background:#f8f8f8; border-right:1px solid #e0e0e0; overflow-y:auto; height:100%; font-family:'Nunito',sans-serif; }
  .nt-panel.collapsed { width:28px; overflow:hidden; }
  .nt-hdr { display:flex; align-items:center; gap:4px; padding:4px 6px; border-bottom:1px solid #e0e0e0; font-size:12px; font-weight:700; }
  .nt-toggle { border:none; background:none; cursor:pointer; font-size:10px; padding:2px 4px; color:#888; }
  .nt-title { flex:1; }
  .nt-body { padding:2px 0; }
  .nt-pg-hdr { display:flex; align-items:center; padding:3px 8px; font-size:11px; font-weight:600; cursor:pointer; user-select:none; }
  .nt-pg-hdr:hover { background:#f0f0f0; }
  .nt-pg-hdr.act { background:#ffe0e8; color:#c04060; }
  .nt-pg-hdr .del { margin-left:auto; font-size:9px; color:#c06060; opacity:0; cursor:pointer; }
  .nt-pg-hdr:hover .del { opacity:0.6; }
  .youshi { display:flex; align-items:center; gap:2px; padding:2px 8px; font-size:11px; cursor:pointer; }
  .youshi .eye { font-size:10px; color:#666; }
  .youshi .eye.off { opacity:0.3; }
  .youshi .nm { font-size:11px; }
  .nt-add { padding:6px 8px; display:flex; flex-direction:column; gap:4px; }
  .nt-add button { font-size:10px; padding:3px 8px; border:1px solid #ddd; border-radius:4px; background:#fff; cursor:pointer; }
  .nt-add button:hover { background:#f0f0f0; }
</style>
