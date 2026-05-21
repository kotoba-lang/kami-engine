/**
 * `@gftdcojp/kami-engine-sdk/gsplat` — Gaussian splat preview helpers.
 *
 * Browser-side glue between `maps.gftd.ai` XRPC (`getGsplatAsset` /
 * `listGsplatAssets` / `bakeGsplatAsset`) and the
 * `kami-app-maps3d` WASM exports (`set_gsplat_asset` /
 * `remove_gsplat_asset`).
 *
 * Scope: preview / QC only (ADR-2605092800). Runtime delivery on
 * `maps.gftd.ai` stays on baked static meshes.
 */

import type {
  GsplatAssetMeta,
  GetGsplatAssetResponse,
  ListGsplatAssetsResponse,
  GsplatWasmExports,
} from "./types.js";

export type {
  GsplatAssetMeta,
  GetGsplatAssetResponse,
  ListGsplatAssetsResponse,
  GsplatWasmExports,
} from "./types.js";

export interface FetchedGsplatAsset {
  meta: GsplatAssetMeta;
  bytes: Uint8Array;
}

/**
 * Resolve a single splat asset by `tileH3` against the maps.gftd.ai
 * XRPC endpoint, then fetch the signed B2 binary. Returns the raw
 * bytes and asset metadata so the caller can choose to also store
 * them in an LRU on the host side.
 *
 * Throws if the XRPC call fails, the signed URL fetch fails, or the
 * payload size disagrees with `meta.byteSize` (truncation guard).
 */
export async function loadGsplatAsset(
  endpoint: string,
  tileH3: string,
  init: { fetch?: typeof fetch; signal?: AbortSignal } = {},
): Promise<FetchedGsplatAsset> {
  const f = init.fetch ?? fetch;
  const xrpcUrl = new URL("/xrpc/ai.gftd.apps.maps.getGsplatAsset", endpoint);
  xrpcUrl.searchParams.set("tileH3", tileH3);
  const xrpcResp = await f(xrpcUrl.toString(), { signal: init.signal });
  if (!xrpcResp.ok) {
    throw new Error(`getGsplatAsset XRPC ${xrpcResp.status}: ${await xrpcResp.text()}`);
  }
  const body = (await xrpcResp.json()) as GetGsplatAssetResponse;
  const blobResp = await f(body.signedUrl, { signal: init.signal });
  if (!blobResp.ok) {
    throw new Error(`gsplat blob ${blobResp.status} for ${body.meta.b2Key}`);
  }
  const buf = await blobResp.arrayBuffer();
  if (buf.byteLength !== body.meta.byteSize) {
    throw new Error(
      `gsplat payload size mismatch for ${tileH3}: got ${buf.byteLength}, expected ${body.meta.byteSize}`,
    );
  }
  return { meta: body.meta, bytes: new Uint8Array(buf) };
}

/**
 * List splat assets covering an H3 cell window. Pagination uses the
 * standard `{offset,limit}` envelope from the XRPC contract.
 */
export async function listGsplatAssets(
  endpoint: string,
  query: { tileH3?: string; sourceDid?: string; offset?: number; limit?: number } = {},
  init: { fetch?: typeof fetch; signal?: AbortSignal } = {},
): Promise<ListGsplatAssetsResponse> {
  const f = init.fetch ?? fetch;
  const url = new URL("/xrpc/ai.gftd.apps.maps.listGsplatAssets", endpoint);
  if (query.tileH3) url.searchParams.set("tileH3", query.tileH3);
  if (query.sourceDid) url.searchParams.set("sourceDid", query.sourceDid);
  url.searchParams.set("offset", String(query.offset ?? 0));
  url.searchParams.set("limit", String(query.limit ?? 50));
  const resp = await f(url.toString(), { signal: init.signal });
  if (!resp.ok) {
    throw new Error(`listGsplatAssets XRPC ${resp.status}: ${await resp.text()}`);
  }
  return (await resp.json()) as ListGsplatAssetsResponse;
}

/**
 * Push a fetched asset into a `kami-app-maps3d` WASM module. Returns
 * the splat count actually loaded (post-cap; the WASM-side CPU sort
 * preview cap is `MAX_SPLATS_PER_CLOUD = 100_000`).
 */
export function pushToWasm(
  wasm: GsplatWasmExports,
  asset: FetchedGsplatAsset,
): number {
  return wasm.set_gsplat_asset(asset.meta.tileH3, asset.bytes, asset.meta.format);
}

/** Mirror of `set_gsplat_asset` removal — drops the GPU buffers for a tile. */
export function removeFromWasm(wasm: GsplatWasmExports, tileH3: string): void {
  wasm.remove_gsplat_asset(tileH3);
}

/**
 * Trigger an async splat→mesh bake on the server side. The XRPC
 * returns immediately with a job ID; the actual work runs as a
 * Vultr k8s pod (ADR-2604251830 L8). Resolves the JSON response.
 */
export async function bakeGsplatAsset(
  endpoint: string,
  payload: { tileH3: string; vertexId?: string; priority?: "low" | "normal" | "high" },
  init: { fetch?: typeof fetch; signal?: AbortSignal } = {},
): Promise<{ bakeJobId: string; queuedAt: string }> {
  const f = init.fetch ?? fetch;
  const url = new URL("/xrpc/ai.gftd.apps.maps.bakeGsplatAsset", endpoint);
  const resp = await f(url.toString(), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(payload),
    signal: init.signal,
  });
  if (!resp.ok) {
    throw new Error(`bakeGsplatAsset XRPC ${resp.status}: ${await resp.text()}`);
  }
  return (await resp.json()) as { bakeJobId: string; queuedAt: string };
}
