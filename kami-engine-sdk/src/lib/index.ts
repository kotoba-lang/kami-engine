/** @gftdcojp/kami-engine-sdk — Svelte 5 VRM character viewer components + headless builders + Genko manga editor. */

// Components
export {
  VrmCanvas,
  VrmViewer,
  ExpressionPanel,
  PosePanel,
  MotionPanel,
  PartPicker,
  VoicePanel,
  EmotionBars,
} from './components/index.js';

// Builders (re-exported for convenience)
export {
  createVrmEngine,
  createMorphController,
  createBoneController,
  createMotionPlayer,
  createPartComposer,
  createVoiceSynth,
  createEmotionAnalyzer,
} from './builders/index.js';

// Types (re-exported for convenience)
export type {
  DualEngineState,
  KamiWasmExports,
  ThreeVrmHandle,
  EngineCapabilities,
  EngineMemoryBudget,
} from './types/engine.js';
export { KAMI_ENGINE_SDK_DEFAULT_MAX_RAM_BYTES } from './types/engine.js';
export type { HumanoidBoneName, RotationAxis, JointLimit, PosePreset } from './types/bone.js';
export type { MorphCategory, MorphTargetDef, ExpressionPreset } from './types/morph.js';
export type { MotionKey, MotionPreset } from './types/motion.js';
export type { PartCategory, HairStyle, PartColor, OutfitStyle } from './types/part.js';
export type { EmotionAxis, EmotionScores, VoiceType } from './types/voice.js';

// Genko (manga editor canvas)
export { genkoEmbedHTML } from './genko/index.js';

// Trackpad (Apple trackpad + mouse gesture unification)
export { kamiTrackpadHTML } from './trackpad/index.js';

// Gsplat (3D Gaussian Splatting preview/QC bridge for maps.gftd.ai)
export {
  loadGsplatAsset,
  listGsplatAssets,
  pushToWasm,
  removeFromWasm,
  bakeGsplatAsset,
} from './gsplat/index.js';
export type {
  GsplatAssetMeta,
  GetGsplatAssetResponse,
  ListGsplatAssetsResponse,
  GsplatWasmExports,
  FetchedGsplatAsset,
} from './gsplat/index.js';

// Manufacturing (3D factory cell, CAM output, robot/material-handling planning)
export {
  createAutonomyOperationPlan,
  createIndustrialSoftwareIntegrationPlan,
  createLogisticsRoutePlan,
  createManufacturingCellPlan,
  createManufacturingOutputPlan,
  createRoboticsApprovalRecord,
  createRoboticsMissionStatus,
  createRoboticsKamiReviewHtml,
  createRoboticsTelemetrySchema,
  createRoboticsWorkProcessPlan,
  closeRoboticsFulfillment,
  ingestRoboticsTelemetry,
  simulateRoboticsMission,
} from './manufacturing/index.js';
export type {
  AutonomyOperationPlan,
  IndustrialSoftwareDomain,
  IndustrialSoftwareIntegrationPlan,
  LogisticsAssetKind,
  LogisticsRoutePlan,
  LogisticsWaypoint,
  ManufacturingCellPlan,
  ManufacturingDeviceKind,
  ManufacturingDeviceRequest,
  ManufacturingOutputPlan,
  ManufacturingOutputRequest,
  ManufacturingPartEnvelope,
  ManufacturingSceneNode,
  ManufacturingWorkflowStep,
  RoboticsApprovalRecord,
  RoboticsBusinessProcess,
  RoboticsFulfillmentClose,
  RoboticsMissionStatus,
  RoboticsMissionSimulation,
  RoboticsProcessDependency,
  RoboticsProcessForm,
  RoboticsTelemetryFrame,
  RoboticsTelemetrySchema,
  RoboticsWorkProcessPlan,
} from './manufacturing/index.js';

// Document (structured document model + KAMI scene bridge)
// Use `import { ... } from '@gftdcojp/kami-engine-sdk/document'` for tree-shaking
export type {
  Emu,
  DocumentRect,
  ShapeGeometry,
  DocumentElement,
  DocumentTextRun,
  DocumentParagraph,
  DocumentTextBody,
  DocumentImage,
  DocumentPage,
  Document,
  DocumentTheme,
} from './document/index.js';

// WebVR (choice-based VR incident-response walkthrough — smartphone WebXR)
// Use `import { ... } from '@gftdcojp/kami-engine-sdk/webvr'` for tree-shaking
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
  SceneDescriptor,
  IncidentBridge,
  CreateIncidentVrEngineOpts,
  IncidentVrEngine,
  MountOpts,
  SceneHandle,
} from './webvr/index.js';
export {
  ZERO_KPI,
  applyKpiDelta,
  INCIDENT_GRAPH,
  applySelection,
  initialState,
  mountIncidentScene,
  createIncidentVrEngine,
  createCineBridge,
  createMockCineBridge,
} from './webvr/index.js';
export type {
  CineBridge,
  CreateCineBridgeOpts,
  CineSceneInput,
  CineSceneArtifacts,
  CineWorldArtifact,
  CineUsdArtifact,
  CineGeomArtifact,
  CineTemporalArtifact,
} from './webvr/index.js';
