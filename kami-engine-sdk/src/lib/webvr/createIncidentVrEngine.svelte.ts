/**
 * webvr/createIncidentVrEngine.svelte.ts — Svelte 5 runes builder that
 * runs the `INCIDENT_GRAPH` Pregel pipeline as a *headless* engine and
 * republishes a `SceneDescriptor` on every transition.
 *
 * The renderer is intentionally pluggable: kami-engine-sdk owns the
 * scenario logic, KPI math, cine-bridge resolution and decision log;
 * the actual scene rendering belongs to a `kami-app-{game}` crate
 * (kami-engine wgpu, per ADR-0031 + the "独自レンダラ禁止 — kami-render
 * wgpu PBR pipeline が唯一" constitutional rule in 40-engine/kami-engine
 * CLAUDE.md). Callers pass an `onScene` callback to receive the latest
 * descriptor and drive their own wgpu / WebXR surface.
 *
 * Usage (Svelte 5):
 *
 * ```svelte
 * <script lang="ts">
 *   import { createIncidentVrEngine } from '@etzhayyim/kami-engine-sdk/webvr';
 *   import { SEMI_PLANT_INCIDENT } from '$lib/scenarios/semiconductor-chem-plant';
 *   import { mountWebvrSurface } from '$lib/kami-app-webvr';
 *
 *   let canvas: HTMLCanvasElement;
 *   const engine = createIncidentVrEngine({
 *     scenario: SEMI_PLANT_INCIDENT,
 *     onScene: (scene) => surface?.update(scene),
 *   });
 *   let surface: ReturnType<typeof mountWebvrSurface> | undefined;
 *   $effect(() => {
 *     if (!canvas) return;
 *     surface = mountWebvrSurface(canvas, { onSelect: engine.select });
 *     return () => surface?.dispose();
 *   });
 * </script>
 *
 * <canvas bind:this={canvas} style="width:100vw;height:100vh"></canvas>
 * ```
 */

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
  /** Called on every published `SceneDescriptor` (initial + transitions + cine landings). */
  onScene?: (scene: SceneDescriptor) => void;
  /** Optional sink for op-log entries (XRPC dispatch, AT Record persist). */
  onOpLog?: IncidentBridge['onOpLog'];
  /**
   * Optional kami-cine pipeline bridge. When supplied, every IncidentNode
   * that declares a `cine` prompt has its scene artifacts resolved before
   * the renderer paints. Pass `createMockCineBridge()` in dev, or
   * `createCineBridge({ endpoint, token })` against `studio.etzhayyim.com`.
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

  /** Restart the scenario from `start`. */
  reset(): void;
  /** Programmatic choice select — also wired by the host renderer's gaze/tap. */
  select(choiceId: string): void;
}

export function createIncidentVrEngine(opts: CreateIncidentVrEngineOpts): IncidentVrEngine {
  let state = $state<IncidentState>(initialState(opts.scenario));
  let scene = $state<SceneDescriptor | undefined>(undefined);

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
    opts.onScene?.(scene);
    opts.onOpLog?.({
      op: state.done ? 'terminate' : 'enter',
      nodeId: state.current,
      kpi: state.kpi,
      at: new Date().toISOString(),
    });
    void (async () => {
      const cine = await resolveCine();
      if (!cine) return;
      scene = buildScene({ cine });
      opts.onScene?.(scene);
      const panel = await resolvePanel(cine);
      if (!panel) return;
      scene = buildScene({ cine, panel });
      opts.onScene?.(scene);
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

  function reset() {
    state = initialState(opts.scenario);
    publish();
  }

  // Re-export the compiled LangGraph so callers can attach to LangGraph
  // Studio or run a headless dry-run from the same engine instance.
  void INCIDENT_GRAPH;

  // Emit the initial descriptor synchronously so callers can attach a
  // renderer in a Svelte `$effect` without missing the first frame.
  publish();

  return {
    get state() { return state; },
    get scene() { return scene; },
    get history() { return state.history; },
    reset,
    select,
  };
}
