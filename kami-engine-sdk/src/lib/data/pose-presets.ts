import type { PosePreset } from '../types/bone.js';

/** 20 pose presets for VRM humanoid characters. */
export const POSE_PRESETS: PosePreset[] = [
  { name: 'T-Pose', key: 'tPose', bones: {} },
  { name: 'A-Pose', key: 'aPose', bones: { leftUpperArm: { z: 30 }, rightUpperArm: { z: -30 } } },
  { name: 'Natural', key: 'natural', bones: { leftUpperArm: { z: 65 }, rightUpperArm: { z: -65 }, leftLowerArm: { y: 10 }, rightLowerArm: { y: -10 }, head: { x: 5, z: -3 } } },
  { name: 'Relax', key: 'relax', bones: { leftUpperArm: { z: 70 }, rightUpperArm: { z: -70 }, leftLowerArm: { y: 20 }, rightLowerArm: { y: -20 }, head: { x: 8 }, spine: { x: 3 } } },
  { name: 'Wave', key: 'wave', bones: { leftUpperArm: { z: 65 }, rightUpperArm: { z: -70 }, rightLowerArm: { y: -120 }, head: { x: 5, z: 5 } } },
  { name: 'Both Up', key: 'bothUp', bones: { leftUpperArm: { z: -20 }, rightUpperArm: { z: 20 }, head: { x: -5 } } },
  { name: 'Hip', key: 'hip', bones: { leftUpperArm: { z: 50 }, leftLowerArm: { y: 80 }, rightUpperArm: { z: -50 }, rightLowerArm: { y: -80 }, spine: { x: 2 } } },
  { name: 'Cross', key: 'cross', bones: { leftUpperArm: { z: 55 }, leftLowerArm: { y: 70 }, rightUpperArm: { z: -55 }, rightLowerArm: { y: -70 }, spine: { x: 5 } } },
  { name: 'Think', key: 'think', bones: { rightUpperArm: { z: -50 }, rightLowerArm: { y: -90 }, head: { x: 10, z: -5, y: 10 }, spine: { x: 3 } } },
  { name: 'Shy', key: 'shy', bones: { leftUpperArm: { z: 65 }, rightUpperArm: { z: -65 }, head: { x: 12, z: -5 }, spine: { x: 5 } } },
  { name: 'Tilt', key: 'tilt', bones: { head: { x: 10, z: -8 } } },
  { name: 'Tilt R', key: 'tiltR', bones: { head: { x: 10, z: 8 } } },
  { name: 'Look Up', key: 'lookUp', bones: { head: { x: -20 }, spine: { x: -3 } } },
  { name: 'Look Down', key: 'lookDown', bones: { head: { x: 25 }, spine: { x: 5 } } },
  { name: 'Turn L', key: 'turnL', bones: { head: { y: 40 }, spine: { y: 10 } } },
  { name: 'Turn R', key: 'turnR', bones: { head: { y: -40 }, spine: { y: -10 } } },
  { name: 'Victory', key: 'victory', bones: { leftUpperArm: { z: -10 }, leftLowerArm: { y: 120 }, rightUpperArm: { z: 10 }, rightLowerArm: { y: -120 }, head: { x: -5 } } },
  { name: 'Pray', key: 'pray', bones: { leftUpperArm: { z: 60 }, leftLowerArm: { y: 90 }, rightUpperArm: { z: -60 }, rightLowerArm: { y: -90 }, head: { x: 10 } } },
  { name: 'Flex', key: 'flex', bones: { leftUpperArm: { z: 10 }, leftLowerArm: { y: 120 }, rightUpperArm: { z: -10 }, rightLowerArm: { y: -120 } } },
  { name: 'Bow', key: 'bow', bones: { spine: { x: 25 }, head: { x: 15 } } },
];
