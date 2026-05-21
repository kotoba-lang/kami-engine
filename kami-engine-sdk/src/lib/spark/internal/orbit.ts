/**
 * spark/internal/orbit.ts — Tiny pointer-orbit camera helper shared by
 * every sample. Avoids depending on `three/addons/controls/OrbitControls`
 * (which is not a peer-dep) and keeps the sample boot < 1 KB gzipped.
 */

import * as THREE from 'three';

export interface OrbitState {
  distance: number;
  azimuth: number;
  elevation: number;
  autoRotate: boolean;
  /** rad/s when autoRotate is on. */
  autoRotateSpeed: number;
}

export interface OrbitController {
  state: OrbitState;
  update(dtSec: number): void;
  apply(camera: THREE.PerspectiveCamera): void;
  dispose(): void;
}

export function createOrbit(canvas: HTMLCanvasElement, init: Partial<OrbitState> = {}): OrbitController {
  const state: OrbitState = {
    distance: init.distance ?? 4,
    azimuth: init.azimuth ?? 0,
    elevation: init.elevation ?? 0.25,
    autoRotate: init.autoRotate ?? true,
    autoRotateSpeed: init.autoRotateSpeed ?? 0.15,
  };

  let dragging = false;
  let lastX = 0;
  let lastY = 0;

  const onDown = (ev: PointerEvent) => {
    dragging = true;
    lastX = ev.clientX;
    lastY = ev.clientY;
    state.autoRotate = false;
    (ev.target as Element | null)?.setPointerCapture?.(ev.pointerId);
  };
  const onMove = (ev: PointerEvent) => {
    if (!dragging) return;
    const dx = (ev.clientX - lastX) / canvas.clientWidth;
    const dy = (ev.clientY - lastY) / canvas.clientHeight;
    state.azimuth -= dx * Math.PI;
    state.elevation = clamp(state.elevation + dy * Math.PI, -1.2, 1.2);
    lastX = ev.clientX;
    lastY = ev.clientY;
  };
  const onUp = () => { dragging = false; };
  const onWheel = (ev: WheelEvent) => {
    ev.preventDefault();
    state.distance = clamp(state.distance * (1 + ev.deltaY * 0.001), 0.8, 30);
  };

  canvas.addEventListener('pointerdown', onDown);
  canvas.addEventListener('pointermove', onMove);
  canvas.addEventListener('pointerup', onUp);
  canvas.addEventListener('pointercancel', onUp);
  canvas.addEventListener('wheel', onWheel, { passive: false });

  return {
    state,
    update(dt) {
      if (state.autoRotate) state.azimuth += state.autoRotateSpeed * dt;
    },
    apply(camera) {
      const cx = Math.cos(state.elevation) * Math.sin(state.azimuth) * state.distance;
      const cy = Math.sin(state.elevation) * state.distance;
      const cz = Math.cos(state.elevation) * Math.cos(state.azimuth) * state.distance;
      camera.position.set(cx, cy, cz);
      camera.lookAt(0, 0, 0);
    },
    dispose() {
      canvas.removeEventListener('pointerdown', onDown);
      canvas.removeEventListener('pointermove', onMove);
      canvas.removeEventListener('pointerup', onUp);
      canvas.removeEventListener('pointercancel', onUp);
      canvas.removeEventListener('wheel', onWheel);
    },
  };
}

function clamp(x: number, lo: number, hi: number): number {
  return x < lo ? lo : x > hi ? hi : x;
}
