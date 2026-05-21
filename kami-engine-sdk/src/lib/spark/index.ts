/**
 * `@gftdcojp/kami-engine-sdk/spark` — Spark 2.0-style web-3DGS sample
 * suite. Four self-contained Three.js samples that demonstrate the
 * concepts from worldlabs.ai/blog/spark-2.0 in browser-safe code:
 *
 *   1. mountSplatCloud         — 3D Gaussian point cloud + painter sort
 *                                + foveated LoD budget
 *   2. mountGaussianEllipsoid  — anisotropic ellipsoid splats (Σ project)
 *   3. mountTemporalSplat4D    — 4D animated splats (keyframe interp)
 *   4. mountDynoSample         — Dyno-style node shader graph
 *
 * Plus helpers:
 *   - compileDynoGraph / dynoNodeLibrary / defaultDynoGraph
 *   - makeGalaxyCloud / makeEllipsoidWall / makeTunnelField / mulberry32
 *   - sampleTemporal (for baking / offline tests)
 *
 * three is a peer dependency. No additional runtime deps.
 *
 * Reference: ADR-2605092000 (FP8 vector substrate), ADR-2605092500
 * (sap-flow vector reasoning). The samples are NOT an L7 inference
 * surface — they're a presentation layer for already-baked splats.
 */

export type {
  Splat3D,
  SplatCloudData,
  TemporalSplat4D,
  TemporalSplatField,
  TemporalSplatKeyframe,
  DynoNode,
  DynoGraph,
  CompiledDynoGraph,
  SparkSampleHandle,
  SparkMountOpts,
} from './types.js';

export {
  mulberry32,
  makeGalaxyCloud,
  makeEllipsoidWall,
  makeTunnelField,
  makeLocationCloud,
  sampleTemporal,
  type SparkLocationKind,
} from './data.js';

export {
  mountSplatCloud,
  createSplatCloudLayer,
  type MountSplatCloudOpts,
  type SplatCloudLayer,
  type CreateSplatCloudLayerOpts,
} from './splat-cloud.js';
export { mountGaussianEllipsoid, type MountGaussianEllipsoidOpts } from './gaussian-ellipsoid.js';
export { mountTemporalSplat4D, type MountTemporalSplat4DOpts } from './temporal-4d.js';
export {
  mountDynoSample,
  compileDynoGraph,
  dynoNodeLibrary,
  defaultDynoGraph,
  type MountDynoSampleOpts,
} from './dyno-graph.js';
