/**
 * spark/temporal-4d.ts — Temporal-4D splat field (animated Gaussians).
 *
 * Each splat carries N keyframes; the vertex shader interpolates them on
 * the GPU against a `uTime` uniform. Output looks like Spark 2.0's
 * "4D animated transition" demos — splats stream along a tunnel, fade
 * in/out at the endpoints, and shift hue across the loop.
 *
 * Keyframe layout in GPU buffers
 * ──────────────────────────────
 * We pack each splat's KF_COUNT keyframes into per-instance attributes
 * `iKf0..iKf{N-1}` where each Kf is a vec4 = (px, py, pz, t). Colors
 * and opacities follow the same layout. KF_COUNT is fixed at compile
 * time (default 4) — covers the common case (entry/sustain·2/exit).
 */

import * as THREE from 'three';
import type {
  SparkMountOpts,
  SparkSampleHandle,
  TemporalSplatField,
} from './types.js';
import { makeTunnelField } from './data.js';
import { bootSample } from './internal/boot.js';

const KF_COUNT = 4;

function buildVert(kfCount: number): string {
  let inputs = '';
  for (let i = 0; i < kfCount; i++) inputs += `attribute vec4 iKf${i};\n`;        // (x,y,z,t)
  for (let i = 0; i < kfCount; i++) inputs += `attribute vec4 iCol${i};\n`;       // (r,g,b,opacity)
  // Build a chained if/else interpolation in GLSL. With 4 keyframes the
  // chain stays short — readable and unrolled by the driver.
  let interp = '';
  for (let i = 0; i < kfCount - 1; i++) {
    interp += `
  if (u >= iKf${i}.w && u <= iKf${i + 1}.w) {
    float span = max(1e-4, iKf${i + 1}.w - iKf${i}.w);
    float k = (u - iKf${i}.w) / span;
    pos = mix(iKf${i}.xyz, iKf${i + 1}.xyz, k);
    col = mix(iCol${i}.rgb, iCol${i + 1}.rgb, k);
    op  = mix(iCol${i}.a,   iCol${i + 1}.a,   k);
  }`;
  }
  return /* glsl */ `
precision highp float;
attribute vec2 aCorner;
attribute float iSize;
${inputs}
uniform float uTime;
uniform float uLoopSec;
uniform float uPixelSize;
varying vec3 vColor;
varying float vOpacity;
varying vec2 vCorner;
void main() {
  vCorner = aCorner;
  float u = fract(uTime / max(uLoopSec, 0.001));
  vec3 pos = iKf0.xyz;
  vec3 col = iCol0.rgb;
  float op = iCol0.a;
${interp}
  vColor = col;
  vOpacity = op;
  vec4 mv = modelViewMatrix * vec4(pos, 1.0);
  // Screen-space radius (isotropic for the temporal sample).
  float screenR = iSize * uPixelSize / max(-mv.z, 0.01);
  vec4 clip = projectionMatrix * mv;
  // Inject quad corner offset in NDC.
  vec2 ndcOff = aCorner * screenR / max(uPixelSize, 1.0);
  clip.xy += ndcOff * clip.w;
  gl_Position = clip;
}
  `;
}

const FRAG = /* glsl */ `
precision mediump float;
varying vec3 vColor;
varying float vOpacity;
varying vec2 vCorner;
void main() {
  float r2 = dot(vCorner, vCorner);
  if (r2 > 1.0) discard;
  float g = exp(-r2 * 4.5);
  float a = g * vOpacity;
  gl_FragColor = vec4(vColor * a, a);
}
`;

export interface MountTemporalSplat4DOpts extends SparkMountOpts {
  /** Initial field. Defaults to `makeTunnelField(8_000)`. */
  field?: TemporalSplatField;
  /** Additive blending. Default true (luminous tunnel). */
  additive?: boolean;
}

export function mountTemporalSplat4D(canvas: HTMLCanvasElement, opts: MountTemporalSplat4DOpts = {}): SparkSampleHandle {
  const boot = bootSample(canvas, { autoRotate: true, cameraDistance: 5, ...opts });
  let field = opts.field ?? makeTunnelField(8_000);
  let mesh: THREE.Mesh | undefined;
  let geom: THREE.InstancedBufferGeometry | undefined;
  let material: THREE.ShaderMaterial | undefined;
  let count = 0;

  function build(f: TemporalSplatField): void {
    if (mesh) boot.scene.remove?.(mesh);
    geom?.dispose?.();
    material?.dispose?.();
    field = f;
    count = f.splats.length;
    geom = new THREE.InstancedBufferGeometry();
    const cornerArr = new Float32Array([
      -1, -1,   1, -1,   1, 1,
      -1, -1,   1,  1,  -1, 1,
    ]);
    geom.setAttribute('aCorner', new THREE.BufferAttribute(cornerArr, 2));
    geom.instanceCount = count;

    const sizes = new Float32Array(count);
    const kfArrays: Float32Array[] = [];
    const colArrays: Float32Array[] = [];
    for (let k = 0; k < KF_COUNT; k++) {
      kfArrays.push(new Float32Array(count * 4));
      colArrays.push(new Float32Array(count * 4));
    }
    for (let i = 0; i < count; i++) {
      const s = f.splats[i]!;
      sizes[i] = (s.base.scale[0] + s.base.scale[1] + s.base.scale[2]) / 3;
      // Resample to exactly KF_COUNT keyframes by linear-time selection
      // along the t axis (first, two middle stops, last).
      const ks = s.keyframes.length > 0 ? s.keyframes : [{ t: 0, position: s.base.position }];
      const ts = [0, 1 / 3, 2 / 3, 1];
      for (let k = 0; k < KF_COUNT; k++) {
        const tt = ts[k]!;
        const samp = sampleAt(s, tt, ks);
        kfArrays[k]!.set([samp.position[0], samp.position[1], samp.position[2], tt], i * 4);
        colArrays[k]!.set([samp.color[0], samp.color[1], samp.color[2], samp.opacity], i * 4);
      }
    }
    geom.setAttribute('iSize', new THREE.InstancedBufferAttribute(sizes, 1));
    for (let k = 0; k < KF_COUNT; k++) {
      geom.setAttribute(`iKf${k}`, new THREE.InstancedBufferAttribute(kfArrays[k]!, 4));
      geom.setAttribute(`iCol${k}`, new THREE.InstancedBufferAttribute(colArrays[k]!, 4));
    }

    material = new THREE.ShaderMaterial({
      vertexShader: buildVert(KF_COUNT),
      fragmentShader: FRAG,
      transparent: true,
      depthWrite: false,
      depthTest: true,
      blending: opts.additive === false ? THREE.NormalBlending : THREE.AdditiveBlending,
      uniforms: {
        uTime: { value: 0 },
        uLoopSec: { value: f.loopSec ?? 6 },
        uPixelSize: { value: canvas.clientHeight || 600 },
      },
    });

    mesh = new THREE.Mesh(geom, material);
    (mesh as any).frustumCulled = false;
    boot.scene.add?.(mesh);
  }
  build(field);

  const removeTick = boot.onTick((_dt, t) => {
    if (material) {
      material.uniforms.uTime.value = t;
      material.uniforms.uPixelSize.value = canvas.clientHeight || 600;
    }
  });

  return {
    dispose() {
      removeTick();
      geom?.dispose?.();
      material?.dispose?.();
      boot.dispose();
    },
    ready: () => boot.ready,
    splatsDrawn: () => count,
    setField: (f) => build(f),
  };
}

// ─────────────────────────────────────────────────────────────────────────
// Local helper — sample arbitrary-length keyframes at u∈[0,1].

function sampleAt(
  splat: TemporalSplatField['splats'][number],
  u: number,
  ks: TemporalSplatField['splats'][number]['keyframes'],
): { position: [number, number, number]; color: [number, number, number]; opacity: number } {
  if (ks.length === 0) {
    return { position: splat.base.position, color: splat.base.color, opacity: splat.base.opacity };
  }
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
