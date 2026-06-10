#!/bin/bash
# SessionStart hook — etzhayyim/kami-engine
#
# Bootstraps the Rust + WASM toolchain so a Claude Code on the web session can
# build, test, and produce browser/actor WASM for the KAMI engine WITHOUT any
# secrets touching the cloud container.
#
# What it installs (all idempotent, all from public sources — no credentials):
#   - rustup wasm32-unknown-unknown target   (kami-web / kami-app-{game} WASM)
#   - wasm-pack                               (`wasm-pack build --target web`)
#   - wasm-tools                              (strip / validate the .wasm)
#
# Deliberately NOT done (operating-entity / no-server-key boundary):
#   - no deploy credentials (Cloudflare / IPFS-pin live in GitHub Actions)
#   - no live publish to *.etzhayyim.com (that is a GitHub-driven / operator step)
#
# Runs only in the remote (web) environment. Local Macs already have the toolchain.
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  echo "session-start: not a remote session — skipping toolchain bootstrap"
  exit 0
fi

BIN_DIR="/usr/local/bin"
WASM_TOOLS_VER="1.225.0"
ARCH="$(uname -m)"

log() { echo "session-start: $*"; }

# ── 1. Rust wasm32 target ────────────────────────────────────────────────────
if command -v rustup >/dev/null 2>&1; then
  if ! rustup target list --installed 2>/dev/null | grep -q '^wasm32-unknown-unknown$'; then
    log "adding rust target wasm32-unknown-unknown"
    rustup target add wasm32-unknown-unknown
  else
    log "wasm32-unknown-unknown target already present"
  fi
else
  log "WARN: rustup not found — WASM builds will be unavailable"
fi

# ── 2. wasm-pack (the documented kami-web build path) ─────────────────────────
if ! command -v wasm-pack >/dev/null 2>&1; then
  log "installing wasm-pack"
  tmp="$(mktemp -d)"
  url="https://github.com/rustwasm/wasm-pack/releases/latest/download/wasm-pack-v0.13.1-${ARCH}-unknown-linux-musl.tar.gz"
  if curl -sSL -o "$tmp/wp.tar.gz" "$url" && tar xzf "$tmp/wp.tar.gz" -C "$tmp" --strip-components=1; then
    cp "$tmp/wasm-pack" "$BIN_DIR/" && log "wasm-pack installed: $(wasm-pack --version)"
  else
    log "WARN: wasm-pack download failed — falling back to 'cargo install wasm-pack' (slow)"
    cargo install wasm-pack --locked >/dev/null 2>&1 || log "WARN: wasm-pack unavailable"
  fi
  rm -rf "$tmp"
else
  log "wasm-pack already present: $(wasm-pack --version)"
fi

# ── 3. wasm-tools (strip / validate) ─────────────────────────────────────────
if ! command -v wasm-tools >/dev/null 2>&1; then
  log "installing wasm-tools ${WASM_TOOLS_VER}"
  tmp="$(mktemp -d)"
  url="https://github.com/bytecodealliance/wasm-tools/releases/download/v${WASM_TOOLS_VER}/wasm-tools-${WASM_TOOLS_VER}-${ARCH}-linux.tar.gz"
  if curl -sSL -o "$tmp/wt.tar.gz" "$url" && tar xzf "$tmp/wt.tar.gz" -C "$tmp"; then
    cp "$tmp/wasm-tools-${WASM_TOOLS_VER}-${ARCH}-linux/wasm-tools" "$BIN_DIR/" && log "wasm-tools installed: $(wasm-tools --version)"
  else
    log "WARN: wasm-tools download failed"
  fi
  rm -rf "$tmp"
else
  log "wasm-tools already present: $(wasm-tools --version)"
fi

log "toolchain ready — cargo test + wasm-pack build enabled (deploy is GitHub-driven)"
