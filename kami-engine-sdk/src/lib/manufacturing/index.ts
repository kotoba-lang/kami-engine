import type {
  AutonomyOperationPlan,
  IndustrialSoftwareDomain,
  IndustrialSoftwareIntegrationPlan,
  ManufacturingCellPlan,
  ManufacturingDeviceKind,
  ManufacturingDeviceRequest,
  ManufacturingOutputPlan,
  ManufacturingOutputRequest,
  ManufacturingPartEnvelope,
  ManufacturingSceneNode,
  ManufacturingWorkflowStep,
  LogisticsAssetKind,
  LogisticsRoutePlan,
  LogisticsWaypoint,
  RoboticsBusinessProcess,
  RoboticsApprovalRecord,
  RoboticsFulfillmentClose,
  RoboticsMissionStatus,
  RoboticsMissionSimulation,
  RoboticsProcessDependency,
  RoboticsProcessForm,
  RoboticsTelemetryFrame,
  RoboticsTelemetrySchema,
  RoboticsWorkProcessPlan,
} from "./types.js";

export type {
  AutonomyOperationPlan,
  IndustrialSoftwareDomain,
  IndustrialSoftwareIntegrationPlan,
  ManufacturingCellPlan,
  ManufacturingDeviceKind,
  ManufacturingDeviceRequest,
  ManufacturingOutputPlan,
  ManufacturingOutputRequest,
  ManufacturingPartEnvelope,
  ManufacturingSceneNode,
  ManufacturingWorkflowStep,
  LogisticsAssetKind,
  LogisticsRoutePlan,
  LogisticsWaypoint,
  RoboticsBusinessProcess,
  RoboticsApprovalRecord,
  RoboticsFulfillmentClose,
  RoboticsMissionStatus,
  RoboticsMissionSimulation,
  RoboticsProcessDependency,
  RoboticsProcessForm,
  RoboticsTelemetryFrame,
  RoboticsTelemetrySchema,
  RoboticsWorkProcessPlan,
} from "./types.js";

const DEFAULT_DEVICES: ManufacturingDeviceRequest[] = [
  { id: "cnc-01", kind: "cnc-mill", name: "CNC mill", workEnvelopeMm: [800, 500, 400] },
  { id: "printer-01", kind: "3d-printer", name: "3D printer", workEnvelopeMm: [300, 300, 300] },
  { id: "arm-01", kind: "robot-arm", name: "Robot arm", workEnvelopeMm: [900, 900, 600] },
  { id: "mh-01", kind: "material-handling", name: "Material handling", workEnvelopeMm: [1600, 500, 300] },
  { id: "inspect-01", kind: "inspection", name: "Inspection station", workEnvelopeMm: [500, 500, 300] },
];

function n(value: number | undefined, fallback: number): number {
  return Number.isFinite(value) && value !== undefined ? Math.max(1, value) : fallback;
}

function normalizePart(part: Partial<ManufacturingPartEnvelope> | undefined): ManufacturingPartEnvelope {
  return {
    widthMm: n(part?.widthMm, 120),
    heightMm: n(part?.heightMm, 80),
    depthMm: n(part?.depthMm, 24),
    material: part?.material || "aluminum-6061",
    process: part?.process || "machinery-equipment",
  };
}

function deviceColor(kind: ManufacturingDeviceKind | "stock" | "fixture" | "workpiece" | "safety-zone"): string {
  switch (kind) {
    case "cnc-mill": return "#64748b";
    case "cnc-lathe": return "#475569";
    case "3d-printer": return "#0f766e";
    case "robot-arm": return "#d97706";
    case "material-handling": return "#2563eb";
    case "inspection": return "#7c3aed";
    case "workpiece": return "#16a34a";
    case "fixture": return "#525252";
    case "stock": return "#71717a";
    case "safety-zone": return "#ef4444";
  }
}

function deviceNode(device: ManufacturingDeviceRequest, index: number): ManufacturingSceneNode {
  const id = device.id || `${device.kind}-${index + 1}`;
  const x = (index - 2) * 420;
  const envelope = device.workEnvelopeMm ?? [320, 260, 220];
  return {
    id,
    kind: device.kind,
    label: device.name || device.kind,
    position: [x, 0, envelope[2] / 2],
    rotation: [0, 0, 0],
    scale: envelope,
    color: deviceColor(device.kind),
  };
}

function workpieceNode(part: ManufacturingPartEnvelope): ManufacturingSceneNode {
  return {
    id: "workpiece",
    kind: "workpiece",
    label: `${part.material || "material"} workpiece`,
    position: [0, 0, part.depthMm / 2 + 20],
    rotation: [0, 0, 0],
    scale: [part.widthMm, part.heightMm, part.depthMm],
    color: deviceColor("workpiece"),
  };
}

function estimateSeconds(kind: ManufacturingDeviceKind, part: ManufacturingPartEnvelope): number {
  const volume = part.widthMm * part.heightMm * part.depthMm;
  switch (kind) {
    case "3d-printer": return Math.round(Math.max(900, volume / 18));
    case "cnc-mill":
    case "cnc-lathe": return Math.round(Math.max(420, volume / 55));
    case "robot-arm": return 90;
    case "material-handling": return 45;
    case "inspection": return 180;
  }
}

function operationFor(kind: ManufacturingDeviceKind, process: string | undefined): string {
  if (kind === "3d-printer") return "additive-print";
  if (kind === "cnc-mill") return process === "metal-fabrication" ? "rough-finish-machine" : "pocket-machine";
  if (kind === "cnc-lathe") return "turning";
  if (kind === "robot-arm") return "pick-place-machine-tend";
  if (kind === "material-handling") return "convey-buffer-route";
  return "dimensional-inspection";
}

export function createManufacturingCellPlan(input: {
  cellId?: string;
  partEnvelope?: Partial<ManufacturingPartEnvelope>;
  devices?: ManufacturingDeviceRequest[];
} = {}): ManufacturingCellPlan {
  const part = normalizePart(input.partEnvelope);
  const devices = (input.devices?.length ? input.devices : DEFAULT_DEVICES).map((device, index) => ({
    ...device,
    id: device.id || `${device.kind}-${index + 1}`,
  }));
  const scene = devices.map(deviceNode).concat([
    {
      id: "fixture-01",
      kind: "fixture",
      label: "Fixture",
      position: [0, 0, 12],
      rotation: [0, 0, 0],
      scale: [part.widthMm + 40, part.heightMm + 40, 24],
      color: deviceColor("fixture"),
    },
    workpieceNode(part),
  ]);
  const safetyZones = devices.map((device, index) => ({
    id: `${device.id || `${device.kind}-${index + 1}`}-safety`,
    kind: "safety-zone" as const,
    label: `${device.name || device.kind} safety zone`,
    position: scene[index].position,
    rotation: [0, 0, 0] as [number, number, number],
    scale: scene[index].scale.map((v) => v + 160) as [number, number, number],
    color: deviceColor("safety-zone"),
  }));
  const workflow = devices.map((device, index) => ({
    id: `step-${String(index + 1).padStart(2, "0")}`,
    deviceId: device.id || `${device.kind}-${index + 1}`,
    operation: operationFor(device.kind, part.process),
    input: index === 0 ? "raw-stock" : `step-${String(index).padStart(2, "0")}`,
    output: `step-${String(index + 1).padStart(2, "0")}`,
    estimatedSeconds: estimateSeconds(device.kind, part),
  }));
  return {
    cellId: input.cellId || `kami-cell-${Date.now().toString(36)}`,
    sceneUnits: "millimeters",
    partEnvelope: part,
    devices,
    scene,
    workflow,
    safetyZones,
    integrations: [
      "kami-engine-sdk/manufacturing",
      "kami-cad scene envelope",
      "kami-cam gcode post",
      "robot-arm waypoint program",
      "material-handling flow route",
    ],
  };
}

function cncProgram(part: ManufacturingPartEnvelope, programNumber: number, operation: string): string {
  const z = -Math.min(part.depthMm, 20);
  return [
    "%",
    `O${String(programNumber).padStart(4, "0")}`,
    "(KAMI CAM - Tsukuru CNC output)",
    "G21 (metric)",
    "G90 (absolute)",
    "G54",
    "G00 Z25.0000",
    "T01 M06",
    "M03 S10000",
    "M08",
    `(${operation})`,
    "G00 X5.0000 Y5.0000",
    `G01 Z${z.toFixed(4)} F240.0`,
    `G01 X${(part.widthMm - 5).toFixed(4)} Y5.0000 F600.0`,
    `G01 X${(part.widthMm - 5).toFixed(4)} Y${(part.heightMm - 5).toFixed(4)} F600.0`,
    `G01 X5.0000 Y${(part.heightMm - 5).toFixed(4)} F600.0`,
    "G00 Z25.0000",
    "M09",
    "M05",
    "M30",
    "%",
  ].join("\n");
}

function printerProgram(part: ManufacturingPartEnvelope, operation: string): string {
  return [
    "; KAMI CAM - Tsukuru 3D printer output",
    "G21 ; metric",
    "G90 ; absolute",
    "M104 S215",
    "M140 S60",
    "G28",
    `; ${operation} ${part.widthMm}x${part.heightMm}x${part.depthMm}mm ${part.material}`,
    "G1 Z0.280 F1200",
    "G1 X10 Y10 E0 F1800",
    `G1 X${part.widthMm.toFixed(3)} Y10 E1.2 F1800`,
    `G1 X${part.widthMm.toFixed(3)} Y${part.heightMm.toFixed(3)} E2.4 F1800`,
    "M104 S0",
    "M140 S0",
    "M84",
  ].join("\n");
}

function jsonProgram(kind: ManufacturingDeviceKind, part: ManufacturingPartEnvelope, operation: string): string {
  return JSON.stringify({
    generator: "kami-engine-sdk/manufacturing",
    kind,
    operation,
    units: "millimeters",
    part,
    waypoints: [
      { id: "home", xyz: [0, 0, 250], speedMmS: 200 },
      { id: "approach", xyz: [20, 20, 120], speedMmS: 120 },
      { id: "process", xyz: [part.widthMm / 2, part.heightMm / 2, part.depthMm + 40], speedMmS: 80 },
      { id: "handoff", xyz: [part.widthMm + 120, 0, 120], speedMmS: 120 },
    ],
  }, null, 2);
}

export function createManufacturingOutputPlan(input: ManufacturingOutputRequest): ManufacturingOutputPlan {
  const part = normalizePart(input.partEnvelope);
  const operation = input.operation || operationFor(input.deviceKind, part.process);
  const estimatedSeconds = estimateSeconds(input.deviceKind, part);
  const previewScene = [deviceNode({ kind: input.deviceKind, id: "device-preview" }, 2), workpieceNode(part)];
  if (input.deviceKind === "cnc-mill" || input.deviceKind === "cnc-lathe") {
    return {
      deviceKind: input.deviceKind,
      operation,
      postProcessor: "fanuc",
      previewScene,
      program: cncProgram(part, input.programNumber ?? 1, operation),
      estimatedSeconds,
      warnings: [],
    };
  }
  if (input.deviceKind === "3d-printer") {
    return {
      deviceKind: input.deviceKind,
      operation,
      postProcessor: "marlin",
      previewScene,
      program: printerProgram(part, operation),
      estimatedSeconds,
      warnings: part.material?.includes("metal") ? ["metal printing requires printer-specific post validation"] : [],
    };
  }
  return {
    deviceKind: input.deviceKind,
    operation,
    postProcessor: input.deviceKind === "robot-arm" ? "robot-json" : input.deviceKind === "material-handling" ? "material-flow-json" : "inspection-json",
    previewScene,
    program: jsonProgram(input.deviceKind, part, operation),
    estimatedSeconds,
    warnings: [],
  };
}

const SOFTWARE_CATALOG: Array<{
  domain: IndustrialSoftwareDomain;
  label: string;
  role: string;
  records: string[];
}> = [
  { domain: "requirements", label: "Requirements / RFQ", role: "captures buyer requirements, constraints, and acceptance criteria", records: ["rfq", "quotation", "acceptanceCriteria"] },
  { domain: "cad", label: "CAD", role: "owns product geometry, assemblies, drawings, and revisions", records: ["partModel", "assembly", "drawing", "revision"] },
  { domain: "cae", label: "CAE", role: "validates structure, thermal behavior, flow, and failure margins", records: ["simulationCase", "mesh", "materialModel", "resultField"] },
  { domain: "cam", label: "CAM", role: "generates toolpaths, G-code, fixtures, setup sheets, and additive builds", records: ["toolpath", "machineProgram", "setupSheet", "fixturePlan"] },
  { domain: "plm", label: "PLM", role: "controls engineering change, BOM, lifecycle, and product release", records: ["ebom", "changeOrder", "releasePackage"] },
  { domain: "mes", label: "MES", role: "executes work orders, dispatches operations, captures traceability", records: ["workOrder", "operation", "lotTrace", "operatorEvent"] },
  { domain: "scada", label: "SCADA / PLC", role: "supervises machine state, alarms, process telemetry, and interlocks", records: ["tag", "alarm", "machineState", "interlock"] },
  { domain: "qms", label: "QMS", role: "manages inspection plans, nonconformance, CAPA, and release", records: ["inspectionPlan", "nonconformance", "capa", "qualityRelease"] },
  { domain: "wms", label: "WMS", role: "controls inventory, bins, picking, staging, and kanban replenishment", records: ["inventoryLot", "bin", "pickTask", "replenishmentTask"] },
  { domain: "tms", label: "TMS", role: "plans carrier booking, shipment execution, tracking, and delivery proof", records: ["shipment", "route", "carrierBooking", "proofOfDelivery"] },
  { domain: "erp", label: "ERP", role: "anchors commercial orders, costing, procurement, invoices, and financial posting", records: ["salesOrder", "purchaseOrder", "invoice", "costRollup"] },
  { domain: "cmms", label: "CMMS / EAM", role: "manages maintenance plans, work requests, spares, and asset reliability", records: ["asset", "maintenancePlan", "workRequest", "sparePart"] },
  { domain: "digital-twin", label: "Digital Twin", role: "fuses 3D scene, machine state, routing, and telemetry for simulation", records: ["sceneGraph", "stateSnapshot", "simulationRun", "telemetryFrame"] },
  { domain: "route-planning", label: "Route Planning", role: "optimizes indoor, yard, road, air, sea, and multimodal movement", records: ["routePlan", "constraint", "eta", "handoff"] },
  { domain: "fleet-control", label: "Fleet Control", role: "dispatches trucks, AGVs, AMRs, forklifts, and autonomous vehicles", records: ["fleetAsset", "dispatch", "mission", "telemetry"] },
  { domain: "drone-control", label: "Drone Control", role: "plans drone missions, geofences, payload drops, and inspection surveys", records: ["flightPlan", "geofence", "payload", "flightTelemetry"] },
  { domain: "autonomous-driving", label: "Autonomous Driving", role: "plans autonomous vehicle missions, ODD, fallback behavior, and safety cases", records: ["odd", "missionPlan", "fallbackPolicy", "safetyCase"] },
];

const DEFAULT_SOFTWARE_FLOW: IndustrialSoftwareDomain[] = [
  "requirements",
  "cad",
  "cae",
  "cam",
  "plm",
  "mes",
  "scada",
  "qms",
  "wms",
  "tms",
  "erp",
  "cmms",
  "digital-twin",
  "route-planning",
  "fleet-control",
  "drone-control",
  "autonomous-driving",
];

export function createIndustrialSoftwareIntegrationPlan(input: {
  planId?: string;
  scope?: string;
  domains?: IndustrialSoftwareDomain[];
} = {}): IndustrialSoftwareIntegrationPlan {
  const selected = input.domains?.length ? input.domains : DEFAULT_SOFTWARE_FLOW;
  const nodes = selected.map((domain, index) => {
    const catalog = SOFTWARE_CATALOG.find((item) => item.domain === domain) ?? SOFTWARE_CATALOG[0];
    const upstream = index > 0 ? [`software-${selected[index - 1]}`] : [];
    const downstream = index < selected.length - 1 ? [`software-${selected[index + 1]}`] : [];
    return {
      id: `software-${domain}`,
      domain,
      label: catalog.label,
      role: catalog.role,
      records: catalog.records,
      upstream,
      downstream,
    };
  });
  const dataContracts = nodes.slice(0, -1).map((node, index) => ({
    id: `contract-${node.domain}-to-${nodes[index + 1].domain}`,
    from: node.id,
    to: nodes[index + 1].id,
    payload: `${node.records[0]} -> ${nodes[index + 1].records[0]}`,
    cadence: index < 6 ? "event" as const : index < 12 ? "batch" as const : "realtime" as const,
  }));
  return {
    planId: input.planId || `kami-software-${Date.now().toString(36)}`,
    scope: input.scope || "design-manufacturing-operations-logistics-autonomy",
    nodes,
    dataContracts,
    controlBoundaries: [
      "engineering release gates CAD/CAE/CAM before MES dispatch",
      "SCADA/PLC remains authoritative for machine safety interlocks",
      "WMS/TMS own custody handoff and shipment traceability",
      "drone/autonomous-driving missions require geofence and fallback policy",
      "digital twin mirrors state; it does not bypass operational approvals",
    ],
    canonicalRecords: nodes.flatMap((node) => node.records.map((record) => `${node.domain}.${record}`)),
  };
}

function waypointDistanceMeters(a: LogisticsWaypoint, b: LogisticsWaypoint): number {
  if (typeof a.lat === "number" && typeof a.lng === "number" && typeof b.lat === "number" && typeof b.lng === "number") {
    const latMeters = (b.lat - a.lat) * 111_320;
    const lngMeters = (b.lng - a.lng) * 111_320 * Math.cos(((a.lat + b.lat) / 2) * Math.PI / 180);
    return Math.hypot(latMeters, lngMeters);
  }
  const dx = ((b.xMm ?? 0) - (a.xMm ?? 0)) / 1000;
  const dy = ((b.yMm ?? 0) - (a.yMm ?? 0)) / 1000;
  const dz = ((b.zMm ?? 0) - (a.zMm ?? 0)) / 1000;
  return Math.hypot(dx, dy, dz);
}

function speedForAsset(kind: LogisticsAssetKind): number {
  switch (kind) {
    case "forklift": return 2.5;
    case "agv": return 1.8;
    case "conveyor": return 0.8;
    case "truck": return 16;
    case "rail": return 22;
    case "ship": return 9;
    case "air-cargo": return 180;
    case "drone": return 12;
    case "autonomous-vehicle": return 10;
  }
}

function modeForAsset(kind: LogisticsAssetKind): LogisticsRoutePlan["mode"] {
  if (kind === "forklift" || kind === "agv" || kind === "conveyor") return "intra-factory";
  if (kind === "drone") return "air";
  if (kind === "truck" || kind === "autonomous-vehicle") return "road";
  if (kind === "ship") return "sea";
  if (kind === "rail") return "rail";
  return "multimodal";
}

export function createLogisticsRoutePlan(input: {
  routeId?: string;
  assetKind?: LogisticsAssetKind;
  waypoints?: LogisticsWaypoint[];
} = {}): LogisticsRoutePlan {
  const assetKind = input.assetKind || "truck";
  const waypoints = input.waypoints?.length && input.waypoints.length >= 2
    ? input.waypoints
    : [
      { id: "origin", label: "Factory dock", xMm: 0, yMm: 0, zMm: 0 },
      { id: "staging", label: "Warehouse staging", xMm: 18_000, yMm: 4_000, zMm: 0 },
      { id: "destination", label: "Outbound handoff", xMm: 42_000, yMm: 8_000, zMm: 0 },
    ];
  const speed = speedForAsset(assetKind);
  const segments = waypoints.slice(0, -1).map((from, index) => {
    const to = waypoints[index + 1];
    const distanceMeters = Math.max(1, Math.round(waypointDistanceMeters(from, to)));
    return {
      id: `segment-${String(index + 1).padStart(2, "0")}`,
      from: from.id,
      to: to.id,
      distanceMeters,
      estimatedSeconds: Math.round(distanceMeters / speed),
      constraints: assetKind === "drone"
        ? ["geofence", "weather", "battery-reserve", "landing-zone"]
        : assetKind === "autonomous-vehicle"
          ? ["ODD", "fallback-driver", "road-rules", "remote-assist"]
          : ["capacity", "dock-window", "custody-scan"],
    };
  });
  const estimatedDistanceMeters = segments.reduce((sum, segment) => sum + segment.distanceMeters, 0);
  return {
    routeId: input.routeId || `kami-route-${Date.now().toString(36)}`,
    assetKind,
    mode: modeForAsset(assetKind),
    waypoints,
    segments,
    estimatedDistanceMeters,
    estimatedSeconds: segments.reduce((sum, segment) => sum + segment.estimatedSeconds, 0),
    handoffRecords: ["wms.pickTask", "wms.inventoryLot", "tms.shipment", "tms.proofOfDelivery"],
    warnings: assetKind === "drone" ? ["validate airspace, weather, payload mass, and local operating rules before flight"] : [],
  };
}

export function createAutonomyOperationPlan(input: {
  operationId?: string;
  assetKind?: AutonomyOperationPlan["assetKind"];
  missionType?: AutonomyOperationPlan["missionType"];
  route?: LogisticsRoutePlan;
} = {}): AutonomyOperationPlan {
  const assetKind = input.assetKind || "drone";
  const missionType = input.missionType || (assetKind === "drone" ? "inspection" : assetKind === "robot-arm" ? "machine-tending" : "delivery");
  const commandProtocol =
    assetKind === "drone" ? "mavlink-json" :
      assetKind === "autonomous-vehicle" ? "ros2-action-json" :
        assetKind === "agv" ? "vda5050-json" :
          "robot-waypoint-json";
  const route = input.route ?? createLogisticsRoutePlan({ assetKind: assetKind === "drone" ? "drone" : assetKind === "autonomous-vehicle" ? "autonomous-vehicle" : "agv" });
  return {
    operationId: input.operationId || `kami-autonomy-${Date.now().toString(36)}`,
    assetKind,
    missionType,
    commandProtocol,
    safetyEnvelope: {
      geofenceMeters: assetKind === "drone" ? 500 : 80,
      maxSpeedMps: assetKind === "drone" ? 12 : assetKind === "autonomous-vehicle" ? 10 : 2,
      requiresHumanApproval: true,
    },
    commands: [
      { id: "precheck", command: "validate-safety-envelope", params: { routeId: route.routeId } },
      { id: "arm", command: "arm-or-enable", params: { approvalRequired: true } },
      { id: "execute", command: "execute-route", params: { waypoints: route.waypoints.map((waypoint) => waypoint.id) } },
      { id: "handoff", command: "confirm-custody-or-completion", params: { records: route.handoffRecords } },
    ],
    telemetryTopics: [
      "asset.pose",
      "asset.health",
      "mission.state",
      "safety.event",
      "custody.handoff",
    ],
    emergencyProcedures: [
      "pause mission and hold position",
      "return to home or nearest safe stop",
      "notify remote operator",
      "write incident and telemetry snapshot",
    ],
  };
}

const ROBOTICS_FORMS: RoboticsProcessForm[] = [
  {
    id: "robotics-sales-intake-v1",
    process: "sales",
    title: "Sales order / RFQ intake",
    requiredFields: ["customerId", "itemOrService", "quantity", "targetDate", "commercialTerms"],
    outputRecords: ["erp.salesOrder", "crm.opportunity", "requirements.rfq"],
  },
  {
    id: "robotics-requirements-v1",
    process: "requirements",
    title: "Product and service requirements",
    requiredFields: ["specification", "acceptanceCriteria", "regulatoryConstraints", "budget"],
    outputRecords: ["requirements.acceptanceCriteria", "plm.requirementSet"],
  },
  {
    id: "robotics-engineering-release-v1",
    process: "engineering",
    title: "CAD / CAE / CAM engineering release",
    requiredFields: ["cadRevision", "bomRevision", "simulationResult", "machineProgram"],
    outputRecords: ["cad.partModel", "plm.ebom", "cam.machineProgram", "qms.inspectionPlan"],
  },
  {
    id: "robotics-procurement-v1",
    process: "procurement",
    title: "Material and vendor procurement",
    requiredFields: ["materialSpec", "supplier", "leadTime", "cost", "certificateRequired"],
    outputRecords: ["erp.purchaseOrder", "wms.inboundLot", "qms.materialCertificate"],
  },
  {
    id: "robotics-production-plan-v1",
    process: "production-planning",
    title: "Production scheduling and capacity plan",
    requiredFields: ["workOrder", "routing", "machine", "operatorOrRobot", "plannedWindow"],
    outputRecords: ["mes.workOrder", "mes.operation", "cmms.assetReservation"],
  },
  {
    id: "robotics-manufacturing-execution-v1",
    process: "manufacturing",
    title: "Robot work execution",
    requiredFields: ["missionId", "robotAsset", "programRef", "fixture", "safetyEnvelope"],
    outputRecords: ["scada.machineState", "mes.operatorEvent", "digital-twin.telemetryFrame"],
  },
  {
    id: "robotics-quality-release-v1",
    process: "quality",
    title: "Inspection and quality release",
    requiredFields: ["inspectionPlan", "measurementData", "nonconformance", "releaseDecision"],
    outputRecords: ["qms.inspectionResult", "qms.nonconformance", "qms.qualityRelease"],
  },
  {
    id: "robotics-warehouse-v1",
    process: "warehouse",
    title: "Warehouse picking and staging",
    requiredFields: ["inventoryLot", "pickTask", "bin", "stagingLocation", "custodyScan"],
    outputRecords: ["wms.pickTask", "wms.inventoryLot", "wms.replenishmentTask"],
  },
  {
    id: "robotics-transport-v1",
    process: "transport",
    title: "Transport and route planning",
    requiredFields: ["origin", "destination", "assetKind", "routeWindow", "handoff"],
    outputRecords: ["tms.shipment", "tms.route", "fleet-control.dispatch", "tms.proofOfDelivery"],
  },
  {
    id: "robotics-installation-v1",
    process: "installation",
    title: "Installation / commissioning",
    requiredFields: ["site", "equipment", "testProtocol", "operatorApproval"],
    outputRecords: ["digital-twin.stateSnapshot", "qms.acceptanceReport", "cmms.asset"],
  },
  {
    id: "robotics-maintenance-v1",
    process: "maintenance",
    title: "Maintenance and reliability",
    requiredFields: ["asset", "failureMode", "sparePart", "maintenanceWindow"],
    outputRecords: ["cmms.workRequest", "cmms.maintenancePlan", "cmms.sparePart"],
  },
  {
    id: "robotics-finance-v1",
    process: "finance",
    title: "Costing, billing, and margin close",
    requiredFields: ["costRollup", "shipmentProof", "qualityRelease", "invoiceTerm"],
    outputRecords: ["erp.costRollup", "erp.invoice", "erp.revenueRecognition"],
  },
];

const ROBOTICS_DEPENDENCIES: RoboticsProcessDependency[] = [
  {
    id: "dep-sales-requirements",
    from: "sales",
    to: "requirements",
    records: ["erp.salesOrder", "crm.opportunity", "requirements.rfq"],
    gate: "commercial terms and requested scope accepted",
  },
  {
    id: "dep-requirements-engineering",
    from: "requirements",
    to: "engineering",
    records: ["requirements.acceptanceCriteria", "plm.requirementSet"],
    gate: "requirements baseline approved",
  },
  {
    id: "dep-requirements-procurement",
    from: "requirements",
    to: "procurement",
    records: ["materialSpec", "budget", "supplierConstraints"],
    gate: "make-buy and sourcing constraints approved",
  },
  {
    id: "dep-engineering-production",
    from: "engineering",
    to: "production-planning",
    records: ["cad.partModel", "plm.ebom", "cam.machineProgram", "qms.inspectionPlan"],
    gate: "engineering release package complete",
  },
  {
    id: "dep-procurement-production",
    from: "procurement",
    to: "production-planning",
    records: ["erp.purchaseOrder", "wms.inboundLot", "qms.materialCertificate"],
    gate: "critical material availability confirmed",
  },
  {
    id: "dep-production-manufacturing",
    from: "production-planning",
    to: "manufacturing",
    records: ["mes.workOrder", "mes.operation", "cmms.assetReservation"],
    gate: "work order dispatched and asset reserved",
  },
  {
    id: "dep-manufacturing-quality",
    from: "manufacturing",
    to: "quality",
    records: ["scada.machineState", "mes.operatorEvent", "digital-twin.telemetryFrame"],
    gate: "manufacturing telemetry and traceability captured",
  },
  {
    id: "dep-quality-warehouse",
    from: "quality",
    to: "warehouse",
    records: ["qms.inspectionResult", "qms.qualityRelease"],
    gate: "quality release approved",
  },
  {
    id: "dep-warehouse-transport",
    from: "warehouse",
    to: "transport",
    records: ["wms.pickTask", "wms.inventoryLot", "wms.replenishmentTask"],
    gate: "picked inventory staged and custody scanned",
  },
  {
    id: "dep-transport-installation",
    from: "transport",
    to: "installation",
    records: ["tms.shipment", "tms.route", "tms.proofOfDelivery"],
    gate: "delivery proof accepted",
  },
  {
    id: "dep-transport-finance",
    from: "transport",
    to: "finance",
    records: ["tms.proofOfDelivery", "erp.salesOrder"],
    gate: "delivery proof and order terms matched",
  },
  {
    id: "dep-installation-maintenance",
    from: "installation",
    to: "maintenance",
    records: ["digital-twin.stateSnapshot", "qms.acceptanceReport", "cmms.asset"],
    gate: "commissioned asset accepted into service",
  },
];

function dependencyProjection(selected: RoboticsProcessForm[]): {
  dependencies: RoboticsProcessDependency[];
  missingPrerequisites: RoboticsProcessDependency[];
} {
  const selectedProcesses = new Set(selected.map((form) => form.process));
  return {
    dependencies: ROBOTICS_DEPENDENCIES.filter((dependency) => (
      selectedProcesses.has(dependency.from) && selectedProcesses.has(dependency.to)
    )),
    missingPrerequisites: ROBOTICS_DEPENDENCIES.filter((dependency) => (
      !selectedProcesses.has(dependency.from) && selectedProcesses.has(dependency.to)
    )),
  };
}

function roboticsScene(cell: ManufacturingCellPlan, route: LogisticsRoutePlan): ManufacturingSceneNode[] {
  const routeNodes: ManufacturingSceneNode[] = route.waypoints.map((waypoint, index) => ({
    id: `route-${waypoint.id}`,
    kind: "material-handling",
    label: waypoint.label,
    position: [waypoint.xMm ?? index * 600, waypoint.yMm ?? 900, waypoint.zMm ?? 20],
    rotation: [0, 0, 0],
    scale: [180, 180, 40],
    color: "#2563eb",
  }));
  return [...cell.scene, ...cell.safetyZones, ...routeNodes];
}

export function createRoboticsWorkProcessPlan(input: {
  planId?: string;
  scope?: string;
  processes?: RoboticsBusinessProcess[];
  cell?: ManufacturingCellPlan;
  route?: LogisticsRoutePlan;
} = {}): RoboticsWorkProcessPlan {
  const selected = input.processes?.length
    ? ROBOTICS_FORMS.filter((form) => input.processes?.includes(form.process))
    : ROBOTICS_FORMS;
  const dependencyGraph = dependencyProjection(selected);
  const cell = input.cell ?? createManufacturingCellPlan({
    cellId: "robotics-review-cell",
    devices: [
      { id: "sales-review", kind: "inspection", name: "Sales and requirements review" },
      { id: "engineering-station", kind: "cnc-mill", name: "Engineering / CAM station" },
      { id: "robot-workcell", kind: "robot-arm", name: "Robot workcell" },
      { id: "warehouse-flow", kind: "material-handling", name: "Warehouse and staging flow" },
      { id: "quality-gate", kind: "inspection", name: "Quality gate" },
    ],
  });
  const route = input.route ?? createLogisticsRoutePlan({ assetKind: "agv" });
  return {
    planId: input.planId || `kami-robotics-process-${Date.now().toString(36)}`,
    scope: input.scope || "sales-to-engineering-to-manufacturing-to-warehouse-to-transport-to-service",
    forms: selected,
    dependencies: dependencyGraph.dependencies,
    missingPrerequisites: dependencyGraph.missingPrerequisites,
    bpmnProcesses: [
      "etzhayyim-root/00-contracts/bpmn/ai/gftd/robotics/planRoboticsBusinessProcess.bpmn",
      "etzhayyim-root/00-contracts/bpmn/ai/gftd/robotics/executeRoboticsWork.bpmn",
      "etzhayyim-root/00-contracts/bpmn/ai/gftd/robotics/planRoboticsTransportAndSales.bpmn",
    ],
    mcpTools: [
      "robotics.process.catalog",
      "robotics.workflow.plan",
      "robotics.kami.scene.plan",
      "robotics.transport.plan",
      "robotics.sales.plan",
      "robotics.mission.plan",
      "robotics.process.dependencies",
    ],
    kamiReview: {
      sceneNodes: roboticsScene(cell, route),
      operatorActions: [
        "review business form completeness",
        "inspect KAMI 3D scene for robot reach and safety-zone collisions",
        "approve robot or transport mission before execution",
        "compare telemetry with BPMN audit trail",
      ],
      telemetryTopics: [
        "robotics.work.state",
        "robotics.asset.pose",
        "robotics.quality.release",
        "robotics.transport.handoff",
        "robotics.sales.fulfillment",
      ],
    },
    integrationRecords: selected.flatMap((form) => form.outputRecords),
    approvalGates: [
      "sales terms accepted before engineering release",
      "engineering release approved before robot work dispatch",
      "SCADA / robot safety envelope approved before motion",
      "quality release approved before shipment",
      "transport custody proof approved before invoice close",
    ],
  };
}

function esc(value: unknown): string {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

export function createRoboticsKamiReviewHtml(plan: RoboticsWorkProcessPlan): string {
  const nodes = plan.kamiReview.sceneNodes;
  const xs = nodes.map((node) => node.position[0]);
  const ys = nodes.map((node) => node.position[1]);
  const minX = Math.min(...xs, -1000);
  const maxX = Math.max(...xs, 1000);
  const minY = Math.min(...ys, -1000);
  const maxY = Math.max(...ys, 1000);
  const spanX = Math.max(1, maxX - minX);
  const spanY = Math.max(1, maxY - minY);
  const markers = nodes.map((node) => {
    const left = ((node.position[0] - minX) / spanX) * 100;
    const top = ((node.position[1] - minY) / spanY) * 100;
    const width = Math.max(10, Math.min(22, node.scale[0] / 80));
    const height = Math.max(10, Math.min(22, node.scale[1] / 80));
    return `<button class="node" style="left:${left.toFixed(2)}%;top:${top.toFixed(2)}%;width:${width.toFixed(1)}px;height:${height.toFixed(1)}px;background:${esc(node.color)}" data-kind="${esc(node.kind)}" title="${esc(node.label)}"><span>${esc(node.label)}</span></button>`;
  }).join("");
  const forms = plan.forms.map((form) => `<li><strong>${esc(form.process)}</strong><span>${esc(form.title)}</span></li>`).join("");
  const dependencies = plan.dependencies.map((dependency) => `<li><strong>${esc(dependency.from)} -> ${esc(dependency.to)}</strong><span>${esc(dependency.gate)}</span></li>`).join("");
  const missing = plan.missingPrerequisites.map((dependency) => `<li><strong>${esc(dependency.from)} -> ${esc(dependency.to)}</strong><span>${esc(dependency.gate)}</span></li>`).join("");
  const gates = plan.approvalGates.map((gate) => `<li>${esc(gate)}</li>`).join("");
  const topics = plan.kamiReview.telemetryTopics.map((topic) => `<code>${esc(topic)}</code>`).join("");
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>${esc(plan.planId)} robotics KAMI review</title>
<style>
:root{font-family:Inter,ui-sans-serif,system-ui,sans-serif;color:#172033;background:#f6f7f9}
body{margin:0}
main{display:grid;grid-template-columns:minmax(0,1fr) 360px;min-height:100vh}
.stage{position:relative;overflow:hidden;background:#eef2f5}
.grid{position:absolute;inset:24px;background-image:linear-gradient(#d6dde5 1px,transparent 1px),linear-gradient(90deg,#d6dde5 1px,transparent 1px);background-size:32px 32px;border:1px solid #c9d1db}
.node{position:absolute;border:0;border-radius:6px;transform:translate(-50%,-50%);box-shadow:0 8px 20px #0f172a26;cursor:pointer}
.node span{position:absolute;left:50%;top:calc(100% + 6px);transform:translateX(-50%);white-space:nowrap;font-size:12px;color:#223047;background:#ffffffe6;border:1px solid #d7dde5;border-radius:4px;padding:2px 5px}
aside{background:#fff;border-left:1px solid #d8dee7;padding:20px;overflow:auto}
h1{font-size:20px;margin:0 0 6px}
h2{font-size:13px;text-transform:uppercase;letter-spacing:0;color:#5c6678;margin:22px 0 10px}
p{margin:0;color:#536071;line-height:1.45}
ul{list-style:none;padding:0;margin:0;display:grid;gap:8px}
li{border:1px solid #e1e6ed;border-radius:6px;padding:9px 10px;display:grid;gap:3px}
li span{color:#566275;font-size:13px}
.topics{display:flex;gap:6px;flex-wrap:wrap}
code{background:#edf2f7;border:1px solid #d8e1ea;border-radius:4px;padding:3px 6px;font-size:12px}
@media(max-width:820px){main{grid-template-columns:1fr}.stage{min-height:58vh}aside{border-left:0;border-top:1px solid #d8dee7}}
</style>
</head>
<body>
<main>
<section class="stage" aria-label="KAMI robotics review scene"><div class="grid">${markers}</div></section>
<aside>
<h1>${esc(plan.planId)}</h1>
<p>${esc(plan.scope)}</p>
<h2>Process Forms</h2>
<ul>${forms}</ul>
<h2>Dependencies</h2>
<ul>${dependencies || "<li>No internal dependencies in selected process slice</li>"}</ul>
<h2>Missing Prerequisites</h2>
<ul>${missing || "<li>None</li>"}</ul>
<h2>Approval Gates</h2>
<ul>${gates}</ul>
<h2>Telemetry</h2>
<div class="topics">${topics}</div>
</aside>
</main>
</body>
</html>`;
}

export function createRoboticsTelemetrySchema(input: { schemaId?: string } = {}): RoboticsTelemetrySchema {
  return {
    schemaId: input.schemaId || "robotics-telemetry-v1",
    topics: [
      { topic: "robotics.asset.pose", requiredFields: ["assetId", "x", "y", "z", "yawDeg", "timestamp"], retention: "hot-24h" },
      { topic: "robotics.work.state", requiredFields: ["missionId", "state", "stepId", "timestamp"], retention: "audit-7y" },
      { topic: "robotics.safety.event", requiredFields: ["assetId", "severity", "event", "timestamp"], retention: "audit-7y" },
      { topic: "robotics.transport.handoff", requiredFields: ["shipmentId", "from", "to", "custodyScan", "timestamp"], retention: "audit-7y" },
      { topic: "robotics.quality.release", requiredFields: ["requestId", "decision", "inspectionRef", "timestamp"], retention: "audit-7y" },
    ],
    stateEnums: {
      missionState: ["planned", "simulated", "approved", "running", "paused", "completed", "failed"],
      safetySeverity: ["info", "warning", "stop", "estop"],
      approvalDecision: ["approve", "reject", "hold"],
    },
  };
}

export function createRoboticsApprovalRecord(input: {
  approvalId?: string;
  requestId?: string;
  decision?: RoboticsApprovalRecord["decision"];
  approverDid?: string;
  scope?: string;
  requiredEvidence?: string[];
} = {}): RoboticsApprovalRecord {
  const decision = input.decision || "hold";
  return {
    approvalId: input.approvalId || `robotics-approval-${Date.now().toString(36)}`,
    requestId: input.requestId || "robotics-request",
    decision,
    approverDid: input.approverDid || "did:web:robotics-operator.gftd.ai",
    approvedAt: new Date().toISOString(),
    scope: input.scope || "robot-motion-and-transport",
    requiredEvidence: input.requiredEvidence || [
      "mission simulation pass or reviewed",
      "safety envelope reviewed",
      "telemetry schema selected",
      "BPMN audit event emitted",
    ],
    auditAction: `robotics.approval.${decision}`,
  };
}

export function simulateRoboticsMission(input: {
  simulationId?: string;
  missionId?: string;
  commands?: Array<{ id?: string; command?: string; params?: Record<string, unknown> }>;
  telemetry?: RoboticsTelemetrySchema;
  sceneNodes?: ManufacturingSceneNode[];
} = {}): RoboticsMissionSimulation {
  const commands = input.commands?.length ? input.commands : [
    { id: "precheck", command: "validate-safety-envelope" },
    { id: "arm", command: "arm-or-enable" },
    { id: "execute", command: "execute-route-or-waypoints" },
    { id: "handoff", command: "confirm-custody-or-completion" },
  ];
  const telemetry = input.telemetry ?? createRoboticsTelemetrySchema();
  const sceneNodes = input.sceneNodes ?? createRoboticsWorkProcessPlan().kamiReview.sceneNodes;
  const checks = [
    {
      id: "commands-present",
      status: commands.length >= 3 ? "pass" as const : "fail" as const,
      detail: `${commands.length} commands planned`,
    },
    {
      id: "safety-command",
      status: commands.some((command) => command.command?.includes("safety") || command.command?.includes("precheck")) ? "pass" as const : "review" as const,
      detail: "mission should start with safety-envelope validation",
    },
    {
      id: "telemetry-contract",
      status: telemetry.topics.length >= 4 ? "pass" as const : "review" as const,
      detail: `${telemetry.topics.length} telemetry topics required`,
    },
    {
      id: "kami-scene",
      status: sceneNodes.some((node) => node.kind === "safety-zone") ? "pass" as const : "review" as const,
      detail: `${sceneNodes.length} KAMI scene nodes checked`,
    },
  ];
  const status = checks.some((check) => check.status === "fail")
    ? "fail"
    : checks.some((check) => check.status === "review")
      ? "review"
      : "pass";
  return {
    simulationId: input.simulationId || `robotics-sim-${Date.now().toString(36)}`,
    missionId: input.missionId || "robotics-mission",
    status,
    checks,
    estimatedSeconds: commands.length * 30,
    requiresHumanApproval: status !== "pass",
  };
}

export function ingestRoboticsTelemetry(input: {
  frameId?: string;
  topic?: string;
  payload?: Record<string, unknown>;
  schema?: RoboticsTelemetrySchema;
} = {}): RoboticsTelemetryFrame {
  const schema = input.schema ?? createRoboticsTelemetrySchema();
  const topic = input.topic || "robotics.work.state";
  const payload = input.payload ?? {};
  const contract = schema.topics.find((entry) => entry.topic === topic);
  const missingFields = contract
    ? contract.requiredFields.filter((field) => !(field in payload))
    : ["knownTopic"];
  return {
    frameId: input.frameId || `robotics-frame-${Date.now().toString(36)}`,
    topic,
    accepted: missingFields.length === 0,
    missingFields,
    payload,
    receivedAt: new Date().toISOString(),
  };
}

export function createRoboticsMissionStatus(input: {
  missionId?: string;
  simulation?: RoboticsMissionSimulation;
  approval?: RoboticsApprovalRecord;
  telemetryFrames?: RoboticsTelemetryFrame[];
} = {}): RoboticsMissionStatus {
  const blockers: string[] = [];
  const evidence: string[] = [];
  const simulation = input.simulation;
  const approval = input.approval;
  const frames = input.telemetryFrames ?? [];
  if (!simulation) blockers.push("mission simulation is missing");
  else {
    evidence.push(`simulation:${simulation.status}`);
    if (simulation.status === "fail") blockers.push("mission simulation failed");
  }
  if (!approval) blockers.push("motion approval is missing");
  else {
    evidence.push(`approval:${approval.decision}`);
    if (approval.decision !== "approve") blockers.push(`approval decision is ${approval.decision}`);
  }
  const rejectedFrames = frames.filter((frame) => !frame.accepted);
  if (rejectedFrames.length) blockers.push("one or more telemetry frames violate schema");
  if (frames.length) evidence.push(`telemetryFrames:${frames.length}`);
  const completed = frames.some((frame) =>
    frame.topic === "robotics.work.state" && frame.payload.state === "completed",
  );
  return {
    missionId: input.missionId || simulation?.missionId || "robotics-mission",
    state: blockers.length ? "blocked" : completed ? "completed" : approval ? "approved" : simulation ? "simulated" : "planned",
    blockers,
    evidence,
    nextActions: blockers.length
      ? ["resolve blockers", "rerun simulation", "record approval"]
      : completed
        ? ["close quality, transport, and invoice records"]
        : ["dispatch mission", "stream telemetry", "monitor safety events"],
  };
}

export function closeRoboticsFulfillment(input: {
  closeId?: string;
  requestId?: string;
  records?: string[];
} = {}): RoboticsFulfillmentClose {
  const records = new Set(input.records ?? []);
  const requiredRecords = [
    "qms.qualityRelease",
    "tms.proofOfDelivery",
    "erp.salesOrder",
    "erp.invoice",
  ];
  const missingRecords = requiredRecords.filter((record) => !records.has(record));
  return {
    closeId: input.closeId || `robotics-close-${Date.now().toString(36)}`,
    requestId: input.requestId || "robotics-request",
    status: missingRecords.length ? "blocked" : "ready-to-invoice",
    requiredRecords,
    missingRecords,
    auditAction: missingRecords.length ? "robotics.fulfillment.blocked" : "robotics.fulfillment.readyToInvoice",
  };
}
