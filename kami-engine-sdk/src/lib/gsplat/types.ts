/** @gftdcojp/kami-engine-sdk/gsplat — Type definitions. */

/** Server-side splat asset metadata (mirror of `vertex_maps_gsplat_asset`). */
export interface GsplatAssetMeta {
  /** AT URI: `at://{authority}/ai.gftd.apps.maps.gsplatAsset/{rkey}` */
  vertexId: string;
  /** Path-based DID that registered this asset. */
  sourceDid: string;
  /** H3 cell (any resolution) the asset covers. */
  tileH3: string;
  /** Backblaze B2 object key (immutable). */
  b2Key: string;
  /** Asset payload size in bytes. */
  byteSize: number;
  /** Decoded splat count. */
  splatCount: number;
  /** Spherical harmonics degree present in the file (0–3). */
  shDegree: number;
  /** Source format hint (`"ply"` or `"splat"`). */
  format: "ply" | "splat";
  /** RFC 3339 timestamp when the asset was generated upstream. */
  generatedAt: string;
  /** Optional bake job ID if this asset was queued for mesh extraction. */
  bakeJobId?: string;
}

/** Wire format of the `getGsplatAsset` query response. */
export interface GetGsplatAssetResponse {
  meta: GsplatAssetMeta;
  /** Pre-signed URL for fetching the binary payload. Short-lived. */
  signedUrl: string;
  /** Seconds until `signedUrl` expires (advisory). */
  expiresInSec: number;
}

/** Wire format of the `listGsplatAssets` query response. */
export interface ListGsplatAssetsResponse {
  assets: GsplatAssetMeta[];
  total: number;
  offset: number;
  limit: number;
}

/** Subset of the wasm-bindgen exports used by `pushToWasm`. */
export interface GsplatWasmExports {
  set_gsplat_asset: (tileH3: string, bytes: Uint8Array, format: string) => number;
  remove_gsplat_asset: (tileH3: string) => void;
}
