/** VRM part category for mesh classification. */
export type PartCategory = 'Body' | 'Hair' | 'Face' | 'Outfit' | 'Accessory';

/** Hair style option. */
export interface HairStyle { name: string; key: string; }

/** Color option with display hex. */
export interface PartColor { name: string; key: string; hex: string; }

/** Outfit style option. */
export interface OutfitStyle { name: string; key: string; }

/** Registry entry for a VRM mesh part. */
export interface PartEntry {
  name: string;
  category: PartCategory;
  source: string;
  visible: boolean;
  object: unknown;
}

/** Part composer state tracking active selections. */
export interface PartComposerState {
  registry: Record<PartCategory, PartEntry[]>;
  activeHairStyle: string;
  activeHairColor: string;
  activeOutfitStyle: string;
  activeOutfitColor: string;
}
