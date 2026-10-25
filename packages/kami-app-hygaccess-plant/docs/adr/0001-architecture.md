# ADR-0001 — kami-app-hygaccess-plant architecture

- Status: Accepted
- Date: 2026-07-18
- Context tags: CFD, finite-volume, mixing-tank, cae-solver-dispatch, render-IR

## Decision

Simulate the sodium-hypochlorite dilution/mixing step for
`int.hygaccess.water-purification-drops` — ONE illustrative process (see
README.md "Scope") — as a real finite-volume CFD scenario built entirely on
existing `kami-engine`-stack pieces: [`kotoba-lang/nagare`](https://github.com/kotoba-lang/nagare)
for the incompressible-NS + scalar-transport solve, dispatched through
[`kotoba-lang/kami-engine-cae-solver`](https://github.com/kotoba-lang/kami-engine-cae-solver)'s
shared `cae.solver/solve` multimethod contract (`[:solver :kind] =
:hygaccess-mixing-tank`), with a visualization layer authored to
`kotoba-lang/webgpu`'s `kami.webgpu.ir` render-IR shape. No new physics engine,
no new Rust crate, no bespoke renderer — per ADR-2607102200 (repo-wide "3D は
すべて kami-engine を使う").

## Decision 1: 2-D vertical cross-section, not a resolved 3-D vessel

`nagare.mesh/block-mesh` (as pinned) builds an unstructured polyMesh from a
rectangular `nx x ny` block with unit depth and exactly four named boundary
patches (`:left :right :bottom :top`) — there is no 3-D mesh generator, no
cylindrical/curved-wall mesh, and no way to place an arbitrary interior
obstacle (an impeller blade, a baffle) in the current API. Modeling the tank
as its 2-D vertical cross-section (documented in `tank.cljc` as
`:tank/cross-section-m` + `:tank/depth-m` for a volume estimate) is therefore
not a simplification chosen for convenience over a more-accurate option this
repo declined — it is the actual ceiling of what the pinned nagare version can
mesh. A genuine 3-D cylindrical vessel with a rotating impeller is out of
scope for this build; see README.md "Scope" for why that is an acceptable
illustrative bound rather than a shortcut around real capability.

## Decision 2: closed recirculating cavity, not an open inlet+outlet pair

`tank.cljc` documents an inlet (stock+water charge nozzle) and an outlet
(drain to the filling line) as domain data. The CFD solve itself, however,
models a **closed batch-agitation phase**: all four mesh boundary patches are
no-slip walls except the top face, which carries a **tangential** (not
wall-normal) fixed-value velocity representing the agitator/inlet-jet
momentum — exactly `nagare.demo`'s own Ghia-validated lid-driven-cavity
topology, just relabeled physically.

Two reasons, not one:

1. **Numerical safety.** An open inlet+outlet pair needs a hand-derived
   mass-conserving BC combination (e.g. `fixedValue` velocity at the inlet,
   `zeroGradient` velocity + `fixedValue` pressure at the outlet) that nagare's
   own test suite does not cover — the ONLY topology nagare's PISO/SIMPLE
   coupling is actually verified against (Ghia centreline comparison) is the
   closed cavity. For an illustrative build with no CFD-verification budget of
   its own, reusing the one topology upstream has already validated is a much
   lower-risk choice than authoring an unvalidated open-boundary combination
   and hoping it converges.
2. **Physical fit.** The tank is explicitly a *small-batch* mixing tank. Batch
   dilution is naturally a closed-vessel process: charge the tank (concentrated
   stock poured in, water already filled), agitate with everything else
   closed, THEN drain to the filling line afterward. The outlet is not
   simultaneously open during agitation in real batch practice either — so the
   closed-cavity CFD topology is not just numerically convenient, it is the
   physically correct model of the phase actually being simulated.

Because the top face's imposed velocity is purely tangential, the
wall-normal (through-boundary) flux is zero at every patch — the cavity is
exactly closed, so NaOCl mass is conserved by construction (confirmed
empirically: mass-conservation defect ~1e-12, floating-point noise). No
hand-derived open-boundary mass balance was needed.

## Decision 3: hand-assembled transient scalar-transport loop

`nagare.transport/solve` (as pinned, SHA `c51fc7d`) solves the STEADY scalar
transport equation `div(phi,phi_scalar) - laplacian(Gamma,phi_scalar) = 0` for
a fixed flow field directly — there is no landed transient (ddt) scalar solve
in nagare at this pin (its own roadmap lists scalar transport as "S2 in
progress" with non-orthogonal correction / turbulence / case-dir IO still
pending; a transient scalar API was not among what actually landed). The task
this repo implements explicitly wants homogeneity CoV to visibly fall
tick-by-tick as agitation proceeds, which needs a genuine transient loop, not
one steady evaluation.

`mixing.cljc`'s `transient-scalar-step` assembles its own backward-Euler ddt
term on top of `nagare.fvm/convection-diffusion`'s steady spatial operator:
`V_cell/dt` added to the diagonal, `V_cell/dt * value_old` added to the
source — literally the same formula `nagare.fvm/momentum` already applies
internally for the vector (velocity) ddt term (see that function's own
"transient (backward Euler)" block), just composed at the call site for a
scalar field from nagare's already-exposed `fvm`/`linsolve` primitives
(`fvm/convection-diffusion`, `linsolve/bicgstab`, `field/with-values`) rather
than reaching into nagare's internals or re-deriving finite-volume math from
scratch. This is the one place this repo extends past what nagare exposes as
a single function call — everything else is nagare's own API used as
documented.

## Decision 4: no `kotoba-lang/webgpu` `deps.edn` dependency (confirmed, not assumed)

`kami.webgpu.ir` (render-IR shape) and `kami.webgpu` (the browser
`init!`/`draw!` executor, with automatic WebGPU-first / WebGL-2.0-fallback
selection via its internal `kotoba.webgl` dependency) live in
`kotoba-lang/webgpu`. This repo's `render.cljc` builds EDN structurally
identical to `kami.webgpu.ir`'s documented contract, but does **not** declare
`io.github.kotoba-lang/webgpu` as a `deps.edn` git dependency, because it does
not resolve outside the west-managed `kotoba-lang` sibling-checkout workspace.
Confirmed empirically before deciding this, not assumed from reading
`deps.edn`:

```
$ cat deps.edn
{:paths ["src"]
 :deps {io.github.kotoba-lang/webgpu
        {:git/url "https://github.com/kotoba-lang/webgpu.git"
         :git/sha "247ae4b585dfa911693e9f258b703bf735045480"}}}
$ kbb -Stree
Cloning: https://github.com/kotoba-lang/webgpu.git
Checking out: https://github.com/kotoba-lang/webgpu.git at 247ae4b...
Error building classpath. Local lib io.github.kotoba-lang/expr not found:
~/.gitlibs/libs/io.github.kotoba-lang/webgpu/expr
```

`kotoba-lang/webgpu`'s own `deps.edn` declares ~20 sibling packages
(`org-w3-webgpu`, `wgsl`, `expr`, `sprite-gpu`, `sprite2d`, `sky`, `shaders`,
`render`, `render-shaders`, `scene2d`, `gpu`, `webgl`, `dance`, `physics`,
`fsm`, `netsync`, `pipelines`, `level`, `host`, `playwright`,
`cartpole-math`) purely via `:local/root "../X"`, with no git-coordinate
alternative — it is only resolvable as a sibling checkout inside
`orgs/kotoba-lang/*`. This matches every actual `kami-app-*` repo surveyed
while building this one: none of `kami-app-car-sim`, `kami-app-giemon-factory`,
or `kami-app-sarutahiko-factory` declare `io.github.kotoba-lang/webgpu` as a
standalone git dependency either — all three are explicitly domain-data-only
ports with wgpu rendering NOT carried over, for exactly this reason (their own
READMEs say so). This repo instead authors its render-IR to the documented
shape by hand (`render.cljc`), proves it well-formed and deterministic with
portable JVM tests (no browser needed for that), and documents the actual
`kami.webgpu/init!`+`draw!` wiring point in README.md "Render status" for use
from inside the `kotoba-lang` workspace, where `kami.webgpu.ir` would resolve.
The render step is therefore **wired but unverified**, not claimed as a
working on-screen visualization — see README.md "Render status" for the full,
honest accounting of what actually has a test proving it versus what does
not.

## Decision 4 addendum (2026-07-18): real WebGPU rendering verified — this was a workspace-layout gap, not a real bug

Re-investigated Decision 4's "wired but unverified" conclusion on the
hypothesis that the original standalone-clone test (a lone `kotoba-lang/
webgpu` checkout with none of its ~20 `:local/root` siblings present) may
have been testing the wrong thing — this workspace has a purpose-built tool,
`west`, for checking out an entire interdependent repo group into the
correct relative sibling layout, and `kotoba-lang/kami-app-amenominaka`
already proved `kami.webgpu` works from inside that layout for a different
`kami-app-*` repo. Set up a proper multi-repo workspace
(`git worktree` outside the superproject + `west init -l manifest` +
`west update` for `webgpu` and its full sibling set — 21 direct
`:local/root` deps plus `host`'s own further `kami-engine-sdk`/`physics-2d`,
24 checkouts total) and re-tested from there. Confirmed **empirically**, not
assumed:

1. `kbb -Stree` against `kotoba-lang/webgpu`'s own `deps.edn` resolves
   cleanly with all 21+2 siblings present as flat checkouts under
   `orgs/kotoba-lang/*` — no `Local lib ... not found` error. The original
   error was real but was testing an intentionally incomplete environment
   (webgpu alone, no siblings), not `kotoba-lang/webgpu` itself.
2. Adding `io.github.kotoba-lang/webgpu {:local/root "../webgpu"}` to this
   repo's own `deps.edn` (as a NEW `:cljs` alias's `:extra-deps`, not the
   base `:deps` — see `README.md` "Render status" for why) hit exactly ONE
   real, narrow conflict: `kami-engine-cae-solver` (pinned `37bbc49`, in the
   base `:deps`) itself depends on `io.github.kotoba-lang/physics` via a
   `:git/sha` coordinate, while `kotoba-lang/webgpu` depends on the SAME lib
   coordinate via `:local/root "../physics"`. `tools.deps` cannot compare a
   `:local/root` manifest against a `:git/sha` manifest for one coordinate
   (`Unable to compare versions for io.github.kotoba-lang/physics ...`).
   Fixed with a one-line `:override-deps {io.github.kotoba-lang/physics
   {:local/root "../physics"}}` in the `:cljs` alias — resolved entirely
   inside this repo's own `deps.edn`, no change to `kotoba-lang/webgpu` or
   `kami-engine-cae-solver` (both shared, high-blast-radius repos) needed or
   made.
3. With that one fix, `kbb -Stree -A:cljs` resolves the full classpath
   cleanly, `amu compile --target wasm32-browser render-demo` compiles the new
   `render_demo.cljs` entry point (86 files, only pre-existing
   `:infer-warning`s inside `kotoba-lang/webgpu` itself, zero errors), and
   `kbb --backend sci -cp test/render test/render/verify_render.cljk` (the harness
   ported from `kami-app-amenominaka`) drove a full headless Chromium on
   macOS to a real WebGPU-drawn frame: `#out` reported `"ok
   cov=0.000018298324379849743 instances=576"`, and the captured screenshot
   shows the tank's 24×24 finite-volume cross-section as a real isometric
   diamond of colour-mapped instanced boxes against the sky background — not
   a blank canvas. The reported `cov` value matches this repo's own
   documented golden-test `:mixing-homogeneity-cov-pct`
   (`1.8298324379849743E-5`%, see README.md "What the golden test observed")
   bit-for-bit, and `576` instances matches the 24×24 mesh exactly —
   confirming the browser path drew the REAL solve output end-to-end
   (`cae.solver/solve :hygaccess-mixing-tank` → `render/tank-render-ir` →
   `kami.webgpu/init!`+`draw!`), not a stub or fixture.
4. Verified this did not regress the base build: `kbb -M:test` (22
   tests / 63 assertions) and `kbb -M:lint` (0 errors) both still pass
   unchanged, and `kbb -Stree` (no alias) still resolves without
   touching `../webgpu` at all — the base `:deps` map is untouched;
   `io.github.kotoba-lang/webgpu` lives only in the new `:cljs` alias.

**Conclusion**: the original "wired but unverified, `kotoba-lang/webgpu`
cannot be used as a standalone dependency" finding was correct as stated —
`kotoba-lang/webgpu` genuinely does not resolve from a lone clone — but the
inference that this therefore meant the render step could not be verified
at all was a workspace-layout gap in the original investigation, not a
limitation of `kotoba-lang/webgpu` or a reason to leave the render step
permanently unverified. Real rendering is now verified end-to-end. No fix
was needed to `kotoba-lang/webgpu` itself; the only change was this repo's
own `deps.edn` gaining a `:cljs` alias (base `:deps`/`-M:test`/`-M:lint`
unaffected) plus the render-demo/harness/CI files described in README.md
"Render status".

## Decision 5: effective (turbulent) viscosity, not molecular

A real agitated small-batch tank (0.5 m/s drive, 0.6 m length scale, water's
molecular kinematic viscosity ~1e-6 m^2/s) sits at Re ~ 3x10^5 — solidly
turbulent. `nagare` does not yet have a landed turbulence closure (its own
roadmap marks RANS k-epsilon/k-omega-SST as pending "S3"). Running nagare's
laminar upwind solver at that literal molecular Reynolds number would not
crash (the upwind convection scheme is unconditionally bounded — a diagonally
dominant M-matrix — regardless of nominal Re), but it WOULD silently
misrepresent unresolved turbulence as a fully-resolved laminar flow, which is
worse than not simulating momentum transport at all. `converge-flow` instead
takes a representative EFFECTIVE (eddy) viscosity as a parameter — the
standard reduced-order engineering shortcut for exactly this situation — set
by default to `5.0e-3 m^2/s`, chosen so the solve runs at Re=60 (against the
default 0.5 m/s / 0.6 m tank), well inside the regime `nagare.demo`'s own
Ghia-cavity verification (Re=100) is actually validated at. The scalar
transport's turbulent diffusivity (`:process/turbulent-diffusivity-m2-s`,
`3.0e-3 m^2/s`) is set on the same footing (order-of-magnitude eddy
diffusivity, not NaOCl's molecular diffusivity in water, ~1.5e-9 m^2/s) —
turbulent mixing, not molecular diffusion, is what actually homogenizes an
agitated tank, and this build says so explicitly rather than quietly using a
number that would take geological time to visibly homogenize anything.

## Decision 6: license — Apache-2.0, not AGPL-3.0

`cloud-itonami-hygiene-access` (the actor this visualizes a piece of) uses
`AGPL-3.0-or-later`, but that is the `cloud-itonami` org's convention, not
`kotoba-lang`'s — confirmed by checking rather than assumed. `nagare` and
`kami-engine-cae-solver` themselves ship no LICENSE file (GitHub reports
`license: null` for both via the API). Surveying real `kami-app-*` and
`kami-engine*` repos with actual content (`kami-engine`, `kotoba-lang/webgpu`,
`kami-app-sarutahiko-factory`, `kami-app-car-sim`, `kami-app-shibuya`,
`kami-app-amenominaka`, `kami-app-tatekata`, `kami-app-fixtures`) shows
**Apache-2.0** as the dominant real convention (54 of 300+ sampled
`kotoba-lang` repos are Apache-2.0, the largest single license after "none";
every populated `kami-app-*`/`kami-engine*` sibling checked is Apache-2.0).
This repo uses Apache-2.0 to match.

## Module map (mirrors the responsibility split in kotoba-lang/nagare and kami-engine-cae-solver)

```
tank.cljc      vessel/tank domain data (:tank/...) + pure derivation fns —
               cross-section area, batch volume, mesh-params, agitation drive
process.cljc   batch/process parameters (:process/...) — target/efficacy-window
               (cited from cloud-itonami-hygiene-access), default-tank/-process
mixing.cljc    mesh + BC construction -> nagare PISO flow solve -> transient
               scalar-transport loop (Decision 3) -> homogeneity CoV metric
solve.cljc     cae.solver/solve :hygaccess-mixing-tank registration (Decision-free:
               a thin multimethod registration, matching cae.high-fidelity's own style)
render.cljc    render-IR construction mirroring kami.webgpu.ir's contract (Decision 4)
```

The invariant: `mixing.cljc` is the only namespace that calls into `nagare`;
`solve.cljc` is the only namespace that calls into `cae.solver`; `render.cljc`
has zero dependency on either (it only consumes the plain result map
`mixing.cljc` produces). Each seam can be tested and reasoned about
independently, mirroring `nagare`'s own `mesh -> field -> fvm -> linsolve ->
solver` layering discipline.

## Decision 7 (2026-07-19 addendum): stepwise `init-state`/`step` API for closed-loop control callers

Added `mixing/init-state`, `mixing/step`, and `mixing/sensor-reading`
alongside (not replacing) `run-mixing-scenario`, for `cloud-itonami/
cloud-itonami-hygiene-access`'s real PID + ISA-18.2-alarm closed control
loop (that repo reads a sensor value from each tick, computes an actuator
command, and needs the NEXT tick's boundary condition to genuinely reflect
it — a monolithic "run the whole thing, get one final number" entrypoint
cannot do that).

**The design question this decision actually answers**: `run-mixing-scenario`
converges the agitation-driven flow field EXACTLY ONCE, then holds the
resulting face-flux `phi` frozen across all 40 scalar-transport ticks — a
quasi-steady-flow assumption. For a genuine per-tick RPM setpoint to mean
anything physically, `step` has to decide what to do with that frozen-flow
assumption when the setpoint actually changes mid-run.

**Decision: re-solve PISO, warm-started, only when the commanded drive
velocity actually changes.** `step` converts the RPM setpoint to a drive
velocity via real agitator kinematics (`tank/agitator-rpm->drive-velocity-
m-s`, `v = pi * D * N/60` — NOT an arbitrary scale factor; see `tank.cljc`).
If that velocity differs from the previous tick's (beyond floating-point
round-trip noise), `mixing.cljc`'s private `reconverge-flow` re-runs
`nagare.solver/advance` — WARM-STARTED from the previous `{:U :p :phi}`,
never from rest — to the new steady flow field before that tick's
`transient-scalar-step` runs. A held setpoint reuses the already-converged
flow untouched.

Two things this buys, both empirically verified (not asserted):

1. **Real physics, not a cosmetic parameter.** A higher commanded RPM
   produces a genuinely recomputed higher face-flux magnitude, which shows
   up as a measurably faster-homogenizing CoV trajectory — `mixing_test.cljc`
   `higher-rpm-mixes-faster-than-lower-rpm` asserts strict CoV dominance at
   every tick between a 90 RPM and a 15 RPM run from the same initial state,
   and that the 90 RPM run ends up at least 100x better-mixed after the same
   number of ticks. If the RPM setpoint were ever accidentally left
   unwired from the boundary condition, this test fails loudly instead of
   silently passing on a cosmetic no-op parameter.
2. **Physical consistency with the already-verified monolithic path.**
   Because a HELD setpoint skips the re-solve entirely, driving `step` 40
   times at a CONSTANT RPM equal to the default tank's own implicit
   agitation-drive velocity reproduces `run-mixing-scenario`'s own
   `:mixing-homogeneity-cov-pct-history` **bit-for-bit** (`mixing_test.cljc`
   `stepwise-with-constant-rpm-matches-monolithic-scenario` — measured
   `diff = 0.0` while authoring this, not just "within tolerance"). The
   stepwise refactor is therefore not a parallel, disconnected
   reimplementation; a constant-command stepwise run IS the monolithic run,
   by construction.

Warm-starting (rather than a from-rest `converge-flow` call on every
setpoint change) is not only a performance choice — it is the physically
correct behaviour: a real agitated fluid that already has momentum doesn't
reset to stationary just because the setpoint moved, it relaxes from
wherever it already is. It also keeps a per-tick RPM change cheap: only the
delta between the old and new boundary condition needs to relax (a handful
of PISO steps, empirically), not the ~300 steps a from-rest solve needs —
measured while authoring this: an 8-tick stepwise run on a 10x10 mesh at two
different constant RPMs (15 and 90) completed in under 1 second combined.

`sensor-reading`'s `:temperature-proxy-c` is explicitly NOT a simulated
temperature — this model has no energy equation anywhere. It is a
flow-speed-derived monotonic stand-in for a real thermal sensor reading in
the control-loop repo, spelled `-proxy-` (not `-c`) specifically so a
downstream reader cannot mistake it for a resolved thermal simulation, per
this repo's existing honesty discipline (see README.md "Where the numbers
come from").

No physical actuation pathway exists anywhere in this codebase or is
intended: "closed-loop" here means the next CFD tick's boundary condition is
a real function of the previous tick's simulated state and a computed
command, entirely inside this repo's own digital twin.
