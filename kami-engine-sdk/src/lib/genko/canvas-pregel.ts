/**
 * canvas-pregel.ts — Client-side LangGraph TS (Pregel) pipelines for ALL
 * canvas mutations (mangaka).
 *
 * Uses `@langchain/langgraph` `StateGraph` so the canvas mutation graphs share
 * the same idiom as the server-side Python graphs on `lg-mangaka` pod
 * (load_document / save_document / detect_faces / score_emotion / debug_canvas_state).
 *
 * Each `CanvasOp` is dispatched to its dedicated compiled graph. Channels =
 * `Annotation.Root({...})` fields; super-steps = `.addNode()` callbacks; edges
 * = `.addEdge()`. Graphs are compiled once at module load and reused.
 *
 * Why client-side Pregel:
 *   • Single mutation entry point: `runCanvasOp(op, ctx)`.
 *   • Same StateGraph shape as the Python pod graphs → portable mental model.
 *   • Per-step diffability — each channel mutation is observable.
 *   • Final step `emit_op` records to opLog (suppressed via `quiet:true` for
 *     per-frame pointer-move ticks).
 */

import { StateGraph, START, END, Annotation } from '@langchain/langgraph';
import type { GenkoDoc } from './stores/doc.svelte';

// ─────────────────────────────────────────────────────────────────────────
// Types

export type Unit = 'mm' | 'px' | string;
export type Handle = 'nw' | 'n' | 'ne' | 'w' | 'e' | 'sw' | 's' | 'se' | 'tail';

export interface NodeSnap {
  x1?: number; y1?: number; x2?: number; y2?: number;
  x?: number; y?: number;
  _imageX?: number; _imageY?: number; _imageScale?: number;
  _tailX?: number | null; _tailY?: number | null;
}

export interface CanvasNode extends NodeSnap {
  _nid?: string;
  type?: string;
  _unit?: Unit;
  _parent?: string;
  _faces?: Array<{ cx: number; cy: number; w?: number; h?: number }>;
  _tailAnchor?: { imageNid?: string; faceIdx?: number };
  [k: string]: unknown;
}

export interface CanvasCtx {
  doc: GenkoDoc;
  overlays: CanvasNode[];
  strokes: Record<string, unknown>[];
  selectedIdx: number;
  dpr: number;
  paperSc: number;
  addOverlay: (o: CanvasNode) => void;
  deleteNode: (nid: string) => void;
  setSelectedIdx: (i: number) => void;
  findByNid: (nid: string) => CanvasNode | undefined;
  requestRedraw: () => void;
  scheduleAutoSave: () => void;
  recordOp: (op: string, target: { nid: string; type: string }, before: unknown, after: unknown) => void;
  pushUndo?: (e: { op: string; nids?: string[]; nid?: string; before?: unknown; after?: unknown; node?: unknown }) => void;
  nid: () => string;
}

// ─── Op discriminated union ──────────────────────────────────────────────

type _CommonOp = { quiet?: boolean };
export type CanvasOp =
  | (_CommonOp & { kind: 'resize'; nid: string; handle: Handle; initial: NodeSnap; dxCss: number; dyCss: number })
  | (_CommonOp & { kind: 'drag'; nid: string; initial: NodeSnap; childrenInitial: Array<{ nid: string; initial: NodeSnap }>; dxCss: number; dyCss: number })
  | (_CommonOp & { kind: 'image_offset'; nid: string; initial: NodeSnap; dxCss: number; dyCss: number })
  | (_CommonOp & { kind: 'tail_drag'; nid: string; initial: { _tailX?: number | null; _tailY?: number | null }; dxCss: number; dyCss: number })
  | (_CommonOp & { kind: 'tail_anchor'; nid: string; imageNid: string; faceIdx: number })
  | (_CommonOp & { kind: 'tail_clear'; nid: string })
  | (_CommonOp & { kind: 'face_add'; imageNid: string; cx: number; cy: number })
  | (_CommonOp & { kind: 'face_clear'; imageNid: string })
  | (_CommonOp & { kind: 'add_node'; node: CanvasNode })
  | (_CommonOp & { kind: 'add_nodes'; nodes: CanvasNode[] })
  | (_CommonOp & { kind: 'delete'; nid: string })
  | (_CommonOp & { kind: 'update_props'; nid: string; patch: Record<string, unknown> });

// ─── State annotation (LangGraph channels) ───────────────────────────────

const last = <T>() => ({ reducer: (_a: T, b: T) => b, default: () => undefined as unknown as T });

const OpStateAnnotation = Annotation.Root({
  // immutable inputs (set once at invocation):
  op:       Annotation<CanvasOp>(last<CanvasOp>()),
  ctx:      Annotation<CanvasCtx>(last<CanvasCtx>()),
  // channels (filled by super-steps):
  dx:       Annotation<number>(last<number>()),
  dy:       Annotation<number>(last<number>()),
  before:   Annotation<unknown>(last<unknown>()),
  after:    Annotation<unknown>(last<unknown>()),
  children: Annotation<Array<{ overlay: CanvasNode; initial: NodeSnap }>>(last<Array<{ overlay: CanvasNode; initial: NodeSnap }>>()),
  emitted:  Annotation<boolean>(last<boolean>()),
});
type OpState = typeof OpStateAnnotation.State;

// ─── Helpers ─────────────────────────────────────────────────────────────

function resolveNode(s: OpState): CanvasNode | undefined {
  const op = s.op as any;
  const nid = 'nid' in op ? op.nid : ('imageNid' in op ? op.imageNid : undefined);
  if (typeof nid === 'string' && nid) return s.ctx.findByNid(nid);
  return undefined;
}

// ─── Shared super-steps ──────────────────────────────────────────────────

const step_compute_delta = (s: OpState): Partial<OpState> => {
  const op = s.op as any;
  if (!('dxCss' in op) || !('dyCss' in op)) return {};
  const node = resolveNode(s);
  let dx = op.dxCss * s.ctx.dpr;
  let dy = op.dyCss * s.ctx.dpr;
  if (node?._unit === 'mm' && s.ctx.paperSc > 0) {
    dx = dx / s.ctx.paperSc;
    dy = dy / s.ctx.paperSc;
  }
  return { dx, dy };
};

const step_snapshot_before = (s: OpState): Partial<OpState> => {
  const node = resolveNode(s);
  if (!node) return {};
  const op = s.op as any;
  if (op.kind === 'resize' || op.kind === 'drag' || op.kind === 'image_offset') {
    return { before: op.initial };
  } else if (op.kind === 'update_props') {
    const patch = op.patch as Record<string, unknown>;
    const before: Record<string, unknown> = {};
    for (const k of Object.keys(patch)) before[k] = (node as Record<string, unknown>)[k];
    return { before };
  } else if (op.kind === 'tail_drag' || op.kind === 'tail_anchor' || op.kind === 'tail_clear') {
    return { before: { _tailX: node._tailX, _tailY: node._tailY, _tailAnchor: node._tailAnchor } };
  } else if (op.kind === 'face_add' || op.kind === 'face_clear') {
    return { before: { _faces: node._faces ? [...node._faces] : [] } };
  } else if (op.kind === 'delete') {
    return { before: { ...node } };
  }
  return {};
};

const step_emit_op = (s: OpState): Partial<OpState> => {
  if ((s.op as any).quiet) return { emitted: false };
  const node = resolveNode(s);
  const op = s.op as any;
  const opName = ({
    resize: 'resize', drag: 'move', image_offset: 'image_offset',
    tail_drag: 'tail_drag', tail_anchor: 'tail_anchor', tail_clear: 'tail_clear',
    face_add: 'face_add', face_clear: 'face_clear',
    add_node: 'add', add_nodes: 'add',
    delete: 'delete', update_props: 'edit',
  } as Record<string, string>)[op.kind] || op.kind;
  const nid =
    'nid' in op ? op.nid :
    ('imageNid' in op ? op.imageNid :
    (op.kind === 'add_node' ? op.node?._nid :
    (op.kind === 'add_nodes' ? 'multi' : '')));
  const type = (node?.type as string) || (op.kind === 'add_nodes' ? 'add' : '');
  s.ctx.recordOp(opName, { nid: String(nid || ''), type: String(type || '') }, s.before, s.after);
  return { emitted: true };
};

const step_redraw_and_save = (s: OpState): Partial<OpState> => {
  s.ctx.requestRedraw();
  s.ctx.scheduleAutoSave();
  return {};
};

// ─── Resize pipeline ─────────────────────────────────────────────────────

const step_resize_gather_cascade = (s: OpState): Partial<OpState> => {
  const node = resolveNode(s);
  if (!node || node.type !== 'panel') return { children: [] };
  return {
    children: s.ctx.overlays
      .filter((c) => c._parent === node._nid)
      .map((overlay) => ({
        overlay,
        initial: { x1: overlay.x1, y1: overlay.y1, x2: overlay.x2, y2: overlay.y2 },
      })),
  };
};

const step_resize_apply_rect = (s: OpState): Partial<OpState> => {
  if (s.op.kind !== 'resize') return {};
  const node = resolveNode(s);
  if (!node) return {};
  const init = s.op.initial;
  const h = s.op.handle;
  const dx = s.dx ?? 0;
  const dy = s.dy ?? 0;
  const hasW = h === 'nw' || h === 'w' || h === 'sw';
  const hasE = h === 'ne' || h === 'e' || h === 'se';
  const hasN = h === 'nw' || h === 'n' || h === 'ne';
  const hasS = h === 'sw' || h === 's' || h === 'se';
  if (hasW && init.x1 != null) node.x1 = init.x1 + dx;
  if (hasE && init.x2 != null) node.x2 = init.x2 + dx;
  if (hasN && init.y1 != null) node.y1 = init.y1 + dy;
  if (hasS && init.y2 != null) node.y2 = init.y2 + dy;
  return { after: { x1: node.x1, y1: node.y1, x2: node.x2, y2: node.y2 } };
};

const step_resize_apply_cascade = (s: OpState): Partial<OpState> => {
  if (s.op.kind !== 'resize' || !s.children || s.children.length === 0) return {};
  const h = s.op.handle;
  const dx = s.dx ?? 0;
  const dy = s.dy ?? 0;
  const hasW = h === 'nw' || h === 'w' || h === 'sw';
  const hasE = h === 'ne' || h === 'e' || h === 'se';
  const hasN = h === 'nw' || h === 'n' || h === 'ne';
  const hasS = h === 'sw' || h === 's' || h === 'se';
  for (const c of s.children) {
    const n = c.overlay;
    const ci = c.initial;
    if (hasW && ci.x1 != null) n.x1 = ci.x1 + dx;
    if (hasE && ci.x2 != null) n.x2 = ci.x2 + dx;
    if (hasN && ci.y1 != null) n.y1 = ci.y1 + dy;
    if (hasS && ci.y2 != null) n.y2 = ci.y2 + dy;
  }
  return {};
};

const step_clamp_invariants = (s: OpState): Partial<OpState> => {
  const swap = (n: CanvasNode) => {
    if (n.x1 != null && n.x2 != null && n.x1 > n.x2) { const t = n.x1; n.x1 = n.x2; n.x2 = t; }
    if (n.y1 != null && n.y2 != null && n.y1 > n.y2) { const t = n.y1; n.y1 = n.y2; n.y2 = t; }
  };
  const node = resolveNode(s);
  if (node) swap(node);
  for (const c of (s.children || [])) swap(c.overlay);
  return {};
};

// ─── Drag pipeline ───────────────────────────────────────────────────────

const step_drag_apply = (s: OpState): Partial<OpState> => {
  if (s.op.kind !== 'drag') return {};
  const node = resolveNode(s);
  if (!node) return {};
  const dx = s.dx ?? 0;
  const dy = s.dy ?? 0;
  const init = s.op.initial;
  if (init.x != null) node.x = init.x + dx;
  if (init.y != null) node.y = init.y + dy;
  if (init.x1 != null) node.x1 = init.x1 + dx;
  if (init.y1 != null) node.y1 = init.y1 + dy;
  if (init.x2 != null) node.x2 = init.x2 + dx;
  if (init.y2 != null) node.y2 = init.y2 + dy;
  if (node.type === 'panel') {
    for (const c of s.op.childrenInitial) {
      const child = s.ctx.findByNid(c.nid);
      if (!child) continue;
      const ci = c.initial;
      if (ci.x1 != null) child.x1 = ci.x1 + dx;
      if (ci.y1 != null) child.y1 = ci.y1 + dy;
      if (ci.x2 != null) child.x2 = ci.x2 + dx;
      if (ci.y2 != null) child.y2 = ci.y2 + dy;
    }
  }
  return { after: { x: node.x, y: node.y, x1: node.x1, y1: node.y1, x2: node.x2, y2: node.y2 } };
};

// ─── Image offset pipeline ───────────────────────────────────────────────

const step_image_offset_apply = (s: OpState): Partial<OpState> => {
  if (s.op.kind !== 'image_offset') return {};
  const node = resolveNode(s);
  if (!node) return {};
  const init = s.op.initial;
  node._imageX = (init._imageX ?? 0) + (s.dx ?? 0);
  node._imageY = (init._imageY ?? 0) + (s.dy ?? 0);
  return { after: { _imageX: node._imageX, _imageY: node._imageY } };
};

// ─── Tail pipelines ──────────────────────────────────────────────────────

const step_tail_drag_apply = (s: OpState): Partial<OpState> => {
  if (s.op.kind !== 'tail_drag') return {};
  const node = resolveNode(s);
  if (!node) return {};
  node._tailX = (s.op.initial._tailX ?? 0) + (s.dx ?? 0);
  node._tailY = (s.op.initial._tailY ?? 0) + (s.dy ?? 0);
  delete (node as any)._tailAnchor;
  return { after: { _tailX: node._tailX, _tailY: node._tailY } };
};

const step_tail_anchor_apply = (s: OpState): Partial<OpState> => {
  if (s.op.kind !== 'tail_anchor') return {};
  const node = resolveNode(s);
  if (!node) return {};
  (node as any)._tailAnchor = { imageNid: s.op.imageNid, faceIdx: s.op.faceIdx };
  node._tailX = null;
  node._tailY = null;
  return { after: { _tailAnchor: (node as any)._tailAnchor } };
};

const step_tail_clear_apply = (s: OpState): Partial<OpState> => {
  if (s.op.kind !== 'tail_clear') return {};
  const node = resolveNode(s);
  if (!node) return {};
  node._tailX = null;
  node._tailY = null;
  delete (node as any)._tailAnchor;
  return { after: { _tailX: null, _tailY: null } };
};

// ─── Face pipelines ──────────────────────────────────────────────────────

const step_face_add_apply = (s: OpState): Partial<OpState> => {
  if (s.op.kind !== 'face_add') return {};
  const node = resolveNode(s);
  if (!node) return {};
  const faces = Array.isArray(node._faces) ? [...node._faces] : [];
  faces.push({ cx: s.op.cx, cy: s.op.cy });
  node._faces = faces;
  return { after: { _faces: faces } };
};

const step_face_clear_apply = (s: OpState): Partial<OpState> => {
  if (s.op.kind !== 'face_clear') return {};
  const node = resolveNode(s);
  if (!node) return {};
  node._faces = [];
  return { after: { _faces: [] } };
};

// ─── Add / Delete / Update props pipelines ───────────────────────────────

const step_add_node_apply = (s: OpState): Partial<OpState> => {
  if (s.op.kind === 'add_node') {
    s.ctx.addOverlay(s.op.node);
    return { after: { nid: s.op.node._nid } };
  } else if (s.op.kind === 'add_nodes') {
    for (const n of s.op.nodes) s.ctx.addOverlay(n);
    return { after: { nids: s.op.nodes.map((n) => n._nid) } };
  }
  return {};
};

const step_add_undo_push = (s: OpState): Partial<OpState> => {
  if (!s.ctx.pushUndo) return {};
  if (s.op.kind === 'add_node') {
    s.ctx.pushUndo({ op: 'add', nids: [s.op.node._nid || ''] });
  } else if (s.op.kind === 'add_nodes') {
    s.ctx.pushUndo({ op: 'add', nids: s.op.nodes.map((n) => n._nid || '') });
  }
  return {};
};

const step_delete_apply = (s: OpState): Partial<OpState> => {
  if (s.op.kind !== 'delete') return {};
  s.ctx.deleteNode(s.op.nid);
  return {};
};

const step_delete_undo_push = (s: OpState): Partial<OpState> => {
  if (s.op.kind !== 'delete' || !s.ctx.pushUndo) return {};
  s.ctx.pushUndo({ op: 'delete', node: s.before });
  return {};
};

const step_update_props_apply = (s: OpState): Partial<OpState> => {
  if (s.op.kind !== 'update_props') return {};
  const node = resolveNode(s);
  if (!node) return {};
  const patch = s.op.patch;
  for (const k of Object.keys(patch)) (node as Record<string, unknown>)[k] = patch[k];
  return { after: patch };
};

const step_update_undo_push = (s: OpState): Partial<OpState> => {
  if (s.op.kind !== 'update_props' || !s.ctx.pushUndo) return {};
  s.ctx.pushUndo({ op: 'edit', nid: s.op.nid, before: s.before, after: { ...s.op.patch } });
  return {};
};

// ─── Compile graphs (one per op kind) ────────────────────────────────────

function buildRectMutationGraph(
  name: string,
  applySteps: Array<{ name: string; fn: (s: OpState) => Partial<OpState> }>,
  opts: { cascade?: boolean; clamp?: boolean } = {},
) {
  let g = new StateGraph(OpStateAnnotation)
    .addNode('snapshot_before', step_snapshot_before)
    .addNode('compute_delta',   step_compute_delta);
  g = g.addEdge(START, 'snapshot_before' as any)
       .addEdge('snapshot_before' as any, 'compute_delta' as any);
  let prev: string = 'compute_delta';
  if (opts.cascade) {
    g = g.addNode('gather_cascade', step_resize_gather_cascade) as any;
    g = g.addEdge(prev as any, 'gather_cascade' as any) as any;
    prev = 'gather_cascade';
  }
  for (const st of applySteps) {
    g = g.addNode(st.name, st.fn) as any;
    g = g.addEdge(prev as any, st.name as any) as any;
    prev = st.name;
  }
  if (opts.cascade) {
    g = g.addNode('apply_cascade', step_resize_apply_cascade) as any;
    g = g.addEdge(prev as any, 'apply_cascade' as any) as any;
    prev = 'apply_cascade';
  }
  if (opts.clamp) {
    g = g.addNode('clamp', step_clamp_invariants) as any;
    g = g.addEdge(prev as any, 'clamp' as any) as any;
    prev = 'clamp';
  }
  g = g.addNode('redraw_save', step_redraw_and_save) as any;
  g = g.addNode('emit_op',     step_emit_op) as any;
  g = g.addEdge(prev as any, 'redraw_save' as any) as any;
  g = g.addEdge('redraw_save' as any, 'emit_op' as any) as any;
  g = g.addEdge('emit_op' as any, END) as any;
  return (g as any).compile({ name });
}

const RESIZE_GRAPH        = buildRectMutationGraph('resize',       [{ name: 'apply_rect',       fn: step_resize_apply_rect }],       { cascade: true, clamp: true });
const DRAG_GRAPH          = buildRectMutationGraph('drag',         [{ name: 'apply_drag',       fn: step_drag_apply }]);
const IMAGE_OFFSET_GRAPH  = buildRectMutationGraph('image_offset', [{ name: 'apply_offset',     fn: step_image_offset_apply }]);
const TAIL_DRAG_GRAPH     = buildRectMutationGraph('tail_drag',    [{ name: 'apply_tail_drag',  fn: step_tail_drag_apply }]);

function buildSimpleGraph(
  name: string,
  applySteps: Array<{ name: string; fn: (s: OpState) => Partial<OpState> }>,
  opts: { snapshot?: boolean; undoStep?: (s: OpState) => Partial<OpState> } = {},
) {
  let g = new StateGraph(OpStateAnnotation);
  let prev: string = START as unknown as string;
  if (opts.snapshot) {
    g = g.addNode('snapshot_before', step_snapshot_before) as any;
    g = (g as any).addEdge(prev, 'snapshot_before');
    prev = 'snapshot_before';
  }
  for (const st of applySteps) {
    g = g.addNode(st.name, st.fn) as any;
    g = (g as any).addEdge(prev, st.name);
    prev = st.name;
  }
  if (opts.undoStep) {
    g = g.addNode('undo_push', opts.undoStep) as any;
    g = (g as any).addEdge(prev, 'undo_push');
    prev = 'undo_push';
  }
  g = g.addNode('redraw_save', step_redraw_and_save) as any;
  g = g.addNode('emit_op',     step_emit_op) as any;
  g = (g as any).addEdge(prev, 'redraw_save');
  g = (g as any).addEdge('redraw_save', 'emit_op');
  g = (g as any).addEdge('emit_op', END);
  return (g as any).compile({ name });
}

const TAIL_ANCHOR_GRAPH = buildSimpleGraph('tail_anchor', [{ name: 'apply', fn: step_tail_anchor_apply }], { snapshot: true });
const TAIL_CLEAR_GRAPH  = buildSimpleGraph('tail_clear',  [{ name: 'apply', fn: step_tail_clear_apply  }], { snapshot: true });
const FACE_ADD_GRAPH    = buildSimpleGraph('face_add',    [{ name: 'apply', fn: step_face_add_apply    }], { snapshot: true });
const FACE_CLEAR_GRAPH  = buildSimpleGraph('face_clear',  [{ name: 'apply', fn: step_face_clear_apply  }], { snapshot: true });
const ADD_GRAPH         = buildSimpleGraph('add',         [{ name: 'apply', fn: step_add_node_apply    }], { undoStep: step_add_undo_push });
const DELETE_GRAPH      = buildSimpleGraph('delete',      [{ name: 'apply', fn: step_delete_apply      }], { snapshot: true, undoStep: step_delete_undo_push });
const UPDATE_PROPS_GRAPH = buildSimpleGraph('update_props',[{ name: 'apply', fn: step_update_props_apply}], { snapshot: true, undoStep: step_update_undo_push });

const OP_GRAPHS: Record<CanvasOp['kind'], any> = {
  resize:        RESIZE_GRAPH,
  drag:          DRAG_GRAPH,
  image_offset:  IMAGE_OFFSET_GRAPH,
  tail_drag:     TAIL_DRAG_GRAPH,
  tail_anchor:   TAIL_ANCHOR_GRAPH,
  tail_clear:    TAIL_CLEAR_GRAPH,
  face_add:      FACE_ADD_GRAPH,
  face_clear:    FACE_CLEAR_GRAPH,
  add_node:      ADD_GRAPH,
  add_nodes:     ADD_GRAPH,
  delete:        DELETE_GRAPH,
  update_props:  UPDATE_PROPS_GRAPH,
};

/** Run a canvas op through its LangGraph Pregel pipeline.
 *
 * Fire-and-forget: the graph mutates `ctx` synchronously through each super-
 * step. We return a promise but callers don't have to await it — the mutation
 * is already visible on `ctx` once the super-steps run. We await internally so
 * LangGraph's checkpointer can resolve.
 */
export async function runCanvasOp(op: CanvasOp, ctx: CanvasCtx): Promise<void> {
  const graph = OP_GRAPHS[op.kind];
  if (!graph) throw new Error(`unknown canvas op: ${(op as any).kind}`);
  // graph.invoke({ op, ctx }) — channels propagate through super-steps.
  await graph.invoke({ op, ctx });
}

export const __PREGEL_GRAPH_NAMES = Object.keys(OP_GRAPHS) as Array<CanvasOp['kind']>;
