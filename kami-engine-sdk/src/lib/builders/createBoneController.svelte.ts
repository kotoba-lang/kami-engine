import type { HumanoidBoneName, RotationAxis, JointLimitsMap, PosePreset } from '../types/bone.js';
import type { KamiWasmExports, ThreeVrmHandle } from '../types/engine.js';
import { JOINT_LIMITS, clampBoneDeg } from '../data/joint-limits.js';

/** Options for creating a bone controller. */
export interface BoneControllerOpts {
  kami: KamiWasmExports | null;
  three: ThreeVrmHandle | null;
  jointLimits?: JointLimitsMap;
  enforceConstraints?: boolean;
}

/** Bone rotation state per bone. */
export type BoneRotationMap = Map<string, { x: number; y: number; z: number }>;

/** All animated bone names for reset. */
const ANIMATED_BONES: string[] = [
  'head', 'neck', 'spine', 'chest', 'hips',
  'leftUpperArm', 'leftLowerArm', 'rightUpperArm', 'rightLowerArm',
  'leftUpperLeg', 'leftLowerLeg', 'rightUpperLeg', 'rightLowerLeg',
];

/**
 * Headless bone rotation controller with anatomical joint clamping.
 *
 * Manages per-bone Euler angle rotations and syncs to the Three.js
 * VRM humanoid. Optionally delegates clamping to Rust WASM when available.
 */
export function createBoneController(opts: BoneControllerOpts) {
  const limits = opts.jointLimits ?? JOINT_LIMITS;
  let enforce = $state(opts.enforceConstraints ?? true);
  let rotations: BoneRotationMap = $state(new Map());

  /** Set bone rotation on a single axis (degrees). Clamps if constraints enabled. */
  function setBone(name: string, axis: RotationAxis, degrees: number) {
    let deg = degrees;

    if (enforce) {
      // Prefer Rust WASM clamping when available
      if (opts.kami?.clampBone) {
        deg = opts.kami.clampBone(name, axis, degrees);
      } else {
        deg = clampBoneDeg(name, axis, degrees, limits);
      }
    }

    // Update state
    const current = rotations.get(name) ?? { x: 0, y: 0, z: 0 };
    current[axis] = deg;
    rotations.set(name, current);

    // Push to Three.js
    const vrm = opts.three?.vrm as any;
    if (vrm?.humanoid) {
      const node = vrm.humanoid.getNormalizedBoneNode(name);
      if (node) {
        const r = deg * Math.PI / 180;
        if (axis === 'x') node.rotation.x = r;
        else if (axis === 'y') node.rotation.y = r;
        else node.rotation.z = r;
      }
    }
  }

  /** Apply a pose preset (reset first, then apply all bone rotations). */
  function applyPose(preset: PosePreset) {
    resetAll();
    for (const [bone, axes] of Object.entries(preset.bones)) {
      for (const [axis, deg] of Object.entries(axes)) {
        setBone(bone, axis as RotationAxis, deg as number);
      }
    }
  }

  /** Reset all bone rotations to 0. */
  function resetAll() {
    const vrm = opts.three?.vrm as any;
    if (vrm?.humanoid) {
      for (const boneName of ANIMATED_BONES) {
        const node = vrm.humanoid.getNormalizedBoneNode(boneName);
        if (node) node.rotation.set(0, 0, 0);
      }
    }
    rotations = new Map();
  }

  /** Update engine references (after late init). */
  function updateEngines(kami: KamiWasmExports | null, three: ThreeVrmHandle | null) {
    opts.kami = kami;
    opts.three = three;
  }

  return {
    get rotations() { return rotations; },
    get enforceConstraints() { return enforce; },
    set enforceConstraints(v: boolean) { enforce = v; },
    setBone,
    applyPose,
    resetAll,
    updateEngines,
  };
}

export type BoneController = ReturnType<typeof createBoneController>;
