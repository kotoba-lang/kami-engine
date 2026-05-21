import { describe, expect, it } from "vitest";
import {
  loadGsplatAsset,
  listGsplatAssets,
  pushToWasm,
  removeFromWasm,
  bakeGsplatAsset,
  type FetchedGsplatAsset,
  type GsplatAssetMeta,
  type GsplatWasmExports,
} from "./index.js";

const META: GsplatAssetMeta = {
  vertexId: "at://did:web:maps.gftd.ai/ai.gftd.apps.maps.gsplatAsset/abc123",
  sourceDid: "did:web:maps.gftd.ai:street_view",
  tileH3: "8c2a1072b59ffff",
  b2Key: "gsplat/8c2a1072b59ffff.ply",
  byteSize: 4,
  splatCount: 1234,
  shDegree: 0,
  format: "ply",
  generatedAt: "2026-05-09T01:00:00Z",
};

function mockJsonFetch(routes: Record<string, unknown>): typeof fetch {
  return (async (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : (input as URL).toString();
    for (const [pattern, body] of Object.entries(routes)) {
      if (url.includes(pattern)) {
        if (body instanceof Uint8Array) {
          // Copy into a fresh ArrayBuffer so the Response body is
          // unambiguously an ArrayBuffer (not SharedArrayBuffer-typed).
          const ab = new ArrayBuffer(body.byteLength);
          new Uint8Array(ab).set(body);
          return new Response(ab, { status: 200 });
        }
        return new Response(JSON.stringify(body), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
    }
    return new Response("not found", { status: 404 });
  }) as unknown as typeof fetch;
}

describe("kami-engine-sdk gsplat", () => {
  it("loadGsplatAsset returns meta + bytes when sizes agree", async () => {
    const bytes = new Uint8Array([1, 2, 3, 4]);
    const fakeFetch = mockJsonFetch({
      "ai.gftd.apps.maps.getGsplatAsset": {
        meta: META,
        signedUrl: "https://b2.example/blob",
        expiresInSec: 60,
      },
      "b2.example/blob": bytes,
    });
    const asset = await loadGsplatAsset("https://maps.gftd.ai", META.tileH3, {
      fetch: fakeFetch,
    });
    expect(asset.meta.tileH3).toBe(META.tileH3);
    expect(asset.bytes.byteLength).toBe(4);
  });

  it("loadGsplatAsset throws on size mismatch (truncation guard)", async () => {
    const bytes = new Uint8Array([1, 2, 3]); // wrong size (3 != meta.byteSize 4)
    const fakeFetch = mockJsonFetch({
      "ai.gftd.apps.maps.getGsplatAsset": {
        meta: META,
        signedUrl: "https://b2.example/blob",
        expiresInSec: 60,
      },
      "b2.example/blob": bytes,
    });
    await expect(
      loadGsplatAsset("https://maps.gftd.ai", META.tileH3, { fetch: fakeFetch }),
    ).rejects.toThrow(/size mismatch/);
  });

  it("listGsplatAssets uses standard offset/limit envelope", async () => {
    const fakeFetch = mockJsonFetch({
      "listGsplatAssets": {
        assets: [META],
        total: 1,
        offset: 0,
        limit: 50,
      },
    });
    const result = await listGsplatAssets(
      "https://maps.gftd.ai",
      { tileH3: META.tileH3 },
      { fetch: fakeFetch },
    );
    expect(result.total).toBe(1);
    expect(result.assets[0].vertexId).toBe(META.vertexId);
  });

  it("pushToWasm forwards bytes + format to the wasm export", () => {
    const calls: { tile: string; len: number; fmt: string }[] = [];
    const wasm: GsplatWasmExports = {
      set_gsplat_asset: (tileH3, bytes, format) => {
        calls.push({ tile: tileH3, len: bytes.byteLength, fmt: format });
        return 7;
      },
      remove_gsplat_asset: () => {},
    };
    const asset: FetchedGsplatAsset = { meta: META, bytes: new Uint8Array([0, 1, 2]) };
    const n = pushToWasm(wasm, asset);
    expect(n).toBe(7);
    expect(calls).toEqual([{ tile: META.tileH3, len: 3, fmt: "ply" }]);
  });

  it("removeFromWasm calls the eviction export", () => {
    const removed: string[] = [];
    const wasm: GsplatWasmExports = {
      set_gsplat_asset: () => 0,
      remove_gsplat_asset: (tileH3) => {
        removed.push(tileH3);
      },
    };
    removeFromWasm(wasm, META.tileH3);
    expect(removed).toEqual([META.tileH3]);
  });

  it("bakeGsplatAsset POSTs JSON and resolves job id", async () => {
    let captured: { method?: string; body?: string } = {};
    const fakeFetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : (input as URL).toString();
      if (url.includes("bakeGsplatAsset")) {
        captured = { method: init?.method, body: init?.body as string };
        return new Response(
          JSON.stringify({ bakeJobId: "job-1", queuedAt: "2026-05-09T01:00:00Z" }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      return new Response("not found", { status: 404 });
    }) as unknown as typeof fetch;

    const r = await bakeGsplatAsset(
      "https://maps.gftd.ai",
      { tileH3: META.tileH3 },
      { fetch: fakeFetch },
    );
    expect(r.bakeJobId).toBe("job-1");
    expect(captured.method).toBe("POST");
    expect(captured.body).toContain(META.tileH3);
  });
});
