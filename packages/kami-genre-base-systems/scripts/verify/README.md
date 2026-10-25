# verify — real compile+execute verification for every genre

Iteration-4 lesson (this session): JVM `codegen/compile` succeeding is **not**
sufficient evidence a `logic.clj` is correct. The i64.const 32-bit truncation
bug (`kotoba-lang/kami-engine#95`) only manifested under CLJS/WASM numeric
semantics — JVM's bitwise ops are full 64-bit `long` ops and never saw it.
16/18 genres also had a separate, unrelated real bug (bare `defatom` symbol
reads instead of the required `atom-val` accessor) that plain `author.clj`
runs never exercised, since `author.clj` only touches the datalevin→scene.edn
data path, never `logic.clj` itself.

This script closes both gaps with two required stages — neither alone is
sufficient:

1. **Compile every `games/{genre}/logic.clj` through the real CLJS path**
   (`kotoba.engine-clj`, the same compiler `network-isekai` uses in
   production), not JVM.
2. **Execute the compiled `.wasm` via a real WASM host**
   (`kami-script-runtime-rs`'s `kami-host` binary — native wasmtime, no
   browser/Playwright toolchain needed for routine checks), not just
   `wasm-tools validate`'s structural check.

## Run

```sh
cd scripts/verify
npm install
./run-verify.sh /path/to/kotoba-lang/kami-script-runtime-rs   # or set KAMI_HOST_REPO
```

Requires sibling checkouts of `kotoba-lang/kami-engine` (for
`kami-engine-clj/`, referenced via `deps.edn`'s `:local/root`) and
`kotoba-lang/kami-script-runtime-rs` (for the `kami-host` binary — built
automatically on first run if not already present).

## Not wired to CI

`gftdcojp`'s org-level GitHub Actions is disabled (confirmed against both the
archived `isekai-network` and the current `network-isekai` repos this
session) — this script has no CI to attach to yet. Run it manually before any
change to `kami-engine-clj`'s compiler or this repo's `logic.clj` files.
