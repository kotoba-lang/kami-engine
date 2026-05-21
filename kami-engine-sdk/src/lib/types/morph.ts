/** VRM morph target category (face mesh groups). */
export type MorphCategory = 'ALL' | 'BRW' | 'EYE' | 'MTH' | 'HA';

/** Morph target definition with index, name, and display metadata. */
export interface MorphTargetDef {
  index: number;
  name: string;
  category: MorphCategory;
  displayName: string;
}

/** Named expression preset mapping morph indices to weights. */
export interface ExpressionPreset {
  name: string;
  key: string;
  morphs: Record<number, number>;
}
