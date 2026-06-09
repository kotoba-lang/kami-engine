# ADR-0032: Pure-Rust glTF / Gaussian-splat asset extension decoders

- Status: Accepted
- Date: 2026-06-09
- Scope: `kami-render` (`gltf-loader` feature), `kami-render::splat_loader`
- Related: ADR-0031 (kami-vrm three-free topology — consumer of `load_glb`)

## Context

`kami-render` could load only baseline glTF 2.0 (float attributes, PNG/JPEG
textures) and the antimatter15 `.splat` / 3DGS `.ply` Gaussian-splat formats.
Modern asset pipelines (gltfpack, gltf-transform, toktx, Niantic SPZ) ship four
encodings we could not read:

| Encoding | What it is | Prior state |
|---|---|---|
| `KHR_mesh_quantization` | BYTE/SHORT vertex attributes + node-TRS dequant | misread as f32 → garbage geometry |
| `EXT_meshopt_compression` | meshoptimizer vertex/index bitstream + filters | unsupported; `gltf` crate can't decode |
| `KHR_texture_basisu` | KTX2 container, ETC1S **or** UASTC GPU texture | unsupported; `image` crate can't decode KTX2 |
| Niantic **SPZ** | gzip-compressed quantized Gaussian splats (`.spz`) | unsupported |

The engine is **WASM-first** (`Backends::BROWSER_WEBGPU | GL`, Rust→WASM). Any
dependency that does not build for `wasm32` would break the primary target.

## Decision

Implement all four decoders **in pure Rust, with no C/C++ dependencies**, so the
loader stays wasm-safe. Verify the two intricate binary codecs against the
canonical upstream encoders rather than trusting a hand-port.

1. **`KHR_mesh_quantization`** — dequantize POSITION/NORMAL manually
   (`gltf_loader::read_quantized_attr`): integer component types → f32, with
   normalized-integer scaling for normals; positions stay in quantized object
   space and are dequantized by the node TRS already applied downstream.
   Texcoords already normalize via the `gltf` crate's `into_f32()`.

2. **`EXT_meshopt_compression`** — new `kami-render::meshopt` module: a faithful
   scalar port of meshoptimizer's vertex codec, index codec, index-sequence
   codec, and the OCTAHEDRAL / QUATERNION / EXPONENTIAL filters. A GLB
   pre-pass (`decode_meshopt_glb`) decodes the compressed buffer views and
   re-emits a clean, extension-free GLB for the `gltf` crate to import.

3. **`KHR_texture_basisu`** — new `kami-render::basisu` module: KTX2 container
   parser + **UASTC LDR** 4×4 block decoder (port of Basis Universal
   `unpack_uastc`), tables auto-generated from the upstream transcoder
   (`uastc_tables.rs`). Decodes straight to RGBA8.

4. **SPZ** — `splat_loader::load_spz`: gunzip (`flate2`) + the legacy v1–v3
   single-stream layout (24-bit fixed-point positions, u8 scale/rot/alpha/color,
   SH coefficients) → `SplatCloud`.

### Loader-level changes

- Switched from `gltf::import_slice` to `Gltf::from_slice_without_validation` +
  manual `import_buffers`/image import. The crate's validator hard-rejects any
  `extensionsRequired` entry it doesn't implement (incl. `KHR_mesh_quantization`,
  `KHR_texture_basisu`) and rejects accessors missing min/max — both fatal for
  files we *can* handle.
- Replaced `gltf::import_images` with a KTX2-aware image loader (KTX2→`basisu`,
  PNG/JPEG→`image`) plus a basisu-aware texture→image map (the crate does not
  parse `KHR_texture_basisu.source`).

### Dependencies

Added to `[workspace.dependencies]`, all wasm-safe:
`flate2` (pure-Rust `rust_backend`/miniz_oxide), `image` (png+jpeg only,
default-features off), `gltf` (already present, promoted to workspace).

## Scope limits (deliberate)

- **basisu: UASTC only.** ETC1S (BasisLZ-supercompressed) KTX2 is detected and
  reported unsupported; the loader substitutes a placeholder and logs a warning.
  A full ETC1S transcoder is a separate, larger effort.
- **Supercompression: `none` + `ZLIB`.** Zstandard KTX2 levels are unsupported
  (no pure-Rust zstd in the dependency set). SPZ targets the gzip v1–v3 layout,
  not the v4 multi-stream Zstd container.
- **GPU paths are unchanged.** Decoders are pure compute; rendering still needs
  wgpu/a browser.

## Consequences

- Quantized, meshopt-compressed, and UASTC-textured glTF/GLB assets now load;
  `.spz` splats load alongside `.splat`/`.ply`. Public `GltfScene` shape is
  unchanged — downstream (`kami-web`, kami-vrm path) is unaffected.
- New `SUPPORTED_EXTENSIONS` registry; unknown required extensions log a
  best-effort warning instead of failing.
- `flate2` is now an unconditional `kami-render` dep (SPZ is always compiled);
  `image` is gated behind `gltf-loader`.

## Verification

Codec correctness is validated against the **real upstream encoders**, not just
self-consistency:

- **meshopt**: vectors produced by the meshoptimizer C++ encoder (vertex / index
  / index-sequence / 3 filters / end-to-end GLB) are decoded bit-exactly.
- **UASTC**: 120 `encode→unpack_uastc` vectors from the Basis Universal encoder
  (solid, 1/2/3-subset, dual-plane, 2-component, BISE bits/trits/quints) decode
  bit-exactly; KTX2 container round-trip + ETC1S-rejection tested.
- **SPZ / mesh_quantization**: round-trip and hand-built-asset integration tests.

46 `kami-render` unit tests pass natively and **headless on `wasm32-wasip1`**
(`scripts/test-wasm.sh`, wasmtime runner via `.cargo/config.toml`), confirming
the decoders run unchanged in a wasm runtime.
