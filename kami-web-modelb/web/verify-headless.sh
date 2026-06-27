#!/usr/bin/env bash
# Headless-browser smoke test: the web Model-B dance actually RUNS in a real
# browser — not just builds. Builds the harness, serves it, loads it in headless
# Chrome, and asserts the compiled-CLJ logic ran in-page (audience seated, show
# ticked). Proves the wasm-in-wasm dance works in a browser.
#
#   kami-web-modelb/web/verify-headless.sh
#
# Needs: wasm-pack, python3, and Chrome (set CHROME=/path/to/chrome to override).
set -euo pipefail
cd "$(dirname "$0")/../.." # repo root

CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
if [ ! -x "$CHROME" ]; then
  CHROME="$(command -v google-chrome-stable || command -v google-chrome || command -v chromium || true)"
fi
[ -n "$CHROME" ] || { echo "no Chrome found — set CHROME=/path/to/chrome"; exit 2; }

echo "▸ building the harness for the browser (wasm-pack)…"
wasm-pack build --target web kami-web-modelb >/dev/null

PORT="${PORT:-8093}"
python3 -m http.server "$PORT" >/dev/null 2>&1 &
SRV=$!
trap 'kill "$SRV" 2>/dev/null || true' EXIT
sleep 2

URL="http://localhost:$PORT/kami-web-modelb/web/index.html"
DOM="$(mktemp)"
echo "▸ running it in headless Chrome…"
"$CHROME" --headless=new --no-sandbox --disable-gpu \
  --user-data-dir="$(mktemp -d)" --virtual-time-budget=9000 \
  --dump-dom "$URL" >"$DOM" 2>/dev/null

status="$(grep -oE 'id="status">[^<]*' "$DOM" | sed 's/.*>//' || true)"
fans="$(grep -oE 'id="fans">[0-9]+' "$DOM" | grep -oE '[0-9]+' || echo 0)"
frames="$(grep -oE 'id="frames">[0-9]+' "$DOM" | grep -oE '[0-9]+' || echo 0)"
echo "  status=${status:-?}  frames=${frames:-0}  fans=${fans:-0}"

if echo "${status:-}" | grep -q running && [ "${fans:-0}" -gt 0 ] && [ "${frames:-0}" -gt 0 ]; then
  echo "✓ web Model-B: the compiled-CLJ dance RUNS in a real browser (${fans} fans seated by CLJ seat-audience, ${frames} frames)"
else
  echo "✗ harness did not run in-browser (status=${status:-?})"; exit 1
fi
