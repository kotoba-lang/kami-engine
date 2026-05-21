import type { HairStyle, PartColor, OutfitStyle } from '../types/part.js';

/** 22 hair styles (R2 key pattern: `hair_{key}_{color}.glb`). */
export const HAIR_STYLES: HairStyle[] = [
  { name: 'Short Straight', key: 'shortStraight' },
  { name: 'Short Wavy', key: 'shortWavy' },
  { name: 'Short Curly', key: 'shortCurly' },
  { name: 'Medium Straight', key: 'mediumStraight' },
  { name: 'Medium Wavy', key: 'mediumWavy' },
  { name: 'Medium Layered', key: 'mediumLayered' },
  { name: 'Long Straight', key: 'longStraight' },
  { name: 'Long Wavy', key: 'longWavy' },
  { name: 'Long Curly', key: 'longCurly' },
  { name: 'Ponytail High', key: 'ponytailHigh' },
  { name: 'Ponytail Low', key: 'ponytailLow' },
  { name: 'Bun Top', key: 'bunTop' },
  { name: 'Bun Low', key: 'bunLow' },
  { name: 'Bob', key: 'bob' },
  { name: 'Pixie', key: 'pixie' },
  { name: 'Buzz', key: 'buzz' },
  { name: 'Undercut', key: 'undercut' },
  { name: 'Mohawk', key: 'mohawk' },
  { name: 'Afro Short', key: 'afroShort' },
  { name: 'Afro Large', key: 'afroLarge' },
  { name: 'Braids Twin', key: 'braidsTwin' },
  { name: 'Braids Single', key: 'braidsSingle' },
];

/** 8 hair/part colors. */
export const HAIR_COLORS: PartColor[] = [
  { name: 'Black', key: 'black', hex: '#141210' },
  { name: 'Brown', key: 'brown', hex: '#664020' },
  { name: 'Blonde', key: 'blonde', hex: '#ebd9b3' },
  { name: 'Red', key: 'red', hex: '#b3331f' },
  { name: 'Pink', key: 'pink', hex: '#f2738c' },
  { name: 'Blue', key: 'blue', hex: '#264dcc' },
  { name: 'Silver', key: 'silver', hex: '#c7c7d1' },
  { name: 'Green', key: 'green', hex: '#268c4d' },
];

/** 11 outfit styles (R2 key pattern: `outfit_{key}_{color}.glb`). */
export const OUTFIT_STYLES: OutfitStyle[] = [
  { name: 'Tank Top', key: 'tankTop' },
  { name: 'T-Shirt', key: 'tshirt' },
  { name: 'Blouse', key: 'blouse' },
  { name: 'Hoodie', key: 'hoodie' },
  { name: 'Jacket', key: 'jacket' },
  { name: 'Dress Casual', key: 'dressCasual' },
  { name: 'Dress Formal', key: 'dressFormal' },
  { name: 'Suit Casual', key: 'suitCasual' },
  { name: 'Suit Formal', key: 'suitFormal' },
  { name: 'Uniform School', key: 'uniformSchool' },
  { name: 'Uniform Military', key: 'uniformMilitary' },
];

/** 8 outfit colors. */
export const OUTFIT_COLORS: PartColor[] = [
  { name: 'White', key: 'white', hex: '#f2f2f2' },
  { name: 'Black', key: 'black', hex: '#1a1a1f' },
  { name: 'Navy', key: 'navy', hex: '#1a2659' },
  { name: 'Red', key: 'red', hex: '#bf2626' },
  { name: 'Pink', key: 'pink', hex: '#f29ab3' },
  { name: 'Gray', key: 'gray', hex: '#73737a' },
  { name: 'Beige', key: 'beige', hex: '#d9c7a6' },
  { name: 'Green', key: 'green', hex: '#338050' },
];
