#!/usr/bin/env bash
# Smoke-test the `kbb -M:kami` pipeline end to end: the matrix CLI emits valid EDN with
# the right per-target decisions, and the compiler turns a game's logic.clj into a
# real WASM module. Guards the bb → kami → kami-engine-clj wiring against silent
# breakage (e.g. a crate rename) that per-crate unit tests wouldn't catch.
#
#   scripts/test-bb-pipeline.sh
set -euo pipefail
cd "$(dirname "$0")/.."
fail() { echo "✗ $1" >&2; exit 1; }

# 1. `kbb -M:spec` is machine-readable EDN with the expected per-target decisions.
spec=$(kbb -M:spec mac)
echo "$spec" | grep -q ':triple "aarch64-apple-darwin"' || fail "spec mac: triple"
echo "$spec" | grep -q ':host "wasmtime"'               || fail "spec mac: host"
bb spec ios    | grep -q ':host "wasmi"'                 || fail "spec ios: no-JIT host"
bb spec switch | grep -q ':triple nil'                   || fail "spec switch: console has no public triple"
echo "✓ bb spec emits the expected matrix EDN"

# 2. `kbb -M:compile` turns logic.clj into a real WASM module.
kbb -M:compile survivors >/dev/null
wasm=kami-clj-play/games/survivors/game.wasm
[ -s "$wasm" ] || fail "kbb -M:compile produced no game.wasm"
[ "$(od -An -tx1 -N4 "$wasm" | tr -d ' ')" = "0061736d" ] || fail "game.wasm missing WASM magic"
echo "✓ bb compile survivors → game.wasm ($(wc -c < "$wasm" | tr -d ' ') bytes, valid WASM)"

echo "✓ bb pipeline smoke OK"
