/**
 * spark/types.ts — Shared types for the Spark-style sample suite
 * (`@gftdcojp/kami-engine-sdk/spark`).
 *
 * Mirrors the Spark 2.0 web-3DGS feature set (worldlabs.ai/blog/spark-2.0):
 *   - 3D Gaussian splat clouds with painter's-algorithm depth sort
 *   - Anisotropic ellipsoid splats (3×3 covariance → screen-space stretch)
 *   - Temporal 4D splats (per-frame keyframe interpolation)
 *   - Dyno-style node shader graph (composable GLSL fragments)
 *   - Foveated rendering hint (radial splat-size bias)
 *
 * Three.js is a peer dependency. All sample mount* helpers accept a
 * canvas + opts and return a `SparkSampleHandle` so a host can dispose
 * the renderer cleanly.
 */

// ─────────────────────────────────────────────────────────────────────────
// 3D splat (Gaussian point cloud)

export interface Splat3D {
  /** World-space position in meters. */
  position: [number, number, number];
  /** Linear RGB, 0..1. */
  color: [number, number, number];
  /** Opacity, 0..1. */
  opacity: number;
  /**
   * Anisotropic scale (semi-axes) in meters. Equal values = isotropic
   * disc; uneven values = stretched ellipsoid (Spark "elliptical" mode).
   */
  scale: [number, number, number];
  /**
   * Optional unit quaternion (x,y,z,w) orienting the ellipsoid. Omit for
   * camera-aligned billboards (Spark "round" mode).
   */
  rotation?: [number, number, number, number];
}

export interface SplatCloudData {
  splats: Splat3D[];
  /** Optional bounding-sphere center for camera framing. */
  center?: [number, number, number];
  /** Optional bounding radius (meters). */
  radius?: number;
}

// ─────────────────────────────────────────────────────────────────────────
// Temporal 4D splat (per-frame keyframe field)

export interface TemporalSplatKeyframe {
  /** Normalized time, 0..1. */
  t: number;
  /** Position override at this keyframe. */
  position: [number, number, number];
  /** Optional color override. Inherits the splat's base color if omitted. */
  color?: [number, number, number];
  /** Optional opacity override. */
  opacity?: number;
}

export interface TemporalSplat4D {
  base: Splat3D;
  /** Sorted by `t` ascending; first keyframe MUST have `t = 0`. */
  keyframes: TemporalSplatKeyframe[];
}

export interface TemporalSplatField {
  splats: TemporalSplat4D[];
  /** Loop duration in seconds. Default 4s. */
  loopSec?: number;
}

// ─────────────────────────────────────────────────────────────────────────
// Dyno shader graph (composable GLSL fragments)

/**
 * A single graph node. Each node contributes one named GLSL function
 * with the canonical signature:
 *
 *   vec4 <name>(vec4 prev, vec2 uv, float t)
 *
 * `prev` is the previous node's output, `uv` is screen-normalized 0..1,
 * `t` is seconds since mount. Nodes are composed left-to-right.
 */
export interface DynoNode {
  /** Stable id used for handle.set* uniforms. */
  id: string;
  /** Human-readable label for HUDs. */
  label?: string;
  /**
   * GLSL function body (just the `{ ... return ...; }` block). The
   * graph compiler wraps it with the canonical signature. Must end
   * with a `return vec4(...)` statement.
   */
  body: string;
  /** Float uniforms exposed to this node. Names become `u_<id>_<name>`. */
  uniforms?: Record<string, number>;
}

export interface DynoGraph {
  nodes: DynoNode[];
}

export interface CompiledDynoGraph {
  /** Full GLSL fragment-shader source. */
  fragmentSource: string;
  /** Initial uniform values keyed by their generated `u_<id>_<name>`. */
  uniforms: Record<string, number>;
}

// ─────────────────────────────────────────────────────────────────────────
// Mount handle

export interface SparkSampleHandle {
  /** Dispose renderer + animation loop. Idempotent. */
  dispose(): void;
  /** True once at least one frame has rendered. */
  ready(): boolean;
  /**
   * Approx splats drawn last frame (after LoD / foveation cull). Useful
   * for HUD overlays. Zero when the sample is not splat-based.
   */
  splatsDrawn(): number;
  /**
   * Set a Dyno-graph uniform at runtime. No-op for non-Dyno samples.
   * Key is the generated `u_<id>_<name>`.
   */
  setUniform?(key: string, value: number): void;
  /** Replace the temporal field. No-op for non-temporal samples. */
  setField?(field: TemporalSplatField): void;
  /** Replace the splat cloud data. No-op for non-cloud samples. */
  setCloud?(data: SplatCloudData): void;
}

// ─────────────────────────────────────────────────────────────────────────
// Common mount opts

export interface SparkMountOpts {
  /** Background clear color, 0xRRGGBB. Default 0xf0ead6 (Nintendo cream — root CLAUDE.md). */
  background?: number;
  /** Camera initial distance (meters). Default 4. */
  cameraDistance?: number;
  /**
   * Splat budget cap (Spark LoD style). The sample sorts back-to-front
   * and renders only the first `splatBudget` after the cut. Default
   * 60_000 (browser-safe).
   */
  splatBudget?: number;
  /**
   * Foveation strength 0..1. 0 = uniform detail; 0.5 = strong center
   * bias. Splats outside the central cone are downsampled before sort.
   * Default 0.
   */
  foveation?: number;
  /** Slow / freeze the auto-rotate camera. Default true. */
  autoRotate?: boolean;
  /** Render device pixel ratio cap. Default min(2, devicePixelRatio). */
  pixelRatioCap?: number;
}
