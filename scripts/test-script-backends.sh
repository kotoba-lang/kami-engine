#!/usr/bin/env bash
# Run the kami-script-runtime test suite under BOTH WASM backends and require
# both to pass (ADR-0037 Phase 1). This is the no-JIT guarantee in CI form:
#
#   - backend-wasmtime (default) — JIT host: macOS / Linux / Windows / Android
#   - backend-wasmi   (no JIT)   — interpreter host: iOS / PS5 / Switch
#
# The guest is the same all-i64 deterministic kami-clj wasm, so the survivors
# core loop and the seeded-RNG determinism test must pass identically on both.
# If they diverge, a console/iOS build would behave differently from desktop —
# this script is the gate that catches that.
#
# Native host build (the org-level .cargo/config.toml forces wasm32 by default,
# so we pin the host triple explicitly — same reason as the `test-native` alias).
#
# Usage:
#   scripts/test-script-backends.sh                 # both backends, full suite
#   scripts/test-script-backends.sh nearest         # filter passed to cargo test
set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${HOST_TARGET:-aarch64-apple-darwin}"
PKG="kami-script-runtime"
EXTRA=("$@")

run() {
  local label="$1"; shift
  echo "=================================================================="
  echo "  $label"
  echo "=================================================================="
  cargo test --target "$TARGET" -p "$PKG" "$@" "${EXTRA[@]}"
}

run "backend: wasmtime (JIT — desktop/android)"
run "backend: wasmi (no JIT — ios/ps5/switch)" --no-default-features --features backend-wasmi

# Bonus: prove the Model-B dance composition actually RUNS under wasm32 — compiled
# to wasm32-wasip1 and executed by the wasmtime runner, with the compiled-CLJ dance
# logic.clj running wasm-in-wasm. This is the browser/Model-B path (wasm32), not
# just native. Gated on wasmtime; skips gracefully if it is not installed.
if command -v wasmtime >/dev/null 2>&1; then
  echo "=================================================================="
  echo "  wasm32 (wasip1): the dance Model-B composition RUNS in wasm"
  echo "=================================================================="
  rustup target add wasm32-wasip1 >/dev/null 2>&1 || true
  cargo test --target wasm32-wasip1 -p "$PKG" --test dance_model_b \
    --no-default-features --features backend-wasmi "${EXTRA[@]}"
else
  echo "… wasmtime not found — skipping the wasm32 run check (brew install wasmtime)"
fi

echo
echo "✓ both backends green — no-JIT parity holds (ADR-0037 Phase 1)"
