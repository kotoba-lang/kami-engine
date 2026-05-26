# Security policy — `@etzhayyim/kami-engine-sdk`

## Reporting a vulnerability

Please report security vulnerabilities **privately** via GitHub's "Report a vulnerability" interface on the upstream repo:

  https://github.com/etzhayyim/kami-engine-sdk/security/advisories/new

(This file is mirrored from the monorepo at `etzhayyim/root` into the SDK subrepo + the published npm package. Reports flow to the same maintainers regardless of where you file them.)

Do **not** open public issues or PRs for security findings until a fix is published — coordinated disclosure protects users between the report and the patch landing.

## What to include in a report

- The version of `@etzhayyim/kami-engine-sdk` you observed the issue in (npm version, or git commit SHA for `link:` consumers)
- The host runtime (browser + version, Node version, OS)
- A minimal reproduction (Svelte component, REPL link, or repro repo)
- The security impact (information disclosure, code execution, denial of service, etc.) and a worst-case scenario if known
- Whether you intend public disclosure on a specific date

## Response timeline

The maintainers are an unincorporated religious voluntary association (per `did:web:etzhayyim.com` constitutional identity) without 24/7 on-call. Realistic timeline targets:

- Acknowledgement within 7 days
- Triage + impact assessment within 30 days
- Coordinated patch + disclosure within 90 days

If the timeline materially slips, the reporter is informed via the same advisory thread.

## Scope

In scope for this policy:

- Code paths inside `@etzhayyim/kami-engine-sdk` (Svelte components, builders, the `./webvr` headless engine, the `./gsplat` bridge, the `./genko` editor, the `./trackpad` and `./document` helpers, the `./manufacturing` planning helpers).
- The published npm tarball's contents (LICENSE / NOTICE / CHARTER-RIDER.md / CHANGELOG.md / README.md / `dist/**`).

Out of scope (file with the relevant repo instead):

- The underlying KAMI Engine WASM module (`kami-web` Rust crate) — file at `https://github.com/etzhayyim/root/security/advisories` (the canonical engine source lives in the monorepo, not the SDK subrepo).
- Three.js / `@pixiv/three-vrm` — file with the upstream three.js or pixiv projects directly. This SDK has been three.js-free since 2026-05-26 per ADR-2605264300, so three's bugs don't reach SDK consumers via this package.
- `@langchain/langgraph` and `@langchain/core` — file with LangChain upstream.

## Charter Rider considerations

This package ships under Apache 2.0 + the etzhayyim Charter Compliance Rider v2.0 (see `CHARTER-RIDER.md`). The Rider's §2 prohibited-use categories — covert-ops vendor data-sovereignty + anti-gatekeeping enforcement — apply to security tooling too. The maintainers do **not** accept paid disclosure contracts ("bug bounty" rewards), red-team-as-a-service engagements, or zero-day brokerage. Reports are accepted on the same terms as any other contribution: free release of the finding back to the public-interest commons, with credit to the reporter (unless they opt out of attribution).

## Acknowledgements

When a patched release ships, the advisory thread + the SDK's `CHANGELOG.md` cite the reporter (or `Anonymous` if attribution is declined).

---

This policy is part of the religious-corp commitment to public-interest software stewardship per ADR-2605192100 (Mission Charter §1.5) + ADR-2605192200 (Charter Rider v2.0). Updates to this file follow the same Council Lv6+ supermajority threshold as the Rider itself.
