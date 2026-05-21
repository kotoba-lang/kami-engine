<script lang="ts">
  /**
   * Toolbar.svelte — Top toolbar for Genko manga editor.
   * 36px horizontal bar with paper texture selector, youshi type selector, and file operations.
   */

  interface Props {
    activeYoushi: string;
    onyoushichange: (type: string) => void;
    onsavepng: () => void;
    onsavesvg: () => void;
    onsavedoc: () => void;
    onloaddoc: () => void;
    onexportoplog: () => void;
    onimportoplog: () => void;
  }

  let {
    activeYoushi,
    onyoushichange,
    onsavepng,
    onsavesvg,
    onsavedoc,
    onloaddoc,
    onexportoplog,
    onimportoplog,
  }: Props = $props();

  let activePaper = $state('Plain');

  const paperOptions = ['IC', 'Art Color', 'Maxon', 'Deleter', 'Plain'];
  const youshiOptions = ['b4manga', 'b4koma', 'none'];
  const paperSelectId = 'genko-toolbar-paper';
  const youshiSelectId = 'genko-toolbar-youshi';
</script>

<div class="toolbar">
  <div class="group">
    <label class="label" for={paperSelectId}>Paper</label>
    <select
      id={paperSelectId}
      class="select"
      value={activePaper}
      onchange={(e) => { activePaper = e.currentTarget.value; }}
    >
      {#each paperOptions as opt}
        <option value={opt}>{opt}</option>
      {/each}
    </select>
  </div>

  <div class="divider"></div>

  <div class="group">
    <label class="label" for={youshiSelectId}>Youshi</label>
    <select
      id={youshiSelectId}
      class="select"
      value={activeYoushi}
      onchange={(e) => onyoushichange(e.currentTarget.value)}
    >
      {#each youshiOptions as opt}
        <option value={opt}>{opt}</option>
      {/each}
    </select>
  </div>

  <div class="divider"></div>

  <div class="group file-ops">
    <button class="btn" onclick={onsavedoc} title="Save Document">Save</button>
    <button class="btn" onclick={onloaddoc} title="Load Document">Load</button>
    <div class="divider"></div>
    <button class="btn" onclick={onsavepng} title="Export PNG">PNG</button>
    <button class="btn" onclick={onsavesvg} title="Export SVG">SVG</button>
  </div>
</div>

<style>
  .toolbar {
    height: 36px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    background: #f8f8f8;
    border-bottom: 1px solid #ddd;
    font-family: 'Nunito', sans-serif;
    font-size: 12px;
    flex-shrink: 0;
  }

  .group {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .label {
    color: #666;
    font-size: 11px;
    font-weight: 600;
  }

  .select {
    height: 24px;
    border: 1px solid #ccc;
    border-radius: 4px;
    background: #fff;
    font-size: 11px;
    font-family: 'Nunito', sans-serif;
    padding: 0 4px;
    cursor: pointer;
  }

  .divider {
    width: 1px;
    height: 20px;
    background: #ddd;
  }

  .file-ops {
    gap: 4px;
    margin-left: auto;
  }

  .btn {
    height: 24px;
    padding: 0 8px;
    border: 1px solid #ccc;
    border-radius: 4px;
    background: #f0ead6;
    font-size: 11px;
    font-family: 'Nunito', sans-serif;
    cursor: pointer;
    white-space: nowrap;
  }

  .btn:hover {
    background: #e6dfc8;
  }

  .btn:active {
    background: #dbd4be;
  }
</style>
