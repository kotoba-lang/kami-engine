#!/usr/bin/env bash
# Run the pure-Rust decoder tests headlessly on WebAssembly (no browser).
#
# Compiles to wasm32-wasip1 and executes the test binary under wasmtime (wired
# up via .cargo/config.toml's runner). wgpu can't run under WASI, so the
# wgpu-backend is excluded; this covers the spz / meshopt / KHR_mesh_quantization
# / KHR_texture_basisu decoders.
#
# Why the RUSTC pinning: on some setups `cargo`/`rustc` on PATH are Homebrew's
# Rust, which ships no wasm std ("can't find crate for core/std"). We drive the
# rustup toolchain explicitly so the wasm target's std is found.
#
# Usage:
#   scripts/test-wasm.sh                      # default: kami-render decoders
#   scripts/test-wasm.sh <extra cargo args>   # e.g. meshopt   (test filter)
set -euo pipefail

TOOLCHAIN="${WASM_TOOLCHAIN:-stable}"
TARGET="${WASM_TARGET:-wasm32-wasip1}"

command -v wasmtime >/dev/null || { echo "error: wasmtime not found (brew install wasmtime)"; exit 1; }
command -v rustup   >/dev/null || { echo "error: rustup not found"; exit 1; }

# Resolve the rustup toolchain's own rustc/cargo (bypassing any Homebrew rustc
# that may shadow it on PATH) and make sure the wasm std is installed.
RUSTC_BIN="$(rustup which --toolchain "$TOOLCHAIN" rustc)"
TC_BIN_DIR="$(dirname "$RUSTC_BIN")"
rustup target add --toolchain "$TOOLCHAIN" "$TARGET" >/dev/null 2>&1 || true

cd "$(dirname "$0")/.."

# --lib: run the crate's unit tests only. Doc-tests are skipped — rustdoc runs
# them in a separate process that's awkward to point at the wasm sysroot, and
# they add no coverage for the binary decoders.
echo "==> cargo test ($TARGET, toolchain=$TOOLCHAIN) via wasmtime"
RUSTC="$TC_BIN_DIR/rustc" RUSTDOC="$TC_BIN_DIR/rustdoc" "$TC_BIN_DIR/cargo" test \
  -p kami-render --lib \
  --no-default-features --features gltf-loader \
  --target "$TARGET" \
  "$@"
