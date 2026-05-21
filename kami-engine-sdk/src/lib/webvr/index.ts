/**
 * @gftdcojp/kami-engine-sdk/webvr — Choice-based VR incident-response
 * walkthrough runtime for smartphone WebXR.
 *
 * Public surface:
 *   - Types: IncidentScenario, IncidentNode, IncidentChoice, IncidentKpi,
 *     IncidentState, IncidentDecisionLog, LocationKind, Stage, Severity
 *   - Pregel: INCIDENT_GRAPH (compiled LangGraph), applySelection, initialState
 *   - Renderer: mountIncidentScene, SceneDescriptor
 *   - Builder: createIncidentVrEngine (Svelte 5 runes)
 *
 * Scenarios themselves are NOT exported from this SDK — the SDK provides
 * the engine; downstream apps supply the scenario data.
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
} from './types.js';

export { ZERO_KPI, applyKpiDelta } from './types.js';

export {
  INCIDENT_GRAPH,
  applySelection,
  initialState,
  type SceneDescriptor,
  type IncidentBridge,
} from './incident-pregel.js';

export { mountIncidentScene, type MountOpts, type SceneHandle } from './webvr-scene.js';

export {
  createIncidentVrEngine,
  type CreateIncidentVrEngineOpts,
  type IncidentVrEngine,
} from './createIncidentVrEngine.svelte.js';

// kami-cine Stage 1-4 bridge (worldModel → usdScene → neuralGeom →
// temporalField). Use createMockCineBridge() in dev; createCineBridge({
// endpoint, token }) against studio.gftd.ai in prod.
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
