/**
 * @etzhayyim/kami-engine-sdk/webvr — Headless choice-based incident-response
 * runtime (state + KPI math + decision log + kami-cine bridge).
 *
 * The renderer is intentionally NOT included — kami-engine-sdk is
 * three.js-free as of 2026-05-26, and the canonical scene surface is a
 * `kami-app-{game}` wgpu crate (see 40-engine/kami-engine CLAUDE.md
 * "独自レンダラ禁止 — kami-render wgpu PBR pipeline が唯一"). Pass an
 * `onScene` callback to `createIncidentVrEngine` and drive your own
 * surface from there.
 *
 * Public surface:
 *   - Types: IncidentScenario, IncidentNode, IncidentChoice, IncidentKpi,
 *     IncidentState, IncidentDecisionLog, LocationKind, Stage, Severity
 *   - Pregel: INCIDENT_GRAPH (compiled LangGraph), applySelection, initialState
 *   - Engine: createIncidentVrEngine (Svelte 5 runes, headless)
 *   - Cine bridge: createCineBridge / createMockCineBridge
 */

export type {
  IncidentScenario,
  IncidentNode,
  IncidentChoice,
  IncidentKpi,
  IncidentState,
  IncidentDecisionLog,
  LocationKind,
  Stage,
  Severity,
  NodeEffectKind,
} from './types.js';

export { ZERO_KPI, applyKpiDelta } from './types.js';

export {
  INCIDENT_GRAPH,
  applySelection,
  initialState,
  type SceneDescriptor,
  type IncidentBridge,
} from './incident-pregel.js';

export {
  createIncidentVrEngine,
  type CreateIncidentVrEngineOpts,
  type IncidentVrEngine,
} from './createIncidentVrEngine.svelte.js';

// kami-cine Stage 1-4 bridge (worldModel → usdScene → neuralGeom →
// temporalField). Use createMockCineBridge() in dev; createCineBridge({
// endpoint, token }) against studio.etzhayyim.com in prod.
export {
  createCineBridge,
  createMockCineBridge,
  type CineBridge,
  type CreateCineBridgeOpts,
  type CineSceneInput,
  type CineSceneArtifacts,
  type CineWorldArtifact,
  type CineUsdArtifact,
  type CineGeomArtifact,
  type CineTemporalArtifact,
} from './cine-bridge.js';
