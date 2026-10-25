# ADR 0001: Retire empty CLJC scaffold

## Status

Accepted.

## Decision

Remove `src/kami_app.cljc`, its namespace-load smoke test, and the JVM/Clojure test
configuration. The source contained only an `ns` declaration and therefore
provided no behavior or compatibility contract to migrate.

Do not create an empty `.kotoba` replacement. That would falsely claim a
language-owned contract while preserving no executable semantics.

Application and engine orchestration, rendering, input, physics, and device execution remain Kami provider
responsibilities. A future portable domain layer must be added as a separately
reviewed `.kotoba` contract with explicit types, effects, resource bounds, and
reference/restricted-JavaScript/Wasm evidence.

## Consequences

This repository is docs-only and has no production JVM dependency. CI rejects
the reintroduction of Clojure-family or Kotoba source without an intentional
replacement of this retirement decision.
