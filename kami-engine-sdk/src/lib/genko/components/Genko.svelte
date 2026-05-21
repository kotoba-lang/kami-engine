<script lang="ts">
  /**
   * Genko — Root manga editor component.
   * Composes: ProjectSelect + NodeTree (left) + Toolbar + Canvas (center) +
   *           ChatPanel (right) + FloatingTools (bottom) + AuthStatus (top-right).
   */
  import { onMount } from 'svelte';
  import NodeTree from './NodeTree.svelte';
  import Canvas from './Canvas.svelte';
  import Toolbar from './Toolbar.svelte';
  import FloatingTools from './FloatingTools.svelte';
  import ChatPanel from './ChatPanel.svelte';
  import AuthStatus from './AuthStatus.svelte';
  import ProjectSelect from './ProjectSelect.svelte';
  import {
    getDoc, setDoc, loadPage, saveCurrentPage, requestRedraw,
    setInitDone, nid, pid, addOverlay, getOverlays, getStrokes,
    getSelectedIdx, setSelectedIdx, deleteNode, findByNid, type GenkoDoc,
  } from '../stores/doc.svelte';
  import { runCanvasOp, type CanvasCtx, type CanvasOp } from '../canvas-pregel';
  import {
    getSession, initAuth, redirectToAuth, authHeaders, type AuthSession,
  } from '../stores/auth.svelte';
  import {
    getProjects, getActiveProjectId, loadProjects, switchProject, createProject,
    type GenkoProject,
  } from '../stores/project.svelte';

  let { nanoid = '', name = 'Mangaka' }: { nanoid?: string; name?: string } = $props();

  const doc = $derived(getDoc());

  // Canvas state
  let zoom = $state(1);
  let panX = $state(0);
  let panY = $state(0);
  let canvasRef: { setZoom: (z: number) => void; fitToView: () => void } | undefined = $state();
  let faceAddMode = $state(false);

  function onFaceAdd(imageNid: string, cx: number, cy: number) {
    void runCanvasOp({ kind: 'face_add', imageNid, cx, cy }, buildCanvasCtx());
    selectedTick++;
  }

  // Canvas ops dispatched through the Pregel pipeline use this ctx. Side-effects
  // (addOverlay / deleteNode / requestRedraw / recordOp / scheduleAutoSave) are
  // bound to the store functions so super-steps stay pure & testable.
  function buildCanvasCtx(): CanvasCtx {
    // paperSc = 2.4 * dpr (matches Canvas.svelte paperFrame()). 0 when paper
    // not drawn (i.e. activeYoushi === 'none').
    const dpr = (typeof devicePixelRatio !== 'undefined' ? devicePixelRatio : 1);
    const paperSc = activeYoushi !== 'none' ? 2.4 * dpr : 0;
    return {
      doc: getDoc(),
      overlays: getOverlays() as any[],
      strokes: getStrokes() as any[],
      selectedIdx: getSelectedIdx(),
      dpr,
      paperSc,
      addOverlay,
      deleteNode,
      setSelectedIdx,
      findByNid: (nid: string) => findByNid(nid) as any,
      requestRedraw,
      scheduleAutoSave,
      recordOp: (op, target, before, after) => void recordOp(op, target, before, after),
      pushUndo,
      nid,
    };
  }

  async function runFaceDetect() {
    const o = selectedOverlay as any;
    if (!o || o.type !== 'ai-image' || !o._genImageUrl) return;
    const d = getDoc();
    try {
      const r = await xrpc('ai.gftd.mangaka.detectFaces', {
        docId: d.docId, imageNid: o._nid,
      });
      if (Array.isArray((r as any)?.faces)) {
        updateSelected({ _faces: (r as any).faces });
      }
    } catch (e) {
      console.warn('detectFaces:', e);
      window.alert('自動顔検出は未デプロイ。手動で「+ 顔追加」してください。');
    }
  }

  // Score Hume image-head emotion for the selected node.
  //   ai-image → single mode (writes `_emotion` directly).
  //   panel    → batch mode (server walks every child ai-image, rolls a
  //              max-saliency aggregate up to this panel's `_emotion`).
  // Backed by the `ai.gftd.mangaka.scoreEmotion` XRPC on the
  // lg-mangaka pod (graph: lg_mangaka.graphs.score_emotion).
  async function runEmotionScore() {
    const o = selectedOverlay as any;
    if (!o) return;
    const d = getDoc();
    try {
      if (o.type === 'ai-image' && o._genImageUrl) {
        const r = await xrpc('ai.gftd.mangaka.scoreEmotion', {
          docId: d.docId, imageNid: o._nid,
        });
        const emo = (r as any)?.emotion;
        if (emo) updateSelected({ _emotion: emo });
      } else if (o.type === 'panel') {
        // Batch mode — server re-scores every ai-image in the doc and
        // recomputes panel aggregates. We only need this panel's record.
        const r = await xrpc('ai.gftd.mangaka.scoreEmotion', {
          docId: d.docId,
        });
        const panelMap = (r as any)?.panelEmotion;
        // panelEmotion is the JSON-stringified map per the lexicon output
        // (server side stringifies to keep lexicon types str-only).
        const parsed = typeof panelMap === 'string' ? JSON.parse(panelMap) : panelMap;
        const myEmotion = parsed?.[o._nid];
        if (myEmotion) updateSelected({ _emotion: myEmotion });
      } else {
        return;
      }
    } catch (e) {
      console.warn('scoreEmotion:', e);
      window.alert('感情採点は未デプロイ。pod 側で lg-mangaka が起動済みか確認してください。');
    }
  }
  let activeYoushi = $state('b4manga');
  let activeBrush = $state('fine');
  let activeMode = $state('select');
  let brushColor = $state<[number, number, number, number]>([0.2, 0.2, 0.2, 1]);
  let brushSize = $state(2);

  // Auth + project state
  let session = $state<AuthSession | null>(null);
  let projects = $state<GenkoProject[]>([]);
  let activeProjectId = $state('');

  // Sidebar collapse state (persisted in localStorage)
  const LS_LEFT_COLLAPSED = 'genko.leftCollapsed';
  const LS_RIGHT_COLLAPSED = 'genko.rightCollapsed';
  function readLs(k: string): boolean {
    try { return typeof localStorage !== 'undefined' && localStorage.getItem(k) === '1'; } catch { return false; }
  }
  function writeLs(k: string, v: boolean) {
    try { if (typeof localStorage !== 'undefined') localStorage.setItem(k, v ? '1' : '0'); } catch { /* ignore quota / disabled */ }
  }
  function readLsStr(k: string, dflt: string): string {
    try { return typeof localStorage !== 'undefined' ? (localStorage.getItem(k) ?? dflt) : dflt; } catch { return dflt; }
  }
  function readLsNum(k: string, dflt: number): number {
    try { const v = typeof localStorage !== 'undefined' ? localStorage.getItem(k) : null; const n = v != null ? parseFloat(v) : NaN; return Number.isFinite(n) ? n : dflt; } catch { return dflt; }
  }
  function writeLsStr(k: string, v: string) {
    try { if (typeof localStorage !== 'undefined') localStorage.setItem(k, v); } catch { /* ignore */ }
  }
  let leftCollapsed = $state(readLs(LS_LEFT_COLLAPSED));
  let rightCollapsed = $state(readLs(LS_RIGHT_COLLAPSED));
  function toggleLeft() { leftCollapsed = !leftCollapsed; writeLs(LS_LEFT_COLLAPSED, leftCollapsed); }
  function toggleRight() { rightCollapsed = !rightCollapsed; writeLs(LS_RIGHT_COLLAPSED, rightCollapsed); }

  // Tool prefs (persisted)
  let fukidashiShape = $state(readLsStr('genko.fukidashiShape', 'normal'));
  let textFont = $state(readLsStr('genko.textFont', 'gothic'));
  let textSize = $state(readLsNum('genko.textSize', 5));
  let textColor = $state(readLsStr('genko.textColor', '#000000'));
  let textStyle = $state(readLsStr('genko.textStyle', 'normal'));

  function onFukidashiShape(s: string) { fukidashiShape = s; writeLsStr('genko.fukidashiShape', s); }
  function onTextFont(f: string) { textFont = f; writeLsStr('genko.textFont', f); }
  function onTextSize(n: number) { textSize = n; writeLsStr('genko.textSize', String(n)); }
  function onTextColor(c: string) { textColor = c; writeLsStr('genko.textColor', c); }
  function onTextStyle(s: string) { textStyle = s; writeLsStr('genko.textStyle', s); }

  // === Panel preset: insert N panels in mm coords inside the inner safe frame ===
  // B4 inner safe frame: 53.5–203.5mm × 72–292mm = 150×220mm
  const INNER_L = 53.5, INNER_T = 72, INNER_R = 203.5, INNER_B = 292;
  const INNER_W = INNER_R - INNER_L; // 150
  const INNER_H = INNER_B - INNER_T; // 220
  type Rect = { x1: number; y1: number; x2: number; y2: number };
  function presetLayout(pid_: string): Rect[] {
    const g = 2; // mm gap
    if (pid_ === '1') return [{ x1: INNER_L, y1: INNER_T, x2: INNER_R, y2: INNER_B }];
    if (pid_ === '2h') {
      const h = (INNER_H - g) / 2;
      return [
        { x1: INNER_L, y1: INNER_T, x2: INNER_R, y2: INNER_T + h },
        { x1: INNER_L, y1: INNER_T + h + g, x2: INNER_R, y2: INNER_B },
      ];
    }
    if (pid_ === '3h') {
      const h = (INNER_H - 2 * g) / 3;
      return [0, 1, 2].map((i) => ({
        x1: INNER_L, y1: INNER_T + i * (h + g), x2: INNER_R, y2: INNER_T + i * (h + g) + h,
      }));
    }
    if (pid_ === '4h') {
      const h = (INNER_H - 3 * g) / 4;
      return [0, 1, 2, 3].map((i) => ({
        x1: INNER_L, y1: INNER_T + i * (h + g), x2: INNER_R, y2: INNER_T + i * (h + g) + h,
      }));
    }
    if (pid_ === '2x2') {
      const w = (INNER_W - g) / 2, h = (INNER_H - g) / 2;
      return [
        { x1: INNER_L, y1: INNER_T, x2: INNER_L + w, y2: INNER_T + h },
        { x1: INNER_L + w + g, y1: INNER_T, x2: INNER_R, y2: INNER_T + h },
        { x1: INNER_L, y1: INNER_T + h + g, x2: INNER_L + w, y2: INNER_B },
        { x1: INNER_L + w + g, y1: INNER_T + h + g, x2: INNER_R, y2: INNER_B },
      ];
    }
    if (pid_ === 'lshape') {
      const h = (INNER_H - g) / 2, w = (INNER_W - g) / 2;
      return [
        { x1: INNER_L, y1: INNER_T, x2: INNER_R, y2: INNER_T + h },
        { x1: INNER_L, y1: INNER_T + h + g, x2: INNER_L + w, y2: INNER_B },
        { x1: INNER_L + w + g, y1: INNER_T + h + g, x2: INNER_R, y2: INNER_B },
      ];
    }
    if (pid_ === 'action') {
      const h = (INNER_H - 2 * g) / 3, w = (INNER_W - g) / 2;
      return [
        { x1: INNER_L, y1: INNER_T, x2: INNER_L + w, y2: INNER_T + h },
        { x1: INNER_L + w + g, y1: INNER_T, x2: INNER_R, y2: INNER_T + h * 2 + g },
        { x1: INNER_L, y1: INNER_T + h + g, x2: INNER_L + w, y2: INNER_T + h * 2 + g },
        { x1: INNER_L, y1: INNER_T + (h + g) * 2, x2: INNER_R, y2: INNER_B },
      ];
    }
    return [];
  }
  function onPanelPreset(pid_: string) {
    const rects = presetLayout(pid_);
    const nodes = rects.map((r, i) => ({
      type: 'panel', _nid: nid(), _visible: true, _unit: 'mm',
      panelName: String(i + 1),
      x1: r.x1, y1: r.y1, x2: r.x2, y2: r.y2,
    }));
    void runCanvasOp({ kind: 'add_nodes', nodes }, buildCanvasCtx());
  }

  function onAddFukidashi() {
    const t = typeof window !== 'undefined' ? window.prompt('吹き出しのテキストを入力 (空欄でも可):', '') : '';
    const cx = (INNER_L + INNER_R) / 2, cy = (INNER_T + INNER_B) / 2;
    const w = 40, h = 20;
    void runCanvasOp({ kind: 'add_node', node: {
      type: 'fukidashi', _nid: nid(), _visible: true, _unit: 'mm',
      shape: fukidashiShape,
      x1: cx - w / 2, y1: cy - h / 2, x2: cx + w / 2, y2: cy + h / 2,
      _tailX: cx, _tailY: cy + h / 2 + 8,
      text: t ?? '',
    } }, buildCanvasCtx());
  }

  function onAddText() {
    const t = typeof window !== 'undefined' ? window.prompt('テキストを入力:', 'テキスト') : 'テキスト';
    if (t == null) return;
    void runCanvasOp({ kind: 'add_node', node: {
      type: 'text', _nid: nid(), _visible: true, _unit: 'mm',
      x: INNER_L + 5, y: INNER_T + 5,
      text: t, fontSize: textSize, fontFamily: textFont, fontStyle: textStyle, color: textColor,
    } }, buildCanvasCtx());
  }
  function onAddSfx() {
    const t = typeof window !== 'undefined' ? window.prompt('SFX (効果音) のテキストを入力:', 'ドン!') : 'ドン!';
    if (t == null) return;
    void runCanvasOp({ kind: 'add_node', node: {
      type: 'text', _nid: nid(), _visible: true, _unit: 'mm',
      x: (INNER_L + INNER_R) / 2 - 30, y: (INNER_T + INNER_B) / 2,
      text: t, fontSize: 18, fontFamily: 'sfx', fontStyle: 'bold', color: '#000000',
      isSfx: true,
    } }, buildCanvasCtx());
  }

  // === Undo / Redo stacks (Ctrl/Cmd+Z, Ctrl/Cmd+Y) ===
  type UndoEntry =
    | { op: 'move'|'edit'; nid: string; before: Record<string, unknown>; after: Record<string, unknown> }
    | { op: 'delete'; node: any }
    | { op: 'add'; nids: string[] };
  let undoStack: UndoEntry[] = [];
  let redoStack: UndoEntry[] = [];
  const UNDO_LIMIT = 100;

  function pushUndo(entry: UndoEntry) {
    undoStack.push(entry);
    if (undoStack.length > UNDO_LIMIT) undoStack.shift();
    redoStack = []; // any new action clears redo
  }
  function findOverlayByNid(targetNid: string): any | null {
    const overlays = getOverlays() as any[];
    return overlays.find((o) => o._nid === targetNid) || null;
  }
  function applyReverse(e: UndoEntry) {
    if (e.op === 'move' || e.op === 'edit') {
      const n = findOverlayByNid(e.nid);
      if (n) for (const k of Object.keys(e.before)) (n as Record<string, unknown>)[k] = e.before[k];
    } else if (e.op === 'delete') {
      addOverlay(e.node);
    } else if (e.op === 'add') {
      for (const n of e.nids) deleteNode(n);
    }
  }
  function applyForward(e: UndoEntry) {
    if (e.op === 'move' || e.op === 'edit') {
      const n = findOverlayByNid(e.nid);
      if (n) for (const k of Object.keys(e.after)) (n as Record<string, unknown>)[k] = e.after[k];
    } else if (e.op === 'delete') {
      const nidD = e.node._nid;
      if (nidD) deleteNode(nidD);
    } else if (e.op === 'add') {
      // For 'add' we can't easily replay; best-effort no-op (redo of an add is rare).
    }
  }
  function undo() {
    const e = undoStack.pop();
    if (!e) return;
    applyReverse(e);
    redoStack.push(e);
    refreshSelected();
    void saveImmediately();
  }
  function redo() {
    const e = redoStack.pop();
    if (!e) return;
    applyForward(e);
    undoStack.push(e);
    refreshSelected();
    void saveImmediately();
  }

  // === Op log (change history) ===
  // Every meaningful edit emits a kind=opLog row into vertex_mangaka via
  // ai.gftd.mangaka.recordOpLog. Fire-and-forget; failures don't block UX.
  async function recordOp(op: string, target: { nid: string; type: string }, before: any, after: any) {
    try {
      const d = getDoc();
      await xrpc('ai.gftd.mangaka.recordOpLog', {
        docId: d.docId, op, nid: target.nid, nodeType: target.type,
        before: JSON.stringify(before || {}),
        after: JSON.stringify(after || {}),
      });
    } catch (e) { /* silent — log entries are best-effort */ }
  }

  // Immediate save (skip 5s debounce) — used after drag.
  async function saveImmediately() {
    const d = getDoc();
    saveCurrentPage();
    try {
      await xrpc('ai.gftd.mangaka.saveDocument', { docId: d.docId, name: d.name, document: JSON.stringify(d), convoId: d.convoId || '' });
    } catch (e) { console.warn('saveImmediately:', e); }
  }

  async function onDragEnd(info: { nid: string; type: string; before: any; after: any }) {
    selectedTick++;
    // Skip noop drags
    const moved = JSON.stringify(info.before) !== JSON.stringify(info.after);
    if (moved && info.nid) {
      pushUndo({ op: 'move', nid: info.nid, before: info.before, after: info.after });
      await recordOp('move', { nid: info.nid, type: info.type }, info.before, info.after);
    }
    // Persist immediately so reload preserves the new position.
    await saveImmediately();
  }

  // === Selection-aware edit panel ===
  // Derived: current selected overlay object (or null).
  // selectedIdx in the store = strokes.length + overlayIdx.
  let selectedTick = $state(0); // bump to re-read selected overlay (forced ticker)
  function onSelectNode(_idx: number) { selectedTick++; activeMode = 'select'; }
  const selectedOverlay = $derived.by(() => {
    selectedTick; // dep
    const idx = getSelectedIdx();
    const strokes = getStrokes();
    const overlays = getOverlays();
    if (idx < strokes.length) return null;
    return overlays[idx - strokes.length] || null;
  });
  function refreshSelected() { selectedTick++; requestRedraw(); }
  function updateSelected(patch: Record<string, unknown>) {
    const o = selectedOverlay;
    if (!o) return;
    const nidU = (o as any)._nid as string;
    if (!nidU) return;
    void runCanvasOp({ kind: 'update_props', nid: nidU, patch }, buildCanvasCtx());
    refreshSelected();
  }
  function deleteSelected() {
    const o = selectedOverlay;
    if (!o) return;
    const nidD = (o as any)._nid as string;
    if (!nidD) return;
    void runCanvasOp({ kind: 'delete', nid: nidD }, buildCanvasCtx());
    setSelectedIdx(-1);
    refreshSelected();
    void saveImmediately();
  }
  function onWindowKeyDown(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement | null)?.tagName || '';
    const inEditor = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';
    const mod = e.metaKey || e.ctrlKey;
    if (mod && (e.key === 'z' || e.key === 'Z')) {
      if (e.shiftKey) { e.preventDefault(); redo(); return; }
      e.preventDefault(); undo(); return;
    }
    if (mod && (e.key === 'y' || e.key === 'Y')) {
      e.preventDefault(); redo(); return;
    }
    if (inEditor) return;
    if (e.key === 'Delete' || e.key === 'Backspace') {
      if (selectedOverlay) { e.preventDefault(); deleteSelected(); }
    } else if (e.key === 'Escape') {
      setSelectedIdx(-1); refreshSelected();
    }
  }

  // Chat state — default mangaka actor roster (storyboard / lineart / toner / letterer)
  const defaultMembers = [
    { displayName: 'Storyboard', style: 'storyboard', role: 'ネーム (LLM)' },
    { displayName: 'Lineart', style: 'lineart', role: 'ペン入れ (KAMI canvas)' },
    { displayName: 'Toner', style: 'toner', role: 'トーン (image gen)' },
    { displayName: 'Letterer', style: 'letterer', role: '写植 (OCR + layout)' },
  ];
  let members = $state(defaultMembers);
  let messages = $state<{ sender: string; text: string; isUser: boolean }[]>([]);

  // Color conversion (Canvas uses RGBA array, HTML color input uses #rrggbb).
  function rgbaToHex(c: number[]): string {
    const r = Math.round(((c[0] ?? 0)) * 255);
    const g = Math.round(((c[1] ?? 0)) * 255);
    const b = Math.round(((c[2] ?? 0)) * 255);
    return '#' + [r, g, b].map(v => Math.max(0, Math.min(255, v)).toString(16).padStart(2, '0')).join('');
  }
  function hexToRgba(hex: string): [number, number, number, number] {
    const m = /^#([0-9a-f]{6})$/i.exec(hex);
    if (!m) return [0, 0, 0, 1];
    const n = parseInt(m[1], 16);
    return [((n >> 16) & 0xff) / 255, ((n >> 8) & 0xff) / 255, (n & 0xff) / 255, 1];
  }
  const brushColorHex = $derived(rgbaToHex(brushColor));

  // XRPC helper
  const XRPC_BASE = typeof location !== 'undefined' ? location.origin + '/xrpc/' : '/xrpc/';
  async function xrpc(method: string, body: Record<string, unknown>) {
    const resp = await fetch(XRPC_BASE + method, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
      body: JSON.stringify(body),
    });
    if (resp.status === 401) throw new Error('auth required');
    return resp.json();
  }

  // AT URI deep-link
  function parseAtUri(): { authority: string; collection: string; rkey: string } | null {
    if (typeof location === 'undefined') return null;
    const m = location.pathname.match(/^\/at\/([^/]+)\/([^/]+)\/(.+)$/);
    if (!m) return null;
    return { authority: m[1], collection: m[2], rkey: decodeURIComponent(m[3]) };
  }

  function safeDeserialize(json: string): boolean {
    try {
      const d: GenkoDoc = JSON.parse(json);
      if (d?.pages?.length) {
        setInitDone(false);
        setDoc(d);
        loadPage(d.activePageIdx || 0);
        setInitDone(true);
        requestRedraw();
        return true;
      }
    } catch (e) { console.warn('deserialize failed:', e); }
    return false;
  }

  /** Build project TOC document from project data. */
  function buildProjectToc(project: Record<string, unknown>): GenkoDoc {
    const docs = (project.documents || []) as Array<Record<string, unknown>>;
    const appHost = nanoid + '.gftd.ai';
    const nodes: Array<Record<string, unknown>> = [];
    const titleNid = nid();
    nodes.push({ id: titleNid, type: 'text', visible: true, data: { type: 'text', _nid: titleNid, _visible: true, text: project.name || 'Project', x: 300, y: 200, fontSize: 52, color: '#222', font: 'sans' } });

    const arcMap = new Map<string, Array<Record<string, unknown>>>();
    for (const d of docs) {
      const arc = (d.arc as string) || 'Other';
      if (!arcMap.has(arc)) arcMap.set(arc, []);
      arcMap.get(arc)!.push(d);
    }

    for (const [arc, epDocs] of arcMap) {
      const groupNid = nid();
      nodes.push({ id: groupNid, type: 'group', visible: true, data: { type: 'group', _nid: groupNid, _visible: true, groupName: arc } });
      for (const d of epDocs) {
        const linkNid = nid();
        nodes.push({ id: linkNid, type: 'link', visible: true, data: {
          type: 'link', _nid: linkNid, _visible: true, _parent: groupNid,
          _href: '/at/' + appHost + '/ai.gftd.mangaka.document/' + d.docId,
          linkTitle: d.title || d.docId, _subtitle: (d.pages || 0) + 'p' + (d.images ? ' ' + d.images + 'img' : ''),
          text: d.title || d.docId, x: 320, y: 400, fontSize: 20, color: '#307050', font: 'sans',
        } });
      }
    }

    return {
      name: (project.name as string) || 'Project', docId: (project.projectId as string) || 'proj',
      convoId: (project.convoId as string) || '',
      pages: [{ id: pid(), name: (project.name as string) || 'Project', youshi: { id: nid(), type: 'b4manga', visible: true }, nodes: nodes as any }],
      activePageIdx: 0,
    };
  }

  async function resolveAtUri() {
    const at = parseAtUri();
    if (!at) return false;
    const isProject = at.collection.endsWith('.project');

    try {
      if (isProject) {
        const r = await xrpc('ai.gftd.mangaka.loadProject', { projectId: at.rkey });
        if (r.error) return false;
        return safeDeserialize(JSON.stringify(buildProjectToc(r)));
      } else {
        const r = await xrpc('ai.gftd.mangaka.loadDocument', { docId: at.rkey });
        const docStr = r.document || r.value_b64;
        if (docStr) return safeDeserialize(docStr);
      }
    } catch (e) { console.warn('AT URI resolve:', e); }
    return false;
  }

  // Auto-save
  let autoSaveTimer: ReturnType<typeof setTimeout> | null = null;
  function scheduleAutoSave() {
    if (typeof localStorage !== 'undefined') {
      try {
        localStorage.setItem('mangaka-' + nanoid, JSON.stringify(getDoc()));
      } catch (error) {
        console.warn('[silent-fail] Genko.svelte: local draft save failed', error);
      }
    }
    if (autoSaveTimer) clearTimeout(autoSaveTimer);
    autoSaveTimer = setTimeout(async () => {
      const d = getDoc();
      saveCurrentPage();
      try {
        await xrpc('ai.gftd.mangaka.saveDocument', { docId: d.docId, name: d.name, document: JSON.stringify(d), convoId: d.convoId || '' });
      } catch (e) { console.warn('auto-save:', e); }
    }, 5000);
  }

  function handleTreeChange() {
    requestRedraw();
    scheduleAutoSave();
    void recordOp('add', { nid: 'multi', type: 'add' }, null, null);
  }

  // ── Toolbar handlers ──
  function onYoushiChange(type: string) { activeYoushi = type; requestRedraw(); }
  async function onSaveDoc() {
    const d = getDoc();
    saveCurrentPage();
    try {
      await xrpc('ai.gftd.mangaka.saveDocument', {
        docId: d.docId, name: d.name, document: JSON.stringify(d), convoId: d.convoId || '',
      });
    } catch (e) { console.warn('saveDocument:', e); }
  }
  function onLoadDoc() {
    if (typeof document === 'undefined') return;
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'application/json,.json';
    input.onchange = () => {
      const file = input.files?.[0];
      if (!file) return;
      const reader = new FileReader();
      reader.onload = () => {
        if (typeof reader.result === 'string') safeDeserialize(reader.result);
      };
      reader.readAsText(file);
    };
    input.click();
  }
  function onSavePng() { console.info('onSavePng: not implemented'); }
  function onSaveSvg() { console.info('onSaveSvg: not implemented'); }
  function onExportOpLog() { console.info('onExportOpLog: not implemented'); }
  function onImportOpLog() { console.info('onImportOpLog: not implemented'); }

  // ── FloatingTools handlers ──
  function onModeChange(mode: string) { activeMode = mode; }
  function onBrushChange(brush: string) { activeBrush = brush; }
  function onColorChange(hex: string) { brushColor = hexToRgba(hex); requestRedraw(); }
  function onSizeChange(size: number) { brushSize = size; requestRedraw(); }
  function onUndo() { console.info('onUndo: not implemented'); }
  function onRedo() { console.info('onRedo: not implemented'); }

  // ── ChatPanel handlers ──
  async function onSend(text: string) {
    messages = [...messages, { sender: 'You', text, isUser: true }];
    try {
      const r = await xrpc('ai.gftd.mangaka.chat', { message: text, convoId: activeProjectId });
      if (r?.reply) {
        messages = [...messages, { sender: r.sender || 'Mangaka', text: r.reply, isUser: false }];
      }
    } catch (e) {
      console.warn('chat:', e);
    }
  }
  function onActorClick(style: string) { console.info('onActorClick:', style); }

  // ── AuthStatus handler ──
  function onSignIn() { redirectToAuth(nanoid); }

  // ── ProjectSelect handlers ──
  function onProjectSelect(convoId: string) {
    activeProjectId = convoId;
    switchProject(convoId, nanoid);
  }
  async function onProjectCreate() {
    if (typeof prompt === 'undefined') return;
    const pname = prompt('Project name?');
    if (!pname) return;
    const p = await createProject(pname, '', nanoid);
    if (p) {
      projects = [...getProjects()];
      activeProjectId = p.convoId;
    }
  }
  async function onProjectRefresh() {
    try {
      await loadProjects(nanoid);
      projects = [...getProjects()];
      activeProjectId = getActiveProjectId();
    } catch (e) { console.warn('refresh projects:', e); }
  }

  onMount(async () => {
    session = initAuth();

    const atPath = parseAtUri();
    if (!atPath) {
      try {
        const sv = localStorage.getItem('mangaka-' + nanoid);
        if (sv) safeDeserialize(sv);
      } catch (error) {
        console.warn('[silent-fail] Genko.svelte: local draft restore failed', error);
      }
    }
    setInitDone(true);

    setTimeout(async () => {
      if (atPath) await resolveAtUri();
      try {
        await loadProjects(nanoid);
        projects = [...getProjects()];
        activeProjectId = getActiveProjectId();
      } catch (e) { console.warn('load projects:', e); }
    }, 300);
  });
</script>

<svelte:window onkeydown={onWindowKeyDown} />
<svelte:head>
  <title>{name}</title>
  <link rel="preconnect" href="https://fonts.googleapis.com" />
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="anonymous" />
  <link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@400;700;900&family=Noto+Serif+JP:wght@400;700;900&family=M+PLUS+Rounded+1c:wght@400;700;900&family=Yusei+Magic&family=Reggae+One&display=swap" />
</svelte:head>

<div class="genko">
  <aside class="genko-left" class:collapsed={leftCollapsed}>
    <div class="genko-project-select">
      <ProjectSelect
        {projects}
        {activeProjectId}
        onselect={onProjectSelect}
        oncreate={onProjectCreate}
        onrefresh={onProjectRefresh}
      />
    </div>
    <div class="genko-tree">
      <NodeTree {nanoid} onchange={handleTreeChange} />
    </div>
  </aside>
  <button
    class="genko-toggle genko-toggle-left"
    class:collapsed={leftCollapsed}
    onclick={toggleLeft}
    title={leftCollapsed ? 'パネルを開く' : 'パネルを閉じる'}
    aria-label="Toggle left panel"
  >{leftCollapsed ? '›' : '‹'}</button>
  <main class="genko-center">
    <div class="genko-topbar">
      <div class="genko-toolbar-slot">
        <Toolbar
          {activeYoushi}
          onyoushichange={onYoushiChange}
          onsavepng={onSavePng}
          onsavesvg={onSaveSvg}
          onsavedoc={onSaveDoc}
          onloaddoc={onLoadDoc}
          onexportoplog={onExportOpLog}
          onimportoplog={onImportOpLog}
        />
      </div>
      <div class="genko-auth-slot">
        <AuthStatus {session} onsignin={onSignIn} />
      </div>
    </div>
    <div class="genko-canvas">
      <Canvas
        bind:this={canvasRef}
        {zoom} {panX} {panY} {activeYoushi}
        {activeBrush} {activeMode}
        {brushColor} {brushSize}
        {faceAddMode}
        onzoomchange={(z) => { zoom = z; }}
        onpanchange={(x, y) => { panX = x; panY = y; }}
        onselect={onSelectNode}
        onmove={() => { selectedTick++; }}
        ondragend={onDragEnd}
        onfaceadd={onFaceAdd}
        runop={(op) => runCanvasOp(op, buildCanvasCtx())}
      />
      <div class="zoom-toolbar">
        <button class="zoom-btn" onclick={() => canvasRef?.setZoom(zoom * 0.8)} title="縮小 (Cmd+wheel)">−</button>
        <span class="zoom-pct">{Math.round(zoom * 100)}%</span>
        <button class="zoom-btn" onclick={() => canvasRef?.setZoom(zoom * 1.25)} title="拡大">+</button>
        <button class="zoom-btn" onclick={() => canvasRef?.fitToView()} title="ページ全体を表示">Fit</button>
        <button class="zoom-btn" onclick={() => canvasRef?.setZoom(1)} title="100%">1:1</button>
      </div>
      {#if selectedOverlay}
        {@const sel = selectedOverlay as Record<string, any>}
        <div class="genko-selected-panel">
          <div class="sp-header">
            <span class="sp-type">{sel.type}</span>
            <span class="sp-id">{(sel._nid || '').slice(0, 10)}</span>
            <button class="sp-x" onclick={() => { setSelectedIdx(-1); refreshSelected(); }} title="閉じる">✕</button>
          </div>
          {#if sel.type === 'fukidashi'}
            <div class="sp-row">
              <span class="sp-label">形状:</span>
              {#each ['normal', 'thought', 'shout', 'whisper'] as s}
                <button class="sp-btn" class:active={sel.shape === s} onclick={() => updateSelected({ shape: s })} title={s}>
                  {s === 'normal' ? '○' : s === 'thought' ? '◌' : s === 'shout' ? '✸' : '⋯'}
                </button>
              {/each}
            </div>
            <div class="sp-row">
              <span class="sp-label">方向:</span>
              <button class="sp-btn" class:active={(sel._textOrientation || 'vertical') !== 'horizontal'}
                onclick={() => updateSelected({ _textOrientation: 'vertical' })} title="縦書き">縦</button>
              <button class="sp-btn" class:active={sel._textOrientation === 'horizontal'}
                onclick={() => updateSelected({ _textOrientation: 'horizontal' })} title="横書き">横</button>
            </div>
            <div class="sp-row">
              <span class="sp-label">しっぽ:</span>
              {#if sel._tailX != null && sel._tailY != null}
                <span style="font:11px monospace;color:#aaa">X {(sel._tailX as number).toFixed(0)} Y {(sel._tailY as number).toFixed(0)}</span>
                <button class="sp-btn" onclick={() => updateSelected({ _tailX: null, _tailY: null })} title="しっぽ削除">×</button>
              {:else}
                <button class="sp-btn" onclick={() => {
                  const bx = ((sel.x1 as number) + (sel.x2 as number)) / 2;
                  const by = Math.max(sel.y1 as number, sel.y2 as number) + (sel._unit === 'mm' ? 6 : 30);
                  updateSelected({ _tailX: bx, _tailY: by });
                }}>追加</button>
              {/if}
            </div>
            <div class="sp-row">
              <span class="sp-label">テキスト:</span>
              <input class="sp-input" type="text" value={sel.text || ''}
                oninput={(e) => updateSelected({ text: e.currentTarget.value })} />
            </div>
            <div class="sp-row">
              <span class="sp-label">幅:</span>
              <input class="sp-num" type="number" min="5" max="200" step="1" value={(sel.x2 - sel.x1).toFixed(0)}
                oninput={(e) => { const w = parseFloat(e.currentTarget.value); if (Number.isFinite(w)) updateSelected({ x2: sel.x1 + w }); }} />
              <span class="sp-label">高さ:</span>
              <input class="sp-num" type="number" min="5" max="200" step="1" value={(sel.y2 - sel.y1).toFixed(0)}
                oninput={(e) => { const h = parseFloat(e.currentTarget.value); if (Number.isFinite(h)) updateSelected({ y2: sel.y1 + h }); }} />
            </div>
            <div class="sp-row">
              <span class="sp-label">X:</span>
              <input class="sp-num" type="number" step="1" value={(sel.x1).toFixed(0)}
                oninput={(e) => { const v = parseFloat(e.currentTarget.value); if (Number.isFinite(v)) { const w = sel.x2 - sel.x1; updateSelected({ x1: v, x2: v + w }); } }} />
              <span class="sp-label">Y:</span>
              <input class="sp-num" type="number" step="1" value={(sel.y1).toFixed(0)}
                oninput={(e) => { const v = parseFloat(e.currentTarget.value); if (Number.isFinite(v)) { const h = sel.y2 - sel.y1; updateSelected({ y1: v, y2: v + h }); } }} />
            </div>
          {:else if sel.type === 'text'}
            <div class="sp-row">
              <span class="sp-label">テキスト:</span>
              <input class="sp-input" type="text" value={sel.text || ''}
                oninput={(e) => updateSelected({ text: e.currentTarget.value })} />
            </div>
            <div class="sp-row">
              <span class="sp-label">フォント:</span>
              <select class="sp-input" value={sel.fontFamily || 'gothic'}
                onchange={(e) => updateSelected({ fontFamily: e.currentTarget.value })}>
                <option value="gothic">ゴシック</option>
                <option value="mincho">明朝</option>
                <option value="maru">丸ゴ</option>
                <option value="handwritten">手書き</option>
                <option value="sfx">SFX</option>
              </select>
            </div>
            <div class="sp-row">
              <span class="sp-label">スタイル:</span>
              {#each ['normal','bold','italic','bolditalic'] as st}
                <button class="sp-btn" class:active={(sel.fontStyle || 'normal') === st} onclick={() => updateSelected({ fontStyle: st })}>{st === 'normal' ? '|' : st === 'bold' ? 'B' : st === 'italic' ? 'I' : 'BI'}</button>
              {/each}
            </div>
            <div class="sp-row">
              <span class="sp-label">サイズ(mm):</span>
              <input class="sp-num" type="number" min="1" max="50" step="0.5" value={sel.fontSize || 5}
                oninput={(e) => { const v = parseFloat(e.currentTarget.value); if (Number.isFinite(v)) updateSelected({ fontSize: v }); }} />
              <span class="sp-label">色:</span>
              <input class="sp-color" type="color" value={sel.color || '#000000'}
                oninput={(e) => updateSelected({ color: e.currentTarget.value })} />
            </div>
            <div class="sp-row">
              <span class="sp-label">X:</span>
              <input class="sp-num" type="number" step="1" value={(sel.x).toFixed(0)}
                oninput={(e) => { const v = parseFloat(e.currentTarget.value); if (Number.isFinite(v)) updateSelected({ x: v }); }} />
              <span class="sp-label">Y:</span>
              <input class="sp-num" type="number" step="1" value={(sel.y).toFixed(0)}
                oninput={(e) => { const v = parseFloat(e.currentTarget.value); if (Number.isFinite(v)) updateSelected({ y: v }); }} />
            </div>
          {:else if sel.type === 'panel'}
            <div class="sp-row">
              <span class="sp-label">名前:</span>
              <input class="sp-input" type="text" value={sel.panelName || ''}
                oninput={(e) => updateSelected({ panelName: e.currentTarget.value })} />
            </div>
            <div class="sp-row">
              <span class="sp-label">X:</span>
              <input class="sp-num" type="number" step="1" value={(sel.x1).toFixed(0)}
                oninput={(e) => { const v = parseFloat(e.currentTarget.value); if (Number.isFinite(v)) { const w = sel.x2 - sel.x1; updateSelected({ x1: v, x2: v + w }); } }} />
              <span class="sp-label">Y:</span>
              <input class="sp-num" type="number" step="1" value={(sel.y1).toFixed(0)}
                oninput={(e) => { const v = parseFloat(e.currentTarget.value); if (Number.isFinite(v)) { const h = sel.y2 - sel.y1; updateSelected({ y1: v, y2: v + h }); } }} />
            </div>
            <div class="sp-row">
              <span class="sp-label">幅:</span>
              <input class="sp-num" type="number" step="1" value={(sel.x2 - sel.x1).toFixed(0)}
                oninput={(e) => { const w = parseFloat(e.currentTarget.value); if (Number.isFinite(w)) updateSelected({ x2: sel.x1 + w }); }} />
              <span class="sp-label">高さ:</span>
              <input class="sp-num" type="number" step="1" value={(sel.y2 - sel.y1).toFixed(0)}
                oninput={(e) => { const h = parseFloat(e.currentTarget.value); if (Number.isFinite(h)) updateSelected({ y2: sel.y1 + h }); }} />
            </div>
          {:else if sel.type === 'ai-image'}
            <div class="sp-row">
              <span class="sp-label">Prompt:</span>
              <span class="sp-text-readonly">{(sel._genPrompt || '').slice(0, 80)}{(sel._genPrompt || '').length > 80 ? '…' : ''}</span>
            </div>
            <div class="sp-row">
              <span class="sp-label">画像 X:</span>
              <input class="sp-num" type="number" step="1" value={(sel._imageX || 0).toFixed(0)}
                oninput={(e) => { const v = parseFloat(e.currentTarget.value); if (Number.isFinite(v)) updateSelected({ _imageX: v }); }} />
              <span class="sp-label">Y:</span>
              <input class="sp-num" type="number" step="1" value={(sel._imageY || 0).toFixed(0)}
                oninput={(e) => { const v = parseFloat(e.currentTarget.value); if (Number.isFinite(v)) updateSelected({ _imageY: v }); }} />
              <span class="sp-label">倍率:</span>
              <input class="sp-num" type="number" min="0.1" max="5" step="0.05" value={(sel._imageScale || 1).toFixed(2)}
                oninput={(e) => { const v = parseFloat(e.currentTarget.value); if (Number.isFinite(v)) updateSelected({ _imageScale: v }); }} />
            </div>
            <div class="sp-row">
              <button class="sp-btn" onclick={() => updateSelected({ _imageX: 0, _imageY: 0, _imageScale: 1 })}>リセット</button>
            </div>
            <div class="sp-row">
              <span class="sp-label">顔:</span>
              {#if (sel._faces || []).length > 0}
                <span style="font:11px monospace;color:#aaa">{(sel._faces as any[]).length}個</span>
                <button class="sp-btn" onclick={() => updateSelected({ _faces: [] })} title="顔マーカー全消去">クリア</button>
              {/if}
              <button class="sp-btn" class:active={faceAddMode}
                onclick={() => { faceAddMode = !faceAddMode; }} title="クリックでマーカー追加モード">
                {faceAddMode ? '✋ 終了' : '+ 顔追加'}
              </button>
              <button class="sp-btn" onclick={runFaceDetect} title="自動顔検出 (Pregel)">🔍 自動</button>
            </div>
          {/if}
          {#if sel.type === 'ai-image' || sel.type === 'panel'}
            <div class="sp-row" style="flex-wrap:wrap;align-items:baseline">
              <span class="sp-label">感情:</span>
              {#if (sel._emotion as any)?.primary?.name}
                {@const emo = sel._emotion as any}
                <span style="font:11px/1.3 ui-monospace,monospace;color:#333">
                  {emo.primary.name}
                  <span style="color:#888">·</span>
                  {Number(emo.primary.score ?? 0).toFixed(2)}
                </span>
                {#if emo.algorithm}
                  <span style="font:10px monospace;color:#a89878">{emo.algorithm}</span>
                {/if}
                {#if emo.sourceCount && emo.sourceCount > 1}
                  <span style="font:10px monospace;color:#888" title="aggregated from {emo.sourceCount} child ai-images (winner {emo.winningChild})">
                    ×{emo.sourceCount}
                  </span>
                {/if}
                {#if Array.isArray(emo.topEmotions) && emo.topEmotions.length > 1}
                  <div style="flex:1 0 100%;font:10px ui-monospace,monospace;color:#888;margin-top:2px">
                    top: {emo.topEmotions.slice(0, 4).map((e: any) => `${e.name}=${Number(e.score ?? 0).toFixed(2)}`).join(' ')}
                  </div>
                {/if}
              {:else}
                <span style="font:11px monospace;color:#aaa">未採点</span>
              {/if}
              <button class="sp-btn" onclick={runEmotionScore}
                title="Hume image-head 感情採点 (compose_scene_3d で蓄積された corpus → distilled centroid → 各 node にスコア attach)">
                🎭 採点
              </button>
              {#if (sel._emotion as any)?.primary?.name}
                <button class="sp-btn" onclick={() => updateSelected({ _emotion: null })} title="感情データ削除">クリア</button>
              {/if}
            </div>
          {/if}
          <div class="sp-row sp-actions">
            <button class="sp-btn sp-danger" onclick={deleteSelected}>🗑 削除</button>
          </div>
        </div>
      {/if}
      <div class="genko-floating">
        <FloatingTools
          {activeMode}
          {activeBrush}
          brushColor={brushColorHex}
          {brushSize}
          {fukidashiShape}
          {textFont}
          {textSize}
          {textColor}
          {textStyle}
          onmodechange={onModeChange}
          onbrushchange={onBrushChange}
          oncolorchange={onColorChange}
          onsizechange={onSizeChange}
          onundo={onUndo}
          onredo={onRedo}
          onpanelpreset={onPanelPreset}
          onfukidashishape={onFukidashiShape}
          onaddfukidashi={onAddFukidashi}
          ontextfont={onTextFont}
          ontextsize={onTextSize}
          ontextcolor={onTextColor}
          ontextstyle={onTextStyle}
          onaddtext={onAddText}
          onaddsfx={onAddSfx}
        />
      </div>
    </div>
  </main>
  <button
    class="genko-toggle genko-toggle-right"
    class:collapsed={rightCollapsed}
    onclick={toggleRight}
    title={rightCollapsed ? 'パネルを開く' : 'パネルを閉じる'}
    aria-label="Toggle right panel"
  >{rightCollapsed ? '‹' : '›'}</button>
  <aside class="genko-right" class:collapsed={rightCollapsed}>
    <ChatPanel
      {members}
      {messages}
      onsend={onSend}
      onactorclick={onActorClick}
    />
  </aside>
</div>

<style>
  .genko {
    display: flex;
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    font-family: 'Nunito', 'Noto Sans JP', sans-serif;
  }
  .genko-left {
    flex-shrink: 0;
    width: 260px;
    height: 100%;
    display: flex;
    flex-direction: column;
    border-right: 1px solid #2a2a2e;
    background: #1a1a1f;
  }
  .genko-project-select {
    flex-shrink: 0;
    border-bottom: 1px solid #2a2a30;
  }
  .genko-tree {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .genko-center {
    flex: 1;
    min-width: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: #0f1115;
  }
  .genko-topbar {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    border-bottom: 1px solid #2a2a30;
  }
  .genko-toolbar-slot {
    flex: 1;
    min-width: 0;
    overflow-x: auto;
  }
  .genko-auth-slot {
    flex-shrink: 0;
    padding-right: 10px;
  }
  .genko-canvas {
    flex: 1;
    min-height: 0;
    display: flex;
    position: relative;
  }
  .zoom-toolbar {
    position: absolute;
    bottom: 12px;
    right: 12px;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px;
    background: rgba(20, 22, 28, 0.85);
    border: 1px solid #3a3f4a;
    border-radius: 6px;
    z-index: 30;
    box-shadow: 0 2px 6px rgba(0,0,0,0.4);
  }
  .zoom-btn {
    background: #2a2d36;
    color: #ddd;
    border: 1px solid #444;
    border-radius: 4px;
    padding: 3px 8px;
    font-size: 12px;
    cursor: pointer;
    min-width: 26px;
  }
  .zoom-btn:hover { background: #3a3f4a; }
  .zoom-pct {
    color: #ddd;
    font-size: 11px;
    min-width: 38px;
    text-align: center;
    font-family: monospace;
  }
  .genko-floating {
    position: absolute;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 10;
    pointer-events: none;
  }
  .genko-floating > :global(.floating-tools) {
    position: static !important;
    transform: none !important;
    pointer-events: auto;
    flex-wrap: nowrap;
    white-space: nowrap;
  }
  .genko-right {
    flex-shrink: 0;
    height: 100%;
    border-left: 1px solid #2a2a30;
  }

  /* Collapsed sidebars — width 0 but kept in DOM so state survives. */
  .genko-left.collapsed,
  .genko-right.collapsed {
    width: 0 !important;
    border: 0 !important;
    overflow: hidden;
  }

  /* Toggle buttons — small thin tabs sitting between sidebar and canvas. */
  .genko-toggle {
    flex-shrink: 0;
    width: 14px;
    height: 56px;
    align-self: center;
    margin-top: 0;
    background: #2a2a30;
    color: #cfcfd6;
    border: 1px solid #3a3a42;
    border-left: 0;
    border-right: 0;
    cursor: pointer;
    font-size: 14px;
    font-weight: 700;
    padding: 0;
    line-height: 56px;
    display: flex;
    align-items: center;
    justify-content: center;
    user-select: none;
    z-index: 5;
  }
  .genko-toggle:hover { background: #3a3a42; color: #fff; }
  .genko-toggle-left { border-radius: 0 6px 6px 0; }
  .genko-toggle-right { border-radius: 6px 0 0 6px; }

  /* Selection edit panel — floating top-right of canvas */
  .genko-selected-panel {
    position: absolute;
    top: 12px;
    right: 12px;
    width: 280px;
    background: #fff;
    border-radius: 10px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
    padding: 8px 10px;
    z-index: 20;
    font-family: 'Nunito', 'Noto Sans JP', sans-serif;
    font-size: 11px;
    color: #222;
  }
  .sp-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-bottom: 6px;
    border-bottom: 1px solid #e0e0e0;
    margin-bottom: 6px;
  }
  .sp-type { font-weight: 800; text-transform: uppercase; font-size: 10px; color: #663300; }
  .sp-id { font-family: monospace; font-size: 9px; color: #999; flex: 1; }
  .sp-x { border: 0; background: transparent; cursor: pointer; font-size: 14px; color: #888; padding: 0 4px; }
  .sp-x:hover { color: #000; }
  .sp-row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 0;
    flex-wrap: wrap;
  }
  .sp-label { font-size: 10px; color: #555; min-width: 50px; }
  .sp-btn {
    min-width: 26px; height: 22px; padding: 0 6px;
    border: 1px solid #ccc; border-radius: 4px; background: #fff;
    cursor: pointer; font-size: 11px; font-weight: 700;
  }
  .sp-btn:hover { background: #f0ead6; }
  .sp-btn.active { background: #f0ead6; border-color: #c8b888; }
  .sp-input {
    flex: 1; min-width: 80px; height: 22px; padding: 0 6px;
    border: 1px solid #ccc; border-radius: 4px; font-size: 11px; background: #fff;
  }
  .sp-num {
    width: 56px; height: 22px; padding: 0 4px;
    border: 1px solid #ccc; border-radius: 4px; font-size: 11px; background: #fff;
  }
  .sp-color {
    width: 26px; height: 22px; padding: 0; border: 1px solid #ccc; border-radius: 4px; background: #fff; cursor: pointer;
  }
  .sp-text-readonly { font-size: 10px; color: #666; flex: 1; }
  .sp-actions { justify-content: flex-end; padding-top: 6px; border-top: 1px solid #f0f0f0; margin-top: 4px; }
  .sp-danger { background: #fee; border-color: #f99; color: #c00; }
  .sp-danger:hover { background: #fdd; }

  @media (max-width: 1100px) {
    .genko-left { width: 220px; }
  }
</style>
