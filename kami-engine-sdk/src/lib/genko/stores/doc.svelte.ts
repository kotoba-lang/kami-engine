/**
 * Genko document store — shared state for canvas + UI.
 * Svelte 5 runes ($state, $derived).
 */

// --- Types ---
export interface GenkoNode {
  id: string;
  type: 'stroke' | 'panel' | 'ai-image' | 'ai-desc' | 'text' | 'link' | 'prompt' | 'group' | 'tone' | 'fukidashi';
  visible: boolean;
  data: Record<string, unknown>;
}

export interface GenkoPage {
  id: string;
  name: string;
  youshi: { id: string; type: string; visible: boolean };
  nodes: GenkoNode[];
}

export interface GenkoDoc {
  name: string;
  docId: string;
  convoId: string;
  pages: GenkoPage[];
  activePageIdx: number;
}

export interface TreeNode {
  gi: number;
  kind: 's' | 'o';
  idx: number;
  nid: string;
  par: string;
  vis: boolean;
  nm: string;
  ref: Record<string, unknown>;
  hasChildren: boolean;
  agent: string;
}

// --- Agent colors ---
const AGENT_COLORS: Record<string, string> = {
  shonen: '#e06060', shojo: '#e060c0', seinen: '#6060e0', yonkoma: '#60c060',
  mecha: '#6090e0', horror: '#904090', background: '#609060', genga: '#c07020',
  director: '#c0a020', '': '#888',
};
export function agentColor(agent: string): string { return AGENT_COLORS[agent] || AGENT_COLORS['']; }
export function agentInitials(agent: string): string { return (agent || '').slice(0, 2).toUpperCase() || ''; }

// --- ID generators ---
let _nidC = 1;
export function nid(): string { return 'n' + (_nidC++); }
export function pid(): string { return 'p' + Date.now().toString(36) + Math.random().toString(36).slice(2, 6); }

// --- Reactive state ---
let _doc = $state<GenkoDoc>({
  name: 'Untitled', docId: '', convoId: '',
  pages: [{ id: pid(), name: 'Page 1', youshi: { id: nid(), type: 'b4manga', visible: true }, nodes: [] }],
  activePageIdx: 0,
});

let _strokes = $state<Record<string, unknown>[]>([]);
let _overlays = $state<Record<string, unknown>[]>([]);
let _selectedIdx = $state(-1);
let _collapsedNodes = $state(new Set<string>());
let _contextNodes = $state(new Set<string>());
let _needsRedraw = $state(false);
let _initDone = $state(false);

// --- Accessors ---
export function getDoc(): GenkoDoc { return _doc; }
export function setDoc(d: GenkoDoc) { _doc = d; }
export function getStrokes() { return _strokes; }
export function getOverlays() { return _overlays; }
export function getSelectedIdx() { return _selectedIdx; }
export function setSelectedIdx(i: number) { _selectedIdx = i; }
export function getCollapsedNodes() { return _collapsedNodes; }
export function getContextNodes() { return _contextNodes; }
export function isInitDone() { return _initDone; }
export function setInitDone(v: boolean) { _initDone = v; }
export function requestRedraw() { _needsRedraw = true; }
export function consumeRedraw(): boolean { if (_needsRedraw) { _needsRedraw = false; return true; } return false; }

export function activePage(): GenkoPage { return _doc.pages[_doc.activePageIdx]; }

// --- Page management ---
export function saveCurrentPage() {
  const pg = activePage();
  pg.nodes = [];
  for (const s of _strokes) pg.nodes.push({ id: (s._nid as string) || nid(), type: 'stroke' as const, visible: (s._visible as boolean) !== false, data: s });
  for (const o of _overlays) pg.nodes.push({ id: (o._nid as string) || nid(), type: (o.type as GenkoNode['type']), visible: (o._visible as boolean) !== false, data: o });
}

export function loadPage(idx: number) {
  if (_initDone) saveCurrentPage();
  _doc.activePageIdx = idx;
  const pg = activePage();
  _strokes = [];
  _overlays = [];
  for (const n of pg.nodes) {
    n.data._nid = n.id;
    n.data._visible = n.visible;
    if (n.type === 'stroke') _strokes.push(n.data); else _overlays.push(n.data);
  }
  _selectedIdx = -1;
  _needsRedraw = true;
}

// --- Node tree computation ---
export function computeTreeNodes(): TreeNode[] {
  const out: TreeNode[] = [];
  let panelCount = 0;
  _strokes.forEach((s, i) => out.push({
    gi: i, kind: 's', idx: i, nid: (s._nid as string) || '', par: (s._parent as string) || '',
    vis: (s._visible as boolean) !== false, nm: 'Stroke ' + (i + 1), ref: s, hasChildren: false, agent: (s._agent as string) || '',
  }));
  _overlays.forEach((o, i) => {
    const gi = _strokes.length + i;
    let nm = o.type as string;
    if (o.type === 'panel') { panelCount++; nm = 'Panel ' + (o.panelName || panelCount); }
    else if (o.type === 'ai-image') nm = 'AI Image' + (o._genPrompt ? ' (' + (o._genPrompt as string).slice(0, 12) + ')' : '');
    else if (o.type === 'ai-desc') nm = 'AI Desc' + (o._genPrompt ? ' (' + (o._genPrompt as string).slice(0, 12) + ')' : '');
    else if (o.type === 'prompt') nm = 'Prompt: ' + ((o.prompt as string) || '').slice(0, 16);
    else if (o.type === 'text') nm = 'Text: ' + ((o.text as string) || '').slice(0, 8);
    else if (o.type === 'link') nm = (o.linkTitle as string) || (o.text as string) || 'Link';
    else if (o.type === 'group') nm = (o.groupName as string) || 'Group';
    else if (o.type === 'tone') nm = 'Tone';
    else if (o.type === 'fukidashi') nm = 'Fukidashi';
    out.push({ gi, kind: 'o', idx: i, nid: (o._nid as string) || '', par: (o._parent as string) || (o._layer as string) || '', vis: (o._visible as boolean) !== false, nm, ref: o, hasChildren: false, agent: (o._agent as string) || '' });
  });
  const nids = new Set(out.map(n => n.nid));
  out.forEach(n => { if (n.par && nids.has(n.par)) { const p = out.find(x => x.nid === n.par); if (p) p.hasChildren = true; } });
  return out;
}

// --- Node operations ---
export function findByNid(id: string): Record<string, unknown> | undefined {
  return _strokes.find(s => s._nid === id) || _overlays.find(o => o._nid === id);
}

export function toggleNodeVisibility(kind: 's' | 'o', idx: number): string {
  if (kind === 's') { _strokes[idx]._visible = !(_strokes[idx]._visible !== false); return (_strokes[idx]._nid as string) || ''; }
  const o = _overlays[idx]; if (o) { o._visible = !(o._visible !== false); return (o._nid as string) || ''; }
  return '';
}

export function deleteNode(dnid: string) {
  _strokes.forEach(s => { if (s._parent === dnid) s._parent = ''; });
  _overlays.forEach(o => { if (o._parent === dnid || o._layer === dnid) { o._parent = ''; o._layer = ''; } });
  const si = _strokes.findIndex(s => s._nid === dnid);
  if (si >= 0) { _strokes.splice(si, 1); if (_selectedIdx === si) _selectedIdx = -1; }
  else {
    const oi = _overlays.findIndex(o => o._nid === dnid);
    if (oi >= 0) { _overlays.splice(oi, 1); if (_selectedIdx === _strokes.length + oi) _selectedIdx = -1; }
  }
  _needsRedraw = true;
}

export function addOverlay(o: Record<string, unknown>) {
  _overlays.push(o);
  _needsRedraw = true;
}

export function toggleCollapsed(nid: string) {
  if (_collapsedNodes.has(nid)) _collapsedNodes.delete(nid); else _collapsedNodes.add(nid);
}

export function toggleContext(nid: string): boolean {
  if (_contextNodes.has(nid)) { _contextNodes.delete(nid); return false; }
  _contextNodes.add(nid); return true;
}
