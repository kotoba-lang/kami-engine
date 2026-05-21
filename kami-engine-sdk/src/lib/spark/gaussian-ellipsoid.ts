/**
 * spark/gaussian-ellipsoid.ts — Anisotropic Gaussian ellipsoid splats.
 *
 * Implements Spark 2.0's "elliptical splat" mode: each splat is a 3D
 * Gaussian with a 3×3 covariance Σ = R·diag(s²)·Rᵀ. The vertex shader
 * builds an instanced screen-aligned quad whose width/height equal the
 * eigenvalues of the projected 2×2 covariance, oriented along the
 * principal eigenvector.
 *
 * Math notes:
 *   - We pass per-instance scale `s = (sx, sy, sz)` and quaternion `q`.
 *   - Σ = R diag(s²) Rᵀ.
 *   - Project Σ to screen using the Jacobian J of the camera pinhole
 *     map, then approximate as a 2D Gaussian: Σ_2d = J · Σ_view · Jᵀ.
 *   - Eigen-decompose Σ_2d → (λ₁, λ₂, θ) → quad axes.
 *
 * For browser performance we use a cheap diagonalization (closed-form
 * 2×2 eigen) and skip the SH evaluation entirely; color is per-instance
 * RGB (which is what most baked 3DGS exports use anyway).
 */

import * as THREE from 'three';
import type { SparkMountOpts, SparkSampleHandle, SplatCloudData } from './types.js';
import { makeEllipsoidWall } from './data.js';
import { bootSample } from './internal/boot.js';

const VERT = /* glsl */ `
precision highp float;
// Per-vertex (quad corner in [-1,1] × [-1,1])
attribute vec2 aCorner;
// Per-instance
attribute vec3 iPos;
attribute vec3 iScale;
attribute vec4 iQuat;       // (x, y, z, w)
attribute vec3 iColor;
attribute float iOpacity;
uniform float uScreenHeight;
varying vec3 vColor;
varying float vOpacity;
varying vec2 vCorner;

mat3 quatToMat3(vec4 q) {
  float x = q.x, y = q.y, z = q.z, w = q.w;
  return mat3(
    1.0 - 2.0*(y*y + z*z), 2.0*(x*y + z*w),       2.0*(x*z - y*w),
    2.0*(x*y - z*w),       1.0 - 2.0*(x*x + z*z), 2.0*(y*z + x*w),
    2.0*(x*z + y*w),       2.0*(y*z - x*w),       1.0 - 2.0*(x*x + y*y)
  );
}

void main() {
  vColor = iColor;
  vOpacity = iOpacity;
  vCorner = aCorner;

  // Σ in world space
  mat3 R = quatToMat3(iQuat);
  mat3 S = mat3(iScale.x, 0.0, 0.0,  0.0, iScale.y, 0.0,  0.0, 0.0, iScale.z);
  mat3 M = R * S;
  mat3 Sigma = M * transpose(M);

  // Transform Σ into view space: Σ_view = Rv · Σ · Rvᵀ
  mat3 Rv = mat3(viewMatrix);
  mat3 SigmaView = Rv * Sigma * transpose(Rv);

  // View-space center
  vec4 viewPos4 = viewMatrix * vec4(iPos, 1.0);
  vec3 viewPos = viewPos4.xyz;
  float z = -viewPos.z;
  if (z < 0.05) z = 0.05;

  // Pinhole projection Jacobian (focal in NDC units = projectionMatrix[0][0]/[1][1])
  float fx = projectionMatrix[0][0];
  float fy = projectionMatrix[1][1];
  mat2 J = mat2(fx / z, 0.0, 0.0, fy / z);

  // 2D screen-space covariance (NDC units)
  mat2 SigmaXY = mat2(SigmaView[0][0], SigmaView[0][1], SigmaView[1][0], SigmaView[1][1]);
  mat2 Sigma2D = J * SigmaXY * transpose(J);

  // Closed-form 2×2 eigen
  float a = Sigma2D[0][0];
  float b = Sigma2D[0][1];
  float d = Sigma2D[1][1];
  float trace = a + d;
  float det = max(a * d - b * b, 1e-12);
  float disc = sqrt(max(trace * trace * 0.25 - det, 0.0));
  float lambda1 = trace * 0.5 + disc;
  float lambda2 = max(trace * 0.5 - disc, 1e-6);

  // Principal eigenvector
  vec2 v1 = (abs(b) > 1e-6) ? normalize(vec2(lambda1 - d, b)) : vec2(1.0, 0.0);
  vec2 v2 = vec2(-v1.y, v1.x);

  // 3σ extent in NDC. Multiply by 0.5 · screenHeight to convert to clip width.
  float r1 = 3.0 * sqrt(lambda1);
  float r2 = 3.0 * sqrt(lambda2);

  // Offset from center in clip space along the eigen axes
  vec2 ndcOff = v1 * (aCorner.x * r1) + v2 * (aCorner.y * r2);

  vec4 clip = projectionMatrix * viewPos4;
  // Add NDC offset back into clip space (multiply by w)
  clip.xy += ndcOff * clip.w;
  gl_Position = clip;
}
`;

const FRAG = /* glsl */ `
precision mediump float;
varying vec3 vColor;
varying float vOpacity;
varying vec2 vCorner;
void main() {
  float r2 = dot(vCorner, vCorner);
  if (r2 > 1.0) discard;
  // Map [-1,1] quad to ±3σ via 9 · r² in the exponent.
  float g = exp(-r2 * 4.5);
  float a = g * vOpacity;
  gl_FragColor = vec4(vColor * a, a);
}
`;

export interface MountGaussianEllipsoidOpts extends SparkMountOpts {
  /** Initial cloud. Defaults to `makeEllipsoidWall()`. */
  cloud?: SplatCloudData;
  /** Additive blending. Default false (normal "over" — better for opaque ellipsoids). */
  additive?: boolean;
}

export function mountGaussianEllipsoid(canvas: HTMLCanvasElement, opts: MountGaussianEllipsoidOpts = {}): SparkSampleHandle {
  const boot = bootSample(canvas, { autoRotate: true, ...opts });
  const data = opts.cloud ?? makeEllipsoidWall();
  const n = data.splats.length;

  // Quad geometry (-1..1) — 6 verts per instance
  const cornerArr = new Float32Array([
    -1, -1,   1, -1,   1, 1,
    -1, -1,   1,  1,  -1, 1,
  ]);
  const geom = new THREE.InstancedBufferGeometry();
  geom.setAttribute('aCorner', new THREE.BufferAttribute(cornerArr, 2));
  geom.instanceCount = n;

  const iPos = new Float32Array(n * 3);
  const iScale = new Float32Array(n * 3);
  const iQuat = new Float32Array(n * 4);
  const iColor = new Float32Array(n * 3);
  const iOpacity = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    const s = data.splats[i]!;
    iPos.set(s.position, i * 3);
    iScale.set(s.scale, i * 3);
    const q = s.rotation ?? [0, 0, 0, 1];
    iQuat.set(q, i * 4);
    iColor.set(s.color, i * 3);
    iOpacity[i] = s.opacity;
  }
  geom.setAttribute('iPos', new THREE.InstancedBufferAttribute(iPos, 3));
  geom.setAttribute('iScale', new THREE.InstancedBufferAttribute(iScale, 3));
  geom.setAttribute('iQuat', new THREE.InstancedBufferAttribute(iQuat, 4));
  geom.setAttribute('iColor', new THREE.InstancedBufferAttribute(iColor, 3));
  geom.setAttribute('iOpacity', new THREE.InstancedBufferAttribute(iOpacity, 1));

  const material = new THREE.ShaderMaterial({
    vertexShader: VERT,
    fragmentShader: FRAG,
    transparent: true,
    depthWrite: false,
    depthTest: true,
    blending: opts.additive ? THREE.AdditiveBlending : THREE.NormalBlending,
    uniforms: {
      uScreenHeight: { value: canvas.clientHeight || 600 },
    },
  });

  const mesh = new THREE.Mesh(geom, material);
  // Disable frustum culling — bounding-sphere on instanced geom is unreliable.
  (mesh as any).frustumCulled = false;
  boot.scene.add?.(mesh);

  const removeTick = boot.onTick(() => {
    material.uniforms.uScreenHeight.value = canvas.clientHeight || 600;
  });

  return {
    dispose() {
      removeTick();
      geom.dispose?.();
      material.dispose?.();
      boot.dispose();
    },
    ready: () => boot.ready,
    splatsDrawn: () => n,
  };
}
