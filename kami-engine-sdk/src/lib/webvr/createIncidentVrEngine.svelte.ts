/**
 * webvr/createIncidentVrEngine.svelte.ts — Svelte 5 runes builder that
 * wires a `mountIncidentScene` Three.js scene to the `INCIDENT_GRAPH`
 * Pregel pipeline.
 *
 * Usage (Svelte 5):
 *
 * ```svelte
 * <script lang="ts">
 *   import { createIncidentVrEngine } from '@gftdcojp/kami-engine-sdk/webvr';
 *   import { SEMI_PLANT_INCIDENT } from '$lib/scenarios/semiconductor-chem-plant';
 *   let canvas: HTMLCanvasElement;
 *   const engine = createIncidentVrEngine({ scenario: SEMI_PLANT_INCIDENT });
 *   $effect(() => { if (canvas) engine.attach(canvas); return () => engine.detach(); });
 * </script>
 *
 * <canvas bind:this={canvas} style="width:100vw;height:100vh"></canvas>
 * ```
 */

import { mountIncidentScene, type SceneHandle } from './webvr-scene.js';
import {
  INCIDENT_GRAPH,
  applySelection,
  initialState,
  type IncidentBridge,
  type SceneDescriptor,
} from './incident-pregel.js';
import type {
  IncidentDecisionLog,
  IncidentScenario,
  IncidentState,
} from './types.js';
import type { CineBridge, CineSceneArtifacts, CinePanelArtifact } from './cine-bridge.js';

export interface CreateIncidentVrEngineOpts {
  scenario: IncidentScenario;
  /** Optional sink for op-log entries (XRPC dispatch, AT Record persist). */
  onOpLog?: IncidentBridge['onOpLog'];
  /** Gaze-dwell ms before auto-confirm. Default 3000 (3s). */
  gazeDwellMs?: number;
  /**
   * Per-node selection deadline in ms. Auto-fires the inaction choice on
   * timeout. Default 10000 (10s). Set 0 to disable.
   */
  selectionDeadlineMs?: number;
  /** Show Enter VR button. Default true. */
  enableVrButton?: boolean;
  /** Auto-speak the briefing on scene transition. Default true. */
  narrate?: boolean;
  /** BCP-47 voice language. Default 'ja-JP'. */
  narrateLang?: string;
  /** Transition fade duration in ms. Default 280, 0 disables. */
  transitionFadeMs?: number;
  /** Render the Gaussian splat ambient backdrop. Default true. */
  useSparkBackdrop?: boolean;
  /** Splat budget per backdrop. Default 6000. */
  sparkSplatBudget?: number;
  /**
   * Optional kami-cine pipeline bridge. When supplied, every IncidentNode
   * that declares a `cine` prompt has its scene artifacts resolved before
   * the renderer paints. Pass `createMockCineBridge()` in dev, or
   * `createCineBridge({ endpoint, token })` against `studio.gftd.ai`.
   */
  cineBridge?: CineBridge;
}

export interface IncidentVrEngine {
  /** Reactive scenario state ($state-wrapped). */
  readonly state: IncidentState;
  /** Most recent scene descriptor (for HUD overlays). */
  readonly scene: SceneDescriptor | undefined;
  /** Decision history (alias of state.history). */
  readonly history: IncidentDecisionLog[];

  /** Attach to a canvas — call once on mount. */
  attach(canvas: HTMLCanvasElement): void;
  /** Tear down the renderer + listeners. */
  detach(): void;
  /** Restart the scenario from `start`. */
  reset(): void;
  /** Programmatic choice select (also called by gaze/tap from the scene). */
  select(choiceId: string): void;
}

export function createIncidentVrEngine(opts: CreateIncidentVrEngineOpts): IncidentVrEngine {
  let state = $state<IncidentState>(initialState(opts.scenario));
  let scene = $state<SceneDescriptor | undefined>(undefined);
  let handle: SceneHandle | undefined;
  let canvasEl: HTMLCanvasElement | undefined;

  // Per-node caches — both Stage 1-4 (cine) and Stage 5-6 (panel).
  const cineCache  = new Map<string, CineSceneArtifacts>();
  const panelCache = new Map<string, CinePanelArtifact>();

  function buildScene(extra?: { cine?: CineSceneArtifacts; panel?: CinePanelArtifact }): SceneDescriptor {
    const node = opts.scenario.nodes[state.current];
    if (!node) throw new Error(`webvr: unknown node "${state.current}"`);
    return {
      location: node.location,
      cameraHint: node.cameraHint,
      choices: node.choices.map((c) => ({ id: c.id, label: c.label, hint: c.hint })),
      briefing: node.briefing,
      severity: node.severity,
      stage: node.stage,
      terminal: node.terminal,
      cine:      extra?.cine  ?? cineCache.get(node.id),
      cinePanel: extra?.panel ?? panelCache.get(node.id),
      effects:   node.effects,
      nodeId:    node.id,
    };
  }

  async function resolveCine(): Promise<CineSceneArtifacts | undefined> {
    const node = opts.scenario.nodes[state.current];
    if (!node?.cine || !opts.cineBridge) return undefined;
    const cached = cineCache.get(node.id);
    if (cached) return cached;
    try {
      const artifacts = await opts.cineBridge.generateScene({
        prompt: node.cine.prompt,
        style: node.cine.style,
        frameStart: 1,
        frameEnd: node.cine.frames ?? 1,
        fps: 24,
        dryRun: true,
      });
      cineCache.set(node.id, artifacts);
      return artifacts;
    } catch (e) {
      // eslint-disable-next-line no-console
      console.warn('[kami webvr] cineBridge.generateScene failed', e);
      return undefined;
    }
  }

  async function resolvePanel(cine: CineSceneArtifacts): Promise<CinePanelArtifact | undefined> {
    const node = opts.scenario.nodes[state.current];
    if (!node?.cine || !opts.cineBridge) return undefined;
    const cached = panelCache.get(node.id);
    if (cached) return cached;
    try {
      const p = await opts.cineBridge.generatePanel({
        pipelineRunId: cine.pipelineRunId,
        panelRkey:     `panel-${node.id}`,
        framing:       cine.worldArtifact?.cameraHint,
        prompt:        node.cine.prompt,
        moodPalette:   cine.worldArtifact?.moodPalette,
      });
      panelCache.set(node.id, p);
      return p;
    } catch (e) {
      // eslint-disable-next-line no-console
      console.warn('[kami webvr] cineBridge.generatePanel failed', e);
      return undefined;
    }
  }

  function publish() {
    scene = buildScene();
    handle?.update(scene);
    opts.onOpLog?.({
      op: state.done ? 'terminate' : 'enter',
      nodeId: state.current,
      kpi: state.kpi,
      at: new Date().toISOString(),
    });
    // Async cine resolution — Stage 1-4 first, then chain Stage 5-6.
    // Each landing re-emits the SceneDescriptor so the renderer can swap
    // mood / camera / gsplat / panel illustration on arrival.
    void (async () => {
      const cine = await resolveCine();
      if (!cine) return;
      scene = buildScene({ cine });
      handle?.update(scene);
      const panel = await resolvePanel(cine);
      if (!panel) return;
      scene = buildScene({ cine, panel });
      handle?.update(scene);
    })();
  }

  function select(choiceId: string) {
    if (state.done) return;
    const before = state;
    const next = applySelection(opts.scenario, before, choiceId);
    state = next;
    const last = next.history[next.history.length - 1];
    if (last && opts.onOpLog) {
      opts.onOpLog({
        op: 'choose',
        nodeId: last.nodeId,
        choiceId: last.choiceId,
        kpi: last.kpiAfter,
        at: last.takenAt,
      });
    }
    publish();
  }

  function attach(canvas: HTMLCanvasElement) {
    canvasEl = canvas;
    handle = mountIncidentScene(canvas, {
      onSelect: select,
      gazeDwellMs: opts.gazeDwellMs,
      selectionDeadlineMs: opts.selectionDeadlineMs,
      enableVrButton: opts.enableVrButton,
      narrate: opts.narrate,
      narrateLang: opts.narrateLang,
      transitionFadeMs: opts.transitionFadeMs,
      useSparkBackdrop: opts.useSparkBackdrop,
      sparkSplatBudget: opts.sparkSplatBudget,
      initial: buildScene(),
    });
    publish();
  }

  function detach() {
    handle?.dispose();
    handle = undefined;
    canvasEl = undefined;
  }

  function reset() {
    state = initialState(opts.scenario);
    publish();
  }

  // Re-export the compiled LangGraph so callers can attach to LangGraph
  // Studio or run a headless dry-run from the same engine instance.
  void INCIDENT_GRAPH;

  return {
    get state() { return state; },
    get scene() { return scene; },
    get history() { return state.history; },
    attach,
    detach,
    reset,
    select,
  };
}
