/**
 * spark/splat-cloud.ts — 3D Gaussian splat point cloud sample.
 *
 * Emulates Spark 2.0's "isotropic round splat" mode with:
 *   - Painter's-algorithm depth sort (back-to-front, every frame)
 *   - LoD budget cap (`splatBudget`) — only the front N after sort are drawn
 *   - Optional foveated rendering — a radial size bias around screen center
 *   - Premultiplied additive "over" blending against the canvas
 *
 * The fragment shader evaluates a 2D Gaussian falloff inside the gl_PointCoord
 * disc, which approximates a screen-space 3DGS without needing the full
 * covariance projection (covered by `gaussian-ellipsoid.ts`).
 */

import * as THREE from 'three';
import type { SparkMountOpts, SparkSampleHandle, SplatCloudData } from './types.js';
import { makeGalaxyCloud } from './data.js';
import { bootSample } from './internal/boot.js';

const VERT = /* glsl */ `
attribute vec3 aColor;
attribute float aOpacity;
attribute float aSize;
varying vec3 vColor;
varying float vOpacity;
uniform float uPixelSize;
uniform float uFoveation;
void main() {
  vColor = aColor;
  vOpacity = aOpacity;
  vec4 mv = modelViewMatrix * vec4(position, 1.0);
  // Perspective-correct point size (matches Spark's ellipsoid screen radius
  // for the isotropic case).
  float screenR = aSize * uPixelSize / max(-mv.z, 0.01);

  gl_Position = projectionMatrix * mv;
  // Foveation: shrink splats far from screen center to mimic Spark's
  // priority-queue budget redistribution.
  vec2 ndc = gl_Position.xy / max(gl_Position.w, 0.0001);
  float radial = length(ndc);
  float fov = mix(1.0, max(0.18, 1.0 - radial * 1.4), clamp(uFoveation, 0.0, 1.0));
  gl_PointSize = max(2.0, screenR * fov);
}
`;

const FRAG = /* glsl */ `
precision mediump float;
varying vec3 vColor;
varying float vOpacity;
void main() {
  vec2 p = gl_PointCoord * 2.0 - 1.0;
  float r2 = dot(p, p);
  if (r2 > 1.0) discard;
  // 2D Gaussian with σ ≈ 0.45 — matches the visual sharpness of a baked
  // 3DGS splat without spilling over the point sprite.
  float g = exp(-r2 * 4.5);
  float a = g * vOpacity;
  gl_FragColor = vec4(vColor * a, a); // premultiplied
}
`;

export interface MountSplatCloudOpts extends SparkMountOpts {
  /** Initial cloud. Defaults to `makeGalaxyCloud(20_000)`. */
  cloud?: SplatCloudData;
  /** Use additive blending (luminous look) vs. normal alpha. Default true. */
  additive?: boolean;
}

// ─────────────────────────────────────────────────────────────────────────
// Scene-attachable splat-cloud layer.
//
// Same shader + painter sort as `mountSplatCloud`, but accepts an existing
// camera and returns a plain Object3D so the host can attach the splats to
// any Three.js scene graph (e.g. the cyber-drill WebVR scene). The host is
// responsible for calling `tick()` once per frame and `dispose()` on
// teardown. No renderer / RAF is owned by this layer.

export interface SplatCloudLayer {
  /** The Object3D you add to your scene. */
  object3D: THREE.Points;
  /** Call once per RAF to refresh the painter sort + uniforms. */
  tick(canvasHeightPx: number): void;
  /** Swap to a new cloud (re-uploads buffers). */
  setCloud(data: SplatCloudData): void;
  /** Splats actually drawn last tick (after budget cap). */
  splatsDrawn(): number;
  /** Free GPU resources. */
  dispose(): void;
}

export interface CreateSplatCloudLayerOpts {
  /** Initial cloud — required (use makeGalaxyCloud or makeLocationCloud). */
  cloud: SplatCloudData;
  /** Camera that drives the painter sort. Required. */
  camera: THREE.PerspectiveCamera;
  /** Render budget cap. Default 60_000. */
  splatBudget?: number;
  /** Foveation 0..1. Default 0. */
  foveation?: number;
  /** Use AdditiveBlending (luminous) vs NormalBlending. Default true. */
  additive?: boolean;
  /** Global opacity multiplier (0..1). Default 1. */
  opacityMul?: number;
}

export function createSplatCloudLayer(opts: CreateSplatCloudLayerOpts): SplatCloudLayer {
  const budget = Math.max(500, opts.splatBudget ?? 60_000);
  const additive = opts.additive !== false;

  const material = new THREE.ShaderMaterial({
    vertexShader: VERT,
    fragmentShader: FRAG,
    transparent: true,
    depthWrite: false,
    depthTest: true,
    blending: additive ? THREE.AdditiveBlending : THREE.NormalBlending,
    uniforms: {
      uPixelSize: { value: 600 },
      uFoveation: { value: Math.max(0, Math.min(1, opts.foveation ?? 0)) },
    },
  });

  let cloud: CloudInternals | undefined;

  function build(data: SplatCloudData): void {
    if (cloud) {
      cloud.geom.dispose?.();
    }
    const total = data.splats.length;
    const cap = Math.min(total, budget);
    const positions = new Float32Array(total * 3);
    const colors = new Float32Array(total * 3);
    const opacities = new Float32Array(total);
    const sizes = new Float32Array(total);
    const opMul = Math.max(0, Math.min(1, opts.opacityMul ?? 1));
    for (let i = 0; i < total; i++) {
      const s = data.splats[i]!;
      positions[i * 3 + 0] = s.position[0];
      positions[i * 3 + 1] = s.position[1];
      positions[i * 3 + 2] = s.position[2];
      colors[i * 3 + 0] = s.color[0];
      colors[i * 3 + 1] = s.color[1];
      colors[i * 3 + 2] = s.color[2];
      opacities[i] = s.opacity * opMul;
      sizes[i] = (s.scale[0] + s.scale[1] + s.scale[2]) / 3;
    }
    const indices = new Uint32Array(cap);
    for (let i = 0; i < cap; i++) indices[i] = i;
    const depths = new Float32Array(total);

    const geom = new THREE.BufferGeometry();
    geom.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    geom.setAttribute('aColor', new THREE.BufferAttribute(colors, 3));
    geom.setAttribute('aOpacity', new THREE.BufferAttribute(opacities, 1));
    geom.setAttribute('aSize', new THREE.BufferAttribute(sizes, 1));
    geom.setIndex(new THREE.BufferAttribute(indices, 1));
    geom.setDrawRange(0, cap);

    // Re-parent the new geometry onto the existing Points object so the
    // host's scene-add stays valid across setCloud() calls.
    if (cloud) {
      cloud.points.geometry = geom;
      cloud.geom = geom;
      cloud.positions = positions;
      cloud.colors = colors;
      cloud.opacities = opacities;
      cloud.sizes = sizes;
      cloud.indices = indices;
      cloud.depths = depths;
      cloud.drawn = cap;
      cloud.total = total;
    } else {
      const points = new THREE.Points(geom, material);
      cloud = { geom, points, positions, colors, opacities, sizes, indices, depths, drawn: cap, total };
    }
  }

  build(opts.cloud);

  const cam = opts.camera;
  const tickFn = (canvasHeightPx: number): void => {
    if (!cloud) return;
    const { positions, depths, indices, total } = cloud;
    cam.updateMatrixWorld?.(true);
    const camPos: { x: number; y: number; z: number } = cam.position;
    for (let i = 0; i < total; i++) {
      const dx = positions[i * 3 + 0]! - camPos.x;
      const dy = positions[i * 3 + 1]! - camPos.y;
      const dz = positions[i * 3 + 2]! - camPos.z;
      depths[i] = dx * dx + dy * dy + dz * dz;
    }
    const fov = (material.uniforms.uFoveation?.value as number) ?? 0;
    if (fov > 0.01) {
      for (let i = 0; i < total; i++) {
        const dx = positions[i * 3 + 0]! - camPos.x;
        const dy = positions[i * 3 + 1]! - camPos.y;
        const dz = positions[i * 3 + 2]! - camPos.z;
        const camLen = Math.sqrt(camPos.x * camPos.x + camPos.y * camPos.y + camPos.z * camPos.z);
        const dot = (camPos.x * dx + camPos.y * dy + camPos.z * dz) / Math.max(1e-3, camLen);
        const lateral = Math.max(0, depths[i]! - dot * dot);
        depths[i] = depths[i]! + lateral * fov * 4;
      }
    }
    const perm = new Uint32Array(total);
    for (let i = 0; i < total; i++) perm[i] = i;
    perm.sort((a, b) => depths[b]! - depths[a]!);
    const cap = Math.min(total, budget);
    for (let i = 0; i < cap; i++) indices[i] = perm[total - cap + i]!;
    cloud.drawn = cap;
    (cloud.geom.index as any).needsUpdate = true;
    cloud.geom.setDrawRange?.(0, cap);
    material.uniforms.uPixelSize.value = canvasHeightPx || 600;
  };

  return {
    get object3D() { return cloud!.points as THREE.Points; },
    tick: tickFn,
    setCloud: build,
    splatsDrawn: () => cloud?.drawn ?? 0,
    dispose: () => {
      cloud?.geom.dispose?.();
      material.dispose?.();
    },
  };
}

interface CloudInternals {
  geom: THREE.BufferGeometry;
  points: THREE.Points;
  positions: Float32Array;
  colors: Float32Array;
  opacities: Float32Array;
  sizes: Float32Array;
  indices: Uint32Array;
  /** Cached depth per splat, reused each frame. */
  depths: Float32Array;
  drawn: number;
  /** Total splats in the source cloud (uncapped). */
  total: number;
}

export function mountSplatCloud(canvas: HTMLCanvasElement, opts: MountSplatCloudOpts = {}): SparkSampleHandle {
  const boot = bootSample(canvas, opts);
  const budget = Math.max(1000, opts.splatBudget ?? 60_000);
  const additive = opts.additive !== false;
  const initialCloud = opts.cloud ?? makeGalaxyCloud(20_000);

  const material = new THREE.ShaderMaterial({
    vertexShader: VERT,
    fragmentShader: FRAG,
    transparent: true,
    depthWrite: false,
    depthTest: true,
    blending: additive ? THREE.AdditiveBlending : THREE.NormalBlending,
    uniforms: {
      uPixelSize: { value: 600 },
      uFoveation: { value: Math.max(0, Math.min(1, opts.foveation ?? 0)) },
    },
  });

  let cloud: CloudInternals | undefined;

  function buildCloud(data: SplatCloudData): void {
    if (cloud) {
      cloud.geom.dispose?.();
      boot.scene.remove?.(cloud.points);
    }
    const total = data.splats.length;
    const cap = Math.min(total, budget);
    const positions = new Float32Array(total * 3);
    const colors = new Float32Array(total * 3);
    const opacities = new Float32Array(total);
    const sizes = new Float32Array(total);
    for (let i = 0; i < total; i++) {
      const s = data.splats[i]!;
      positions[i * 3 + 0] = s.position[0];
      positions[i * 3 + 1] = s.position[1];
      positions[i * 3 + 2] = s.position[2];
      colors[i * 3 + 0] = s.color[0];
      colors[i * 3 + 1] = s.color[1];
      colors[i * 3 + 2] = s.color[2];
      opacities[i] = s.opacity;
      // Isotropic radius — anisotropic ellipsoids belong to gaussian-ellipsoid sample.
      sizes[i] = (s.scale[0] + s.scale[1] + s.scale[2]) / 3;
    }
    const indices = new Uint32Array(cap);
    for (let i = 0; i < cap; i++) indices[i] = i;
    const depths = new Float32Array(total);

    const geom = new THREE.BufferGeometry();
    geom.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    geom.setAttribute('aColor', new THREE.BufferAttribute(colors, 3));
    geom.setAttribute('aOpacity', new THREE.BufferAttribute(opacities, 1));
    geom.setAttribute('aSize', new THREE.BufferAttribute(sizes, 1));
    geom.setIndex(new THREE.BufferAttribute(indices, 1));
    geom.setDrawRange(0, cap);
    const points = new THREE.Points(geom, material);
    boot.scene.add?.(points);
    cloud = { geom, points, positions, colors, opacities, sizes, indices, depths, drawn: cap, total };
  }

  buildCloud(initialCloud);

  // Painter's-algorithm sort runs every frame on a single typed-array
  // permutation. ~5 ms for 60k splats on M2; cheap because we only touch
  // the index buffer and reuse the depth scratch space.
  const cam = boot.camera;
  const tmp = new THREE.Vector3();
  const removeTick = boot.onTick(() => {
    if (!cloud) return;
    const { positions, depths, indices, total } = cloud;
    cam.updateMatrixWorld?.(true);
    const camPos: { x: number; y: number; z: number } = cam.position;
    // Compute squared distance from camera (cheap surrogate for depth).
    for (let i = 0; i < total; i++) {
      const dx = positions[i * 3 + 0]! - camPos.x;
      const dy = positions[i * 3 + 1]! - camPos.y;
      const dz = positions[i * 3 + 2]! - camPos.z;
      depths[i] = dx * dx + dy * dy + dz * dz;
    }
    // Foveation: bias depths by radial distance so the priority queue
    // (here: sort) preserves more central splats than peripheral ones.
    const fov = (material.uniforms.uFoveation?.value as number) ?? 0;
    if (fov > 0.01) {
      const inv = 1.0 / Math.max(1e-3, cam.position.length?.() ?? 1);
      for (let i = 0; i < total; i++) {
        // Approximate radial offset using projected XY against camera vector.
        const dx = positions[i * 3 + 0]! - camPos.x;
        const dy = positions[i * 3 + 1]! - camPos.y;
        const dz = positions[i * 3 + 2]! - camPos.z;
        const camLen = Math.sqrt(camPos.x * camPos.x + camPos.y * camPos.y + camPos.z * camPos.z);
        const dot = (camPos.x * dx + camPos.y * dy + camPos.z * dz) / Math.max(1e-3, camLen);
        const lateral = Math.max(0, depths[i]! - dot * dot);
        // Larger `lateral` ⇒ pushed deeper, so they fall outside the budget.
        depths[i] = depths[i]! + lateral * fov * 4 * inv;
      }
    }
    // Sort all `total` indices by depth descending (farthest first → drawn
    // first → blended over by nearer splats; this is the "over" operator).
    const perm = new Uint32Array(total);
    for (let i = 0; i < total; i++) perm[i] = i;
    perm.sort((a, b) => depths[b]! - depths[a]!);
    // Keep only the closest `budget` (the back N are culled — they're the
    // most-occluded farthest splats anyway).
    const cap = Math.min(total, budget);
    // The last `cap` entries are the nearest; copy them in painter order
    // (still back-to-front within the kept set).
    for (let i = 0; i < cap; i++) indices[i] = perm[total - cap + i]!;
    cloud.drawn = cap;
    (cloud.geom.index as any).needsUpdate = true;
    cloud.geom.setDrawRange?.(0, cap);
    material.uniforms.uPixelSize.value = canvas.clientHeight || 600;
  });

  return {
    dispose() {
      removeTick();
      if (cloud) cloud.geom.dispose?.();
      material.dispose?.();
      boot.dispose();
    },
    ready: () => boot.ready,
    splatsDrawn: () => cloud?.drawn ?? 0,
    setCloud: (data) => buildCloud(data),
  };
}
