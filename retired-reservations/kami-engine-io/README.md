# kotoba-lang/kami-engine-io

> **Archived scaffold (ADR-2607102200 addendum 8–11).**  
> clj-wgsl migration placeholder — **not** implementation SSoT.  
> west group: `archived` (skipped by default `west update`).

## Where to go instead

| need | package |
|---|---|
| Browser WebGPU executor | `kotoba-lang/webgpu` |
| Game browser host | `kotoba-lang/host` |
| Engine SDK | `kotoba-lang/kami-engine-sdk` |
| App chrome (ex kami-ui-sdk) | `kotoba-lang/kami-engine-app-sdk` |

Do not add new code here.

## Status

Docs-only. The former `.cljc` file contained only an `ns` declaration and no
contract, data, function, or runtime behavior, so it was retired rather than
renamed into a misleading empty `.kotoba` module.

See [ADR 0001](docs/adr/0001-retire-empty-cljc-scaffold.md).

