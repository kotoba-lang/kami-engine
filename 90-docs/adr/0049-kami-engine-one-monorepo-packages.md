# ADR-0049 — kami-engine becomes one monorepo for the kami-* family (`packages/<repo>/`)

- Status: accepted (owner decision 2026-10-25); pilot of 3 packages
- Date: 2026-10-25
- Supersedes: the note in `AGENTS.md` that nested sub-projects were moved out of this
  repository (ADR-2607102200) and that a nested tree must not be restored. That note
  stays true for the top-level pointer directories (`kami-gameplay/`, `kami-render/`,
  `kami-physics/`, ...), which are unchanged; it no longer forbids code under `packages/`.

## Decision

`kotoba-lang/kami-engine` is the single repository for the `kami-*` family, the way
`amu` is one repository. The ~60 standalone `kotoba-lang/kami-*` repositories are
consolidated here step by step.

- **Layout.** Each repository lands in `packages/<repo name>/` (for example
  `packages/kami-scene-contracts/`), never at the top level, so the existing
  pointer directories keep their meaning. `packages/README.md` is the index with one
  row per package.
- **Convention.** The same as `kotoba-lang/kotoba` `lib/<id>/`: `imports.json`
  (source repo, commit, tree, per-file sha256, `excluded`) and `local-changes.json`
  (every deviation, with imported-sha256, sha256 and reason). Files are byte-identical
  to the source commit except the recorded local changes.
- **Not carried.** `.github/` (hosted CI definitions of the old repository) and any
  file this repository's `.gitignore` drops; both are listed under `excluded`.
- **License.** A source repository without a LICENSE is admitted under Apache-2.0: the
  license text is added and `imports.json` says so on that file (`added`). (The three
  pilot repositories already carry Apache-2.0.)
- **Dependencies.** A git dependency on another `kotoba-lang/kami-*` repository that is
  itself imported becomes `{:local/root "../<repo>"}`. Other dependencies are unchanged.
  No JVM/Maven dependencies or JVM-only aliases (`:mvn/version`, `:main-opts`,
  `:exec-fn`, cognitect test-runner, clj-kondo) remain in an imported `deps.edn`.
- **CI/CD.** The official path is Amu selfhost and Kotoba native. Any other runner
  (JVM, Node, kbb) is an oracle only, so a package whose suites cannot run under kbb is
  still admitted on provenance and hash evidence. No JVM/nbb/Node is added to CI here.
- **Old repositories.** Archiving an old repository is a separate, later step after its
  package is merged and it has no open issues or pull requests.

## Consequences

- One history, one place for cross-package changes and ADRs.
- `imports.json` makes each package's origin verifiable; later divergence from the old
  repository is expected and is tracked in the monorepo only.
- The top-level `kami-*` pointer directories and `wit/` contract are untouched.
