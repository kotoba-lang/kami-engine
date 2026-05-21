/** KAMI Engine Genko SDK — manga editor canvas (WebGPU). */

/** Legacy HTML embed (monolithic template, kept for non-Svelte deploys) */
export { genkoEmbedHTML } from './genko-embed.js';

/** Svelte components */
export { default as Genko } from './components/Genko.svelte';
export { default as NodeTree } from './components/NodeTree.svelte';
export { default as NodeRow } from './components/NodeRow.svelte';
export { default as Canvas } from './components/Canvas.svelte';

/** Stores */
export {
  type GenkoDoc, type GenkoPage, type GenkoNode, type TreeNode,
  getDoc, setDoc, getStrokes, getOverlays,
  getSelectedIdx, setSelectedIdx,
  getCollapsedNodes, getContextNodes,
  activePage, saveCurrentPage, loadPage,
  computeTreeNodes, findByNid,
  toggleNodeVisibility, deleteNode, addOverlay,
  toggleCollapsed, toggleContext,
  agentColor, agentInitials,
  nid, pid, requestRedraw, consumeRedraw,
  isInitDone, setInitDone,
} from './stores/doc.svelte.js';
