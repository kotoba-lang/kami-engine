# Kami consolidation must converge on host-independent Kotoba components

Status: accepted owner clarification, 2026-10-22.

Tracking: [JUN-269](https://linear.app/com-junkawasaki/issue/JUN-269) and
[AWAI-53](https://linear.app/kotobalabs/issue/AWAI-53), the existing issue UUID
previously labeled JUN-267.

## Problem

The portable-source draft preserves existing CLJK projects and their behavior,
but its follow-up work expanded JVM, SCI and compiled CLJS/Node tests as a
qualification path. Those tests prove properties of the old host readings.
They do not remove those hosts from the library, build or normal CLI.

The owner explicitly reaffirmed removing JVM and CLJS/Node dependence. The
existing Kotoba [native CLI migration contract](https://github.com/kotoba-lang/kotoba/blob/2de9ccd18a2e559dbb75019968d413435fb0a087/docs/native-cli-migration.md)
already requires Amu-produced native code for normal CLI, build and required
CI, with CLJ selected explicitly for compatibility/bootstrap only.

At that inspected main, `bin/kotoba` sends most commands to `bin/kotoba-clj`,
which executes `clojure`. A successful `--runtime sci` probe therefore selects
a guest host while still starting the JVM launcher. Calling it public CLI
evidence is accurate; calling it native or host-independent evidence is not.

## Decision

Kotoba source and the complete selected component are the migration target.
Preserved CLJK APIs, aliases, source, tests, branches and WIP remain explicit
compatibility inputs. Source co-location, compatibility regression results,
native migration and runtime admission are separate states.

For each selected component, define its public API and full dependency closure,
port that closure to Kotoba, and build, link and execute the actual artifacts
through the owning Amu/native route. Required verification must run with JVM,
GraalVM, CLJS/Node and nbb unavailable, including a control proving those
executables are denied. Check semantic results, source/tool digests, bounded
resources and authority/refusal behavior. Do not replace a component with a
scalar shadow or a smaller kernel to pass the gate. Unsupported APIs remain
explicit native blockers, with no implicit hosted fallback.

Historical oracle runs may explain regressions. New hosted test infrastructure
must not become the default completion path or a new production/build
dependency. It cannot clear a native migration hold. Stop extending that path
as the means of finishing this wave. Before retiring the selected Kami source
repos, verify the merged owner main against the component's declared native
acceptance as well as preservation, compatibility and consumer requirements.

Amu and execution/runtime foundations stay in their owning repositories.
This decision moves portable library logic, not low-level runtime authority.
Existing disabled Actions, the239 dependency-pin holds, unrelated WIP and
signed locks remain unchanged. No service or deployment change is authorized.

## Current evidence and next work

`maintenance/kami-native-migration.json` inventories all70 source projects.
Its initial observation found four `.kotoba` source files across three projects;
the inventory records subsequent candidates and their actual qualification.
This wave records no whole-component native qualification for the70 projects;
the suffix count itself does not measure API coverage. Existing scalar,
generated-JavaScript and typed-Wasm probes do not establish that missing proof.

The pinned self-built compiler and native standard-library gate in Kotoba are
the starting route. Their existing fixture scope does not prove the normal
public CLI cutover or these Kami components. Inventory required APIs and
dependencies, then port and test real closures. Preserve current functional
branch corrections and the historical test results without presenting their
JVM/SCI/compiled success as native migration completion.

This supersedes any interpretation of the portable-source consolidation ADR
that makes successful hosted tests sufficient for completing this migration.
The repository placement, provenance, namespace isolation and source recovery
decisions remain applicable.
