# ADR-0048 — Align character/scene pipeline with global interchange standards (GLB/glTF/USD/MaterialX), as pure kotoba-clj/wasm libraries, no Rust

- Status: accepted (all 5 decision items implemented and shipped 2026-07-05: `kotoba-lang/
  org-khronos-glb` extracted; `org-khronos-gltf` gained a parser + chain retargeting;
  `org-openusd` gained a real USDA parser; `org-materialx` gained real node-defs + a parser +
  render-IR bridge; `kotoba-lang/character`/`kami-gen-hybrid` adopted the tagged-point
  coordinate-space convention; `cloud-murakumo`/`kami-gen-ml3d` gained the `:autorig` UniRig
  path. See "Naming correction" below for a post-ship repo rename.)
- Date: 2026-07-05
- Builds on: ADR-0031 (kami-vrm three-free topology), ADR-0039 (kototama/render-IR single entry),
  ADR-0043 (VRM dance scenes in CLJ/EDN), ADR-0044 (EDN render-IR three.js/VRM parity)
- Related (superproject `com-junkawasaki/root`): ADR-2605261800 (`kami-usd`/`kami-usd-native`
  tinyusdz-WASM/Rust-fallback path reservation — **not superseded**, see "Relationship to
  ADR-2605261800" below), ADR-2607010930 (clj-wgsl migration — former Rust crates restored as
  standalone `kotoba-lang/*` `.cljc` repos), ADR-2607051100/1110/1120/1130 (`kami-gen-*` 4-way
  character-generation comparison — the concrete bugs motivating this ADR)
- Supersedes: ADR-0032 (Pure-Rust glTF/Gaussian-splat asset decoders — its premise, Rust
  decoders in `kami-render`/`kami-webgpu-rs`, no longer holds; those crates now ship WGSL
  shaders only, no `.rs`)

## Context

**A documentation staleness bug, found while drafting this ADR.** This repo's own `CLAUDE.md`
(as of this ADR's date) describes a 29-crate Rust workspace (`kami-core`, `kami-render`,
`kami-vrm`, `kami-gltf`, `kami-sdf`, `kami-mesher`, `kami-scad`, `kami-terrain`,
`kami-vegetation`, `kami-skeleton`, …) with explicit prohibitions ("JS/Clojure による Rust
ロジック再実装禁止 — wasm-bindgen で WASM 呼び出し", "独自レンダラ禁止 — kami-render wgpu PBR
pipeline が唯一"). **This is no longer true.** Verified directly for this ADR:
`find . -iname Cargo.toml` and `find . -iname '*.rs'` at this repo's root return **zero**
results; `kami-render/src` and `kami-webgpu-rs/src` contain **only** `.wgsl` shader files (no
`.rs`); `kami-script-runtime/src` is empty; `.github/workflows/ci.yml` runs a job literally
named "WIT single-source + no-rust guard" that **fails the build** if it finds any
`Cargo.toml`/`Cargo.lock`/`*.rs`/`rust-toolchain*`/`.cargo/` anywhere in this repo. The former
Rust crate logic (`character`, `mesher`, `vrm`, `skeleton`, `sdf`, `scad`, `terrain`,
`vegetation`, `nerf`, `gltf`) was ported to standalone zero-dep `.cljc` repos under
`kotoba-lang/*` (ADR-2607010930, "clj-wgsl migration"). **`CLAUDE.md`'s crate table and Rust
prohibitions describe a pre-migration state and need a follow-up documentation-only fix**
(tracked as a Consequence below, not blocking this ADR).

**Why this ADR now.** Building 4 sibling character-generation pipelines
(`kotoba-lang/kami-gen-{procedural,sdf-agent,ml3d,hybrid}`, ADR-2607051100/1110/1120/1130) as a
head-to-head comparison surfaced two real, shipped bugs, both now fixed:

1. `kami-gen-procedural` exported a VRM where the head/hood/hair/eyes/beak/clothing mesh had
   **no skin binding at all** — a static mesh silently left unattached to any bone.
2. `kami-gen-hybrid` exported a VRM where eye/iris/pupil/eyebrow vertices were skinned to
   **`leftShoulder`/`rightShoulder` instead of `head`**, because `kotoba-lang/character` mixes
   world-space (body/clothing) and head-local (head/eyes/hair) coordinates in one part list,
   and the skinning step compared both as if they shared one frame.

A follow-up coscientist-style research pass (Generation/Reflection/Ranking across 4 parallel
research agents, per this org's Arbor-HTR×AI-Co-Scientist pattern,
`90-docs/adr/2606141500-keiei-arbor-coscientist-engine.md`) evaluated OpenUSD adoption and
current HuggingFace models/research against these gaps. Verdict, condensed: OpenUSD's
`UsdSkel` schema (explicit `geomBindTransform` / joint-local rest-pose / world-space
`bindTransforms` naming) would likely have caught bug #1 by construction (missing-binding
validators) but **not** bug #2 (a coordinate-transform logic error is possible regardless of
container format) — real-time/WebGPU USD runtime support is also immature industry-wide (AOUSD
formed its Characters/Motion/Interactivity working group only March 2026). **The concrete,
adoptable fix for bug #2's *class* of error is Rust's `euclid`-crate phantom-typed
`Point3D<T, Space>` pattern (also Bevy's `Transform`/`GlobalTransform` split)** — but this repo
has a hard no-Rust policy (see above), so this ADR designs the equivalent discipline natively
in kotoba-clj.

## Decision

Four parts, matching the 4 numbered points of the direction that produced this ADR.

### 1. Coordinate-space safety without Rust — tagged points + `kotoba-lang/spec`, not phantom types

kotoba-clj has no static type system (verified: `kotoba-lang/spec`'s own README explicitly
rejects `clojure.spec`/`malli` for this exact reason — "JVM-rooted and not portable to the
kotoba-WASM host" — and ships a portable `kotoba.lang.spec/validate`/`explain` instead). We
therefore cannot get Rust's zero-cost *compile-time* phantom-type guarantee. The honest,
adoptable equivalent:

- Every position value that crosses a function boundary in a geometry-producing library
  (`character`, `vrm`, `mesher`, `skeleton`, the new format libs below) is a **tagged map**,
  never a bare `[x y z]` vector: `{:space :world :xyz [x y z]}` / `{:space :head-local :xyz
  [x y z]}` / `{:space :bind-pose :xyz [x y z]}`. Space names are the vocabulary borrowed
  directly from `UsdSkel`'s own schema (world-space `bindTransforms`, joint-local rest pose,
  `geomBindTransform`) — reusing USD's naming, not USD's runtime.
- A single conversion function per space pair (`world->head-local`, `head-local->world`, …)
  is the *only* sanctioned way to change a value's `:space` tag. No function may read `:xyz`
  off a map without checking `:space` first.
- Every public function that consumes points **asserts** the expected `:space` via
  `kotoba.lang.spec/validate` (throws with both the expected and actual space in the error —
  this org's "no silent fallback" convention, matching `cloud-murakumo.gen/resolve-model`'s
  own doc comment) — this is a **checked runtime contract**, not a compile-time guarantee, and
  this ADR says so plainly rather than overclaiming Rust-equivalent safety.
- A `clj-kondo` custom lint rule (this org already runs `kbb -M:lint` as standard,
  CLAUDE.md/repos.edn convention) flags a bare vector literal passed where a tagged-point map
  is the declared parameter shape — a *static*, best-effort second layer, catching the most
  common mistake (forgetting the wrapper entirely) without needing a real type checker.
- Retrofit target (proves the convention on the exact bugs that motivated it): `character`'s
  head/body/eye generators, `vrm`'s skin-weight assembly, and `kami-gen-hybrid`'s
  `vrm_export.clj` (already carries an ad-hoc `to-world-space` fix for bug #2 — this ADR's
  follow-up is to replace that one-off function with the general tagged-point convention so
  the next library can't reintroduce the same bug class).

### 2. Auto-rigging — UniRig via `cloud-murakumo`, not a WASM port

Research ranked **UniRig** (SIGGRAPH/TOG 2025, VAST-AI-Research/Tripo, MIT license, HF weights
`VAST-AI/UniRig`, ingests `.glb`/`.vrm` directly — matching `kami-gen-ml3d`'s TRELLIS/
Hunyuan3D-2 output formats) as the strongest available auto-rig, ahead of the fallback
`RigAnything` (Adobe, better demonstrated non-humanoid generalization) and clearly ahead of
`kami-gen-ml3d`'s own hand-rolled bbox-height-slice heuristic. UniRig is a multi-billion-
parameter autoregressive transformer — **reimplementing it in kotoba-clj/wasm is not a goal
of this ADR**; it is GPU inference, and this org already has a real, working GPU-inference
orchestration layer for exactly this shape of problem (`gftdcojp/cloud-murakumo`'s
`:generation` app, `:model3d` function, `:fn/engine :trellis`, `resources/murakumo.edn`).

- Add a new `cloud-murakumo` generation function, `:autorig` (`:fn/kind :gen :fn/engine
  :unirig :fn/modality :rig`, `:gen {:in [:glb :vrm] :out [:vrm]}`), following the exact
  `:model3d`/`:trellis` function shape already declared in `resources/murakumo.edn`.
  `kami-gen-ml3d` gains a second pipeline stage: submit the TRELLIS/Hunyuan3D-2 mesh output to
  `:autorig` instead of (or as an ensemble check against) its own bbox-heuristic
  (`kami.gen.ml3d.rig/auto-rig-glb`) — 2026 head-to-head benchmarks (StraySpark) found even
  best-in-class auto-riggers need human cleanup on faces/secondary structures, so keep the
  heuristic as a bounded fallback/cross-check, not delete it.
- Same fail-closed DI pattern already proven in `kami-gen-ml3d`/`kami-gen-hybrid`: `:execute`
  is injected, a mock returns a fixture for tests, `real-execute` requires a live backend and
  throws loudly without one. No live GPU inference is authorized by this ADR alone — firing it
  for real is a separate, explicit, costed decision (same standing note as ADR-2607051120).

### 3. Non-humanoid rigs — generic glTF skin as the base, VRM's `humanBones` as an optional layer

VRM 1.0's mandatory `humanBones` mapping (15 required bones) is a poor fit for non-humanoid
silhouettes (the `kami-gen-*` penguin-kigurumi test case is the concrete example). Plain glTF
skinning (`skins`/`joints`/`inverseBindMatrices`) has **no humanoid assumption at all** and is
fully viable standalone (verified: glTF 2.0 spec, §"Skins"). Decision:

- `kotoba-lang/gltf` (currently a write-only GLB/glTF-JSON exporter, ported from the deleted
  Rust `kami-gltf` crate) gains a **parser** (it has none today — confirmed, its own README:
  "there was no parsing/loading logic in the file"), making it bidirectional. This closes
  ADR-0044's own gap-inventory row ("glTF loader | export-only stub | full `GLTFLoader`").
  Generic glTF skin (no VRM layer) becomes the base interchange for any character whose body
  plan doesn't map to `humanBones` — the plain skin/joint contract, not VRM's stricter one.
- A new chain-based retargeting module (`kotoba.gltf.retarget`, inside `kotoba-lang/gltf` or a
  thin sibling repo if it grows large enough to warrant one) maps **bone chains**, not fixed
  bone *names* — the design Unreal's IK Rig/IK Retargeter uses precisely because it
  generalizes to non-standard/non-biped skeletons where VRM's named-bone contract can't. This
  is a design *reference*, not a dependency — no Unreal code is used.
  VRM stays fully available and is the right choice whenever a character genuinely *is*
  humanoid (spring-bones, MToon, expressions are real VRM value that plain glTF skin doesn't
  replace) — this ADR adds an alternative path, it does not deprecate VRM.

### 4. OpenUSD / MaterialX — real interchange-format libraries, not a runtime replacement

Both `kotoba-lang/usd` and `kotoba-lang/materialx` exist today but are narrow, one-directional
stubs: `usd` is a 94-line USDA (ASCII-text) **emitter only**, no parser, no binary
`.usdc`/`.usdz`, no composition-arc validation; `materialx` is a 23-line generic-XML **emitter
only** with no MaterialX node-definition knowledge and no parser. Decision:

- `kotoba-lang/usd` gains a real **USDA (ASCII) parser**, symmetric with its existing emitter,
  scoped explicitly to the text subset — binary `.usdc`/`.usdz` and full composition
  (references/payloads/variants) are an explicit **non-goal of this ADR**, left to the
  already-reserved `kami-usd` (tinyusdz-via-Emscripten primary path) / `kami-usd-native`
  (Rust-fallback, gated) plan — see "Relationship to ADR-2605261800" below. This gives
  `kami-gen-*` and `kami-engine`'s render-IR a working, bidirectional, pure-kotoba-clj text
  interchange for simple scene/character round-trips today, without waiting on that larger
  effort.
- `kotoba-lang/materialx` gains a real **standard-node-definition table** (`ND_standard_
  surface`, `ND_image`, `ND_normalmap`, etc. — sourced from MaterialX's own published node
  defs) plus a parser, so ADR-0044's render-IR `:materials` vocabulary can round-trip to/from
  real MaterialX graphs for DCC interop (an artist can author a material in a MaterialX-aware
  tool and bring it into kami-engine's render-IR, and vice versa).
- Both are positioned as **authoring/interchange formats that convert into and out of
  render-IR**, not a replacement for it — kami-engine's WebGPU renderer keeps consuming the
  EDN render-IR (ADR-0040/0041/0044) exactly as today. This matches the coscientist research's
  verdict: USD's genuine strengths (composition/layering, DCC interop, explicit schema) are
  real but orthogonal to WebGPU/WASM runtime rendering, which stays render-IR's job.

### 5. Format-per-library split — extract `kotoba-lang/glb`

The binary GLB container (12-byte header + JSON chunk + BIN chunk) is currently implemented
**twice**, independently, in two different repos: write-only inside `kotoba-lang/gltf`
(`export-glb-byte-seq`/`export-glb`) and read-only inside `kotoba-lang/vrm`
(`vrm.glb/parse-glb`). Decision: extract a new, tiny, dependency-free
**`kotoba-lang/glb`** repo — binary container codec only (chunk framing, no glTF-JSON or VRM
semantics), real parse **and** write. `kotoba-lang/gltf` and `kotoba-lang/vrm` both migrate to
depend on `kotoba-lang/glb` via `:local/root` instead of each maintaining their own half of the
same codec — one canonical implementation, matching this ADR's "format, not project, is the
unit of a library" principle (the same principle already applied to `sdf`/`scad`/`mesher` as
separate repos rather than one monolith).

## Relationship to ADR-2605261800 (`kami-usd`/`kami-usd-native`)

**Not superseded, not in conflict — different scope.** ADR-2605261800 reserves a path to a
*full* USD experience: `kami-usd` (tinyusdz via Emscripten → WASM, Hydra render delegate,
`omni.usd` API compatibility — i.e., a real USD *viewer/runtime*), with `kami-usd-native` as a
from-scratch Rust fallback gated behind a specific failure condition (tinyusdz WASM viability
gate at R1.1) requiring Council Lv6+ attestation. This ADR's `kotoba-lang/usd` work is a much
narrower, immediately-buildable **ASCII-text-only read/write interchange library** for the
character/scene alignment goal in this ADR's Context — it does not attempt Hydra, binary
`.usdc`/`.usdz`, or composition arcs, and does not touch the Rust-fallback gate at all (it adds
zero Rust anywhere). Both paths can proceed independently; if/when `kami-usd`'s tinyusdz-WASM
path lands, `kotoba-lang/usd`'s ASCII round-trip logic can be reused as the fast/simple case
inside it rather than being thrown away.

## Consequences

- (+) Closes ADR-0044's long-standing "glTF loader: export-only stub" gap.
- (+) Removes a real duplicated-implementation risk (GLB codec logic drifting apart between
  `gltf` and `vrm`).
- (+) Gives non-humanoid characters (this org's own `kami-gen-*` penguin test case) a real,
  designed path that doesn't fight VRM's humanoid contract.
- (+) USD/MaterialX interchange becomes real (parse, not just emit) without any Rust or
  Emscripten dependency, usable immediately by `kami-gen-*` and render-IR tooling.
- (−) The tagged-point convention (#1) is a **runtime-checked discipline, not a compile-time
  guarantee** — it can still be bypassed by a function that skips validation. This ADR accepts
  that honestly rather than overclaiming Rust-`euclid`-equivalent safety; the `clj-kondo` rule
  is the only static backstop, and it is best-effort (literal-shape detection, not full
  dataflow analysis).
- (−) `kotoba-lang/usd`'s ASCII-only scope means binary `.usdc`/`.usdz` files from external
  DCC tools still need `kami-usd`'s (not-yet-landed) tinyusdz path — this ADR does not close
  that gap, it explicitly leaves it to ADR-2605261800.
- (−) UniRig integration is GPU-inference-gated (same real-money caveat as ADR-2607051120) —
  this ADR designs the wiring, it does not authorize spending.
- **Follow-up, not blocking**: `kami-engine/CLAUDE.md`'s crate table and Rust-prohibition
  section describe the pre-`clj-wgsl-migration` state and should be corrected in a
  documentation-only commit (tracked separately from this ADR's technical decision).

## Alternatives Considered

1. **Full OpenUSD adoption as the runtime scene format**, replacing render-IR. Rejected — the
   coscientist research found real-time/WebGPU USD runtime support industry-immature (AOUSD's
   Characters/Motion/Interactivity group formed March 2026, Core Spec 1.0 ratified December
   2025); this would mean building a USD-to-WebGPU runtime from nothing, a far larger bet than
   the actual problems (missing glTF parser, no non-humanoid rig path, no space-safety
   discipline) require.
2. **Reimplement UniRig natively in kotoba-clj/wasm.** Rejected — it is a multi-billion-
   parameter transformer; there is no realistic path to a WASM-native reimplementation, and
   this org already has a working GPU-inference orchestration layer (`cloud-murakumo`) built
   for exactly this shape of external-model integration.
3. **Adopt Rust `euclid`-style phantom types after all, scoped only to a small new geometry
   crate, as a deliberate one-off exception to the no-Rust CI guard.** Rejected per the user's
   explicit direction for this ADR (no Rust dependency) and because it would require carving
   an exception into `kami-engine`'s own CI guard for one feature, undermining the guard's
   purpose.

## Naming correction (2026-07-05, post-ship)

`kotoba-lang/glb`, `kotoba-lang/gltf`, `kotoba-lang/usd`, `kotoba-lang/materialx` were renamed
to `kotoba-lang/org-khronos-glb`, `kotoba-lang/org-khronos-gltf`, `kotoba-lang/org-openusd`,
`kotoba-lang/org-materialx` respectively (GitHub rename, redirects preserved; `deps.edn`
`:local/root` paths and `manifest/west.yml` pins updated in the same pass). Rationale: these 4
repos implement external-standards-body formats (khronos.org, openusd.org, materialx.org), not
this org's own invented concepts (unlike `character`/`mesher`/`skeleton`/`sdf`/`scad`, which
keep their bare names) — a short generic name like `usd` risks future collision as a
language-lib slug. Reverse-domain naming for the publishing body, following the same
after-the-fact rename precedent already established in ADR-2607041500 (`com-` prefix pass over
1,027 vendor-compat repos, which explicitly permits "anyone may correct an individual repo's
name later — GitHub rename + path update"). Every mention of the old bare names in this ADR's
body above is historical (describes what was decided/built under those names at the time);
new references elsewhere in this org should use the `org-*` names.

## References

- `90-docs/adr/2607051100/1110/1120/1130-kami-gen-*` (com-junkawasaki/root) — the 4-way
  comparison and the two real bugs motivating this ADR
- `90-docs/adr/2606141500-keiei-arbor-coscientist-engine.md` (com-junkawasaki/root) —
  Generation/Reflection/Ranking/Evolution methodology applied to the research behind this ADR
- `orgs/kotoba-lang/spec/README.md` — portable (kotoba-WASM-safe) validate/explain, and its
  explicit rejection of `clojure.spec`/`malli` for portability reasons
- `orgs/kotoba-lang/usd/README.md`, `orgs/kotoba-lang/materialx/README.md`,
  `orgs/kotoba-lang/gltf/README.md`, `orgs/kotoba-lang/vrm/src/vrm/glb.cljc`
- `kami-usd/README.md`, `kami-usd-native/README.md` (this repo) — ADR-2605261800 reservation
- `gftdcojp/cloud-murakumo/resources/murakumo.edn` (`:apps :generation :functions :model3d`)
  — the function shape `:autorig` follows
- UniRig: arXiv:2504.12451 (SIGGRAPH/TOG 2025), `huggingface.co/VAST-AI/UniRig`,
  `github.com/VAST-AI-Research/UniRig`
- OpenUSD `UsdSkel`: `openusd.org/dev/api/_usd_skel__intro.html`,
  `openusd.org/dev/api/_usd_skel__schemas.html`
- Rust `euclid` phantom-typed geometry (design reference, not a dependency):
  `docs.rs/euclid`; Bevy `Transform`/`GlobalTransform` split:
  `docs.rs/bevy/latest/bevy/transform/components/struct.GlobalTransform.html`
