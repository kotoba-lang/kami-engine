/** VRM 1.0 humanoid bone names (54 total). */
export type HumanoidBoneName =
  | 'hips' | 'spine' | 'chest' | 'upperChest' | 'neck' | 'head'
  | 'leftShoulder' | 'leftUpperArm' | 'leftLowerArm' | 'leftHand'
  | 'rightShoulder' | 'rightUpperArm' | 'rightLowerArm' | 'rightHand'
  | 'leftUpperLeg' | 'leftLowerLeg' | 'leftFoot' | 'leftToes'
  | 'rightUpperLeg' | 'rightLowerLeg' | 'rightFoot' | 'rightToes'
  | 'jaw' | 'leftEye' | 'rightEye'
  | `${'left' | 'right'}${'Thumb' | 'Index' | 'Middle' | 'Ring' | 'Little'}${'Metacarpal' | 'Proximal' | 'Intermediate' | 'Distal'}`;

/** Rotation axis identifier. */
export type RotationAxis = 'x' | 'y' | 'z';

/** Anatomical joint rotation limit (degrees) per axis [min, max]. */
export interface JointLimit {
  x?: [min: number, max: number];
  y?: [min: number, max: number];
  z?: [min: number, max: number];
}

/** Map of bone names to their anatomical joint limits. */
export type JointLimitsMap = Partial<Record<HumanoidBoneName, JointLimit>>;

/** A single bone rotation value. */
export interface BoneRotation {
  bone: HumanoidBoneName;
  axis: RotationAxis;
  degrees: number;
}

/** Named pose preset with per-bone rotation map. */
export interface PosePreset {
  name: string;
  key: string;
  bones: Record<string, Partial<Record<RotationAxis, number>>>;
}
