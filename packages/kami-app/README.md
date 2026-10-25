# kotoba-lang/kami-app

Historical reservation for the legacy `kami-engine/kami-app` Rust crate,
retained as documentation during the clj-wgsl migration.

## Status

Docs-only. The former `.cljc` file contained only an `ns` declaration and no
contract, data, function, or runtime behavior, so it was retired rather than
renamed into a misleading empty `.kotoba` module.

Application and engine orchestration belongs to Kami providers. A future Kotoba domain contract requires
a new ADR, explicit typed ABI, bounded resource profile, and
reference/Web/Wasm conformance before production source is added.

See [ADR 0001](docs/adr/0001-retire-empty-cljc-scaffold.md).
