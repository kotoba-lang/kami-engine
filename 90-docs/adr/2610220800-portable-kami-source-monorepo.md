# Portable Kami projects in the engine source monorepo

Status: proposed; source and history are imported, qualification is in progress.

Tracking: https://linear.app/kotobalabs/issue/JUN-267

## Context

The reviewed family contains 97 repositories. Its portable contracts, scene
loaders, authoring SDKs, simulation models, media tools and reference assets
share an engine authoring boundary. Keeping each as a separate repository
makes dependency floors, fixture paths and source ownership difficult to review
together. Their runtime and deployment boundaries still differ.

`kami-engine` already owns the EDN/WIT interface, adapter registry, fixtures and
portable gameplay/render/physics projects. It is an asset and contract owner;
native runtimes remain external. This decision supersedes the physical-location
part of ADR-2607102200 for the explicitly cataloged portable projects. It does
not transfer provider, compiler, service or application authority to the root.

## Decision

1. Co-locate 67 independently selectable portable source projects at
   the explicitly cataloged `packages/<source-directory>/` paths. Preserve their complete upstream tracked trees,
   license/notice files, original manifests and byte/mode provenance.
2. Include the three existing `kami-gameplay`, `kami-render` and `kami-physics`
   projects in the same source catalog. The 70 IDs start with `kotoba.`; the
   152 exact aliases include the 137 original repository selectors and 15
   previous draft IDs. Canonical segments follow the naming Spec's
   `[a-z][A-Za-z0-9]*` grammar, using lowerCamelCase for multiword segments.
   `maintenance/kami-source-id-migrations.json` binds the explicit corrections
   to the pinned Spec; lookup never converts hyphens/case implicitly. Directory
   paths and Lisp namespace APIs remain separate. A dotted source ID is neither
   a field traversal nor an event subject.
3. Keep classpaths project-scoped. `kami.scene` has an identical contract mirror
   in two projects, and the two `kami.app-fixtures` surfaces differ. Catalog
   discovery must not combine all source directories or invent a runtime
   admission, RID, grant, provider registration or built-in dispatch table.
4. Retain seven retired empty scaffolds under `retired-reservations/` as
   documentation and recoverable history, with no executable source ID.
5. Keep the Rust `kami-apps` workspace and `kami-engine-guest` compiler separate.
   Review the 20 product applications' source and deployment ownership
   separately. No published URL, release identity, service or production route
   changes in this wave. Browser runtime/broker integration is not established
   by source co-location.
6. Preserve all selected histories, local refs and historical PR heads in
   verified bundles. Record nonancestor branch decisions explicitly. Existing
   declined or superseded proposals are preserved without silently reactivating
   their APIs. Original checkouts, WIP, stashes and broken worktree metadata
   remain intact.
7. Preserve the original 239 dependency-pin holds and signed workspace locks.
   Relocate declared internal edges and canonical external development paths
   with typed-EDN checks. Review internal version advances and any necessary
   external compatibility exception against actual source and consumer tests.
   The root retains `:paths []` and does not acquire an aggregate package path.

The ML3D adapter's private GPU-provider assembly remains external. Its public
source is retained; importing it does not grant access to that assembly or
authorize a model download, paid generation or remote inference. Its dependency
and host qualification is still open. Private provider source, WIP and revision
details must not be copied into this public repository.

## Source lookup

```sh
node scripts/kami-sources.mjs --json kotoba.kami.engine.animation
node scripts/kami-sources.mjs --json kami-engine-animation
node scripts/kami-sources.mjs --json io.github.kotoba-lang/kami-engine-animation
```

Lookup returns the selected package and its existing source directories.
It does not execute the package. Exact aliases resolve to the same descriptor;
unknown or ambiguous selectors, retired scaffolds, catalog admission claims
and escaping directories are refused. Relative paths are checked through the
filesystem, including symlinks, and lookup works outside the repository cwd.

The imported README files are upstream source records. The catalog and this
ADR define the new source location; legacy standalone location statements are
historical. Existing legacy pointer directories and built artifacts are not
restored as a second implementation tree. The old SDK gitlink remains a
compatibility marker pending separate consumer cleanup.

## Qualification and retirement

```sh
node scripts/check-kami-sources.mjs
KBB_NO_STDLIB=1 KBB_NO_NPM=1 kbb --backend sci --config scripts/source-check.nbb.edn scripts/check-kami-dependencies.cljk
node scripts/test-kami-sources.mjs /absolute/github/workspaces/codex/TASK/source-cli-controls
```

The source check binds 1,677 original tracked file preimages to the imported
tree and explicitly records configuration and test-host repairs. Dependency
checks compare literal manifests, rather than accepting a rewritten file on
its own. Per-project runtime suites, native Kotoba/JavaScript/typed-Wasm goldens,
cross-project fixture consumers and branch decisions remain required.

The initial JVM probe is diagnostic, not completion evidence: 54 of 70 project
suites passed and 16 exposed unresolved host, fixture, dependency or model
issues. Repairs must be rerun against their actual consumers. A passing local
suite does not qualify an unmeasured native/GPU/live provider.

The subsequent targeted repairs now pass JVM fixture/provider contracts and
JVM/SCI/compiled Node modeling and scene-author tests. The compiled host exposed
a namespaced-map printer default that changed revision/signature bytes; explicit
canonical printing preserves the existing JVM `k1` identities. The UUID exchange
reader admits only well-formed UUID tags with all bounded-reader checks retained.
Whole doubles and fractional JSON values follow the decoded-value wire contract.
Canonical fixture catalogs name real CLJK files while validators accept old
source-extension aliases. Their engine mirror tests inspect all12 real files.

The two-project host matrix is deliberately partial:

```sh
python3 scripts/test-kami-compiled.py /absolute/github/workspaces/codex/TASK/compiled --project kotoba.kami.engine.modeling
python3 scripts/test-kami-compiled.py /absolute/github/workspaces/codex/TASK/compiled --project kotoba.kami.mangaka.sceneAuthor
python3 scripts/test-kami-compiled.py /absolute/github/workspaces/codex/TASK/compiled --project kotoba.kami.mangaka.sceneAuthor --plant-failure
KOTOBA_CLI=/absolute/path/to/kotoba/bin/kotoba node scripts/test-kami-imports.mjs /absolute/github/workspaces/codex/TASK/import-probes
```

The runner hashes every source view and changes only selected tests' macro
require to the shared pinned compiled bridge. Production bytes and assertion
bodies are unchanged by that host wiring. The planted failure/error must exit1.
The separate import probes invoke the real public Kotoba CLI with explicit
source roots, asserting the frozen revision/UUID round-trip and fractional JSON
value plus every implementation origin inside the selected owner package.
Java Ed25519 tests remain JVM-only with a checked import; the nine CAE tests now
run on both Node hosts using the same measured fixture. Giemon stays separate:
only its immutable Apache-licensed provider-contract test data is included.
All56 dependency relocations remain unchanged, with two separately validated
CLI option changes pointing fixture mirror operations at the actual owner.

Fourteen of the16 remaining branch heads now have pinned semantic decisions:
seven retired ns-only scaffolds, the Rust-retirement rescue, the already-fixed
HUD smoke test, three SDK heads with exact historical implementation/test/sample
postimages and superseded README notes, the NV fix, and current vehicle docs.
The Echem power-run and vehicle-regeneration proposals contain novel behavior
and remain under review rather than being labeled merged or discarded.

Do not archive a source repository until its source/branch/dependency and
declared-host gates pass on actual owner main, followed by fresh remote and
local preservation guards and an archive readback. Record any remaining hold
per project. Do not enable disabled Actions to obtain a green badge. Existing
disabled workflows remain disabled; zero Actions runs is distinct from CI
success. Archive and deletion are separate operations.

This wave does not prove the whole organization consolidated. Remaining
families and product/runtime boundaries continue under JUN-267.
