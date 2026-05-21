/**
 * Genko canvas store — drawing/brush/viewport state.
 * Svelte 5 runes ($state).
 */

// --- Types ---
export type BrushType = 'fine' | 'pen' | 'marker' | 'brush' | 'flat' | 'eraser';
export type CanvasMode = 'draw' | 'select' | 'panel' | 'tone' | 'fukidashi' | 'text';
export type BrushColor = [number, number, number, number];

export interface YoushiTemplate {
  id: string;
  name: string;
  width: number;
  height: number;
  drawFunction: boolean;
}

// --- Youshi templates ---
export const YOUSHI: Record<string, YoushiTemplate> = {
  b4manga: { id: 'b4manga', name: 'B4 Manga', width: 257, height: 364, drawFunction: true },
  b4koma:  { id: 'b4koma',  name: 'B4 4-Koma', width: 257, height: 364, drawFunction: true },
  none:    { id: 'none',    name: 'Free Canvas', width: 1920, height: 1080, drawFunction: false },
};

// --- Reactive state ---
let _activeBrush = $state<BrushType>('fine');
let _activeMode = $state<CanvasMode>('draw');
let _brushColor = $state<BrushColor>([0.2, 0.2, 0.2, 1]);
let _brushSize = $state(2);
let _brushOpacity = $state(1);
let _zoom = $state(1);
let _panX = $state(0);
let _panY = $state(0);
let _activeYoushi = $state('b4manga');

// --- Brush accessors ---
export function getActiveBrush(): BrushType { return _activeBrush; }
export function setActiveBrush(b: BrushType) { _activeBrush = b; }

export function getActiveMode(): CanvasMode { return _activeMode; }
export function setActiveMode(m: CanvasMode) { _activeMode = m; }

export function getBrushColor(): BrushColor { return _brushColor; }
export function setBrushColor(c: BrushColor) { _brushColor = c; }

export function getBrushSize(): number { return _brushSize; }
export function setBrushSize(s: number) { _brushSize = s; }

export function getBrushOpacity(): number { return _brushOpacity; }
export function setBrushOpacity(o: number) { _brushOpacity = o; }

// --- Viewport accessors ---
export function getZoom(): number { return _zoom; }
export function setZoom(z: number) { _zoom = z; }

export function getPanX(): number { return _panX; }
export function setPanX(x: number) { _panX = x; }

export function getPanY(): number { return _panY; }
export function setPanY(y: number) { _panY = y; }

// --- Youshi accessors ---
export function getActiveYoushi(): string { return _activeYoushi; }
export function setActiveYoushi(id: string) { _activeYoushi = id; }

export function getYoushiTemplate(): YoushiTemplate {
  return YOUSHI[_activeYoushi] || YOUSHI.b4manga;
}

// --- Convenience ---
export function resetViewport(): void {
  _zoom = 1;
  _panX = 0;
  _panY = 0;
}

export function resetBrush(): void {
  _activeBrush = 'fine';
  _brushColor = [0.2, 0.2, 0.2, 1];
  _brushSize = 2;
  _brushOpacity = 1;
}
