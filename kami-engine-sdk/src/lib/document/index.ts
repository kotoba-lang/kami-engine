/** @gftdcojp/kami-engine-sdk/document — Document model types + KAMI Engine scene bridge. */

export type {
  Emu,
  DocumentRect,
  ShapeGeometry,
  DocumentElement,
  TextRunStyle,
  DocumentTextRun,
  DocumentParagraph,
  DocumentTextBody,
  DocumentImage,
  DocumentPage,
  Document,
  DocumentTheme,
} from "./types.js";

export {
  EMU_PER_INCH,
  EMU_PER_PT,
  PX_PER_INCH,
  EMU_TO_PX,
  emuToPx,
  pxToEmu,
  emuToPt,
} from "./types.js";

export {
  checkWebGPU,
  getGPUInfo,
  loadKamiWasm,
  isKamiReady,
  documentPageToScene,
  renderPageKami,
  computeFitScale,
  hitTestElements,
} from "./scene-bridge.js";
