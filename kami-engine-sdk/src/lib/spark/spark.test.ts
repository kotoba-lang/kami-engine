import { describe, expect, it } from 'vitest';
import {
  mulberry32,
  makeGalaxyCloud,
  makeEllipsoidWall,
  makeTunnelField,
  sampleTemporal,
  compileDynoGraph,
  defaultDynoGraph,
  dynoNodeLibrary,
  type DynoGraph,
} from './index.js';

describe('mulberry32', () => {
  it('is deterministic per seed', () => {
    const a = mulberry32(42);
    const b = mulberry32(42);
    const xs = Array.from({ length: 8 }, () => a());
    const ys = Array.from({ length: 8 }, () => b());
    expect(xs).toEqual(ys);
  });
  it('differs across seeds', () => {
    expect(mulberry32(1)()).not.toBe(mulberry32(2)());
  });
});

describe('makeGalaxyCloud', () => {
  it('returns the requested splat count', () => {
    const c = makeGalaxyCloud(500);
    expect(c.splats.length).toBe(500);
    expect(c.center).toEqual([0, 0, 0]);
    expect(c.radius).toBeGreaterThan(0);
  });
  it('produces colors in [0,1] and finite positions', () => {
    const c = makeGalaxyCloud(100, 7);
    for (const s of c.splats) {
      for (const v of s.color) {
        expect(v).toBeGreaterThanOrEqual(0);
        expect(v).toBeLessThanOrEqual(1);
      }
      for (const p of s.position) expect(Number.isFinite(p)).toBe(true);
      expect(s.scale[0]).toBeGreaterThan(0);
    }
  });
});

describe('makeEllipsoidWall', () => {
  it('builds a 12×12 grid of anisotropic ellipsoids with quaternion rotation', () => {
    const c = makeEllipsoidWall();
    expect(c.splats.length).toBe(144);
    const anisotropic = c.splats.filter((s) => s.scale[0] !== s.scale[1]);
    expect(anisotropic.length).toBeGreaterThan(0);
    for (const s of c.splats) {
      expect(s.rotation).toBeDefined();
      // Unit quaternion (z-axis rotation)
      const [x, y, z, w] = s.rotation!;
      const norm = Math.sqrt(x * x + y * y + z * z + w * w);
      expect(norm).toBeCloseTo(1, 4);
    }
  });
});

describe('makeTunnelField + sampleTemporal', () => {
  it('produces 4-keyframe splats with sorted t and matching loopSec', () => {
    const f = makeTunnelField(50, 0xab, 8);
    expect(f.splats.length).toBe(50);
    expect(f.loopSec).toBe(8);
    for (const s of f.splats) {
      const ts = s.keyframes.map((k) => k.t);
      const sorted = [...ts].sort((a, b) => a - b);
      expect(ts).toEqual(sorted);
      expect(ts[0]).toBe(0);
      expect(ts[ts.length - 1]).toBe(1);
    }
  });
  it('linearly interpolates between adjacent keyframes', () => {
    const f = makeTunnelField(1, 0xc1);
    const s = f.splats[0]!;
    // First keyframe has opacity 0; second (t=0.15) has opacity 0.9.
    const a = sampleTemporal(s, 0);
    const b = sampleTemporal(s, 0.15);
    const mid = sampleTemporal(s, 0.075);
    expect(a.opacity).toBeCloseTo(0, 5);
    expect(b.opacity).toBeCloseTo(0.9, 5);
    // Midpoint should be near 0.45 (linear).
    expect(mid.opacity).toBeCloseTo(0.45, 2);
  });
  it('returns the last keyframe at u=1', () => {
    const f = makeTunnelField(1, 0xd2);
    const s = f.splats[0]!;
    const end = sampleTemporal(s, 1);
    const last = s.keyframes[s.keyframes.length - 1]!;
    expect(end.position[0]).toBeCloseTo(last.position[0], 5);
  });
});

describe('compileDynoGraph', () => {
  it('emits a function per node and chains via `col = nodeId(col, vUv, uTime)`', () => {
    const g = defaultDynoGraph();
    const c = compileDynoGraph(g);
    for (const n of g.nodes) {
      expect(c.fragmentSource).toContain(`vec4 ${n.id}(vec4 prev, vec2 uv, float t)`);
      expect(c.fragmentSource).toContain(`col = ${n.id}(col, vUv, uTime);`);
    }
  });
  it('generates u_<id>_<name> uniforms with the provided initial values', () => {
    const c = compileDynoGraph(defaultDynoGraph());
    expect(c.uniforms).toHaveProperty('u_splatBackdrop_intensity');
    expect(c.uniforms).toHaveProperty('u_rgbeBoost_exposure');
    expect(c.uniforms).toHaveProperty('u_hueShift_speed');
    expect(c.fragmentSource).toContain('uniform float u_vignette_strength;');
  });
  it('rejects duplicate node ids', () => {
    const dup: DynoGraph = {
      nodes: [
        dynoNodeLibrary.vignette!(),
        dynoNodeLibrary.vignette!(),
      ],
    };
    expect(() => compileDynoGraph(dup)).toThrow(/duplicate node id/);
  });
  it('rejects invalid uniform names', () => {
    const bad: DynoGraph = {
      nodes: [{ id: 'x', body: '{ return prev; }', uniforms: { '0bad': 1 } }],
    };
    expect(() => compileDynoGraph(bad)).toThrow(/invalid uniform name/);
  });
  it('rejects invalid node ids', () => {
    const bad: DynoGraph = { nodes: [{ id: '0bad', body: '{ return prev; }' }] };
    expect(() => compileDynoGraph(bad)).toThrow(/invalid node id/);
  });
});

describe('dynoNodeLibrary', () => {
  it('exposes the five built-in nodes with stable ids', () => {
    expect(Object.keys(dynoNodeLibrary).sort()).toEqual(
      ['hueShift', 'rgbeBoost', 'scanlines', 'splatBackdrop', 'vignette'],
    );
    expect(dynoNodeLibrary.vignette!().id).toBe('vignette');
  });
});
