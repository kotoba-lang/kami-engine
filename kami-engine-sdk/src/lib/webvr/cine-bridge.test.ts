import { describe, expect, it } from 'vitest';
import { createCineBridge, createMockCineBridge } from './cine-bridge.js';

describe('kami-engine-sdk webvr / cine-bridge', () => {
  it('mock bridge returns a scene_ready-shaped result', async () => {
    const b = createMockCineBridge();
    expect(b.isMock()).toBe(true);
    const r = await b.generateScene({ prompt: 'night shift SCADA HMI alarm' });
    expect(r.status).toBe('mock');
    expect(r.pipelineRunId).toMatch(/^run_/);
    expect(r.worldArtifact?.modelCid).toMatch(/^mock:\/\//);
    expect(r.usdArtifact?.bboxCm).toBeDefined();
  });

  it('mock derives camera hint + mood palette from prompt', async () => {
    const b = createMockCineBridge();
    const r = await b.generateScene({ prompt: 'closeup of reactor tank R-12' });
    expect(r.worldArtifact?.cameraHint).toBe('Closeup');
    const r2 = await b.generateScene({ prompt: 'critical fire runaway in chemical yard' });
    expect(r2.worldArtifact?.moodPalette).toContain('crimson');
  });

  it('forceMock=true overrides endpoint', async () => {
    const b = createCineBridge({ endpoint: 'https://x', forceMock: true });
    expect(b.isMock()).toBe(true);
    const r = await b.generateScene({ prompt: 'x' });
    expect(r.status).toBe('mock');
  });

  it('decodes a live pod response shape', async () => {
    const fakeFetch = async (_url: any, _init: any): Promise<Response> =>
      new Response(JSON.stringify({
        pipeline_run_id: 'pod_abc',
        status: 'scene_ready',
        stage_records: {
          worldModel:    { asset_cid: 'b2://world.json', seed: 42, token_count: 128, summary: 'sum', camera_hint: 'FullShot' },
          usdScene:      { usda_cid: 'b2://x.usda', usdc_cid: 'b2://x.usdc', layer_count: 5, bbox: { minX: 0, minY: 0, minZ: 0, maxX: 100, maxY: 100, maxZ: 100 } },
          neuralGeom:    { asset_cid: 'b2://geom.ply', format: 'gaussianSplat', point_count: 90000, url: 'https://b2.example/geom.ply' },
          temporalField: { asset_cid: 'b2://t.bin', format: 'gaussian4d', frame_start: 1, frame_end: 24, fps: 24 },
        },
      }), { status: 200, headers: { 'content-type': 'application/json' } });
    const b = createCineBridge({ endpoint: 'https://studio.example/xrpc', fetchImpl: fakeFetch as unknown as typeof fetch });
    expect(b.isMock()).toBe(false);
    const r = await b.generateScene({ prompt: 'p' });
    expect(r.status).toBe('scene_ready');
    expect(r.pipelineRunId).toBe('pod_abc');
    expect(r.geomArtifact?.format).toBe('gaussianSplat');
    expect(r.geomArtifact?.url).toBe('https://b2.example/geom.ply');
    expect(r.temporalArtifact?.fps).toBe(24);
  });

  it('falls back to mock on http error and surfaces error', async () => {
    const fakeFetch = async (): Promise<Response> => new Response('boom', { status: 500 });
    const b = createCineBridge({ endpoint: 'https://x', fetchImpl: fakeFetch as unknown as typeof fetch });
    const r = await b.generateScene({ prompt: 'p' });
    expect(r.status).toBe('error');
    expect(r.error).toContain('HTTP 500');
  });

  it('mock generatePanel returns a mock-status artifact with a B2 stub key', async () => {
    const b = createMockCineBridge();
    const r = await b.generatePanel({
      pipelineRunId: 'run_test',
      panelRkey: 'panel-detect',
      framing: 'MediumShot',
      prompt: 'SCADA HMI red alert at 02:14',
      moodPalette: ['indigo', 'steel', 'cyan'],
    });
    expect(r.status).toBe('mock');
    expect(r.pipelineRunId).toBe('run_test');
    expect(r.panelRkey).toBe('panel-detect');
    expect(r.panelBlobKey).toMatch(/^mock:\/\/panel\//);
    // panelUrl is a `data:image/png;…` URL in real browsers but empty in
    // jsdom (no canvas backend). Either is acceptable; assert non-null.
    expect(typeof r.panelUrl).toBe('string');
    expect(r.scorePermille).toBeGreaterThan(0);
  });

  it('decodes a live cine_generate_panel response shape', async () => {
    const fakeFetch = async (_url: any, _init: any): Promise<Response> =>
      new Response(JSON.stringify({
        pipeline_run_id: 'pod_abc',
        status: 'panels_rendered',
        panels: [{
          panel_rkey: 'panel-1',
          panel_blob_key: 'b2://panels/panel-1.png',
          url: 'https://b2.example/panels/panel-1.png',
          score: 0.86,
        }],
      }), { status: 200, headers: { 'content-type': 'application/json' } });
    const b = createCineBridge({
      endpoint: 'https://studio.example/xrpc/ai.gftd.mangaka.cineGenerateScene',
      fetchImpl: fakeFetch as unknown as typeof fetch,
    });
    const r = await b.generatePanel({
      pipelineRunId: 'run_x', panelRkey: 'panel-1',
      framing: 'Closeup', prompt: 'reactor',
    });
    expect(r.status).toBe('panels_rendered');
    expect(r.panelUrl).toBe('https://b2.example/panels/panel-1.png');
    expect(r.scorePermille).toBe(860);
  });
});
