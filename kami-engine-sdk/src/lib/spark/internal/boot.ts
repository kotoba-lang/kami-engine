/**
 * spark/internal/boot.ts — Shared renderer + camera + resize bootstrap.
 *
 * All four samples (splat-cloud, gaussian-ellipsoid, temporal-4d, dyno)
 * call `bootSample(canvas, opts)` once; the returned handle owns the
 * renderer / scene / camera / clock / orbit-controller and exposes a
 * `tick` hook every RAF.
 */

import * as THREE from 'three';
import type { SparkMountOpts } from '../types.js';
import { createOrbit, type OrbitController } from './orbit.js';

export interface SampleBootstrap {
  renderer: THREE.WebGLRenderer;
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  clock: THREE.Clock;
  orbit: OrbitController;
  /** Number of frames rendered so far (incremented after each render). */
  frame: number;
  /** True once render() has run at least once. */
  ready: boolean;
  /**
   * Add a per-frame callback. Returns an unsubscribe fn. Callbacks run in
   * insertion order before the renderer draws.
   */
  onTick(fn: (dtSec: number, tSec: number) => void): () => void;
  dispose(): void;
}

export function bootSample(canvas: HTMLCanvasElement, opts: SparkMountOpts = {}): SampleBootstrap {
  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: false });
  renderer.setPixelRatio(Math.min(opts.pixelRatioCap ?? 2, (typeof window !== 'undefined' ? window.devicePixelRatio : 1) || 1));
  renderer.setSize(canvas.clientWidth || 800, canvas.clientHeight || 600, false);
  renderer.outputColorSpace = THREE.SRGBColorSpace;

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(opts.background ?? 0xf0ead6);

  const camera = new THREE.PerspectiveCamera(
    55,
    (canvas.clientWidth || 800) / (canvas.clientHeight || 600),
    0.01,
    100,
  );
  const orbit = createOrbit(canvas, {
    distance: opts.cameraDistance ?? 4,
    autoRotate: opts.autoRotate ?? true,
  });
  orbit.apply(camera);

  const clock = new THREE.Clock();
  const ticks = new Set<(dt: number, t: number) => void>();
  const state = { frame: 0, ready: false, disposed: false };

  let raf = 0;
  const loop = () => {
    if (state.disposed) return;
    const dt = clock.getDelta();
    const t = clock.getElapsedTime();
    orbit.update(dt);
    orbit.apply(camera);
    for (const fn of ticks) fn(dt, t);
    renderer.render(scene, camera);
    state.frame += 1;
    state.ready = true;
    raf = requestAnimationFrame(loop);
  };
  raf = requestAnimationFrame(loop);

  const onResize = () => {
    const w = canvas.clientWidth || 800;
    const h = canvas.clientHeight || 600;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  };
  if (typeof window !== 'undefined') window.addEventListener('resize', onResize);

  return {
    renderer,
    scene,
    camera,
    clock,
    orbit,
    get frame() { return state.frame; },
    get ready() { return state.ready; },
    onTick(fn) {
      ticks.add(fn);
      return () => ticks.delete(fn);
    },
    dispose() {
      if (state.disposed) return;
      state.disposed = true;
      cancelAnimationFrame(raf);
      if (typeof window !== 'undefined') window.removeEventListener('resize', onResize);
      orbit.dispose();
      renderer.dispose?.();
    },
  };
}
