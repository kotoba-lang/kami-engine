/**
 * spark/dyno-graph.ts — Dyno-style node shader graph sample.
 *
 * Inspired by Spark 2.0's "Dyno shader graph" (an alternative to raw
 * GLSL for the user-programmable GPU pipeline). We compose a chain of
 * named nodes — each contributing one GLSL function with the canonical
 * signature `vec4 <name>(vec4 prev, vec2 uv, float t)` — into a single
 * fullscreen fragment shader.
 *
 *   compileDynoGraph(graph)              → CompiledDynoGraph
 *   mountDynoSample(canvas, graph, opts) → SparkSampleHandle
 *
 * The graph runs as a fullscreen pass on an OrthographicCamera, so it's
 * also a convenient post-fx playground.
 *
 * Built-in node helpers (see `dynoNodeLibrary`):
 *   - splatBackdrop : radial Gaussian backdrop matching the splat suite
 *   - rgbeBoost     : RGBE-style HDR boost (mantissa·2^exp)
 *   - hueShift      : screen-space hue rotation
 *   - vignette      : radial darkening
 *   - scanlines     : retro CRT scanlines
 */

import * as THREE from 'three';
import type {
  CompiledDynoGraph,
  DynoGraph,
  DynoNode,
  SparkMountOpts,
  SparkSampleHandle,
} from './types.js';

// ─────────────────────────────────────────────────────────────────────────
// Compile

/**
 * Compile a `DynoGraph` into a fullscreen fragment-shader source and an
 * initial uniform map. Node names are sanitized to GLSL identifiers; a
 * duplicate-id check is done at compile time so a graph can't silently
 * shadow itself.
 */
export function compileDynoGraph(graph: DynoGraph): CompiledDynoGraph {
  const seen = new Set<string>();
  for (const n of graph.nodes) {
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(n.id)) {
      throw new Error(`dyno: invalid node id "${n.id}" (must match /^[A-Za-z_][A-Za-z0-9_]*$/)`);
    }
    if (seen.has(n.id)) throw new Error(`dyno: duplicate node id "${n.id}"`);
    seen.add(n.id);
  }

  const uniformDecls: string[] = ['uniform float uTime;', 'uniform vec2 uResolution;'];
  const uniforms: Record<string, number> = {};
  const fnDecls: string[] = [];
  const calls: string[] = ['vec4 col = vec4(0.0);'];

  for (const n of graph.nodes) {
    // Per-node uniforms
    if (n.uniforms) {
      for (const [name, value] of Object.entries(n.uniforms)) {
        if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) {
          throw new Error(`dyno: invalid uniform name "${name}" on node "${n.id}"`);
        }
        const key = `u_${n.id}_${name}`;
        uniformDecls.push(`uniform float ${key};`);
        uniforms[key] = value;
      }
    }
    fnDecls.push(`vec4 ${n.id}(vec4 prev, vec2 uv, float t) ${n.body}`);
    calls.push(`col = ${n.id}(col, vUv, uTime);`);
  }

  const fragmentSource = /* glsl */ `
precision mediump float;
varying vec2 vUv;
${uniformDecls.join('\n')}

${fnDecls.join('\n\n')}

void main() {
  ${calls.join('\n  ')}
  gl_FragColor = vec4(col.rgb, 1.0);
}
`;
  return { fragmentSource, uniforms };
}

// ─────────────────────────────────────────────────────────────────────────
// Built-in node library

export const dynoNodeLibrary: Record<string, () => DynoNode> = {
  splatBackdrop: () => ({
    id: 'splatBackdrop',
    label: 'Splat backdrop',
    uniforms: { intensity: 0.85 },
    body: /* glsl */ `{
      vec2 p = uv * 2.0 - 1.0;
      float r = length(p);
      vec3 a = vec3(0.93, 0.91, 0.83);   // Nintendo cream
      vec3 b = vec3(0.35, 0.55, 0.95);   // azure
      vec3 c = vec3(0.95, 0.62, 0.30);   // amber
      vec3 col = mix(a, b, smoothstep(0.0, 0.9, r));
      col = mix(col, c, smoothstep(0.6, 1.4, r) * 0.4);
      float g = exp(-r * r * 1.5);
      return vec4(col * (0.7 + g * u_splatBackdrop_intensity), 1.0);
    }`,
  }),

  rgbeBoost: () => ({
    id: 'rgbeBoost',
    label: 'RGBE HDR boost',
    uniforms: { exposure: 1.6 },
    body: /* glsl */ `{
      // Treat the alpha as a faux RGBE exponent so HDR splat captures
      // pop without blowing out.
      vec3 boosted = prev.rgb * pow(2.0, u_rgbeBoost_exposure * 0.5);
      return vec4(boosted / (boosted + vec3(1.0)), prev.a);
    }`,
  }),

  hueShift: () => ({
    id: 'hueShift',
    label: 'Hue shift',
    uniforms: { speed: 0.15 },
    body: /* glsl */ `{
      float a = t * u_hueShift_speed;
      float c = cos(a), s = sin(a);
      mat3 m = mat3(
        0.299 + 0.701*c + 0.168*s,   0.587 - 0.587*c + 0.330*s,   0.114 - 0.114*c - 0.497*s,
        0.299 - 0.299*c - 0.328*s,   0.587 + 0.413*c + 0.035*s,   0.114 - 0.114*c + 0.292*s,
        0.299 - 0.300*c + 1.250*s,   0.587 - 0.588*c - 1.050*s,   0.114 + 0.886*c - 0.203*s
      );
      return vec4(clamp(m * prev.rgb, 0.0, 2.0), prev.a);
    }`,
  }),

  vignette: () => ({
    id: 'vignette',
    label: 'Vignette',
    uniforms: { strength: 0.55 },
    body: /* glsl */ `{
      vec2 p = uv - 0.5;
      float v = smoothstep(0.7, 0.2, length(p));
      return vec4(prev.rgb * mix(1.0 - u_vignette_strength, 1.0, v), prev.a);
    }`,
  }),

  scanlines: () => ({
    id: 'scanlines',
    label: 'Scanlines',
    uniforms: { strength: 0.18, density: 220.0 },
    body: /* glsl */ `{
      float line = sin(uv.y * u_scanlines_density + t * 2.0) * 0.5 + 0.5;
      float k = mix(1.0, line, u_scanlines_strength);
      return vec4(prev.rgb * k, prev.a);
    }`,
  }),
};

/** Convenience: a "spark demo" preset graph wiring all five built-ins. */
export function defaultDynoGraph(): DynoGraph {
  return {
    nodes: [
      dynoNodeLibrary.splatBackdrop!(),
      dynoNodeLibrary.rgbeBoost!(),
      dynoNodeLibrary.hueShift!(),
      dynoNodeLibrary.vignette!(),
      dynoNodeLibrary.scanlines!(),
    ],
  };
}

// ─────────────────────────────────────────────────────────────────────────
// Mount

const VERT = /* glsl */ `
precision mediump float;
attribute vec3 position;
attribute vec2 uv;
varying vec2 vUv;
void main() {
  vUv = uv;
  gl_Position = vec4(position.xy, 0.0, 1.0);
}
`;

export interface MountDynoSampleOpts extends SparkMountOpts {
  /** Graph to compile. Defaults to `defaultDynoGraph()`. */
  graph?: DynoGraph;
}

export function mountDynoSample(canvas: HTMLCanvasElement, opts: MountDynoSampleOpts = {}): SparkSampleHandle {
  const graph = opts.graph ?? defaultDynoGraph();
  const compiled = compileDynoGraph(graph);

  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: false });
  renderer.setPixelRatio(Math.min(opts.pixelRatioCap ?? 2, (typeof window !== 'undefined' ? window.devicePixelRatio : 1) || 1));
  renderer.setSize(canvas.clientWidth || 800, canvas.clientHeight || 600, false);

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(opts.background ?? 0xf0ead6);
  const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);

  // Fullscreen triangle (NDC-space). Uses RawShaderMaterial so we control
  // the vertex/uv attributes explicitly.
  const geom = new THREE.BufferGeometry();
  geom.setAttribute('position', new THREE.Float32BufferAttribute([-1, -1, 0, 3, -1, 0, -1, 3, 0], 3));
  geom.setAttribute('uv', new THREE.Float32BufferAttribute([0, 0, 2, 0, 0, 2], 2));

  const uniformBag: Record<string, { value: number | THREE.Vector2 }> = {
    uTime: { value: 0 as any },
    uResolution: { value: new THREE.Vector2(canvas.clientWidth || 800, canvas.clientHeight || 600) },
  };
  for (const [k, v] of Object.entries(compiled.uniforms)) uniformBag[k] = { value: v };

  const material = new THREE.RawShaderMaterial({
    vertexShader: VERT,
    fragmentShader: compiled.fragmentSource,
    uniforms: uniformBag,
    depthTest: false,
    depthWrite: false,
  });

  const mesh = new THREE.Mesh(geom, material);
  (mesh as any).frustumCulled = false;
  scene.add?.(mesh);

  const clock = new THREE.Clock();
  let raf = 0;
  const state = { ready: false, disposed: false };

  const loop = () => {
    if (state.disposed) return;
    (uniformBag.uTime!.value as any) = clock.getElapsedTime();
    const w = canvas.clientWidth || 800;
    const h = canvas.clientHeight || 600;
    if (renderer.domElement && (renderer.domElement.width !== w || renderer.domElement.height !== h)) {
      renderer.setSize(w, h, false);
    }
    (uniformBag.uResolution!.value as THREE.Vector2).set?.(w, h);
    renderer.render(scene, camera);
    state.ready = true;
    raf = requestAnimationFrame(loop);
  };
  raf = requestAnimationFrame(loop);

  return {
    dispose() {
      if (state.disposed) return;
      state.disposed = true;
      cancelAnimationFrame(raf);
      geom.dispose?.();
      material.dispose?.();
      renderer.dispose?.();
    },
    ready: () => state.ready,
    splatsDrawn: () => 0,
    setUniform(key, value) {
      const u = uniformBag[key];
      if (u) (u.value as any) = value;
    },
  };
}
