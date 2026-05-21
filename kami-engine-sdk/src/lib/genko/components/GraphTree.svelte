<!--
  GraphTree — Graph-native chapter/page tree fetched via edge_contains.

  Shows chapters for the active project (via associated workId) and, on expand,
  their pages. Data comes from:
    POST /xrpc/ai.gftd.mangaka.listChapters { workId }
    POST /xrpc/ai.gftd.mangaka.listPages    { chapterId }
  Both queries use server-side edge_contains traversal (not vertex scan).
-->
<script lang="ts">
  import { authHeaders } from '../stores/auth.svelte.js';
  import { setDoc, loadPage, nid, pid, type GenkoDoc, type GenkoPage } from '../stores/doc.svelte.js';

  type ChapterRow = {
    id: string;
    titleJP?: string;
    title?: string;
    chapterNumFromEdge?: string | number;
    status?: string;
    volumeId?: string;
    vertex_id?: string;
  };
  type PageRow = {
    id: string;
    title?: string;
    altText?: string;
    pageNumFromEdge?: string | number;
    compositedImageCid?: string;
    vertex_id?: string;
  };

  let { projectId, nanoid = '' }: { projectId: string; nanoid?: string } = $props();

  let chapters = $state<ChapterRow[]>([]);
  let pagesByChapter = $state<Record<string, PageRow[]>>({});
  let expandedChapters = $state<Set<string>>(new Set());
  let loadingChapters = $state(false);
  let errorMsg = $state('');

  // Map project convoId → workId. For SIP the work rkey is 'spirit-in-physics'.
  // Future: look up the actual work from a dedicated project→work edge.
  function workIdForProject(pid: string): string {
    if (pid === 'cv-1776656821112-1') return 'spirit-in-physics';
    if (pid === 'cv-1776435105422-1') return ''; // ghost-hacker has no work record yet
    return '';
  }

  async function loadChapters(workId: string) {
    if (!workId) { chapters = []; return; }
    loadingChapters = true;
    errorMsg = '';
    try {
      const r = await fetch('/xrpc/ai.gftd.mangaka.listChapters', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({ workId, limit: 50 }),
      });
      if (r.ok) {
        const data = await r.json() as { items?: ChapterRow[]; via?: string };
        const items = Array.isArray(data.items) ? data.items : [];
        // Sort by chapterNumFromEdge (string from RisingWave) numerically
        items.sort((a, b) => Number(a.chapterNumFromEdge ?? 0) - Number(b.chapterNumFromEdge ?? 0));
        chapters = items;
      } else {
        errorMsg = `listChapters ${r.status}`;
      }
    } catch (e) {
      errorMsg = `fetch failed: ${String(e).slice(0, 80)}`;
    } finally {
      loadingChapters = false;
    }
  }

  async function loadPages(chapterId: string) {
    if (pagesByChapter[chapterId]) return;
    try {
      const r = await fetch('/xrpc/ai.gftd.mangaka.listPages', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({ chapterId, limit: 50 }),
      });
      if (r.ok) {
        const data = await r.json() as { items?: PageRow[] };
        const items = Array.isArray(data.items) ? data.items : [];
        items.sort((a, b) => Number(a.pageNumFromEdge ?? 0) - Number(b.pageNumFromEdge ?? 0));
        pagesByChapter[chapterId] = items;
      }
    } catch { /* ignore */ }
  }

  function toggleChapter(ch: ChapterRow) {
    if (expandedChapters.has(ch.id)) {
      expandedChapters.delete(ch.id);
    } else {
      expandedChapters.add(ch.id);
      loadPages(ch.id);
    }
    expandedChapters = new Set(expandedChapters);
  }

  // --- Inline edit state ---
  let editingChapterId = $state<string>('');
  let editTitle = $state('');
  let editStatus = $state<'draft' | 'published' | 'archived'>('draft');
  let savingEdit = $state(false);
  let saveError = $state('');

  function startEdit(ch: ChapterRow, event: Event) {
    event.stopPropagation(); // don't toggle expand
    editingChapterId = ch.id;
    editTitle = String(ch.titleJP ?? ch.title ?? '');
    editStatus = (ch.status === 'published' || ch.status === 'archived') ? ch.status : 'draft';
    saveError = '';
  }

  function cancelEdit() {
    editingChapterId = '';
    saveError = '';
  }

  async function saveEdit() {
    if (!editingChapterId) return;
    savingEdit = true;
    saveError = '';
    try {
      const r = await fetch('/xrpc/ai.gftd.mangaka.publishChapter', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({
          chapterId: editingChapterId,
          title: editTitle,
          status: editStatus,
        }),
      });
      if (!r.ok) {
        saveError = `save failed: ${r.status}`;
        return;
      }
      const data = await r.json() as { chapterId?: string; status?: string; emitted?: number };
      // Update local state
      chapters = chapters.map((c) =>
        c.id === editingChapterId ? { ...c, titleJP: editTitle, title: editTitle, status: editStatus } : c
      );
      if (data.emitted && data.emitted > 0) console.log(`[edit] derive emitted ${data.emitted} post(s)`);
      editingChapterId = '';
    } catch (e) {
      saveError = `network: ${String(e).slice(0, 60)}`;
    } finally {
      savingEdit = false;
    }
  }

  // --- Page inline edit state ---
  let editingPageId = $state<string>('');
  let pageEditAlt = $state('');
  let pageEditPageNum = $state<number>(1);
  let pageSaving = $state(false);
  let pageSaveError = $state('');

  function startPageEdit(pg: PageRow, event: Event) {
    event.stopPropagation();
    editingPageId = pg.id;
    pageEditAlt = String(pg.altText ?? pg.title ?? '');
    pageEditPageNum = Number(pg.pageNumFromEdge ?? 1);
    pageSaveError = '';
  }
  function cancelPageEdit() { editingPageId = ''; pageSaveError = ''; }

  async function savePageEdit(chapterId: string) {
    if (!editingPageId) return;
    pageSaving = true;
    pageSaveError = '';
    try {
      const r = await fetch('/xrpc/ai.gftd.mangaka.updatePage', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({
          pageId: editingPageId,
          altText: pageEditAlt,
          pageNum: pageEditPageNum,
        }),
      });
      if (!r.ok) { pageSaveError = `save failed: ${r.status}`; return; }
      // Update local state
      pagesByChapter[chapterId] = (pagesByChapter[chapterId] ?? []).map((p) =>
        p.id === editingPageId ? { ...p, altText: pageEditAlt, pageNumFromEdge: pageEditPageNum } : p
      ).sort((a, b) => Number(a.pageNumFromEdge ?? 0) - Number(b.pageNumFromEdge ?? 0));
      editingPageId = '';
    } catch (e) {
      pageSaveError = `network: ${String(e).slice(0, 60)}`;
    } finally { pageSaving = false; }
  }

  // --- Chapter creation state ---
  let creatingChapter = $state(false);
  let newChapterTitle = $state('');
  let newChapterNum = $state<number>(1);
  let createError = $state('');

  function startCreateChapter() {
    creatingChapter = true;
    newChapterTitle = '';
    newChapterNum = (chapters.length > 0
      ? Math.max(...chapters.map((c) => Number(c.chapterNumFromEdge ?? 0))) + 1
      : 1);
    createError = '';
  }
  function cancelCreateChapter() { creatingChapter = false; createError = ''; }

  async function submitCreateChapter() {
    const workId = workIdForProject(projectId);
    if (!workId) { createError = 'no workId for project'; return; }
    if (!newChapterTitle) { createError = 'title required'; return; }
    try {
      const workAtUri = `at://mng4k4x1.gftd.ai/ai.gftd.mangaka.work/${workId}`;
      const r = await fetch('/xrpc/ai.gftd.mangaka.addChapter', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({
          workId: workAtUri,
          chapterNum: newChapterNum,
          titleJP: newChapterTitle,
          status: 'draft',
        }),
      });
      if (!r.ok) { createError = `addChapter ${r.status}`; return; }
      creatingChapter = false;
      // Re-load chapters to pick up the new one
      await loadChapters(workId);
    } catch (e) {
      createError = `network: ${String(e).slice(0, 60)}`;
    }
  }

  // --- Open chapter in canvas ---
  // Builds a GenkoDoc from chapter metadata + its edge_contains pages, each page
  // rendered as a single ai-image node when compositedImageCid is present.
  async function openChapter(ch: ChapterRow, event: Event) {
    event.stopPropagation();
    // Ensure pages are loaded
    await loadPages(ch.id);
    const pages = pagesByChapter[ch.id] ?? [];
    const genkoPages: GenkoPage[] = pages.length > 0
      ? pages.map((pg, i) => {
          const nodes: GenkoDoc['pages'][number]['nodes'] = [];
          const cid = pg.compositedImageCid;
          if (cid) {
            nodes.push({
              id: nid(),
              type: 'ai-image',
              visible: true,
              data: {
                type: 'ai-image',
                _nid: nid(),
                _visible: true,
                src: `https://atproto.gftd.ai/xrpc/com.atproto.sync.getBlob?did=did:web:mng4k4x1.gftd.ai&cid=${cid}`,
                x: 0, y: 0, w: 1080, h: 1920,
                altText: pg.altText ?? '',
              },
            });
          }
          return {
            id: pid(),
            name: `p.${pg.pageNumFromEdge ?? (i + 1)} — ${pg.altText ?? pg.id}`.slice(0, 64),
            youshi: { id: nid(), type: 'b4manga', visible: true },
            nodes,
          };
        })
      : [{
          id: pid(),
          name: `${ch.titleJP ?? ch.title ?? ch.id}`,
          youshi: { id: nid(), type: 'b4manga', visible: true },
          nodes: [],
        }];
    const doc: GenkoDoc = {
      name: `ch.${ch.chapterNumFromEdge ?? '?'} — ${ch.titleJP ?? ch.title ?? ch.id}`,
      docId: `chapter-${ch.id}`,
      convoId: projectId,
      pages: genkoPages,
      activePageIdx: 0,
    };
    setDoc(doc);
    loadPage(0);
  }

  $effect(() => {
    const workId = workIdForProject(projectId);
    loadChapters(workId);
  });
</script>

<div class="graph-tree">
  <div class="gt-hdr">
    <span class="gt-title">Chapters</span>
    {#if loadingChapters}<span class="gt-loading">…</span>{/if}
    {#if workIdForProject(projectId) && !creatingChapter}
      <button class="gt-add-btn" title="add chapter" onclick={startCreateChapter}>+</button>
    {/if}
  </div>
  {#if creatingChapter}
    <div class="gt-create-form">
      <input type="number" class="gt-create-num" bind:value={newChapterNum} min="1" placeholder="#" />
      <input type="text" class="gt-create-title" bind:value={newChapterTitle} placeholder="Chapter title" />
      <button class="gt-btn-save" onclick={submitCreateChapter}>✓</button>
      <button class="gt-btn-cancel" onclick={cancelCreateChapter}>×</button>
      {#if createError}<span class="gt-edit-err">{createError}</span>{/if}
    </div>
  {/if}
  {#if errorMsg}
    <div class="gt-err">{errorMsg}</div>
  {:else if chapters.length === 0 && projectId}
    <div class="gt-empty">no chapters</div>
  {:else}
    <ul class="gt-list">
      {#each chapters as ch (ch.id)}
        <li class="gt-chapter">
          {#if editingChapterId === ch.id}
            <div class="gt-ch-edit">
              <span class="gt-ch-num">ch.{ch.chapterNumFromEdge ?? '?'}</span>
              <input class="gt-edit-input" type="text" bind:value={editTitle} placeholder="Chapter title" />
              <select class="gt-edit-status" bind:value={editStatus}>
                <option value="draft">draft</option>
                <option value="published">published</option>
                <option value="archived">archived</option>
              </select>
              <button class="gt-btn-save" onclick={saveEdit} disabled={savingEdit}>{savingEdit ? '…' : '✓'}</button>
              <button class="gt-btn-cancel" onclick={cancelEdit}>×</button>
              {#if saveError}<span class="gt-edit-err">{saveError}</span>{/if}
            </div>
          {:else}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="gt-ch-row">
              <span class="gt-ch-arrow" onclick={() => toggleChapter(ch)}>{expandedChapters.has(ch.id) ? '▾' : '▸'}</span>
              <span class="gt-ch-num">ch.{ch.chapterNumFromEdge ?? '?'}</span>
              <span class="gt-ch-title" title="Open in canvas" onclick={(e) => openChapter(ch, e)}>{ch.titleJP ?? ch.title ?? ch.id}</span>
              {#if ch.status === 'published'}<span class="gt-ch-status">✓</span>{/if}
              <button class="gt-ch-open-btn" title="open in canvas" onclick={(e) => openChapter(ch, e)}>↗</button>
              <button class="gt-ch-edit-btn" title="edit" onclick={(e) => startEdit(ch, e)}>✎</button>
            </div>
          {/if}
          {#if expandedChapters.has(ch.id)}
            <ul class="gt-pages">
              {#each pagesByChapter[ch.id] ?? [] as pg (pg.id)}
                <li class="gt-page">
                  {#if editingPageId === pg.id}
                    <div class="gt-pg-edit">
                      <input type="number" class="gt-pg-edit-num" bind:value={pageEditPageNum} min="1" />
                      <input type="text" class="gt-pg-edit-alt" bind:value={pageEditAlt} placeholder="alt text" />
                      <button class="gt-btn-save" onclick={() => savePageEdit(ch.id)} disabled={pageSaving}>{pageSaving ? '…' : '✓'}</button>
                      <button class="gt-btn-cancel" onclick={cancelPageEdit}>×</button>
                      {#if pageSaveError}<span class="gt-edit-err">{pageSaveError}</span>{/if}
                    </div>
                  {:else}
                    <span class="gt-pg-num">p.{pg.pageNumFromEdge ?? '?'}</span>
                    <span class="gt-pg-alt">{pg.altText ?? pg.title ?? pg.id}</span>
                    {#if pg.compositedImageCid}<span class="gt-pg-img">🖼</span>{/if}
                    <button class="gt-pg-edit-btn" title="edit" onclick={(e) => startPageEdit(pg, e)}>✎</button>
                  {/if}
                </li>
              {/each}
              {#if !pagesByChapter[ch.id]}
                <li class="gt-page-loading">…</li>
              {/if}
            </ul>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .graph-tree {
    border-top: 1px solid #e0d9c8;
    padding: 8px 4px;
    font-size: 12px;
    max-height: 45vh;
    overflow-y: auto;
  }
  .gt-hdr {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0 6px 6px;
    font-weight: 600;
    color: #555;
  }
  .gt-loading { color: #888; font-size: 10px; }
  .gt-err { color: #c33; padding: 4px 6px; }
  .gt-empty { color: #999; padding: 4px 6px; font-style: italic; }
  .gt-list, .gt-pages { list-style: none; margin: 0; padding: 0; }
  .gt-chapter { margin-bottom: 2px; }
  .gt-ch-row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 6px;
    cursor: pointer;
    border-radius: 4px;
  }
  .gt-ch-row:hover { background: #f5efd8; }
  .gt-ch-arrow { width: 10px; color: #888; font-size: 10px; }
  .gt-ch-num { font-weight: 600; color: #4a6c8c; min-width: 36px; }
  .gt-ch-title { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .gt-ch-status { color: #2a7; font-size: 10px; }
  .gt-pages { padding-left: 20px; border-left: 1px dashed #ccc; margin-left: 8px; }
  .gt-page {
    display: flex;
    gap: 4px;
    padding: 2px 6px;
    color: #666;
  }
  .gt-pg-num { font-weight: 500; color: #8a7b5c; min-width: 28px; }
  .gt-pg-alt { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .gt-pg-img { font-size: 10px; }
  .gt-page-loading { color: #999; padding: 2px 6px; }

  /* inline edit */
  .gt-ch-edit-btn {
    background: transparent; border: none; cursor: pointer;
    color: #888; font-size: 11px; padding: 0 4px; opacity: 0.5;
    transition: opacity 120ms;
  }
  .gt-ch-row:hover .gt-ch-edit-btn,
  .gt-ch-row:hover .gt-ch-open-btn { opacity: 1; }
  .gt-ch-edit-btn:hover,
  .gt-ch-open-btn:hover { color: #4a6c8c; }
  .gt-ch-open-btn {
    background: transparent; border: none; cursor: pointer;
    color: #888; font-size: 11px; padding: 0 4px; opacity: 0.5;
    transition: opacity 120ms;
  }
  .gt-ch-title { cursor: pointer; }
  .gt-ch-title:hover { color: #4a6c8c; text-decoration: underline; }
  .gt-ch-edit {
    display: flex; align-items: center; gap: 4px;
    padding: 4px 6px; background: #fdfaea; border-radius: 4px;
  }
  .gt-edit-input {
    flex: 1; min-width: 60px;
    border: 1px solid #cfc7ad; border-radius: 3px;
    padding: 2px 4px; font-size: 12px; font-family: inherit;
  }
  .gt-edit-status {
    border: 1px solid #cfc7ad; border-radius: 3px;
    padding: 1px 2px; font-size: 10px; background: white;
  }
  .gt-btn-save, .gt-btn-cancel {
    background: transparent; border: 1px solid #cfc7ad; border-radius: 3px;
    padding: 1px 6px; cursor: pointer; font-size: 10px;
  }
  .gt-btn-save { color: #2a7; font-weight: 700; }
  .gt-btn-save:hover { background: #e8f5e0; }
  .gt-btn-cancel { color: #888; }
  .gt-btn-cancel:hover { background: #f0e4e4; color: #c33; }
  .gt-edit-err {
    color: #c33; font-size: 10px; margin-left: 4px;
  }
  /* Chapter create form */
  .gt-add-btn {
    background: transparent; border: 1px solid #cfc7ad; border-radius: 3px;
    padding: 0 6px; cursor: pointer; color: #4a6c8c; font-weight: 700;
  }
  .gt-add-btn:hover { background: #f5efd8; }
  .gt-create-form {
    display: flex; align-items: center; gap: 4px;
    padding: 4px 6px; margin: 2px 0 6px; background: #fdfaea; border-radius: 4px;
  }
  .gt-create-num {
    width: 40px; border: 1px solid #cfc7ad; border-radius: 3px;
    padding: 2px 4px; font-size: 11px;
  }
  .gt-create-title {
    flex: 1; border: 1px solid #cfc7ad; border-radius: 3px;
    padding: 2px 4px; font-size: 12px; font-family: inherit;
  }
  /* Page edit */
  .gt-pg-edit-btn {
    background: transparent; border: none; cursor: pointer;
    color: #aaa; font-size: 10px; padding: 0 3px; opacity: 0;
    transition: opacity 120ms;
  }
  .gt-page:hover .gt-pg-edit-btn { opacity: 1; }
  .gt-pg-edit-btn:hover { color: #4a6c8c; }
  .gt-pg-edit {
    display: flex; align-items: center; gap: 3px; flex: 1;
    padding: 2px 4px; background: #fdfaea; border-radius: 3px;
  }
  .gt-pg-edit-num {
    width: 36px; border: 1px solid #cfc7ad; border-radius: 3px;
    padding: 1px 3px; font-size: 11px;
  }
  .gt-pg-edit-alt {
    flex: 1; border: 1px solid #cfc7ad; border-radius: 3px;
    padding: 1px 3px; font-size: 11px; font-family: inherit;
  }
</style>
