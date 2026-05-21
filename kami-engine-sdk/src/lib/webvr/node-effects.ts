/**
 * webvr/node-effects.ts — Per-node visual effects layered above the
 * location's spark backdrop. Each effect is a small Three.js Group with
 * a `tick(dtSec, tSec)` updater and a `dispose()` for teardown.
 *
 * The registry is intentionally tiny — 7 effects covering the dramatic
 * moments in a typical OT-incident playbook (alarm, fire, exfil, press,
 * dawn, recovery, monitor flicker). Authors stack them per-node via
 * `IncidentNode.effects`.
 */

import * as THREE from 'three';
import type { NodeEffectKind, Severity } from './types.js';

export interface NodeEffectInstance {
  group: THREE.Group;
  tick(dtSec: number, tSec: number): void;
  dispose(): void;
}

export interface BuildNodeEffectOpts {
  /** Optional severity hint — drives intensity / hue. */
  severity?: Severity;
  /** Random seed for deterministic per-node variation. */
  seed?: number;
}

export function buildNodeEffect(
  kind: NodeEffectKind,
  opts: BuildNodeEffectOpts = {},
): NodeEffectInstance {
  const seed = opts.seed ?? 0x42;
  switch (kind) {
    case 'redAlarm':       return _redAlarm(opts.severity ?? 'high');
    case 'orangeSmoke':    return _orangeSmoke(seed);
    case 'dataLeak':       return _dataLeak(seed);
    case 'pressFlash':     return _pressFlash(seed);
    case 'dawnLight':      return _dawnLight();
    case 'greenCheck':     return _greenCheck();
    case 'monitorFlicker': return _monitorFlicker(seed);
  }
}

// ─────────────────────────────────────────────────────────────────────────
// 1. redAlarm — pulsing red beacon above the briefing.

function _redAlarm(severity: Severity): NodeEffectInstance {
  const group = new THREE.Group();
  const intensity =
    severity === 'critical' ? 1.0 :
    severity === 'high'     ? 0.85 :
    severity === 'medium'   ? 0.65 : 0.45;
  const beaconMat = new THREE.MeshBasicMaterial({
    color: 0xff2236, transparent: true, opacity: 0.85, depthWrite: false,
  });
  const beacon = new THREE.Mesh(new THREE.SphereGeometry(0.22, 24, 16), beaconMat);
  beacon.position.set(0, 5.4, -3.0);
  group.add(beacon);

  // Halo plane around the beacon for a "glow" hint.
  const haloMat = new THREE.MeshBasicMaterial({
    color: 0xff5c5c, transparent: true, opacity: 0.45,
    depthWrite: false, side: THREE.DoubleSide,
  });
  const halo = new THREE.Mesh(new THREE.RingGeometry(0.3, 0.7, 32), haloMat);
  halo.position.set(0, 5.4, -3.0);
  halo.rotation.x = Math.PI / 2;
  group.add(halo);

  // Bottom "wash" — large dim red plane on the floor below.
  const washMat = new THREE.MeshBasicMaterial({
    color: 0xff0c1a, transparent: true, opacity: 0.06,
    depthWrite: false, side: THREE.DoubleSide,
  });
  const wash = new THREE.Mesh(new THREE.PlaneGeometry(14, 14), washMat);
  wash.position.set(0, 0.02, -3);
  wash.rotation.x = -Math.PI / 2;
  group.add(wash);

  return {
    group,
    tick: (_dt, t) => {
      const p = 0.55 + 0.45 * Math.sin(t * 4.5);
      beaconMat.opacity = (0.55 + 0.4 * p) * intensity;
      haloMat.opacity = (0.18 + 0.45 * p) * intensity;
      halo.scale.setScalar(1 + p * 0.45);
      washMat.opacity = (0.04 + 0.06 * p) * intensity;
    },
    dispose: () => {
      beacon.geometry.dispose?.(); beaconMat.dispose?.();
      halo.geometry.dispose?.();   haloMat.dispose?.();
      wash.geometry.dispose?.();   washMat.dispose?.();
    },
  };
}

// ─────────────────────────────────────────────────────────────────────────
// 2. orangeSmoke — particle column rising from a reactor.

function _orangeSmoke(seed: number): NodeEffectInstance {
  const group = new THREE.Group();
  const count = 320;
  const positions = new Float32Array(count * 3);
  const colors = new Float32Array(count * 3);
  const ages = new Float32Array(count); // [0, life)
  const lifes = new Float32Array(count);
  const speeds = new Float32Array(count);
  const drifts = new Float32Array(count * 2);
  const rng = _mulberry32(seed);

  for (let i = 0; i < count; i++) {
    _resetSmoke(i, positions, colors, ages, lifes, speeds, drifts, rng, true);
  }

  const geom = new THREE.BufferGeometry();
  geom.setAttribute('position', new THREE.BufferAttribute(positions, 3));
  geom.setAttribute('color', new THREE.BufferAttribute(colors, 3));

  const mat = new THREE.PointsMaterial({
    size: 0.18, vertexColors: true, transparent: true, opacity: 0.75,
    depthWrite: false, blending: THREE.AdditiveBlending,
    sizeAttenuation: true,
  });
  const points = new THREE.Points(geom, mat);
  group.add(points);

  return {
    group,
    tick: (dt, _t) => {
      for (let i = 0; i < count; i++) {
        ages[i]! += dt;
        if (ages[i]! >= lifes[i]!) {
          _resetSmoke(i, positions, colors, ages, lifes, speeds, drifts, rng, false);
        } else {
          positions[i * 3 + 1]! += speeds[i]! * dt;
          positions[i * 3 + 0]! += drifts[i * 2 + 0]! * dt;
          positions[i * 3 + 2]! += drifts[i * 2 + 1]! * dt;
          // Fade as it rises.
          const lifeRatio = ages[i]! / lifes[i]!;
          const fade = 1 - lifeRatio;
          colors[i * 3 + 0]! = 1.0 * fade;
          colors[i * 3 + 1]! = (0.45 - 0.3 * lifeRatio) * fade;
          colors[i * 3 + 2]! = 0.15 * fade * 0.5;
        }
      }
      (geom.attributes.position as any).needsUpdate = true;
      (geom.attributes.color as any).needsUpdate = true;
    },
    dispose: () => { geom.dispose?.(); mat.dispose?.(); },
  };
}

function _resetSmoke(
  i: number,
  positions: Float32Array, colors: Float32Array,
  ages: Float32Array, lifes: Float32Array,
  speeds: Float32Array, drifts: Float32Array,
  rng: () => number, randomAge: boolean,
): void {
  // Emit from one of three reactor centers (matches the chemicalYard layout).
  const cxs = [-3.2, 0, 3.2];
  const cx = cxs[Math.floor(rng() * 3)]!;
  positions[i * 3 + 0]! = cx + (rng() - 0.5) * 0.6;
  positions[i * 3 + 1]! = 3.0 + rng() * 0.4;
  positions[i * 3 + 2]! = -7.0 + (rng() - 0.5) * 0.6;
  speeds[i]! = 0.7 + rng() * 0.8;
  drifts[i * 2 + 0]! = (rng() - 0.5) * 0.15;
  drifts[i * 2 + 1]! = (rng() - 0.5) * 0.05;
  lifes[i]! = 1.4 + rng() * 1.6;
  ages[i]! = randomAge ? rng() * lifes[i]! : 0;
  colors[i * 3 + 0]! = 1.0;
  colors[i * 3 + 1]! = 0.45;
  colors[i * 3 + 2]! = 0.15;
}

// ─────────────────────────────────────────────────────────────────────────
// 3. dataLeak — animated lines streaming out toward the wall.

function _dataLeak(seed: number): NodeEffectInstance {
  const group = new THREE.Group();
  const lines: Array<{ mesh: THREE.Mesh; mat: THREE.MeshBasicMaterial; phase: number; speed: number }> = [];
  const rng = _mulberry32(seed);
  const SOURCE = new THREE.Vector3(0, 1.4, -4); // a server rack
  // 16 streams aiming OUTward through the back wall.
  for (let i = 0; i < 16; i++) {
    const yaw = (i / 16) * Math.PI * 2 + rng() * 0.2;
    const pitch = (rng() - 0.5) * 0.5;
    const dir = new THREE.Vector3(
      Math.sin(yaw) * Math.cos(pitch),
      Math.sin(pitch),
      -Math.abs(Math.cos(yaw) * Math.cos(pitch)) - 0.3, // bias backwards (outside)
    ).normalize();
    const length = 6 + rng() * 4;
    const mat = new THREE.MeshBasicMaterial({
      color: 0x73e0c8, transparent: true, opacity: 0.0,
      depthWrite: false, side: THREE.DoubleSide,
    });
    const mesh = new THREE.Mesh(new THREE.PlaneGeometry(0.025, length), mat);
    // Position midpoint along the ray.
    const mid = SOURCE.clone().add(dir.clone().multiplyScalar(length / 2));
    mesh.position.copy(mid);
    // Orient the plane so its long axis lies along `dir`.
    mesh.lookAt(SOURCE.x, SOURCE.y, SOURCE.z); // back-face the source
    mesh.rotateX(Math.PI / 2);
    group.add(mesh);
    lines.push({ mesh, mat, phase: rng(), speed: 0.6 + rng() * 0.9 });
  }

  return {
    group,
    tick: (_dt, t) => {
      for (const l of lines) {
        const u = ((t * l.speed + l.phase) % 1);
        // 0..0.2 ramp in, 0.6..1 fade out
        const a = u < 0.2 ? u / 0.2 : u > 0.6 ? Math.max(0, 1 - (u - 0.6) / 0.4) : 1;
        l.mat.opacity = a * 0.75;
      }
    },
    dispose: () => {
      for (const l of lines) {
        l.mesh.geometry.dispose?.();
        l.mat.dispose?.();
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────────────────
// 4. pressFlash — random white camera-flash pops.

function _pressFlash(seed: number): NodeEffectInstance {
  const group = new THREE.Group();
  const rng = _mulberry32(seed);
  type Flash = { mesh: THREE.Mesh; mat: THREE.MeshBasicMaterial; t0: number; life: number; nextAt: number };
  const flashes: Flash[] = [];
  // 6 pre-built billboards at random press-room positions.
  for (let i = 0; i < 6; i++) {
    const mat = new THREE.MeshBasicMaterial({
      color: 0xffffff, transparent: true, opacity: 0,
      depthWrite: false, side: THREE.DoubleSide,
    });
    const mesh = new THREE.Mesh(new THREE.PlaneGeometry(0.7, 0.7), mat);
    mesh.position.set(
      (rng() - 0.5) * 8,
      1.4 + rng() * 1.2,
      -2 - rng() * 3,
    );
    mesh.lookAt(0, 1.6, 0);
    group.add(mesh);
    flashes.push({ mesh, mat, t0: -1, life: 0.18 + rng() * 0.18, nextAt: rng() * 1.5 });
  }
  return {
    group,
    tick: (_dt, t) => {
      for (const f of flashes) {
        if (f.t0 < 0) {
          if (t >= f.nextAt) { f.t0 = t; }
        } else {
          const u = (t - f.t0) / f.life;
          if (u >= 1) {
            f.mat.opacity = 0;
            f.t0 = -1;
            f.nextAt = t + 0.4 + Math.random() * 1.8;
          } else {
            // Sharp rise, slow decay.
            const a = u < 0.08 ? u / 0.08 : 1 - (u - 0.08) / 0.92;
            f.mat.opacity = a;
            f.mesh.scale.setScalar(1 + (1 - a) * 1.4);
          }
        }
      }
    },
    dispose: () => {
      for (const f of flashes) {
        f.mesh.geometry.dispose?.();
        f.mat.dispose?.();
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────────────────
// 5. dawnLight — warm directional beam from one side.

function _dawnLight(): NodeEffectInstance {
  const group = new THREE.Group();
  const beam = new THREE.DirectionalLight(0xffd9a0, 1.6);
  beam.position.set(8, 5, -2);
  group.add(beam);
  // Visible "shaft" as a stretched transparent rectangle.
  const shaftMat = new THREE.MeshBasicMaterial({
    color: 0xffd9a0, transparent: true, opacity: 0.16,
    depthWrite: false, side: THREE.DoubleSide,
  });
  const shaft = new THREE.Mesh(new THREE.PlaneGeometry(0.6, 12), shaftMat);
  shaft.position.set(4, 3, -4);
  shaft.rotation.set(0, 0.4, 0.35);
  group.add(shaft);

  return {
    group,
    tick: (_dt, t) => {
      // Slow gentle bobbing.
      shaftMat.opacity = 0.14 + 0.06 * Math.sin(t * 0.6);
    },
    dispose: () => {
      shaft.geometry.dispose?.(); shaftMat.dispose?.();
    },
  };
}

// ─────────────────────────────────────────────────────────────────────────
// 6. greenCheck — expanding green ring (recovery success).

function _greenCheck(): NodeEffectInstance {
  const group = new THREE.Group();
  const ringMat = new THREE.MeshBasicMaterial({
    color: 0x5e9d56, transparent: true, opacity: 0.7,
    depthWrite: false, side: THREE.DoubleSide,
  });
  const ring = new THREE.Mesh(new THREE.RingGeometry(0.4, 0.55, 48), ringMat);
  ring.position.set(0, 4.5, -3);
  ring.rotation.x = -Math.PI / 6;
  group.add(ring);

  return {
    group,
    tick: (_dt, t) => {
      // 2.4 s cycle: expand & fade.
      const cycle = ((t * 0.42) % 1);
      const s = 0.6 + cycle * 4.0;
      ring.scale.setScalar(s);
      ringMat.opacity = (1 - cycle) * 0.75;
    },
    dispose: () => {
      ring.geometry.dispose?.(); ringMat.dispose?.();
    },
  };
}

// ─────────────────────────────────────────────────────────────────────────
// 7. monitorFlicker — 5 wall monitors cycling color (SCADA alarm bank).

function _monitorFlicker(seed: number): NodeEffectInstance {
  const group = new THREE.Group();
  const rng = _mulberry32(seed);
  type Mon = { mesh: THREE.Mesh; mat: THREE.MeshBasicMaterial; phase: number; rate: number };
  const monitors: Mon[] = [];
  const xs = [-3.6, -1.8, 0, 1.8, 3.6];
  for (const x of xs) {
    const mat = new THREE.MeshBasicMaterial({
      color: 0x4dc1ff, transparent: true, opacity: 0.85,
      depthWrite: false, side: THREE.DoubleSide,
    });
    const mesh = new THREE.Mesh(new THREE.PlaneGeometry(0.95, 0.6), mat);
    mesh.position.set(x, 1.55, -5.95);
    group.add(mesh);
    monitors.push({ mesh, mat, phase: rng(), rate: 1.4 + rng() * 1.6 });
  }
  const PALETTE = [
    new THREE.Color(0x4dc1ff), // blue (normal)
    new THREE.Color(0xffb84d), // amber (warning)
    new THREE.Color(0xff4d4d), // red (alarm)
    new THREE.Color(0xf3f3f3), // bleach (login)
  ];
  return {
    group,
    tick: (_dt, t) => {
      for (const m of monitors) {
        const u = (t * m.rate + m.phase * 4);
        // Bias toward red when severe context — we don't see severity here so cycle equally.
        const i = Math.floor(u) % PALETTE.length;
        const f = u - Math.floor(u);
        const a = PALETTE[i]!;
        const b = PALETTE[(i + 1) % PALETTE.length]!;
        m.mat.color.setRGB(
          a.r + (b.r - a.r) * f,
          a.g + (b.g - a.g) * f,
          a.b + (b.b - a.b) * f,
        );
        // Flicker opacity slightly.
        m.mat.opacity = 0.7 + 0.25 * Math.sin(t * 10 + m.phase * 8);
      }
    },
    dispose: () => {
      for (const m of monitors) {
        m.mesh.geometry.dispose?.();
        m.mat.dispose?.();
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────────────────
// Tiny RNG (re-implemented locally to keep this file dependency-free).

function _mulberry32(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
