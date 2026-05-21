/** Orbit camera state (spherical coordinates). */
export interface OrbitState {
  yaw: number;
  pitch: number;
  distance: number;
  targetY: number;
}

/** Named camera position preset. */
export interface CameraPreset {
  name: string;
  key: string;
  yaw: number;
  pitch: number;
  distance: number;
  targetY: number;
}
