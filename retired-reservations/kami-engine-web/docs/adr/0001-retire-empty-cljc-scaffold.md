# ADR 0001: Retire empty CLJC scaffold

## Status

Accepted.

## Decision

Remove `src/kami_engine_web.cljc`, its namespace-load smoke test, and the
JVM/Clojure test configuration. The source contained only an `ns` declaration
and docstring and therefore provided no behavior or compatibility contract to
migrate.

Do not create an empty `.kotoba` replacement. That would falsely claim a
language-owned application contract while preserving no executable semantics.

This repository was already documented as an archived scaffold, not the
implementation SSoT (ADR-2607102200; canonical is `webgpu` / `kami-web`)
before this retirement; removing the empty scaffold does not change where
the real implementation lives.

## Consequences

This repository is docs-only and has no production JVM dependency. CI rejects
the reintroduction of Clojure-family or Kotoba source without an intentional
replacement of this retirement decision.
