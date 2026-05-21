/**
 * spark/data.ts — Deterministic sample data generators.
 *
 * Procedural content lets the demo run with zero asset fetches and zero
 * network. All RNGs are seedable (mulberry32) so a given seed always
 * produces the same cloud / field, useful for screenshots and tests.
 */

import type {
  Splat3D,
  SplatCloudData,
  TemporalSplat4D,
  TemporalSplatField,
} from './types.js';

// ─────────────────────────────────────────────────────────────────────────
// Seeded RNG

export function mulberry32(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// ─────────────────────────────────────────────────────────────────────────
// 3D splat clouds

/**
 * Generate a galaxy-style 3DGS sample cloud — log-spiral arms with
 * radial color shift. Roughly emulates the look of a baked Marble world.
 */
export function makeGalaxyCloud(count: number, seed = 0xc01dbeef): SplatCloudData {
  const rng = mulberry32(seed);
  const splats: Splat3D[] = [];
  const arms = 4;
  for (let i = 0; i < count; i++) {
    const arm = i % arms;
    const r = Math.sqrt(rng()) * 2.6;
    const theta = (arm / arms) * Math.PI * 2 + r * 1.8 + (rng() - 0.5) * 0.4;
    const x = Math.cos(theta) * r;
    const z = Math.sin(theta) * r;
    const y = (rng() - 0.5) * 0.3 * (1 - r / 3);
    const radial = r / 2.6;
    const r1 = 0.95 - radial * 0.45 + (rng() - 0.5) * 0.05;
    const g1 = 0.6 + radial * 0.2 + (rng() - 0.5) * 0.05;
    const b1 = 0.45 + radial * 0.5 + (rng() - 0.5) * 0.05;
    const s = 0.018 + (1 - radial) * 0.012 + rng() * 0.004;
    splats.push({
      position: [x, y, z],
      color: [clamp01(r1), clamp01(g1), clamp01(b1)],
      opacity: 0.55 + rng() * 0.3,
      scale: [s, s, s],
    });
  }
  return { splats, center: [0, 0, 0], radius: 2.8 };
}

/**
 * Generate a wall-of-ellipsoids cloud — a 12×12 grid of anisotropic
 * splats with rotating per-row stretch axes. Shows off Spark's
 * "elliptical" splat rendering.
 */
export function makeEllipsoidWall(seed = 0xa11ce): SplatCloudData {
  const rng = mulberry32(seed);
  const splats: Splat3D[] = [];
  const cols = 12;
  const rows = 12;
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const x = (c - cols / 2) * 0.32;
      const y = (r - rows / 2) * 0.32;
      const z = (rng() - 0.5) * 0.05;
      const phase = (r / rows) * Math.PI;
      const sx = 0.08 + Math.abs(Math.cos(phase)) * 0.12;
      const sy = 0.08 + Math.abs(Math.sin(phase)) * 0.12;
      const sz = 0.04;
      // Rotation around z: half-angle = phase/2
      const half = phase / 2;
      const rotZ: [number, number, number, number] = [0, 0, Math.sin(half), Math.cos(half)];
      const hue = (c / cols + r / rows) * 0.5;
      const [rr, gg, bb] = hsvToRgb(hue, 0.55, 0.95);
      splats.push({
        position: [x, y, z],
        color: [rr, gg, bb],
        opacity: 0.85,
        scale: [sx, sy, sz],
        rotation: rotZ,
      });
    }
  }
  return { splats, center: [0, 0, 0], radius: 2.4 };
}

// ─────────────────────────────────────────────────────────────────────────
// Temporal 4D fields

/**
 * Animated tunnel of splats spiraling along the z-axis, looping every
 * `loopSec`. Each splat has two keyframes (t=0 entry, t=1 exit) and
 * cycles color through HSV.
 */
export function makeTunnelField(count: number, seed = 0x4d, loopSec = 6): TemporalSplatField {
  const rng = mulberry32(seed);
  const splats: TemporalSplat4D[] = [];
  for (let i = 0; i < count; i++) {
    const theta = rng() * Math.PI * 2;
    const radius = 0.4 + rng() * 0.6;
    const zStart = -3.5 + rng() * 0.4;
    const zEnd = 3.5 - rng() * 0.4;
    const hue = (i / count) % 1;
    const [rr, gg, bb] = hsvToRgb(hue, 0.7, 1.0);
    const [rr2, gg2, bb2] = hsvToRgb((hue + 0.5) % 1, 0.7, 1.0);
    const s = 0.025 + rng() * 0.015;
    splats.push({
      base: {
        position: [Math.cos(theta) * radius, Math.sin(theta) * radius, zStart],
        color: [rr, gg, bb],
        opacity: 0.9,
        scale: [s, s, s],
      },
      keyframes: [
        {
          t: 0,
          position: [Math.cos(theta) * radius, Math.sin(theta) * radius, zStart],
          color: [rr, gg, bb],
          opacity: 0.0,
        },
        {
          t: 0.15,
          position: [Math.cos(theta) * radius, Math.sin(theta) * radius, zStart * 0.6],
          color: [rr, gg, bb],
          opacity: 0.9,
        },
        {
          t: 0.85,
          position: [Math.cos(theta + 0.6) * radius, Math.sin(theta + 0.6) * radius, zEnd * 0.6],
          color: [rr2, gg2, bb2],
          opacity: 0.9,
        },
        {
          t: 1,
          position: [Math.cos(theta + 0.6) * radius, Math.sin(theta + 0.6) * radius, zEnd],
          color: [rr2, gg2, bb2],
          opacity: 0.0,
        },
      ],
    });
  }
  return { splats, loopSec };
}

// ─────────────────────────────────────────────────────────────────────────
// Internal helpers

function clamp01(x: number): number {
  return x < 0 ? 0 : x > 1 ? 1 : x;
}

function hsvToRgb(h: number, s: number, v: number): [number, number, number] {
  const i = Math.floor(h * 6);
  const f = h * 6 - i;
  const p = v * (1 - s);
  const q = v * (1 - f * s);
  const t = v * (1 - (1 - f) * s);
  const m = i % 6;
  if (m === 0) return [v, t, p];
  if (m === 1) return [q, v, p];
  if (m === 2) return [p, v, t];
  if (m === 3) return [p, q, v];
  if (m === 4) return [t, p, v];
  return [v, p, q];
}

// ─────────────────────────────────────────────────────────────────────────
// Location splat clouds — procedural environmental Gaussian fields for
// the cyber-drill VR. Each location ships ~3000–5000 splats arranged to
// evoke its silhouette without needing a real photogrammetry capture.

/**
 * Per-location procedural Gaussian splat cloud. `LocationKind` mirrors the
 * webvr scenario type but is kept as a plain string union so this module
 * has no upward dependency on `../webvr/types`.
 */
export type SparkLocationKind =
  | 'scadaRoom'
  | 'cleanroom'
  | 'chemicalYard'
  | 'utilityRoom'
  | 'serverRoom'
  | 'executiveRoom'
  | 'press';

export function makeLocationCloud(loc: SparkLocationKind, seed = 0x600d): SplatCloudData {
  const rng = mulberry32(seed ^ _hashStr(loc));
  switch (loc) {
    case 'scadaRoom':    return _scadaRoom(rng);
    case 'cleanroom':    return _cleanroom(rng);
    case 'chemicalYard': return _chemicalYard(rng);
    case 'utilityRoom':  return _utilityRoom(rng);
    case 'serverRoom':   return _serverRoom(rng);
    case 'executiveRoom':return _executiveRoom(rng);
    case 'press':        return _press(rng);
  }
}

function _hashStr(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 0x01000193);
  return h >>> 0;
}

function _push(splats: Splat3D[], x: number, y: number, z: number, r: number, g: number, b: number, op: number, scale: number) {
  splats.push({ position: [x, y, z], color: [clamp01(r), clamp01(g), clamp01(b)], opacity: clamp01(op), scale: [scale, scale, scale] });
}

function _scadaRoom(rng: () => number): SplatCloudData {
  const splats: Splat3D[] = [];
  // 3 wall monitors at z=-6, x=-2.4/0/+2.4 → blue/teal glow halos.
  for (const cx of [-2.4, 0, 2.4]) {
    for (let i = 0; i < 260; i++) {
      const u = rng() * Math.PI * 2;
      const r = Math.sqrt(rng()) * 0.95;
      const x = cx + Math.cos(u) * r;
      const y = 1.4 + Math.sin(u) * r * 0.55;
      const z = -5.95 + (rng() - 0.5) * 0.15;
      _push(splats, x, y, z, 0.55 + rng() * 0.3, 0.85 + rng() * 0.15, 1.0, 0.85, 0.06 + rng() * 0.03);
    }
  }
  // Console at z=-5, y=0.5 — amber warning LEDs.
  for (let i = 0; i < 600; i++) {
    const x = (rng() - 0.5) * 5.8;
    const y = 0.6 + (rng() - 0.5) * 0.45;
    const z = -5.0 + (rng() - 0.5) * 1.0;
    const hot = rng() < 0.22;
    _push(splats, x, y, z,
      hot ? 1.0 : 0.7 + rng() * 0.1,
      hot ? 0.7 + rng() * 0.2 : 0.78,
      hot ? 0.18 : 0.85,
      0.7, 0.035 + rng() * 0.02);
  }
  // Ambient blue room haze (luminous fill).
  for (let i = 0; i < 1800; i++) {
    const x = (rng() - 0.5) * 16;
    const y = rng() * 5.5;
    const z = -10 + rng() * 9.5;
    _push(splats, x, y, z, 0.45 + rng() * 0.15, 0.55 + rng() * 0.15, 0.7 + rng() * 0.1, 0.3 + rng() * 0.15, 0.09 + rng() * 0.05);
  }
  return { splats, center: [0, 1.6, -4], radius: 8 };
}

function _cleanroom(rng: () => number): SplatCloudData {
  const splats: Splat3D[] = [];
  // 5 lithography steppers as vertical cylinders at z=-6.5, x=-4.4..4.4.
  for (const cx of [-4.4, -2.2, 0, 2.2, 4.4]) {
    for (let i = 0; i < 320; i++) {
      const u = rng() * Math.PI * 2;
      const r = Math.sqrt(rng()) * 0.65;
      const x = cx + Math.cos(u) * r;
      const z = -6.5 + Math.sin(u) * r;
      const y = rng() * 1.8;
      _push(splats, x, y, z, 0.55 + rng() * 0.3, 0.85, 0.7 + rng() * 0.2, 0.55 + rng() * 0.3, 0.04 + rng() * 0.03);
    }
  }
  // White ambient mist (HEPA-filtered air look).
  for (let i = 0; i < 2400; i++) {
    const x = (rng() - 0.5) * 16;
    const y = rng() * 5.5;
    const z = -10 + rng() * 9.5;
    _push(splats, x, y, z, 0.95, 0.98, 1.0, 0.28 + rng() * 0.18, 0.1 + rng() * 0.05);
  }
  return { splats, center: [0, 1.6, -5], radius: 9 };
}

function _chemicalYard(rng: () => number): SplatCloudData {
  const splats: Splat3D[] = [];
  // 3 reactor tanks at z=-7, x=-3.2/0/3.2 — ember-orange volumetric glow.
  for (const cx of [-3.2, 0, 3.2]) {
    for (let i = 0; i < 480; i++) {
      const u = rng() * Math.PI * 2;
      const r = Math.sqrt(rng()) * 1.05;
      const x = cx + Math.cos(u) * r;
      const z = -7.0 + Math.sin(u) * r;
      const y = rng() * 3.0;
      const hot = y / 3.0;
      _push(splats, x, y, z, 0.95, 0.5 - hot * 0.25, 0.18 + hot * 0.1, 0.55 + rng() * 0.4, 0.06 + rng() * 0.04);
    }
  }
  // Steam plumes rising above each tank.
  for (const cx of [-3.2, 0, 3.2]) {
    for (let i = 0; i < 240; i++) {
      const u = rng() * Math.PI * 2;
      const lift = rng();
      const r = 0.4 + lift * 0.9;
      const x = cx + Math.cos(u) * r * 0.6;
      const z = -7.0 + Math.sin(u) * r * 0.6;
      const y = 3 + lift * 2.5;
      _push(splats, x, y, z, 0.8 + rng() * 0.2, 0.8, 0.78, 0.18 + rng() * 0.15, 0.1 + lift * 0.07);
    }
  }
  // Sodium-light ambient haze (warm orange).
  for (let i = 0; i < 1600; i++) {
    const x = (rng() - 0.5) * 16;
    const y = rng() * 5.5;
    const z = -10 + rng() * 9.5;
    _push(splats, x, y, z, 0.85 + rng() * 0.15, 0.55 + rng() * 0.1, 0.32 + rng() * 0.05, 0.32 + rng() * 0.15, 0.1 + rng() * 0.05);
  }
  return { splats, center: [0, 2, -6], radius: 10 };
}

function _utilityRoom(rng: () => number): SplatCloudData {
  const splats: Splat3D[] = [];
  // 5 pipe cabinets at z=-6.
  for (const cx of [-3.6, -1.8, 0, 1.8, 3.6]) {
    for (let i = 0; i < 220; i++) {
      const x = cx + (rng() - 0.5) * 0.7;
      const z = -6 + (rng() - 0.5) * 0.6;
      const y = rng() * 2;
      _push(splats, x, y, z, 0.7, 0.72, 0.78, 0.4 + rng() * 0.3, 0.04 + rng() * 0.02);
    }
  }
  // Cool steel-grey ambient.
  for (let i = 0; i < 2000; i++) {
    const x = (rng() - 0.5) * 16;
    const y = rng() * 5.5;
    const z = -10 + rng() * 9.5;
    _push(splats, x, y, z, 0.7 + rng() * 0.1, 0.72 + rng() * 0.1, 0.78 + rng() * 0.08, 0.28 + rng() * 0.15, 0.1 + rng() * 0.05);
  }
  return { splats, center: [0, 1.6, -4], radius: 8 };
}

function _serverRoom(rng: () => number): SplatCloudData {
  const splats: Splat3D[] = [];
  // 7 server racks at z=-6, x=-3.9..3.9 → blinking green/red LEDs.
  for (const cx of [-3.9, -2.6, -1.3, 0, 1.3, 2.6, 3.9]) {
    for (let i = 0; i < 260; i++) {
      const x = cx + (rng() - 0.5) * 0.7;
      const z = -6 + (rng() - 0.5) * 0.8;
      const y = rng() * 2.1;
      const led = rng() < 0.22;
      _push(splats, x, y, z,
        led ? (rng() < 0.5 ? 0.2 : 1.0) : 0.05,
        led ? (rng() < 0.5 ? 0.95 : 0.15) : 0.06,
        led ? 0.15 : 0.08,
        led ? 0.95 : 0.25, 0.025 + rng() * 0.02);
    }
  }
  // Cool teal ambient (boosted to keep the room readable).
  for (let i = 0; i < 1800; i++) {
    const x = (rng() - 0.5) * 16;
    const y = rng() * 5.5;
    const z = -10 + rng() * 9.5;
    _push(splats, x, y, z, 0.32 + rng() * 0.1, 0.55 + rng() * 0.12, 0.6 + rng() * 0.1, 0.28 + rng() * 0.15, 0.1 + rng() * 0.05);
  }
  return { splats, center: [0, 1.5, -4], radius: 8 };
}

function _executiveRoom(rng: () => number): SplatCloudData {
  const splats: Splat3D[] = [];
  // Conference table at z=-5.5 — warm wood reflection field.
  for (let i = 0; i < 800; i++) {
    const x = (rng() - 0.5) * 4;
    const z = -5.5 + (rng() - 0.5) * 1.6;
    const y = 1.05 + (rng() - 0.5) * 0.18;
    _push(splats, x, y, z, 0.65 + rng() * 0.2, 0.46 + rng() * 0.15, 0.28 + rng() * 0.1, 0.55 + rng() * 0.3, 0.04 + rng() * 0.03);
  }
  // 3 chair silhouettes.
  for (const cx of [-1.4, 0, 1.4]) {
    for (let i = 0; i < 160; i++) {
      const x = cx + (rng() - 0.5) * 0.45;
      const z = -5.5 + (rng() - 0.5) * 0.45;
      const y = rng() * 1.1;
      _push(splats, x, y, z, 0.32 + rng() * 0.1, 0.22 + rng() * 0.08, 0.15 + rng() * 0.06, 0.5 + rng() * 0.3, 0.04 + rng() * 0.02);
    }
  }
  // Warm sunset ambient.
  for (let i = 0; i < 1900; i++) {
    const x = (rng() - 0.5) * 16;
    const y = rng() * 5.5;
    const z = -10 + rng() * 9.5;
    _push(splats, x, y, z, 1.0, 0.85, 0.62, 0.32 + rng() * 0.18, 0.1 + rng() * 0.05);
  }
  return { splats, center: [0, 1.6, -4], radius: 8 };
}

function _press(rng: () => number): SplatCloudData {
  const splats: Splat3D[] = [];
  // Podium at z=-5.5 — high-contrast white face under flash.
  for (let i = 0; i < 400; i++) {
    const x = (rng() - 0.5) * 3;
    const z = -5.5 + (rng() - 0.5) * 0.55;
    const y = 0.55 + (rng() - 0.5) * 0.55;
    _push(splats, x, y, z, 0.95, 0.95, 0.95, 0.7 + rng() * 0.25, 0.045 + rng() * 0.02);
  }
  // Camera-flash starbursts (random white pops).
  for (let i = 0; i < 40; i++) {
    const x = (rng() - 0.5) * 12;
    const y = 1.2 + rng() * 1.4;
    const z = -2 - rng() * 4;
    _push(splats, x, y, z, 1, 1, 1, 0.9, 0.18 + rng() * 0.1);
  }
  // Pinky-red press-conference ambient.
  for (let i = 0; i < 1700; i++) {
    const x = (rng() - 0.5) * 16;
    const y = rng() * 5.5;
    const z = -10 + rng() * 9.5;
    _push(splats, x, y, z, 0.95 + rng() * 0.05, 0.75 + rng() * 0.1, 0.75 + rng() * 0.08, 0.32 + rng() * 0.15, 0.1 + rng() * 0.05);
  }
  return { splats, center: [0, 1.6, -3], radius: 9 };
}

/**
 * Sample a temporal splat at normalized time `u` in [0,1). Linearly
 * interpolates between the two surrounding keyframes. Exported for
 * tests and for non-render uses (e.g., baking to a static cloud).
 */
export function sampleTemporal(splat: TemporalSplat4D, u: number): {
  position: [number, number, number];
  color: [number, number, number];
  opacity: number;
} {
  const ks = splat.keyframes;
  if (ks.length === 0) {
    return { position: splat.base.position, color: splat.base.color, opacity: splat.base.opacity };
  }
  // Find the interval [a, b].
  let a = ks[0]!;
  let b = ks[ks.length - 1]!;
  for (let i = 0; i < ks.length - 1; i++) {
    if (u >= ks[i]!.t && u <= ks[i + 1]!.t) {
      a = ks[i]!;
      b = ks[i + 1]!;
      break;
    }
  }
  const span = Math.max(1e-6, b.t - a.t);
  const k = (u - a.t) / span;
  const lerp = (x: number, y: number) => x + (y - x) * k;
  const aColor = a.color ?? splat.base.color;
  const bColor = b.color ?? splat.base.color;
  const aOp = a.opacity ?? splat.base.opacity;
  const bOp = b.opacity ?? splat.base.opacity;
  return {
    position: [lerp(a.position[0], b.position[0]), lerp(a.position[1], b.position[1]), lerp(a.position[2], b.position[2])],
    color: [lerp(aColor[0], bColor[0]), lerp(aColor[1], bColor[1]), lerp(aColor[2], bColor[2])],
    opacity: lerp(aOp, bOp),
  };
}
