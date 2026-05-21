/**
 * webvr/cine-bridge.ts — Thin client for the kami-cine pipeline
 * (`gftd:kami-cine@1.0.0`, stages 1-4: worldModel → usdScene → neuralGeom
 * → temporalField). Wires the cyber-drill VR to a remote
 * `studio.gftd.ai` LangGraph pod when available, or falls back to a
 * deterministic mock so the demo runs offline.
 *
 * Pipeline reference: `40-engine/kami-engine/wit/cine/package.wit`.
 * XRPC entry point on the mangaka pod:
 *   POST /xrpc/ai.gftd.mangaka.cineGenerateScene
 *
 * Float discipline (AT Lexicon): bbox is integer cm, frame range is
 * integer frames, fps is integer.
 */

// ─────────────────────────────────────────────────────────────────────────
// Artifact shapes (subset of WIT, JSON-flattened)

export interface CineWorldArtifact {
  /** B2 / R2 content-addressed CID. */
  modelCid: string;
  /** Stable seed used by downstream stages. */
  seed: number;
  /** Tokens consumed at expansion. */
  tokenCount: number;
  /** LLM-extracted scene summary (for HUD display). */
  summary?: string;
  /** Mood / lighting palette. */
  moodPalette?: string[];
  /** Camera hint (FullShot / MediumShot / Closeup / OverShoulder / Dutch). */
  cameraHint?: string;
}

export interface CineUsdArtifact {
  usdaCid: string;
  usdcCid: string;
  layerCount: number;
  /** Axis-aligned bbox in cm. */
  bboxCm: { minX: number; minY: number; minZ: number; maxX: number; maxY: number; maxZ: number };
}

export interface CineGeomArtifact {
  assetCid: string;
  format: 'gaussianSplat' | 'nerf' | 'sdf' | 'mesh' | 'hybrid';
  pointCount?: number;
  /** Optional HTTPS URL to fetch the asset (B2 / R2 public). */
  url?: string;
}

export interface CineTemporalArtifact {
  assetCid: string;
  format: 'gaussian4d' | 'dynamicNerf' | 'voxelGrid4d' | 'neuralFlow';
  frameStart: number;
  frameEnd: number;
  fps: number;
}

/** Bundled stage 1-4 result returned by `generateScene`. */
export interface CineSceneArtifacts {
  pipelineRunId: string;
  /** "scene_ready" | "error" — mirror of the pod's status field. */
  status: 'scene_ready' | 'error' | 'mock';
  worldArtifact?: CineWorldArtifact;
  usdArtifact?: CineUsdArtifact;
  geomArtifact?: CineGeomArtifact;
  temporalArtifact?: CineTemporalArtifact;
  /** Set when `status === 'error'`. */
  error?: string;
}

export interface CineSceneInput {
  /** Scenario-supplied prompt. */
  prompt: string;
  /** Optional style hint (e.g. "industrial-blueprint", "noir-night-shift"). */
  style?: string;
  /** Existing pipelineRunId for resume; auto-generated otherwise. */
  pipelineRunId?: string;
  /** Spatial extents in cm (default room-size). */
  extentsCm?: CineUsdArtifact['bboxCm'];
  /** Frame range for stage 4 (default 1 frame still). */
  frameStart?: number;
  frameEnd?: number;
  fps?: number;
  /** Skip persist on the pod (inspection mode). Default true for cyber-drill. */
  dryRun?: boolean;
}

// ─────────────────────────────────────────────────────────────────────────
// Panel (Stage 5-6) artifact + input

export interface CinePanelArtifact {
  pipelineRunId: string;
  panelRkey: string;
  /** B2 / R2 content-addressed key. */
  panelBlobKey: string;
  /** HTTPS URL (signed when live, data: URL when mock). */
  panelUrl: string;
  /** gpt-4o-mini-vision 7-axis composite score, 0-1 fixed-point ×1000. */
  scorePermille: number;
  /** Status — mirror of pod field. */
  status: 'panels_rendered' | 'error' | 'mock';
  error?: string;
}

export interface CinePanelInput {
  pipelineRunId: string;
  panelRkey: string;
  /** Free-form framing hint (`FullShot` / `MediumShot` / `Closeup` / …). */
  framing?: string;
  /** Prompt seeding the diffusion pass (often the node's cine prompt). */
  prompt: string;
  /** Mood palette tokens (carried from worldArtifact). */
  moodPalette?: string[];
  /** Optional severity tint name for HUD-styled mock fallback. */
  severityTint?: string;
}

// ─────────────────────────────────────────────────────────────────────────
// Bridge interface

export interface CineBridge {
  /** Run stages 1-4 and return the bundled artifact set. */
  generateScene(input: CineSceneInput): Promise<CineSceneArtifacts>;
  /** Run stages 5-6 (neuralRender + diffusionPass) for one panel. */
  generatePanel(input: CinePanelInput): Promise<CinePanelArtifact>;
  /** True when the bridge is operating in mock mode. */
  isMock(): boolean;
}

export interface CreateCineBridgeOpts {
  /**
   * HTTPS endpoint for the LangGraph pod. Example:
   *   "https://studio.gftd.ai/xrpc/ai.gftd.mangaka.cineGenerateScene"
   * When omitted or unreachable, the bridge falls back to mock.
   */
  endpoint?: string;
  /** Bearer / session token to attach as `Authorization: Bearer …`. */
  token?: string;
  /** Force mock mode regardless of endpoint (useful in tests / dev). */
  forceMock?: boolean;
  /** Timeout for the remote call. Default 8000 ms. */
  timeoutMs?: number;
  /** Optional `fetch` override (vitest / jsdom). */
  fetchImpl?: typeof fetch;
}

// ─────────────────────────────────────────────────────────────────────────
// Default + mock implementations

export function createCineBridge(opts: CreateCineBridgeOpts = {}): CineBridge {
  const fetchImpl = opts.fetchImpl ?? (typeof fetch !== 'undefined' ? fetch.bind(globalThis) : undefined);
  const useMock = opts.forceMock || !opts.endpoint || !fetchImpl;
  const timeoutMs = opts.timeoutMs ?? 8000;
  // Stage 5-6 endpoint sibling. The mangaka pod exposes both methods at the
  // same authority; for cyber-drill we let `endpoint` point at the scene
  // method and derive panel as a sibling path.
  const panelEndpoint = opts.endpoint
    ? opts.endpoint.replace(/cineGenerateScene$/, 'cineGeneratePanel')
    : undefined;

  async function generateScene(input: CineSceneInput): Promise<CineSceneArtifacts> {
    if (useMock) return _mockResult(input);
    try {
      const ac = new AbortController();
      const timer = setTimeout(() => ac.abort(), timeoutMs);
      const res = await fetchImpl!(opts.endpoint!, {
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          ...(opts.token ? { authorization: `Bearer ${opts.token}` } : {}),
        },
        body: JSON.stringify({
          prompt: input.prompt,
          style: input.style ?? '',
          world_kind: 'threeD',
          extents_cm: input.extentsCm,
          frame_start: input.frameStart ?? 1,
          frame_end: input.frameEnd ?? 1,
          fps: input.fps ?? 24,
          dry_run: input.dryRun ?? true,
          pipeline_run_id: input.pipelineRunId,
        }),
        signal: ac.signal,
      });
      clearTimeout(timer);
      if (!res.ok) {
        return { pipelineRunId: input.pipelineRunId ?? _newRunId(), status: 'error', error: `HTTP ${res.status}` };
      }
      const body = await res.json() as Record<string, unknown>;
      return _decodePodResponse(body, input);
    } catch (e) {
      // Network failure → fall back to mock (so demo never blocks).
      const mock = await _mockResult(input);
      return { ...mock, error: (e as Error)?.message };
    }
  }

  async function generatePanel(input: CinePanelInput): Promise<CinePanelArtifact> {
    if (useMock || !panelEndpoint || !fetchImpl) return _mockPanel(input);
    try {
      const ac = new AbortController();
      const timer = setTimeout(() => ac.abort(), timeoutMs);
      const res = await fetchImpl(panelEndpoint, {
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          ...(opts.token ? { authorization: `Bearer ${opts.token}` } : {}),
        },
        body: JSON.stringify({
          pipeline_run_id: input.pipelineRunId,
          page_rkey: input.panelRkey,
          panels: [{
            panel_rkey: input.panelRkey,
            framing: input.framing ?? 'MediumShot',
            prompt: input.prompt,
            mood_palette: input.moodPalette ?? [],
          }],
        }),
        signal: ac.signal,
      });
      clearTimeout(timer);
      if (!res.ok) {
        return {
          pipelineRunId: input.pipelineRunId, panelRkey: input.panelRkey,
          panelBlobKey: '', panelUrl: '', scorePermille: 0,
          status: 'error', error: `HTTP ${res.status}`,
        };
      }
      const body = await res.json() as Record<string, unknown>;
      const panels = (body['panels'] as Array<Record<string, unknown>> | undefined) ?? [];
      const p = panels[0] ?? {};
      return {
        pipelineRunId: String(body['pipeline_run_id'] ?? input.pipelineRunId),
        panelRkey:     String(p['panel_rkey'] ?? input.panelRkey),
        panelBlobKey:  String(p['panel_blob_key'] ?? ''),
        panelUrl:      String(p['url'] ?? p['panel_url'] ?? ''),
        scorePermille: Math.round(Number(p['score'] ?? 0) * 1000),
        status: 'panels_rendered',
      };
    } catch (e) {
      const m = await _mockPanel(input);
      return { ...m, error: (e as Error)?.message };
    }
  }

  return { generateScene, generatePanel, isMock: () => useMock };
}

export function createMockCineBridge(): CineBridge {
  return createCineBridge({ forceMock: true });
}

// ─────────────────────────────────────────────────────────────────────────
// Internal helpers

function _newRunId(): string {
  // Compact lexicographically-sortable id (tid-style without crypto/base32 deps).
  const t = Date.now();
  const r = Math.floor(Math.random() * 0xffffff).toString(36);
  return `run_${t.toString(36)}_${r}`;
}

/**
 * Decode the pod's `cine_generate_scene` JSON response into the bridge's
 * artifact shape. Field names are pod-side snake_case (per ADR-0095 RW
 * canonical columns); we map them to camelCase for the client.
 */
function _decodePodResponse(body: Record<string, unknown>, input: CineSceneInput): CineSceneArtifacts {
  const records = (body['stage_records'] as Record<string, any> | undefined) ?? {};
  const w = records['worldModel'];
  const u = records['usdScene'];
  const g = records['geom'] ?? records['neuralGeom'];
  const t = records['temporalField'];
  return {
    pipelineRunId: String(body['pipeline_run_id'] ?? input.pipelineRunId ?? _newRunId()),
    status: (body['status'] as CineSceneArtifacts['status']) ?? 'scene_ready',
    worldArtifact: w ? {
      modelCid:   String(w['asset_cid'] ?? w['model_cid'] ?? ''),
      seed:       Number(w['seed']        ?? 0),
      tokenCount: Number(w['token_count'] ?? 0),
      summary:    w['summary']     as string | undefined,
      moodPalette: w['mood_palette'] as string[] | undefined,
      cameraHint:  w['camera_hint']  as string | undefined,
    } : undefined,
    usdArtifact: u ? {
      usdaCid: String(u['usda_cid'] ?? ''),
      usdcCid: String(u['usdc_cid'] ?? ''),
      layerCount: Number(u['layer_count'] ?? 0),
      bboxCm: (u['bbox'] as CineUsdArtifact['bboxCm']) ?? input.extentsCm ?? _defaultBbox(),
    } : undefined,
    geomArtifact: g ? {
      assetCid: String(g['asset_cid'] ?? ''),
      format:   (g['format'] as CineGeomArtifact['format']) ?? 'gaussianSplat',
      pointCount: g['point_count'] as number | undefined,
      url: g['url'] as string | undefined,
    } : undefined,
    temporalArtifact: t ? {
      assetCid:   String(t['asset_cid'] ?? ''),
      format:     (t['format'] as CineTemporalArtifact['format']) ?? 'gaussian4d',
      frameStart: Number(t['frame_start'] ?? input.frameStart ?? 1),
      frameEnd:   Number(t['frame_end']   ?? input.frameEnd   ?? 1),
      fps:        Number(t['fps']         ?? input.fps        ?? 24),
    } : undefined,
    error: body['error'] as string | undefined,
  };
}

function _defaultBbox(): CineUsdArtifact['bboxCm'] {
  // 8×3×8 m room.
  return { minX: -400, minY: 0, minZ: -400, maxX: 400, maxY: 300, maxZ: 400 };
}

/**
 * Mock result — deterministic per-prompt envelope that downstream code
 * can treat exactly like a real pod response. No network, no LLM, no B2.
 */
async function _mockResult(input: CineSceneInput): Promise<CineSceneArtifacts> {
  const runId = input.pipelineRunId ?? _newRunId();
  // Deterministic seed from prompt hash so retries are stable.
  const seed = _hash32(input.prompt);
  // Camera hint chosen from prompt keywords.
  const camera =
    /closeup|details|reactor|tank/i.test(input.prompt) ? 'Closeup' :
    /overview|panorama|wide|yard/i.test(input.prompt) ? 'FullShot' :
    /shoulder|operator|console/i.test(input.prompt) ? 'OverShoulder' :
    'MediumShot';
  // Mood palette from severity keywords.
  const moodPalette =
    /critical|fire|explosion|runaway/i.test(input.prompt) ? ['ember', 'crimson', 'amber'] :
    /press|coverup|fail/i.test(input.prompt) ? ['crimson', 'charcoal', 'ash'] :
    /recover|success|board/i.test(input.prompt) ? ['oak', 'cream', 'sage'] :
    /night|02:14|shift/i.test(input.prompt) ? ['indigo', 'steel', 'cyan'] :
    ['steel', 'cream', 'cyan'];
  return {
    pipelineRunId: runId,
    status: 'mock',
    worldArtifact: {
      modelCid: `mock://world/${runId}.json`,
      seed,
      tokenCount: 256,
      summary: `mock summary — ${input.prompt.slice(0, 64)}…`,
      moodPalette,
      cameraHint: camera,
    },
    usdArtifact: {
      usdaCid: `mock://usd/${runId}.usda`,
      usdcCid: `mock://usd/${runId}.usdc`,
      layerCount: 3,
      bboxCm: input.extentsCm ?? _defaultBbox(),
    },
    // No geom / temporal in mock — the renderer falls back to primitives.
  };
}

function _hash32(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 0x01000193);
  return h >>> 0;
}

/**
 * Synthesise a plausible diffusion-pass panel via canvas-2D. The mock
 * encodes:
 *   - a mood-palette gradient backdrop (from `worldArtifact.moodPalette`)
 *   - the framing label as a kammoku-styled overlay
 *   - the prompt summary as caption
 * → emitted as a PNG `data:` URL the renderer can plug into a Three.js
 * MeshBasicMaterial map. Deterministic per (pipelineRunId, panelRkey).
 */
async function _mockPanel(input: CinePanelInput): Promise<CinePanelArtifact> {
  const w = 1024;
  const h = 576;
  let dataUrl = '';
  if (typeof document !== 'undefined') {
    const cv = document.createElement('canvas');
    cv.width = w; cv.height = h;
    const ctx = cv.getContext('2d');
    if (ctx) {
      const colors = (input.moodPalette ?? ['steel', 'cream', 'cyan']).map(_paletteToHex);
      const g = ctx.createLinearGradient(0, 0, w, h);
      g.addColorStop(0, colors[0] ?? '#26303d');
      g.addColorStop(0.6, colors[1] ?? '#f0ead6');
      g.addColorStop(1, colors[2] ?? '#4dc1ff');
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, w, h);
      // Vignette
      const radial = ctx.createRadialGradient(w / 2, h / 2, w * 0.2, w / 2, h / 2, w * 0.65);
      radial.addColorStop(0, 'rgba(0,0,0,0)');
      radial.addColorStop(1, 'rgba(0,0,0,0.55)');
      ctx.fillStyle = radial;
      ctx.fillRect(0, 0, w, h);
      // Framing badge
      ctx.fillStyle = input.severityTint ?? '#e07b1c';
      ctx.fillRect(36, 36, 360, 64);
      ctx.fillStyle = '#fff';
      ctx.font = 'bold 30px sans-serif';
      ctx.textBaseline = 'middle';
      ctx.fillText(`STAGE 6 · ${input.framing ?? 'MediumShot'}`, 56, 68);
      // Caption (wrapped)
      ctx.fillStyle = '#ffffff';
      ctx.font = '26px sans-serif';
      _wrapText(ctx, input.prompt, 48, h - 180, w - 96, 32);
      // Watermark
      ctx.fillStyle = 'rgba(255,255,255,0.55)';
      ctx.font = 'italic 18px sans-serif';
      ctx.fillText('kami-cine · diffusionPass (mock)', 48, h - 36);
      dataUrl = cv.toDataURL('image/png');
    }
  }
  return {
    pipelineRunId: input.pipelineRunId,
    panelRkey:     input.panelRkey,
    panelBlobKey:  `mock://panel/${input.pipelineRunId}/${input.panelRkey}.png`,
    panelUrl:      dataUrl,
    scorePermille: 820, // mocks always grade "well"
    status:        'mock',
  };
}

function _paletteToHex(token: string): string {
  const map: Record<string, string> = {
    ember:   '#e0623a',
    crimson: '#a4242e',
    amber:   '#d4a73a',
    indigo:  '#3b4574',
    steel:   '#5c6675',
    cyan:    '#4dc1ff',
    cream:   '#f0ead6',
    oak:     '#8b6a3f',
    sage:    '#a8b59a',
    charcoal:'#26303d',
    ash:     '#9aa3b2',
  };
  return map[token] ?? '#888888';
}

function _wrapText(ctx: CanvasRenderingContext2D, text: string, x: number, y: number, maxWidth: number, lineHeight: number): void {
  let line = '';
  let yy = y;
  for (const ch of text) {
    const test = line + ch;
    if (ctx.measureText(test).width > maxWidth && line.length > 0) {
      ctx.fillText(line, x, yy);
      yy += lineHeight;
      line = ch;
    } else {
      line = test;
    }
  }
  if (line) ctx.fillText(line, x, yy);
}
