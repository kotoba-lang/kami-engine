/**
 * webvr/incident-pregel.ts — Client-side LangGraph TS (Pregel) StateGraph
 * that drives a choice-based VR incident-response walkthrough.
 *
 * Mirrors the kami-cine 8-stage Pregel pattern (60-apps/ai-gftd-project-mangaka/
 * lg/lg_mangaka/graphs/cine_generate_scene.py) but specialized for an
 * interactive selection scenario rather than a render pipeline:
 *
 *   ┌─ start
 *   │
 *   ├─ s1_loadScenario     — pin scenario + initialize state
 *   ├─ s2_resolveLocation  — current node → VR location + camera hint
 *   ├─ s3_buildScene       — emit scene-update descriptor (rooms + props)
 *   ├─ s4_offerChoices     — surface 2-4 choices on floating panels
 *   ├─ s5_awaitSelection   — interrupt: player picks a choice id
 *   ├─ s6_applyChoice      — KPI delta + history append + advance current
 *   ├─ s7_renderBriefing   — produce briefing descriptor for the new node
 *   ├─ s8_persistLog       — emit op-log entry (terminal also writes outcome)
 *   │
 *   └─ END    (when state.done; else conditional edge back to s2_resolveLocation)
 *
 * The graph is `compiled once` at module load and re-used. Each tick runs
 * one super-step batch up to s4_offerChoices, then halts on an `interrupt`
 * channel until the host (Svelte component) supplies a `selection`. The
 * remaining steps then commit the selection and either loop or finalize.
 *
 * State channels follow the LangGraph `Annotation.Root({...})` idiom;
 * super-steps are pure node callbacks. Side-effects (DOM, Three.js) are
 * routed through the `bridge` callback set in the initial state so the
 * graph itself stays renderable in jsdom for vitest.
 */

import { StateGraph, START, END, Annotation } from '@langchain/langgraph';
import {
  applyKpiDelta,
  ZERO_KPI,
  type IncidentChoice,
  type IncidentDecisionLog,
  type IncidentKpi,
  type IncidentNode,
  type IncidentScenario,
  type IncidentState,
  type LocationKind,
  type NodeEffectKind,
} from './types.js';
import type { CineBridge, CineSceneArtifacts } from './cine-bridge.js';

// ─────────────────────────────────────────────────────────────────────────
// Scene descriptor (consumed by the host renderer; the SDK no longer ships a built-in renderer)

export interface SceneDescriptor {
  location: LocationKind;
  cameraHint?: IncidentNode['cameraHint'];
  /** Floating choice panels, positioned by the renderer. */
  choices: ReadonlyArray<{
    id: string;
    label: string;
    hint?: string;
  }>;
  /** Multi-line briefing text. */
  briefing: string;
  /** Severity tint for the floating UI. */
  severity: IncidentNode['severity'];
  /** Stage badge text. */
  stage: IncidentNode['stage'];
  /** Terminal flag — render outcome screen. */
  terminal?: IncidentNode['terminal'];
  /**
   * Resolved kami-cine artifacts (Stage 1-4) for this node. Present only
   * when both `IncidentNode.cine` and `cineBridge` are configured. The
   * renderer uses `geomArtifact.url` to swap from box-primitives to
   * Gaussian Splat when available, and surfaces `worldArtifact.summary`
   * in the HUD.
   */
  cine?: CineSceneArtifacts;

  /**
   * Resolved kami-cine panel (Stage 5-6, diffusion). Present once
   * `cineBridge.generatePanel(...)` returns; the renderer displays
   * `panelUrl` as a floating illustration plane left of the briefing.
   */
  cinePanel?: import('./cine-bridge.js').CinePanelArtifact;
  /** Per-node visual effects to render. */
  effects?: ReadonlyArray<NodeEffectKind>;
  /** Stable node id — used as seed + cache key for effects. */
  nodeId?: string;
}

export interface IncidentBridge {
  /** Called when the scene should be (re)rendered. */
  onScene?: (scene: SceneDescriptor) => void;
  /** Called after every choice is applied. */
  onChoice?: (decision: IncidentDecisionLog) => void;
  /** Called once on terminal node. */
  onTerminal?: (state: IncidentState) => void;
  /** Optional persistence sink (XRPC / AT Record dispatch). */
  onOpLog?: (entry: {
    op: 'enter' | 'choose' | 'terminate';
    nodeId: string;
    choiceId?: string;
    kpi: IncidentKpi;
    at: string;
  }) => void;
}

// ─────────────────────────────────────────────────────────────────────────
// State annotation

const last = <T>() => ({ reducer: (_a: T, b: T) => b, default: () => undefined as unknown as T });
const append = <T>() => ({
  reducer: (a: T[] | undefined, b: T[] | undefined): T[] => {
    if (!a && !b) return [];
    if (!a) return [...(b as T[])];
    if (!b) return [...a];
    return [...a, ...b];
  },
  default: () => [] as T[],
});

const IncidentStateAnnotation = Annotation.Root({
  scenario: Annotation<IncidentScenario>(last<IncidentScenario>()),
  bridge:   Annotation<IncidentBridge>(last<IncidentBridge>()),
  // pinned at start, advanced by s6_applyChoice:
  current:  Annotation<string>(last<string>()),
  kpi:      Annotation<IncidentKpi>(last<IncidentKpi>()),
  history:  Annotation<IncidentDecisionLog[]>(append<IncidentDecisionLog>()),
  done:     Annotation<boolean>(last<boolean>()),
  outcome:  Annotation<IncidentNode['terminal']>(last<IncidentNode['terminal']>()),
  // per-tick channels:
  pendingScene:     Annotation<SceneDescriptor | undefined>(last<SceneDescriptor | undefined>()),
  pendingSelection: Annotation<string | undefined>(last<string | undefined>()),
});
type _State = typeof IncidentStateAnnotation.State;

// ─────────────────────────────────────────────────────────────────────────
// Super-steps

function _node(s: _State): IncidentNode {
  const n = s.scenario.nodes[s.current];
  if (!n) throw new Error(`webvr: unknown node id "${s.current}" in scenario "${s.scenario.id}"`);
  return n;
}

function _findChoice(node: IncidentNode, choiceId: string): IncidentChoice {
  const c = node.choices.find((x) => x.id === choiceId);
  if (!c) {
    throw new Error(
      `webvr: choice id "${choiceId}" not offered at node "${node.id}" ` +
        `(available: ${node.choices.map((x) => x.id).join(', ') || '<none>'})`,
    );
  }
  return c;
}

const s1_loadScenario = (s: _State): Partial<_State> => {
  // First tick only — initializes when current/kpi/history are unset.
  if (s.current) return {};
  return {
    current: s.scenario.start,
    kpi: ZERO_KPI,
    history: [],
    done: false,
  };
};

const s2_resolveLocation = (_s: _State): Partial<_State> => {
  // No-op placeholder; location is read in s3 via _node(s).location.
  return {};
};

const s3_buildScene = (s: _State): Partial<_State> => {
  const n = _node(s);
  const scene: SceneDescriptor = {
    location: n.location,
    cameraHint: n.cameraHint,
    choices: n.choices.map((c) => ({ id: c.id, label: c.label, hint: c.hint })),
    briefing: n.briefing,
    severity: n.severity,
    stage: n.stage,
    terminal: n.terminal,
  };
  return { pendingScene: scene };
};

const s4_offerChoices = (s: _State): Partial<_State> => {
  if (s.pendingScene && s.bridge?.onScene) s.bridge.onScene(s.pendingScene);
  if (s.bridge?.onOpLog) {
    s.bridge.onOpLog({ op: 'enter', nodeId: s.current, kpi: s.kpi, at: _now() });
  }
  return {};
};

const s5_awaitSelection = (s: _State): Partial<_State> => {
  // Terminal: skip selection.
  const n = _node(s);
  if (n.terminal || n.choices.length === 0) return {};
  // Non-terminal: this node yields a no-op; the host calls
  // engine.select(choiceId) which re-invokes the graph with
  // `pendingSelection` set. If pendingSelection is missing here, the
  // outer engine treats the graph as "paused" and returns.
  return {};
};

const s6_applyChoice = (s: _State): Partial<_State> => {
  const n = _node(s);
  if (n.terminal || n.choices.length === 0) {
    return { done: true, outcome: n.terminal ?? 'success' };
  }
  if (!s.pendingSelection) return {}; // paused — wait for host
  const choice = _findChoice(n, s.pendingSelection);
  const nextKpi = applyKpiDelta(s.kpi, choice.delta);
  const decision: IncidentDecisionLog = {
    nodeId: n.id,
    choiceId: choice.id,
    takenAt: _now(),
    kpiAfter: nextKpi,
    grade: choice.grade,
  };
  return {
    current: choice.next,
    kpi: nextKpi,
    history: [decision],
    pendingSelection: undefined,
  };
};

const s7_renderBriefing = (s: _State): Partial<_State> => {
  // The new current node's briefing is produced by s3 on the next loop.
  // Notify the bridge of the just-applied decision here.
  const last_ = s.history?.[s.history.length - 1];
  if (last_ && s.bridge?.onChoice) s.bridge.onChoice(last_);
  return {};
};

const s8_persistLog = (s: _State): Partial<_State> => {
  const last_ = s.history?.[s.history.length - 1];
  if (last_ && s.bridge?.onOpLog) {
    s.bridge.onOpLog({
      op: 'choose',
      nodeId: last_.nodeId,
      choiceId: last_.choiceId,
      kpi: last_.kpiAfter,
      at: last_.takenAt,
    });
  }
  const n = _node(s);
  if (n.terminal && !s.done) {
    if (s.bridge?.onTerminal) s.bridge.onTerminal({
      current: s.current,
      kpi: s.kpi,
      history: s.history,
      done: true,
      outcome: n.terminal,
    });
    if (s.bridge?.onOpLog) {
      s.bridge.onOpLog({ op: 'terminate', nodeId: n.id, kpi: s.kpi, at: _now() });
    }
    return { done: true, outcome: n.terminal };
  }
  return {};
};

function _now(): string {
  return new Date().toISOString();
}

// ─────────────────────────────────────────────────────────────────────────
// Graph compilation

const _builder = new StateGraph(IncidentStateAnnotation)
  .addNode('s1_loadScenario', s1_loadScenario)
  .addNode('s2_resolveLocation', s2_resolveLocation)
  .addNode('s3_buildScene', s3_buildScene)
  .addNode('s4_offerChoices', s4_offerChoices)
  .addNode('s5_awaitSelection', s5_awaitSelection)
  .addNode('s6_applyChoice', s6_applyChoice)
  .addNode('s7_renderBriefing', s7_renderBriefing)
  .addNode('s8_persistLog', s8_persistLog)
  .addEdge(START, 's1_loadScenario')
  .addEdge('s1_loadScenario', 's2_resolveLocation')
  .addEdge('s2_resolveLocation', 's3_buildScene')
  .addEdge('s3_buildScene', 's4_offerChoices')
  .addEdge('s4_offerChoices', 's5_awaitSelection')
  .addEdge('s5_awaitSelection', 's6_applyChoice')
  .addEdge('s6_applyChoice', 's7_renderBriefing')
  .addEdge('s7_renderBriefing', 's8_persistLog')
  .addConditionalEdges('s8_persistLog', (s: _State) => (s.done ? END : 's2_resolveLocation'), {
    [END]: END,
    s2_resolveLocation: 's2_resolveLocation',
  });

export const INCIDENT_GRAPH = _builder.compile();

// ─────────────────────────────────────────────────────────────────────────
// Stateless helper (used by tests + a thin facade in
// createIncidentVrEngine.svelte.ts). Pure function over scenario state —
// no Three.js, no DOM.

export function applySelection(
  scenario: IncidentScenario,
  state: IncidentState,
  choiceId: string,
): IncidentState {
  if (state.done) return state;
  const node = scenario.nodes[state.current];
  if (!node) throw new Error(`unknown current node "${state.current}"`);
  if (node.terminal) return { ...state, done: true, outcome: node.terminal };
  const choice = _findChoice(node, choiceId);
  const kpi = applyKpiDelta(state.kpi, choice.delta);
  const decision: IncidentDecisionLog = {
    nodeId: node.id,
    choiceId: choice.id,
    takenAt: _now(),
    kpiAfter: kpi,
    grade: choice.grade,
  };
  const next = scenario.nodes[choice.next];
  if (!next) throw new Error(`choice "${choice.id}" points at unknown node "${choice.next}"`);
  return {
    current: choice.next,
    kpi,
    history: [...state.history, decision],
    done: !!next.terminal,
    outcome: next.terminal,
  };
}

/** Construct an initial IncidentState for a scenario. */
export function initialState(scenario: IncidentScenario): IncidentState {
  return {
    current: scenario.start,
    kpi: ZERO_KPI,
    history: [],
    done: false,
  };
}
