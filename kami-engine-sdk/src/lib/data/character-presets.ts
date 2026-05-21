/** Character preset definition (colors + expression + pose). */
export interface CharacterPreset {
  id: string;
  name: string;
  img: string;
  colors: {
    skin: string;
    hair: string;
    eye: string;
    top: string;
    bot: string;
  };
  expr: Record<string, number>;
  pose: Record<string, Record<string, number>>;
}

/**
 * Default character presets.
 *
 * Sofia is the default character applied on VRM load.
 * Color values are applied to VRM materials by name matching
 * (SKIN/BODY/FACE → skin, HAIR → hair, IRIS → eye, TOPS → top, BOTTOM/SHOE → bot).
 */
export const CHARACTER_PRESETS: CharacterPreset[] = [
  {
    id: 'char1',
    name: 'Sofia',
    img: '/avatar/refs/char1_blonde.png',
    colors: { skin: '#f7dcc8', hair: '#f0dca0', eye: '#5c9fd4', top: '#ffffff', bot: '#2a2a38' },
    expr: { happy: 0.25 },
    pose: {
      leftUpperArm: { z: 65 },
      rightUpperArm: { z: -65 },
      leftLowerArm: { y: 10 },
      rightLowerArm: { y: -10 },
      head: { x: 5, z: -3 },
    },
  },
  {
    id: 'char2',
    name: 'Kuro',
    img: '/avatar/refs/char2_dark.png',
    colors: { skin: '#ede5df', hair: '#1a1a1a', eye: '#cc2233', top: '#151515', bot: '#151515' },
    expr: { relaxed: 0.3 },
    pose: {
      leftUpperArm: { z: 50 },
      rightUpperArm: { z: -50 },
      leftLowerArm: { y: 60 },
      rightLowerArm: { y: -60 },
      head: { x: 8, z: 5 },
      spine: { x: 5 },
    },
  },
];

/** VRM material name → color key mapping. */
const MAT_COLOR_MAP: [RegExp, keyof CharacterPreset['colors']][] = [
  [/SKIN|BODY|FACE_00|FACEMOUTH/i, 'skin'],
  [/HAIR/i, 'hair'],
  [/IRIS/i, 'eye'],
  [/TOPS/i, 'top'],
  [/BOTTOM|SHOE/i, 'bot'],
];

/**
 * Apply character preset colors to a Three.js VRM scene.
 *
 * Traverses all meshes, matches material names to color keys,
 * and sets base color + MToon shade color (70% darker).
 */
export function applyCharacterColors(vrmScene: any, colors: CharacterPreset['colors']): void {
  if (!vrmScene) return;
  vrmScene.traverse((obj: any) => {
    if (!obj.isMesh) return;
    const mats = Array.isArray(obj.material) ? obj.material : [obj.material];
    for (const mat of mats) {
      const n = (mat.name || '').toUpperCase();
      for (const [pattern, key] of MAT_COLOR_MAP) {
        if (pattern.test(n)) {
          // Remove texture so pure color is used (texture × color = tinted)
          if (mat.map) { mat.map = null; }
          mat.color?.set(colors[key]);
          // MToon shade color (70% of base for 2-tone shading)
          if (mat.uniforms?.shadeColorFactor) {
            const c = mat.color.clone().multiplyScalar(0.7);
            mat.uniforms.shadeColorFactor.value.set(c.r, c.g, c.b);
          }
          mat.needsUpdate = true;
          break;
        }
      }
    }
  });
}
