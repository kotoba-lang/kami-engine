/**
 * KAMI Document Types — generic document model for structured document editing.
 *
 * Provides a common type system for document editors (PPTX, PDF, etc.)
 * that maps to KAMI Engine scene graph and the platform graph persistence layer.
 *
 * EMU (English Metric Unit) = 1/914400 inch. Used as the canonical coordinate system
 * for document layouts (inherited from OOXML, compatible with PDF points via conversion).
 */

/** EMU (English Metric Unit) = 1/914400 inch. Canonical document coordinate unit. */
export type Emu = number;

/** Conversion constants for EMU. */
export const EMU_PER_INCH = 914400;
export const EMU_PER_PT = 12700;
export const PX_PER_INCH = 96;
export const EMU_TO_PX = PX_PER_INCH / EMU_PER_INCH;

/** Convert EMU to pixels at given scale. */
export function emuToPx(emu: Emu, scale: number = 1): number {
  return emu * EMU_TO_PX * scale;
}

/** Convert pixels to EMU at given scale. */
export function pxToEmu(px: number, scale: number = 1): number {
  return px / EMU_TO_PX / scale;
}

/** Convert EMU to points (1 pt = 1/72 inch). */
export function emuToPt(emu: Emu): number {
  return emu / EMU_PER_PT;
}

/** A positioned, sized rectangle in EMU coordinate space. */
export interface DocumentRect {
  x: Emu;
  y: Emu;
  w: Emu;
  h: Emu;
}

/** Shape geometry types supported by KAMI Engine rendering. */
export type ShapeGeometry =
  | "rect"
  | "ellipse"
  | "roundRect"
  | "triangle"
  | "arrow"
  | "line"
  | "freeform"
  | "textBox";

/** A document element (shape, image, etc.) with position and visual properties. */
export interface DocumentElement extends DocumentRect {
  id: string;
  parentId: string;
  type: ShapeGeometry;
  name: string;
  rotation: number;
  fill: string | null;
  stroke: string | null;
  strokeWidth: number;
}

/** Text formatting for a single text run. */
export interface TextRunStyle {
  bold: boolean;
  italic: boolean;
  underline: boolean;
  size: number;
  color: string;
  font: string;
}

/** A contiguous run of text with uniform formatting. */
export interface DocumentTextRun extends TextRunStyle {
  text: string;
}

/** A paragraph containing multiple text runs. */
export interface DocumentParagraph {
  level: number;
  spacing: number;
  align: "left" | "center" | "right" | "justify";
  runs: DocumentTextRun[];
}

/** Text body attached to a document element. */
export interface DocumentTextBody {
  align: "left" | "center" | "right" | "justify";
  verticalAlign: "top" | "middle" | "bottom";
  paragraphs: DocumentParagraph[];
}

/** An image element in a document. */
export interface DocumentImage extends DocumentRect {
  id: string;
  parentId: string;
  blob: Uint8Array | null;
  mime: string;
}

/** A single page/slide in a document. */
export interface DocumentPage {
  id: string;
  order: number;
  layoutRef: string;
  background: string | null;
  elements: DocumentElement[];
  images: DocumentImage[];
}

/** A complete document (presentation, PDF, etc.). */
export interface Document {
  id: string;
  title: string;
  width: Emu;
  height: Emu;
  pages: DocumentPage[];
}

/** Color theme for a document. */
export interface DocumentTheme {
  name: string;
  colors: Record<string, string>;
}
