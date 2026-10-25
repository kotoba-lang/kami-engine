> **Scaffold / not the implementation SSoT (ADR-2607102200).** Canonical: `webgpu / kami-web`.

# kotoba-lang/kami-engine-web

Zero-dep portable `.cljc` — restored from the legacy `kami-engine/kami-engine-web` Rust crate
(deleted in the kotoba-lang Rust removal) as part of the **clj-wgsl migration** (ADR-2607010930,
`com-junkawasaki/root`).

## Status

Docs-only. The restored `.cljc` file contained only an `ns` declaration and no
contract, data, function, or runtime behavior, so it was retired rather than
left as a misleading pending scaffold or renamed into an empty `.kotoba`
module. Native execution (wgpu / wasmtime / wasmi) stays substrate; canonical
implementation lives in `webgpu` / `kami-web`.

See [ADR 0001](docs/adr/0001-retire-empty-cljc-scaffold.md).
