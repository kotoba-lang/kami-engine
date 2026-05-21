import { describe, expect, it } from "vitest";
import {
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
} from "./index.js";

describe("kami-engine-sdk manufacturing", () => {
  it("creates a 3D manufacturing cell with workflow and safety zones", () => {
    const plan = createManufacturingCellPlan({
      cellId: "cell-test",
      partEnvelope: { widthMm: 100, heightMm: 80, depthMm: 20, material: "aluminum-6061" },
      devices: [
        { id: "cnc-a", kind: "cnc-mill" },
        { id: "printer-a", kind: "3d-printer" },
        { id: "arm-a", kind: "robot-arm" },
        { id: "mh-a", kind: "material-handling" },
      ],
    });

    expect(plan.cellId).toBe("cell-test");
    expect(plan.sceneUnits).toBe("millimeters");
    expect(plan.workflow).toHaveLength(4);
    expect(plan.safetyZones).toHaveLength(4);
    expect(plan.integrations).toContain("kami-cam gcode post");
  });

  it("generates CNC and 3D printer output plans", () => {
    const cnc = createManufacturingOutputPlan({
      deviceKind: "cnc-mill",
      programNumber: 7,
      partEnvelope: { widthMm: 100, heightMm: 80, depthMm: 20 },
    });
    expect(cnc.postProcessor).toBe("fanuc");
    expect(cnc.program).toContain("O0007");
    expect(cnc.program).toContain("G21");

    const printer = createManufacturingOutputPlan({
      deviceKind: "3d-printer",
      partEnvelope: { widthMm: 40, heightMm: 30, depthMm: 12, material: "pla" },
    });
    expect(printer.postProcessor).toBe("marlin");
    expect(printer.program).toContain("M104");
  });

  it("generates robot and material-handling JSON programs", () => {
    const robot = createManufacturingOutputPlan({
      deviceKind: "robot-arm",
      partEnvelope: { widthMm: 80, heightMm: 60, depthMm: 16 },
    });
    expect(robot.postProcessor).toBe("robot-json");
    expect(JSON.parse(robot.program).waypoints).toHaveLength(4);

    const materialHandling = createManufacturingOutputPlan({
      deviceKind: "material-handling",
      partEnvelope: { widthMm: 80, heightMm: 60, depthMm: 16 },
    });
    expect(materialHandling.postProcessor).toBe("material-flow-json");
  });

  it("creates an industrial software integration plan", () => {
    const plan = createIndustrialSoftwareIntegrationPlan({ planId: "software-plan" });
    expect(plan.planId).toBe("software-plan");
    expect(plan.nodes.map((node) => node.domain)).toContain("cad");
    expect(plan.nodes.map((node) => node.domain)).toContain("mes");
    expect(plan.nodes.map((node) => node.domain)).toContain("drone-control");
    expect(plan.nodes.map((node) => node.domain)).toContain("autonomous-driving");
    expect(plan.canonicalRecords).toContain("cam.machineProgram");
    expect(plan.controlBoundaries.length).toBeGreaterThan(0);
  });

  it("creates logistics route plans for drones and autonomous vehicles", () => {
    const droneRoute = createLogisticsRoutePlan({
      routeId: "route-drone",
      assetKind: "drone",
      waypoints: [
        { id: "dock", label: "Dock", lat: 35.0, lng: 139.0 },
        { id: "roof", label: "Roof", lat: 35.001, lng: 139.002 },
      ],
    });
    expect(droneRoute.mode).toBe("air");
    expect(droneRoute.warnings[0]).toContain("airspace");

    const avRoute = createLogisticsRoutePlan({ assetKind: "autonomous-vehicle" });
    expect(avRoute.mode).toBe("road");
    expect(avRoute.segments[0].constraints).toContain("ODD");
  });

  it("creates autonomy operation plans", () => {
    const drone = createAutonomyOperationPlan({ assetKind: "drone", missionType: "delivery" });
    expect(drone.commandProtocol).toBe("mavlink-json");
    expect(drone.safetyEnvelope.requiresHumanApproval).toBe(true);

    const vehicle = createAutonomyOperationPlan({ assetKind: "autonomous-vehicle", missionType: "yard-transfer" });
    expect(vehicle.commandProtocol).toBe("ros2-action-json");
    expect(vehicle.telemetryTopics).toContain("mission.state");
  });

  it("creates a robotics business process plan for KAMI review and MCP tools", () => {
    const plan = createRoboticsWorkProcessPlan({
      planId: "robotics-plan",
      processes: ["sales", "manufacturing", "transport", "finance"],
    });
    expect(plan.planId).toBe("robotics-plan");
    expect(plan.forms.map((form) => form.process)).toEqual(["sales", "manufacturing", "transport", "finance"]);
    expect(plan.mcpTools).toContain("robotics.workflow.plan");
    expect(plan.mcpTools).toContain("robotics.process.dependencies");
    expect(plan.bpmnProcesses[0]).toContain("planRoboticsBusinessProcess.bpmn");
    expect(plan.kamiReview.sceneNodes.length).toBeGreaterThan(4);
    expect(plan.integrationRecords).toContain("erp.salesOrder");
    expect(plan.dependencies.map((dependency) => dependency.id)).toContain("dep-transport-finance");
    expect(plan.missingPrerequisites.map((dependency) => dependency.id)).toContain("dep-production-manufacturing");
    expect(plan.approvalGates).toContain("quality release approved before shipment");
  });

  it("renders a KAMI robotics review HTML surface", () => {
    const plan = createRoboticsWorkProcessPlan({ planId: "robotics-html" });
    const html = createRoboticsKamiReviewHtml(plan);
    expect(html).toContain("<!doctype html>");
    expect(html).toContain("Robot workcell");
    expect(html).toContain("Dependencies");
    expect(html).toContain("robotics.asset.pose");
    expect(html).toContain("Approval Gates");
  });

  it("creates robotics telemetry, approval, and simulation chain outputs", () => {
    const telemetry = createRoboticsTelemetrySchema();
    expect(telemetry.topics.map((topic) => topic.topic)).toContain("robotics.safety.event");

    const approval = createRoboticsApprovalRecord({
      requestId: "req-approval",
      decision: "approve",
      approverDid: "did:web:ops.gftd.ai",
    });
    expect(approval.auditAction).toBe("robotics.approval.approve");
    expect(approval.requiredEvidence).toContain("mission simulation pass or reviewed");

    const simulation = simulateRoboticsMission({
      missionId: "mission-1",
      telemetry,
    });
    expect(simulation.status).toBe("pass");
    expect(simulation.checks.map((check) => check.id)).toContain("kami-scene");
    expect(simulation.requiresHumanApproval).toBe(false);
  });

  it("ingests telemetry, computes mission status, and closes fulfillment", () => {
    const simulation = simulateRoboticsMission({ missionId: "mission-status" });
    const approval = createRoboticsApprovalRecord({ requestId: "req-status", decision: "approve" });
    const frame = ingestRoboticsTelemetry({
      topic: "robotics.work.state",
      payload: {
        missionId: "mission-status",
        state: "completed",
        stepId: "handoff",
        timestamp: "2026-04-25T00:00:00Z",
      },
    });
    expect(frame.accepted).toBe(true);

    const status = createRoboticsMissionStatus({
      missionId: "mission-status",
      simulation,
      approval,
      telemetryFrames: [frame],
    });
    expect(status.state).toBe("completed");

    const close = closeRoboticsFulfillment({
      requestId: "req-status",
      records: ["qms.qualityRelease", "tms.proofOfDelivery", "erp.salesOrder", "erp.invoice"],
    });
    expect(close.status).toBe("ready-to-invoice");
  });
});
