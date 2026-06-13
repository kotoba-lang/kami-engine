// Browser smoke test for the clj↔Rust GPU bridge.
//
// Proves the full Rust GPU path on REAL WebGPU without a CLJS runtime:
//   KamiCljHost.create(canvas)  → bootstrap (kami-render RenderContext)
//   register_mesh / register_material
//   submit_frame(meta, buffer)  → decode KAMI columnar → instanced draw → present
// `frame.json` + `frame.bin` were precomputed by clj (`dev/gen_demo.clj`):
// a camera at [0,1.5,7] and two cubes at x = ±2 on a Nintendo-cream clear.

import init, { KamiCljHost } from "../pkg/kami_clj_host.js";

const status = document.getElementById("status");
const setStatus = (msg, cls) => { status.textContent = msg; status.className = cls || ""; };

// One unit cube, interleaved pos3 + normal3 + uv2 (stride 32B), 24 verts / 36 idx.
function cube() {
  const f = [ // [nx,ny,nz, then 4 corners as [x,y,z,u,v]]
    [ 0, 0, 1, -.5,-.5, .5, 0,0,  .5,-.5, .5, 1,0,  .5, .5, .5, 1,1, -.5, .5, .5, 0,1],
    [ 0, 0,-1,  .5,-.5,-.5, 0,0, -.5,-.5,-.5, 1,0, -.5, .5,-.5, 1,1,  .5, .5,-.5, 0,1],
    [ 1, 0, 0,  .5,-.5, .5, 0,0,  .5,-.5,-.5, 1,0,  .5, .5,-.5, 1,1,  .5, .5, .5, 0,1],
    [-1, 0, 0, -.5,-.5,-.5, 0,0, -.5,-.5, .5, 1,0, -.5, .5, .5, 1,1, -.5, .5,-.5, 0,1],
    [ 0, 1, 0, -.5, .5, .5, 0,0,  .5, .5, .5, 1,0,  .5, .5,-.5, 1,1, -.5, .5,-.5, 0,1],
    [ 0,-1, 0, -.5,-.5,-.5, 0,0,  .5,-.5,-.5, 1,0,  .5,-.5, .5, 1,1, -.5,-.5, .5, 0,1],
  ];
  const verts = [];
  const idx = [];
  f.forEach((face, fi) => {
    const [nx, ny, nz] = face;
    for (let c = 0; c < 4; c++) {
      const o = 3 + c * 5;
      verts.push(face[o], face[o + 1], face[o + 2], nx, ny, nz, face[o + 3], face[o + 4]);
    }
    const b = fi * 4;
    idx.push(b, b + 1, b + 2, b, b + 2, b + 3);
  });
  return { verts: new Float32Array(verts), idx: new Uint32Array(idx) };
}

async function main() {
  try {
    if (!navigator.gpu) { setStatus("WebGPU not available in this browser", "err"); return; }
    setStatus("loading wasm…");
    await init();                                  // instantiate kami-clj-host wasm

    const canvas = document.getElementById("c");
    setStatus("bootstrapping wgpu…");
    const host = await KamiCljHost.create(canvas);  // kami-render RenderContext

    const { verts, idx } = cube();
    host.register_mesh("mesh/cube", verts, idx);
    host.register_material("mat/leaf", new Float32Array([0.36, 0.65, 0.30, 1.0])); // leafy green

    setStatus("fetching precomputed clj frame…");
    const meta = await (await fetch("./frame.json")).text();
    const buf = new Uint8Array(await (await fetch("./frame.bin")).arrayBuffer());

    host.submit_frame(meta, buf);                   // decode + instanced draw + present
    window.__kamiRendered = true;                   // signal for headless screenshot
    setStatus("✅ rendered 2 cubes via clj render-IR → Rust wgpu → WebGPU", "ok");
  } catch (e) {
    console.error(e);
    setStatus("❌ " + (e?.message || e), "err");
    window.__kamiError = String(e?.message || e);
  }
}

main();
