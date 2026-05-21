export type ManufacturingDeviceKind =
  | "cnc-mill"
  | "cnc-lathe"
  | "3d-printer"
  | "robot-arm"
  | "material-handling"
  | "inspection";

export interface ManufacturingPartEnvelope {
  widthMm: number;
  heightMm: number;
  depthMm: number;
  material?: string;
  process?: string;
}

export interface ManufacturingDeviceRequest {
  id?: string;
  kind: ManufacturingDeviceKind;
  name?: string;
  workEnvelopeMm?: [number, number, number];
}

export interface ManufacturingSceneNode {
  id: string;
  kind: ManufacturingDeviceKind | "stock" | "fixture" | "workpiece" | "safety-zone";
  label: string;
  position: [number, number, number];
  rotation: [number, number, number];
  scale: [number, number, number];
  color: string;
}

export interface ManufacturingWorkflowStep {
  id: string;
  deviceId: string;
  operation: string;
  input: string;
  output: string;
  estimatedSeconds: number;
}

export interface ManufacturingCellPlan {
  cellId: string;
  sceneUnits: "millimeters";
  partEnvelope: ManufacturingPartEnvelope;
  devices: ManufacturingDeviceRequest[];
  scene: ManufacturingSceneNode[];
  workflow: ManufacturingWorkflowStep[];
  safetyZones: ManufacturingSceneNode[];
  integrations: string[];
}

export interface ManufacturingOutputRequest {
  deviceKind: ManufacturingDeviceKind;
  partEnvelope: ManufacturingPartEnvelope;
  operation?: string;
  programNumber?: number;
}

export interface ManufacturingOutputPlan {
  deviceKind: ManufacturingDeviceKind;
  operation: string;
  postProcessor: "fanuc" | "marlin" | "robot-json" | "material-flow-json" | "inspection-json";
  previewScene: ManufacturingSceneNode[];
  program: string;
  estimatedSeconds: number;
  warnings: string[];
}

export type IndustrialSoftwareDomain =
  | "requirements"
  | "cad"
  | "cae"
  | "cam"
  | "plm"
  | "mes"
  | "scada"
  | "qms"
  | "wms"
  | "tms"
  | "erp"
  | "cmms"
  | "digital-twin"
  | "route-planning"
  | "fleet-control"
  | "drone-control"
  | "autonomous-driving";

export interface IndustrialSoftwareNode {
  id: string;
  domain: IndustrialSoftwareDomain;
  label: string;
  role: string;
  records: string[];
  upstream: string[];
  downstream: string[];
}

export interface IndustrialSoftwareIntegrationPlan {
  planId: string;
  scope: string;
  nodes: IndustrialSoftwareNode[];
  dataContracts: Array<{
    id: string;
    from: string;
    to: string;
    payload: string;
    cadence: "realtime" | "event" | "batch";
  }>;
  controlBoundaries: string[];
  canonicalRecords: string[];
}

export type LogisticsAssetKind =
  | "forklift"
  | "agv"
  | "conveyor"
  | "truck"
  | "rail"
  | "ship"
  | "air-cargo"
  | "drone"
  | "autonomous-vehicle";

export interface LogisticsWaypoint {
  id: string;
  label: string;
  lat?: number;
  lng?: number;
  xMm?: number;
  yMm?: number;
  zMm?: number;
}

export interface LogisticsRoutePlan {
  routeId: string;
  assetKind: LogisticsAssetKind;
  mode: "intra-factory" | "yard" | "road" | "air" | "sea" | "rail" | "multimodal";
  waypoints: LogisticsWaypoint[];
  segments: Array<{
    id: string;
    from: string;
    to: string;
    distanceMeters: number;
    estimatedSeconds: number;
    constraints: string[];
  }>;
  estimatedDistanceMeters: number;
  estimatedSeconds: number;
  handoffRecords: string[];
  warnings: string[];
}

export interface AutonomyOperationPlan {
  operationId: string;
  assetKind: "drone" | "autonomous-vehicle" | "agv" | "robot-arm";
  missionType: "survey" | "delivery" | "inspection" | "line-feeding" | "machine-tending" | "yard-transfer";
  commandProtocol: "mavlink-json" | "ros2-action-json" | "vda5050-json" | "robot-waypoint-json";
  safetyEnvelope: {
    geofenceMeters: number;
    maxSpeedMps: number;
    requiresHumanApproval: boolean;
  };
  commands: Array<{
    id: string;
    command: string;
    params: Record<string, unknown>;
  }>;
  telemetryTopics: string[];
  emergencyProcedures: string[];
}

export type RoboticsBusinessProcess =
  | "sales"
  | "requirements"
  | "engineering"
  | "procurement"
  | "production-planning"
  | "manufacturing"
  | "quality"
  | "warehouse"
  | "transport"
  | "installation"
  | "maintenance"
  | "finance";

export interface RoboticsProcessForm {
  id: string;
  process: RoboticsBusinessProcess;
  title: string;
  requiredFields: string[];
  outputRecords: string[];
}

export interface RoboticsProcessDependency {
  id: string;
  from: RoboticsBusinessProcess;
  to: RoboticsBusinessProcess;
  records: string[];
  gate: string;
}

export interface RoboticsWorkProcessPlan {
  planId: string;
  scope: string;
  forms: RoboticsProcessForm[];
  dependencies: RoboticsProcessDependency[];
  missingPrerequisites: RoboticsProcessDependency[];
  bpmnProcesses: string[];
  mcpTools: string[];
  kamiReview: {
    sceneNodes: ManufacturingSceneNode[];
    operatorActions: string[];
    telemetryTopics: string[];
  };
  integrationRecords: string[];
  approvalGates: string[];
}

export interface RoboticsTelemetrySchema {
  schemaId: string;
  topics: Array<{
    topic: string;
    requiredFields: string[];
    retention: "hot-24h" | "audit-7y";
  }>;
  stateEnums: Record<string, string[]>;
}

export interface RoboticsApprovalRecord {
  approvalId: string;
  requestId: string;
  decision: "approve" | "reject" | "hold";
  approverDid: string;
  approvedAt: string;
  scope: string;
  requiredEvidence: string[];
  auditAction: string;
}

export interface RoboticsMissionSimulation {
  simulationId: string;
  missionId: string;
  status: "pass" | "review" | "fail";
  checks: Array<{
    id: string;
    status: "pass" | "review" | "fail";
    detail: string;
  }>;
  estimatedSeconds: number;
  requiresHumanApproval: boolean;
}

export interface RoboticsTelemetryFrame {
  frameId: string;
  topic: string;
  accepted: boolean;
  missingFields: string[];
  payload: Record<string, unknown>;
  receivedAt: string;
}

export interface RoboticsMissionStatus {
  missionId: string;
  state: "planned" | "simulated" | "approved" | "running" | "completed" | "blocked";
  blockers: string[];
  evidence: string[];
  nextActions: string[];
}

export interface RoboticsFulfillmentClose {
  closeId: string;
  requestId: string;
  status: "ready-to-invoice" | "blocked";
  requiredRecords: string[];
  missingRecords: string[];
  auditAction: string;
}
