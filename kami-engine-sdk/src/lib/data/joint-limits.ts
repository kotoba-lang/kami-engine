import type { JointLimitsMap } from '../types/bone.js';

/**
 * Default anatomical joint rotation limits (degrees).
 *
 * Derived from orthopedic range-of-motion references. Prevents humanly
 * impossible poses such as hyperextended elbows or backwards shoulders.
 * Matches the Rust `kami-skeleton::defaultHumanoidConstraints()`.
 */
export const JOINT_LIMITS: JointLimitsMap = {
  head:          { x: [-60, 60],   y: [-80, 80],   z: [-40, 40] },
  neck:          { x: [-30, 30],   y: [-45, 45],   z: [-30, 30] },
  spine:         { x: [-30, 30],   y: [-30, 30],   z: [-20, 20] },
  chest:         { x: [-15, 15],   y: [-15, 15],   z: [-10, 10] },
  hips:          { x: [-30, 30],   y: [-30, 30],   z: [-15, 15] },
  leftUpperArm:  { x: [-60, 90],   y: [-45, 90],   z: [-30, 180] },
  rightUpperArm: { x: [-60, 90],   y: [-90, 45],   z: [-180, 30] },
  leftLowerArm:  { x: [-5, 5],     y: [0, 145],    z: [-5, 5] },
  rightLowerArm: { x: [-5, 5],     y: [-145, 0],   z: [-5, 5] },
  leftUpperLeg:  { x: [-30, 120],  y: [-45, 30],   z: [-20, 45] },
  rightUpperLeg: { x: [-30, 120],  y: [-30, 45],   z: [-45, 20] },
  leftLowerLeg:  { x: [-140, 0],   y: [-5, 5],     z: [-5, 5] },
  rightLowerLeg: { x: [-140, 0],   y: [-5, 5],     z: [-5, 5] },
};

/**
 * Clamp a bone rotation (degrees) to anatomical limits.
 *
 * Falls back to unclamped if no limit is defined for the bone/axis.
 * This is the TS fallback — prefer the Rust WASM `clampBone()` when available.
 */
export function clampBoneDeg(
  boneName: string,
  axis: 'x' | 'y' | 'z',
  degrees: number,
  limits: JointLimitsMap = JOINT_LIMITS,
): number {
  const lim = limits[boneName as keyof JointLimitsMap];
  if (!lim) return degrees;
  const range = lim[axis];
  if (!range) return degrees;
  const [mn, mx] = range;
  return Math.max(mn, Math.min(mx, degrees));
}

/**
 * Clamp a bone rotation (radians) to anatomical limits.
 *
 * Converts limit bounds to radians before clamping.
 */
export function clampBoneRad(
  boneName: string,
  axis: 'x' | 'y' | 'z',
  radians: number,
  limits: JointLimitsMap = JOINT_LIMITS,
): number {
  const lim = limits[boneName as keyof JointLimitsMap];
  if (!lim) return radians;
  const range = lim[axis];
  if (!range) return radians;
  const D = Math.PI / 180;
  return Math.max(range[0] * D, Math.min(range[1] * D, radians));
}
