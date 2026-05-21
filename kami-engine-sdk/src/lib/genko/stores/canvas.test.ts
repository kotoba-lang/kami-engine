import { describe, it, expect } from 'vitest';
import {
  getActiveBrush, setActiveBrush,
  getActiveMode, setActiveMode,
  getBrushColor, setBrushColor,
  getBrushSize, setBrushSize,
  getZoom, setZoom, getPanX, setPanX, getPanY, setPanY,
  getActiveYoushi, setActiveYoushi,
  YOUSHI, resetViewport, resetBrush,
} from './canvas.svelte';

describe('canvas store', () => {
  it('defaults are correct', () => {
    expect(getActiveBrush()).toBe('fine');
    expect(getActiveMode()).toBe('draw');
    expect(getBrushSize()).toBe(2);
    expect(getZoom()).toBe(1);
    expect(getActiveYoushi()).toBe('b4manga');
  });

  it('brush type changes', () => {
    setActiveBrush('marker');
    expect(getActiveBrush()).toBe('marker');
    setActiveBrush('fine');
  });

  it('mode changes', () => {
    setActiveMode('panel');
    expect(getActiveMode()).toBe('panel');
    setActiveMode('draw');
  });

  it('color is RGBA tuple', () => {
    setBrushColor([1, 0, 0, 1]);
    expect(getBrushColor()).toEqual([1, 0, 0, 1]);
    setBrushColor([0.2, 0.2, 0.2, 1]);
  });

  it('zoom/pan management', () => {
    setZoom(2.5);
    setPanX(100);
    setPanY(200);
    expect(getZoom()).toBe(2.5);
    expect(getPanX()).toBe(100);
    expect(getPanY()).toBe(200);
    resetViewport();
    expect(getZoom()).toBe(1);
    expect(getPanX()).toBe(0);
    expect(getPanY()).toBe(0);
  });

  it('YOUSHI templates exist', () => {
    expect(YOUSHI.b4manga).toBeDefined();
    expect(YOUSHI.b4manga.width).toBe(257);
    expect(YOUSHI.b4manga.height).toBe(364);
    expect(YOUSHI.b4koma).toBeDefined();
    expect(YOUSHI.none.drawFunction).toBe(false);
  });

  it('resetBrush restores defaults', () => {
    setActiveBrush('eraser');
    setBrushSize(10);
    resetBrush();
    expect(getActiveBrush()).toBe('fine');
    expect(getBrushSize()).toBe(2);
  });
});
