#!/usr/bin/env bash
# Real compile+execute verification for every games/{genre}/logic.clj.
#
# Two stages, both required -- neither alone is sufficient (iteration-4
# lesson this session, kotoba-lang/kami-engine#95): compiling via the real
# CLJS path (not JVM) catches JS-numeric-semantics bugs JVM tests can't see;
# then actually EXECUTING the emitted bytes via a real WASM host (not just
# `wasm-tools validate`, which only checks structural validity) catches
# anything validate can't -- see PR#95's own history for exactly that gap.
#
# Requires a sibling checkout of kotoba-lang/kami-script-runtime-rs (for the
# `kami-host` binary) -- pass its path as $1, or set KAMI_HOST_REPO.
set -euo pipefail
cd "$(dirname "$0")"

KAMI_HOST_REPO="${1:-${KAMI_HOST_REPO:-../../../kami-script-runtime-rs}}"

echo "== stage 1: compile every games/{genre}/logic.clj via the real CLJS path =="
amu compile --target wasm32-browser verify   # builds out/verify.js (a node-script bundle)
node out/verify.js               # -main actually runs kotoba.engine-clj + writes out/*.wasm

echo
echo "== stage 2: execute every compiled .wasm via kami-host (real WASM execution) =="
KAMI_HOST="$KAMI_HOST_REPO/target/release/kami-host"
if [ ! -x "$KAMI_HOST" ]; then
  echo "building kami-host (release, one-time)..."
  (cd "$KAMI_HOST_REPO" && cargo build --release --bin kami-host)
fi

fail=0
for wasm in out/*.wasm; do
  genre=$(basename "$wasm" .wasm)
  if "$KAMI_HOST" "$wasm" 60 7 >/tmp/verify-"$genre".log 2>&1; then
    entities=$(grep -o 'entities=[[:space:]]*[0-9]*' /tmp/verify-"$genre".log | tail -1)
    echo "OK   $genre ($entities)"
  else
    echo "FAIL $genre (see /tmp/verify-$genre.log)"
    fail=1
  fi
done

exit $fail
