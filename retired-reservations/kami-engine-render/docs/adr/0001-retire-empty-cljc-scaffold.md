# ADR 0001: Retire empty CLJC scaffold

## Status

Accepted.

## Decision

Remove `src/kami_engine_render.cljc`, its namespace-load smoke test, and the
JVM/Clojure test configuration. The source contained only an `ns` declaration
and docstring and therefore provided no behavior or compatibility contract to
migrate.

Do not create an empty `.kotoba` replacement. That would falsely claim a
language-owned application contract while preserving no executable semantics.

This repository was already documented as an archived clj-wgsl migration
placeholder (ADR-2607102200 addendum 8-9, `west group: archived`) and not the
implementation SSoT before this retirement; removing the empty scaffold does
not change where the real implementation lives (see README "Where to go
instead").

## Consequences

This repository is docs-only and has no production JVM dependency. CI rejects
the reintroduction of Clojure-family or Kotoba source without an intentional
replacement of this retirement decision.
