/** KAMI Engine WASM exported functions (wasm-bindgen). */
export interface KamiWasmExports {
  runEmbedVrm(canvasId: string, vrmUrl: string): Promise<void>;
  setVrmMorph(index: number, weight: number): void;
  setVrmMorphByName(name: string, weight: number): void;
  getVrmMorphNames(): string;
  resetVrmMorphs(): void;
  setVrmCamera(yaw: number, pitch: number, distance: number): void;
  /** Evaluate procedural motion at time t (Rust joint-clamped). */
  evaluateMotion?(motionKey: string, time: number): string;
  /** Clamp bone rotation to anatomical limits (Rust). Returns clamped degrees. */
  clampBone?(boneName: string, axis: string, degrees: number): number;
  /** Get the list of VRM bone names (L4, post-L1/L2 skeleton reconstruction). */
  getVrmBoneNames?(): string;
  /** Set a bone's rotation (quaternion xyzw). Returns true if applied. */
  setVrmBoneRotation?(boneName: string, x: number, y: number, z: number, w: number): boolean;
  /** Clear all pose overrides, returning to bind pose. */
  resetVrmPose?(): void;
  /** Skeleton debug JSON (bones + joint count). */
  getVrmSkeletonInfo?(): string;
  /** List VRM draw-batch labels as JSON (`"{meshName}:{materialName}"`). */
  getVrmMeshLabels?(): string;
  /** Toggle visibility of batches whose label contains `substring`. */
  setVrmMeshVisibility?(substring: string, visible: boolean): number;
  /** Compose a VRM by taking `category` parts from `preset` and other parts
   * from `base`. Returns composed GLB bytes. Used by hot preset swap. */
  composeVrmWithPreset?(base: Uint8Array, preset: Uint8Array, category: string): Uint8Array;
}

/** Three.js VRM handle with renderer, scene, camera. */
export interface ThreeVrmHandle {
  vrm: unknown;
  scene: unknown;
  camera: unknown;
  renderer: unknown;
  controls: unknown;
  clock: unknown;
  dispose(): void;
}

/** Dual-engine state shared between KAMI (WebGPU) and Three.js (WebGL). */
export interface DualEngineState {
  kami: KamiWasmExports | null;
  three: ThreeVrmHandle | null;
  vrmUrl: string;
  loading: boolean;
  error: string | null;
  morphNames: string[];
}

/** Runtime engine capabilities detected at init. */
export interface EngineCapabilities {
  webgpu: boolean;
  webgl: boolean;
}

/** Default maximum RAM budget for KAMI Engine SDK consumers: 2 GiB. */
export const KAMI_ENGINE_SDK_DEFAULT_MAX_RAM_BYTES = 2 * 1024 * 1024 * 1024;

/** Memory budget metadata exposed by the SDK. */
export interface EngineMemoryBudget {
  /** Maximum RAM budget in bytes. */
  maxRamBytes: number;
  /** Maximum RAM budget in mebibytes (MiB). */
  maxRamMiB: number;
}

/* ------------------------------------------------------------------ */
/*  Shader Presets — per-pipeline material configuration               */
/* ------------------------------------------------------------------ */

/**
 * Shader preset selection for different rendering styles.
 *
 * Each preset maps to a pair of WGSL vertex/fragment entry points in `pbr.wgsl`.
 * The engine creates separate wgpu::RenderPipeline per preset.
 */
export type ShaderPreset = 'pbr' | 'voxel' | 'unlit';

/** PBR shader preset — Cook-Torrance BRDF for characters/objects. */
export interface ShaderPresetPbr {
  type: 'pbr';
  /** Entry points: vs_main / fs_main. */
  metallic: number;
  roughness: number;
  /** Subsurface scattering model (0=none, 1=Burley, 2=Random Walk). */
  sssModel: number;
}

/**
 * Voxel shader preset — Lambert diffuse for block terrain (Minecraft-style).
 *
 * No specular, no rim light. Hemisphere ambient + distance fog + AO hint.
 * Per-vertex color from palette (12 floats/vertex: pos3+norm3+uv2+color4).
 * Entry points: vs_color / fs_color.
 */
export interface ShaderPresetVoxel {
  type: 'voxel';
  /** Fog start distance in blocks. Default: 48. */
  fogStart: number;
  /** Fog end distance in blocks. Default: 128. */
  fogEnd: number;
  /** Maximum fog opacity (0-1). Default: 0.6. */
  fogMaxOpacity: number;
  /** Fog color [r, g, b]. Default: [0.53, 0.65, 0.75]. */
  fogColor: [number, number, number];
}

/** Unlit shader preset — texture/color only, no lighting. */
export interface ShaderPresetUnlit {
  type: 'unlit';
}

/** Union of all shader presets. */
export type ShaderPresetConfig = ShaderPresetPbr | ShaderPresetVoxel | ShaderPresetUnlit;

/** Default shader presets. */
export const SHADER_PRESETS: Record<ShaderPreset, ShaderPresetConfig> = {
  pbr: { type: 'pbr', metallic: 0.0, roughness: 0.8, sssModel: 0 },
  voxel: { type: 'voxel', fogStart: 48, fogEnd: 128, fogMaxOpacity: 0.6, fogColor: [0.53, 0.65, 0.75] },
  unlit: { type: 'unlit' },
};

/* ------------------------------------------------------------------ */
/*  LOD (Level of Detail) Configuration                                */
/* ------------------------------------------------------------------ */

/**
 * Voxel LOD configuration for distance-based level of detail.
 *
 * LOD 0: Full greedy mesh (16^3, ~800-2000 verts/chunk)
 * LOD 1: 2×2×2 down-sample → 8^3 greedy mesh (~100-400 verts)
 * LOD 2: 4×4×4 down-sample → 4^3 greedy mesh (~20-80 verts)
 * LOD 3: Single dominant-color cube (24 verts)
 */
export interface VoxelLodConfig {
  /** Distance thresholds in blocks [lod1Start, lod2Start, lod3Start]. Default: [32, 64, 128]. */
  thresholds: [number, number, number];
  /** How often to re-evaluate LOD levels (in frames). Lower = more responsive, higher = less CPU. Default: 10. */
  updateInterval: number;
  /** Force all chunks to this LOD level (-1 = auto distance-based). Default: -1. */
  forceLod: number;
}

/** SDF character LOD configuration — marching cubes resolution scaling by distance. */
export interface SdfLodConfig {
  /** Distance thresholds for resolution halving [halfRes, quarterRes]. Default: [32, 64]. */
  thresholds: [number, number];
  /** Base resolution at LOD 0 (clamped to 8-256). Default: 32. */
  baseResolution: number;
}

/** Combined LOD configuration for voxel world + SDF characters. */
export interface LodConfig {
  voxel: VoxelLodConfig;
  sdf: SdfLodConfig;
}

/** Default LOD configuration. */
export const LOD_DEFAULTS: LodConfig = {
  voxel: { thresholds: [32, 64, 128], updateInterval: 10, forceLod: -1 },
  sdf: { thresholds: [32, 64], baseResolution: 32 },
};

/* ------------------------------------------------------------------ */
/*  Voxel World Types                                                  */
/* ------------------------------------------------------------------ */

/** Block type enum matching kami-game/src/voxel.rs. */
export enum BlockType {
  Air = 0, Dirt = 1, Grass = 2, Stone = 3, Water = 4, Sand = 5,
  Wood = 6, Leaf = 7, Ore = 8, Brick = 9, Glass = 10, Metal = 11,
  Snow = 12, Lava = 13, Ice = 14, Gravel = 15,
}

/** Voxel chunk coordinate (16×16×16 block region). */
export interface ChunkCoord {
  cx: number;
  cy: number;
  cz: number;
}

/** Player physics state exported from WASM render loop. */
export interface PlayerPhysicsState {
  position: [number, number, number];
  velY: number;
  onGround: boolean;
}

/** Sky/weather state from day-night cycle. */
export interface SkyState {
  timePhase: 'NIGHT' | 'DAWN' | 'DAY' | 'DUSK';
  worldTime: number;
  skyColor: [number, number, number];
}

/** Debug performance metrics. */
export interface PerfMetrics {
  fps: number;
  frameP50: number;
  frameP95: number;
  frameP99: number;
  chunkCount: number;
  entityCount: number;
  lodDistribution: Record<number, number>;
}

/** Combined WASM game state (window.__kami_isekai_state). */
export interface IsekaiGameState extends PlayerPhysicsState, SkyState {
  biome: string;
  mineCount: number;
  placeCount: number;
  moving: boolean;
}
