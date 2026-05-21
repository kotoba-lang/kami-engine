<script lang="ts">
  /**
   * ProjectSelect.svelte — Project dropdown with New/Refresh buttons for Genko manga editor.
   * Compact selector that fits in a sidebar header area.
   */

  interface Project {
    convoId: string;
    name: string;
  }

  interface Props {
    projects: Project[];
    activeProjectId: string;
    onselect: (convoId: string) => void;
    oncreate: () => void;
    onrefresh: () => void;
  }

  let { projects, activeProjectId, onselect, oncreate, onrefresh }: Props = $props();
</script>

<div class="project-select">
  <select
    class="select"
    value={activeProjectId}
    onchange={(e) => onselect(e.currentTarget.value)}
  >
    <option value="">Project…</option>
    <option value="none">(None)</option>
    {#each projects as proj}
      <option value={proj.convoId}>{proj.name}</option>
    {/each}
  </select>
  <button class="btn icon-btn" onclick={oncreate} title="New project" aria-label="New project">+</button>
  <button class="btn icon-btn" onclick={onrefresh} title="Refresh projects" aria-label="Refresh">↻</button>
</div>

<style>
  .project-select {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 8px;
    font-family: 'Nunito', sans-serif;
  }

  .select {
    flex: 1;
    height: 26px;
    border: 1px solid #333;
    border-radius: 4px;
    background: #252528;
    color: #e0e0e0;
    font-size: 11px;
    font-family: 'Nunito', sans-serif;
    padding: 0 4px;
    min-width: 0;
  }

  .btn {
    height: 26px;
    padding: 0 8px;
    border: 1px solid #444;
    border-radius: 4px;
    background: #f0ead6;
    color: #1a1a1f;
    font-size: 10px;
    font-weight: 700;
    font-family: 'Nunito', sans-serif;
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .icon-btn {
    width: 26px;
    padding: 0;
    font-size: 14px;
    line-height: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .btn:hover {
    background: #e6dfc8;
  }
</style>
