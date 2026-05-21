/**
 * KAMI Document Scene Bridge — converts document model to KAMI Engine scene JSON.
 *
 * Provides WebGPU detection, KAMI WASM loading, and document → scene conversion
 * for rendering documents via kami-web's `run_with_scene()`.
 *
 * Usage:
 * ```ts
 * import { checkWebGPU, loadKamiWasm, documentPageToScene, renderPageKami } from '@gftdcojp/kami-engine-sdk/document';
 *
 * const hasGpu = await checkWebGPU();
 * const kamiReady = await loadKamiWasm();
 * if (kamiReady) {
 *   await renderPageKami('canvas-id', page, doc);
 * }
 * ```
 */

import type {
  Document,
  DocumentPage,
  DocumentElement,
  ShapeGeometry,
} from "./types.js";
import { EMU_PER_INCH } from "./types.js";

// ---------------------------------------------------------------------------
// WebGPU Detection
// ---------------------------------------------------------------------------

/** Check if WebGPU is available in the current browser. */
export async function checkWebGPU(): Promise<boolean> {
  if (typeof navigator === "undefined" || !navigator.gpu) return false;
  try {
    const adapter = await navigator.gpu.requestAdapter();
    return adapter !== null;
  } catch {
    return false;
  }
}

/** Get GPU adapter info string (vendor, architecture, device). */
export async function getGPUInfo(): Promise<string | null> {
  if (typeof navigator === "undefined" || !navigator.gpu) return null;
  try {
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) return null;
    const infoFn = (adapter as GPUAdapter & { requestAdapterInfo?: () => Promise<GPUAdapterInfo> }).requestAdapterInfo;
    if (!infoFn) return null;
    const info = await infoFn.call(adapter);
    return `${info.vendor} ${info.architecture} (${info.device})`;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// KAMI WASM Module
// ---------------------------------------------------------------------------

/** Dynamic import type for kami_web WASM module. */
interface KamiWebModule {
  default: () => Promise<void>;
  run_with_scene: (canvasId: string, sceneJson: string) => Promise<void>;
}

let kamiModule: KamiWebModule | null = null;

/**
 * Load KAMI Engine WASM module from the given URL.
 *
 * @param wasmUrl - URL to kami_web.js entry (default: `/pkg/kami_web.js`)
 * @returns true if loaded successfully
 */
export async function loadKamiWasm(wasmUrl: string = "/pkg/kami_web.js"): Promise<boolean> {
  if (kamiModule) return true;
  try {
    const mod = await import(/* @vite-ignore */ wasmUrl) as KamiWebModule;
    await mod.default();
    kamiModule = mod;
    return true;
  } catch {
    console.warn("[kami-document] KAMI engine WASM not available at", wasmUrl);
    return false;
  }
}

/** Check if KAMI WASM is loaded and ready. */
export function isKamiReady(): boolean {
  return kamiModule !== null;
}

// ---------------------------------------------------------------------------
// Document → KAMI Scene Conversion
// ---------------------------------------------------------------------------

/**
 * Convert a document page to KAMI scene JSON string.
 *
 * KAMI scene format uses world units (1 unit = 1 inch) with orthographic camera.
 * Elements are positioned in a 2D plane (Z=0) with the camera looking down.
 */
export function documentPageToScene(
  page: DocumentPage,
  doc: Document,
  textExtractor?: (element: DocumentElement) => string | null,
): string {
  const emuToWorld = 1 / EMU_PER_INCH;
  const objects: Record<string, unknown>[] = [];

  // Background plane
  const bgColor = page.background ? hexToRgba(page.background) : [1, 1, 1, 1];
  objects.push({
    type: "rect",
    position: [0, 0, -0.01],
    size: [doc.width * emuToWorld, doc.height * emuToWorld],
    color: bgColor,
  });

  // Elements (shapes)
  for (const el of page.elements) {
    const cx = (el.x + el.w / 2) * emuToWorld;
    const cy = (el.y + el.h / 2) * emuToWorld;
    const w = el.w * emuToWorld;
    const h = el.h * emuToWorld;
    const fill = el.fill ? hexToRgba(el.fill) : [0.27, 0.45, 0.77, 1];

    const obj: Record<string, unknown> = {
      type: mapGeometryToKami(el.type),
      position: [cx, cy, 0],
      size: [w, h],
      color: fill,
      rotation: el.rotation,
    };

    // Extract text if available
    if (textExtractor) {
      const text = textExtractor(el);
      if (text) obj.text = text;
    }

    objects.push(obj);
  }

  const scene = {
    camera: {
      position: [doc.width * emuToWorld / 2, doc.height * emuToWorld / 2, 10],
      target: [doc.width * emuToWorld / 2, doc.height * emuToWorld / 2, 0],
      orthographic: true,
    },
    objects,
  };

  return JSON.stringify(scene);
}

/**
 * Render a document page using KAMI Engine WebGPU.
 *
 * @returns true if rendered via KAMI, false if not available
 */
export async function renderPageKami(
  canvasId: string,
  page: DocumentPage,
  doc: Document,
  textExtractor?: (element: DocumentElement) => string | null,
): Promise<boolean> {
  if (!kamiModule) return false;

  try {
    const sceneJson = documentPageToScene(page, doc, textExtractor);
    await kamiModule.run_with_scene(canvasId, sceneJson);
    return true;
  } catch (err) {
    console.warn("[kami-document] KAMI render failed:", err);
    return false;
  }
}

// ---------------------------------------------------------------------------
// Hit Testing (viewport-agnostic)
// ---------------------------------------------------------------------------

/**
 * Compute the scale factor to fit a document into a viewport.
 *
 * @param docWidth - Document width in EMU
 * @param docHeight - Document height in EMU
 * @param viewportWidth - Viewport width in pixels
 * @param viewportHeight - Viewport height in pixels
 * @param padding - Padding in pixels
 */
export function computeFitScale(
  docWidth: number,
  docHeight: number,
  viewportWidth: number,
  viewportHeight: number,
  padding: number = 20,
): number {
  const pxPerEmu = 96 / EMU_PER_INCH;
  const pxW = docWidth * pxPerEmu;
  const pxH = docHeight * pxPerEmu;
  const scaleX = (viewportWidth - padding * 2) / pxW;
  const scaleY = (viewportHeight - padding * 2) / pxH;
  return Math.min(scaleX, scaleY, 1);
}

/**
 * Hit-test document elements at viewport coordinates.
 *
 * @returns The topmost element at the given coordinates, or null.
 */
export function hitTestElements(
  elements: DocumentElement[],
  docWidth: number,
  docHeight: number,
  canvasX: number,
  canvasY: number,
  canvasWidth: number,
  canvasHeight: number,
  scale: number,
): DocumentElement | null {
  const pxPerEmu = 96 / EMU_PER_INCH;
  const w = docWidth * pxPerEmu * scale;
  const h = docHeight * pxPerEmu * scale;
  const offsetX = (canvasWidth - w) / 2;
  const offsetY = (canvasHeight - h) / 2;
  const sx = canvasX - offsetX;
  const sy = canvasY - offsetY;

  for (let i = elements.length - 1; i >= 0; i--) {
    const el = elements[i];
    const ex = el.x * pxPerEmu * scale;
    const ey = el.y * pxPerEmu * scale;
    const ew = el.w * pxPerEmu * scale;
    const eh = el.h * pxPerEmu * scale;

    if (sx >= ex && sx <= ex + ew && sy >= ey && sy <= ey + eh) {
      return el;
    }
  }

  return null;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Convert hex color string (RRGGBB) to RGBA array [0..1]. */
function hexToRgba(hex: string): [number, number, number, number] {
  const r = parseInt(hex.slice(0, 2), 16) / 255;
  const g = parseInt(hex.slice(2, 4), 16) / 255;
  const b = parseInt(hex.slice(4, 6), 16) / 255;
  return [r, g, b, 1];
}

/** Map document shape geometry to KAMI scene object type. */
function mapGeometryToKami(type: ShapeGeometry): string {
  switch (type) {
    case "ellipse": return "ellipse";
    case "roundRect": return "rounded_rect";
    case "line": return "line";
    case "triangle": return "triangle";
    default: return "rect";
  }
}
