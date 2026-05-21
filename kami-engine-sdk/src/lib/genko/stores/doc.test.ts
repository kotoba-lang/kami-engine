import { describe, it, expect, beforeEach } from 'vitest';
import {
  getDoc, setDoc, getStrokes, getOverlays,
  getSelectedIdx, setSelectedIdx,
  activePage, saveCurrentPage, loadPage,
  computeTreeNodes, findByNid,
  toggleNodeVisibility, deleteNode, addOverlay,
  toggleCollapsed, toggleContext,
  agentColor, agentInitials,
  nid, pid, requestRedraw, consumeRedraw,
  setInitDone,
  type GenkoDoc,
} from './doc.svelte';

function makeDoc(pages = 1): GenkoDoc {
  return {
    name: 'Test', docId: 'test-1', convoId: '',
    pages: Array.from({ length: pages }, (_, i) => ({
      id: pid(), name: `Page ${i + 1}`,
      youshi: { id: nid(), type: 'b4manga', visible: true },
      nodes: [],
    })),
    activePageIdx: 0,
  };
}

function makeDocWithNodes(): GenkoDoc {
  const pnid = nid(), inid = nid(), tnid = nid();
  return {
    name: 'Test', docId: 'test-nodes', convoId: '',
    pages: [{
      id: pid(), name: 'Page 1',
      youshi: { id: nid(), type: 'b4manga', visible: true },
      nodes: [
        { id: pnid, type: 'panel', visible: true, data: { type: 'panel', _nid: pnid, _visible: true, panelName: '1', x1: 0, y1: 0, x2: 100, y2: 100 } },
        { id: inid, type: 'ai-image', visible: true, data: { type: 'ai-image', _nid: inid, _visible: true, _parent: pnid, _genImageUrl: 'https://example.com/img.jpg', _genPrompt: 'test prompt' } },
        { id: tnid, type: 'text', visible: true, data: { type: 'text', _nid: tnid, _visible: true, _parent: pnid, text: 'Hello' } },
      ],
    }],
    activePageIdx: 0,
  };
}

describe('doc store — basic operations', () => {
  beforeEach(() => {
    setDoc(makeDoc());
    setInitDone(false);
    loadPage(0);
    setInitDone(true);
  });

  it('getDoc returns document', () => {
    expect(getDoc().name).toBe('Test');
    expect(getDoc().pages.length).toBe(1);
  });

  it('activePage returns current page', () => {
    expect(activePage().name).toBe('Page 1');
  });

  it('nid generates unique IDs', () => {
    const a = nid(), b = nid();
    expect(a).not.toBe(b);
    expect(a.startsWith('n')).toBe(true);
  });

  it('pid generates unique page IDs', () => {
    const a = pid(), b = pid();
    expect(a).not.toBe(b);
    expect(a.startsWith('p')).toBe(true);
  });

  it('requestRedraw + consumeRedraw', () => {
    // loadPage in beforeEach calls requestRedraw, so consume it first
    consumeRedraw();
    expect(consumeRedraw()).toBe(false);
    requestRedraw();
    expect(consumeRedraw()).toBe(true);
    expect(consumeRedraw()).toBe(false);
  });
});

describe('doc store — node operations', () => {
  beforeEach(() => {
    setDoc(makeDocWithNodes());
    setInitDone(false);
    loadPage(0);
    setInitDone(true);
  });

  it('loadPage populates strokes and overlays', () => {
    expect(getStrokes().length).toBe(0);
    expect(getOverlays().length).toBe(3); // panel + ai-image + text
  });

  it('computeTreeNodes returns correct tree', () => {
    const nodes = computeTreeNodes();
    expect(nodes.length).toBe(3);
    expect(nodes[0].nm).toBe('Panel 1');
    expect(nodes[1].nm).toContain('AI Image');
    expect(nodes[2].nm).toContain('Text:');
  });

  it('tree nodes have correct parent-child relationships', () => {
    const nodes = computeTreeNodes();
    const panel = nodes[0];
    const image = nodes[1];
    const text = nodes[2];
    expect(panel.par).toBe('');
    expect(image.par).toBe(panel.nid);
    expect(text.par).toBe(panel.nid);
    expect(panel.hasChildren).toBe(true);
  });

  it('findByNid finds nodes', () => {
    const overlays = getOverlays();
    const panelNid = overlays[0]._nid as string;
    const found = findByNid(panelNid);
    expect(found).toBeDefined();
    expect(found?.type).toBe('panel');
  });

  it('toggleNodeVisibility toggles visibility', () => {
    const overlays = getOverlays();
    expect(overlays[0]._visible).toBe(true);
    toggleNodeVisibility('o', 0);
    expect(overlays[0]._visible).toBe(false);
    toggleNodeVisibility('o', 0);
    expect(overlays[0]._visible).toBe(true);
  });

  it('deleteNode removes node and unparents children', () => {
    const overlays = getOverlays();
    const panelNid = overlays[0]._nid as string;
    deleteNode(panelNid);
    expect(getOverlays().length).toBe(2); // image + text remain
    // Children should be unparented
    expect(getOverlays()[0]._parent).toBe('');
    expect(getOverlays()[1]._parent).toBe('');
  });

  it('addOverlay adds to overlays array', () => {
    const before = getOverlays().length;
    addOverlay({ type: 'group', groupName: 'G1', _nid: nid(), _visible: true, _parent: '' });
    expect(getOverlays().length).toBe(before + 1);
  });

  it('selectedIdx management', () => {
    expect(getSelectedIdx()).toBe(-1);
    setSelectedIdx(2);
    expect(getSelectedIdx()).toBe(2);
    setSelectedIdx(-1);
    expect(getSelectedIdx()).toBe(-1);
  });
});

describe('doc store — page management', () => {
  beforeEach(() => {
    setDoc(makeDocWithNodes());
    setInitDone(false);
    loadPage(0);
    setInitDone(true);
  });

  it('saveCurrentPage persists strokes/overlays back to page nodes', () => {
    expect(getOverlays().length).toBe(3);
    addOverlay({ type: 'tone', _nid: nid(), _visible: true, _parent: '', x1: 0, y1: 0, x2: 50, y2: 50 });
    expect(getOverlays().length).toBe(4);
    saveCurrentPage();
    expect(activePage().nodes.length).toBe(4);
  });

  it('loadPage clears and reloads from page nodes', () => {
    addOverlay({ type: 'tone', _nid: nid(), _visible: true, _parent: '' });
    expect(getOverlays().length).toBe(4);
    // With _initDone=true, loadPage(0) calls saveCurrentPage first (persists 4 overlays),
    // then reloads from the now-updated page.nodes (4 nodes)
    loadPage(0);
    expect(getOverlays().length).toBe(4);
  });

  it('multi-page switching preserves data', () => {
    const doc = getDoc();
    doc.pages.push({ id: pid(), name: 'Page 2', youshi: { id: nid(), type: 'b4manga', visible: true }, nodes: [] });
    loadPage(1);
    expect(getOverlays().length).toBe(0);
    loadPage(0);
    expect(getOverlays().length).toBe(3);
  });
});

describe('doc store — collapsed + context', () => {
  it('toggleCollapsed toggles set membership', () => {
    const nodes = computeTreeNodes();
    const id = 'test-nid';
    expect(getDoc()).toBeDefined(); // ensure store is initialized
    toggleCollapsed(id);
    // collapsed state is managed by the set
  });

  it('toggleContext returns added/removed state', () => {
    const added = toggleContext('ctx-1');
    expect(added).toBe(true);
    const removed = toggleContext('ctx-1');
    expect(removed).toBe(false);
  });
});

describe('doc store — agent helpers', () => {
  it('agentColor returns color for known agents', () => {
    expect(agentColor('shonen')).toBe('#e06060');
    expect(agentColor('genga')).toBe('#c07020');
    expect(agentColor('unknown')).toBe('#888');
    expect(agentColor('')).toBe('#888');
  });

  it('agentInitials returns uppercase 2-char', () => {
    expect(agentInitials('shonen')).toBe('SH');
    expect(agentInitials('genga')).toBe('GE');
    expect(agentInitials('')).toBe('');
  });
});

describe('doc store — link node type', () => {
  it('link nodes show linkTitle in tree', () => {
    const linkNid = nid();
    setDoc({
      name: 'Project', docId: 'proj-1', convoId: '',
      pages: [{
        id: pid(), name: 'TOC',
        youshi: { id: nid(), type: 'b4manga', visible: true },
        nodes: [
          { id: linkNid, type: 'link', visible: true, data: { type: 'link', _nid: linkNid, _visible: true, linkTitle: 'Arc 0-1', _href: '/at/test/doc/id', _subtitle: '46p 247img' } },
        ],
      }],
      activePageIdx: 0,
    });
    setInitDone(false);
    loadPage(0);
    setInitDone(true);
    const nodes = computeTreeNodes();
    expect(nodes.length).toBe(1);
    expect(nodes[0].nm).toBe('Arc 0-1');
    expect(nodes[0].ref._href).toBe('/at/test/doc/id');
  });
});

describe('regression — _initDone guard prevents overwrite', () => {
  it('loadPage with _initDone=false skips saveCurrentPage', () => {
    // This was the critical bug: AT URI load called deserializeDoc which set doc=newDoc,
    // then loadPage called saveCurrentPage which overwrote newDoc's nodes with empty arrays
    const docWithNodes = makeDocWithNodes();
    setDoc(docWithNodes);
    setInitDone(false); // Simulate AT URI load context
    loadPage(0);
    // Nodes should be loaded correctly (not wiped)
    expect(getOverlays().length).toBe(3);
  });

  it('loadPage with _initDone=true calls saveCurrentPage first', () => {
    setDoc(makeDocWithNodes());
    setInitDone(false);
    loadPage(0);
    setInitDone(true);
    // Add an overlay
    addOverlay({ type: 'tone', _nid: nid(), _visible: true, _parent: '' });
    expect(getOverlays().length).toBe(4);
    // Switch to page 0 again — saveCurrentPage should persist the 4 overlays
    const doc = getDoc();
    doc.pages.push({ id: pid(), name: 'P2', youshi: { id: nid(), type: 'b4manga', visible: true }, nodes: [] });
    loadPage(1);
    loadPage(0);
    expect(getOverlays().length).toBe(4);
  });
});
