/**
 * webvr/types.ts — Choice-based VR incident response scenario types.
 *
 * Scenario shape: a directed graph of `IncidentNode`s, each with 2-4
 * `IncidentChoice`s. Selecting a choice advances the StateGraph to a target
 * node, accruing `kpi` deltas (mttd / mttr / downtimeMin / regulatoryRisk /
 * dataLossGb). Terminal nodes are flagged `terminal: 'success' | 'partial' |
 * 'failure'` and end the run.
 *
 * Float discipline: AT Lexicon disallows float (see root CLAUDE.md). All
 * KPI deltas are integers with explicit units. Time = seconds, cost = JPY x10
 * (yen-deci), risk = permille (0-1000).
 */

export type Stage =
  | 'detect'        // NIST CSF DETECT (DE.AE / DE.CM)
  | 'triage'        // RESPOND.Analysis (RS.AN)
  | 'contain'       // RESPOND.Mitigation (RS.MI)
  | 'communicate'   // RESPOND.Communications (RS.CO) — METI / IPA / police
  | 'eradicate'     // RESPOND.Mitigation continued
  | 'recover'       // RECOVER (RC.RP / RC.IM)
  | 'govern';       // GOVERN (GV) — lessons learned

export type Severity = 'info' | 'low' | 'medium' | 'high' | 'critical';

export type LocationKind =
  | 'scadaRoom'     // 中央監視室 (SCADA / MES HMI)
  | 'cleanroom'     // クリーンルーム (露光・CVD・エッチング)
  | 'chemicalYard'  // 薬液タンクヤード (フォトレジスト・現像液)
  | 'utilityRoom'   // 用力室 (純水・特殊ガス・排ガス処理)
  | 'serverRoom'    // 情報系サーバ室 (Purdue L4-L5)
  | 'executiveRoom' // 役員会議室
  | 'press';        // プレス対応室

export interface IncidentKpi {
  /** Mean Time To Detect (seconds since incident t0). */
  mttdSec: number;
  /** Mean Time To Respond / Recover (seconds since incident t0). */
  mttrSec: number;
  /** Production downtime (minutes). */
  downtimeMin: number;
  /** Regulatory exposure (permille, 0-1000) — METI / 警察庁 / GHS / 消防法 / 個情法. */
  regulatoryRiskPermille: number;
  /** Estimated data exfiltration (GB). 0 if pre-exfil. */
  dataLossGb: number;
  /** Estimated direct cost (JPY x10, i.e. yen-deci). 100 = 10 yen, 100000 = 10,000 yen. */
  costYenDeci: number;
}

export interface IncidentChoice {
  /** Stable choice id (camelCase). */
  id: string;
  /** Short human-readable label (Japanese primary). */
  label: string;
  /** Optional gloss / hint (one line). */
  hint?: string;
  /** Target node id; choosing this advances StateGraph there. */
  next: string;
  /** KPI delta applied on selection. */
  delta: Partial<IncidentKpi>;
  /** Pedagogic correctness grade: 'best' = playbook, 'ok' = acceptable, 'bad' = anti-pattern. */
  grade: 'best' | 'ok' | 'bad';
  /** Short rationale shown post-selection. */
  rationale: string;
  /** Reference to the SSoT framework rule that grades the choice. */
  reference?: {
    framework: 'NIST-CSF-2.0' | 'IEC-62443-3-3' | 'METI-Factory-CSG' | 'IPA-J-CSIP' | 'JPCERT';
    control: string; // e.g. "RS.MI-1", "SR 7.3", "F-7"
  };
}

export interface IncidentNode {
  /** Stable node id (camelCase). */
  id: string;
  /** NIST CSF stage. */
  stage: Stage;
  /** Severity at this node. */
  severity: Severity;
  /** VR location to teleport the player to before showing this node. */
  location: LocationKind;
  /** Multi-line briefing (Japanese primary), shown as a floating panel. */
  briefing: string;
  /** Available choices (2-4). Empty iff terminal. */
  choices: IncidentChoice[];
  /** Set iff this is a terminal node. */
  terminal?: 'success' | 'partial' | 'failure';
  /** Cinematic camera hint for the VR scene (string label, consumed by the host renderer). */
  cameraHint?: 'overview' | 'console' | 'tankClose' | 'doorway' | 'briefingTable';
  /**
   * Optional kami-cine generation hint for this node. When set, the Pregel
   * graph calls `cineBridge.generateScene(...)` and attaches the resulting
   * artifacts to the SceneDescriptor. With `cineBridge` unset the field is
   * ignored.
   */
  cine?: {
    prompt: string;
    style?: string;
    /** Frames of temporal field to request (default 1 = still). */
    frames?: number;
  };

  /**
   * Optional per-node visual effects layered on top of the location's
   * splat backdrop. Each kind is a string label that the host renderer
   * maps to its own effect implementation (kami-engine-sdk no longer
   * ships a built-in renderer registry — see ADR-0031 + the "独自レンダラ
   * 禁止" rule in 40-engine/kami-engine CLAUDE.md).
   */
  effects?: NodeEffectKind[];
}

/**
 * Per-node visual effect labels. Each is a stable string id that the host
 * renderer (a `kami-app-{game}` wgpu surface, per the "独自レンダラ禁止"
 * rule in 40-engine/kami-engine CLAUDE.md) maps to its own effect.
 * Stack multiple effects per node by listing them in `IncidentNode.effects`.
 *
 * - `redAlarm`        — Pulsing red beacon overhead.
 * - `orangeSmoke`     — Rising particle column (chemical fire / runaway).
 * - `dataLeak`        — Animated lines streaming outward from a console.
 * - `pressFlash`      — Random white camera-flash pops.
 * - `dawnLight`       — Warm directional beam (post-incident calm).
 * - `greenCheck`      — Soft green expanding ring (recovery confirmed).
 * - `monitorFlicker`  — Multi-monitor color flicker (control-room alert).
 */
export type NodeEffectKind =
  | 'redAlarm'
  | 'orangeSmoke'
  | 'dataLeak'
  | 'pressFlash'
  | 'dawnLight'
  | 'greenCheck'
  | 'monitorFlicker';

export interface IncidentScenario {
  /** Scenario id (NSID-style). */
  id: string;
  /** Display title. */
  title: string;
  /** One-paragraph synopsis. */
  synopsis: string;
  /** Entry node id; must exist in `nodes`. */
  start: string;
  /** All scenario nodes keyed by id. */
  nodes: Record<string, IncidentNode>;
}

export interface IncidentDecisionLog {
  nodeId: string;
  choiceId: string;
  /** Wall-clock ISO 8601 of the decision. */
  takenAt: string;
  /** KPI snapshot after applying the choice's delta. */
  kpiAfter: IncidentKpi;
  /** Grade of the choice. */
  grade: IncidentChoice['grade'];
}

export interface IncidentState {
  /** Current node id. */
  current: string;
  /** Accumulated KPI. */
  kpi: IncidentKpi;
  /** Ordered decision history. */
  history: IncidentDecisionLog[];
  /** True once a terminal node has been reached. */
  done: boolean;
  /** Terminal outcome (only set when done). */
  outcome?: IncidentNode['terminal'];
}

export const ZERO_KPI: IncidentKpi = Object.freeze({
  mttdSec: 0,
  mttrSec: 0,
  downtimeMin: 0,
  regulatoryRiskPermille: 0,
  dataLossGb: 0,
  costYenDeci: 0,
});

/** Apply a `delta` to a KPI, returning a new frozen KPI snapshot. */
export function applyKpiDelta(base: IncidentKpi, delta: Partial<IncidentKpi>): IncidentKpi {
  return Object.freeze({
    mttdSec: base.mttdSec + (delta.mttdSec ?? 0),
    mttrSec: base.mttrSec + (delta.mttrSec ?? 0),
    downtimeMin: base.downtimeMin + (delta.downtimeMin ?? 0),
    regulatoryRiskPermille: clampPermille(
      base.regulatoryRiskPermille + (delta.regulatoryRiskPermille ?? 0),
    ),
    dataLossGb: Math.max(0, base.dataLossGb + (delta.dataLossGb ?? 0)),
    costYenDeci: Math.max(0, base.costYenDeci + (delta.costYenDeci ?? 0)),
  });
}

function clampPermille(v: number): number {
  if (v < 0) return 0;
  if (v > 1000) return 1000;
  return v | 0;
}
