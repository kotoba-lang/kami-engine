# kami-app-cad

Browser NURBS/CAD workspace for `kotoba-lang`. It owns the CAD *authoring UI*:
loft sections and control points, a dimensionally-constrained sketch, extruded
solids with Boolean operations, a feature history, precision snapping and
measurement, and EDN project exchange. It does **not** own the geometry kernel —
curves, sketch solving, lofting, solids and Boolean ops all come from
`kotoba-lang/kami-engine-cad` (`kami.cad`), per the canonical 3D stack rule
(`kami-app-*` orchestrates; the engine owns geometry).

Rendering goes through the shared `kotoba-lang/webgpu` pipeline
(`kami.webgpu.mesh`), not a renderer local to this app. UI markup and styling are
generated exclusively from `kotoba-lang/html` and `kotoba-lang/css` — there is no
hand-written HTML or CSS in `src/`.

Public app: <https://kotoba-lang.github.io/kami-app-cad/>

## What is in this repo

| Path | Owns |
| --- | --- |
| `src/kami/cad/project.cljk` | Project document format (`:kami/version 5`), validation, and migration from versions 1–4 and from a bare section vector |
| `src/kami/cad/dimensional_sketch.cljk` | The constrained construction sketch (3 lines, 6 constraints) that drives every loft section's width and height |
| `src/kami/cad/ui.cljk` | The page, as `html`/`css` EDN. `build.cljk` spits it to `public/index.html` |
| `src/kami/cad/app.cljk` | Browser wiring: state atom, undo/redo, command line, WebGPU upload/draw loop, localStorage persistence, EDN and CSV export |
| `test/` | `clojure.test` suites for the project format and the dimensional sketch |

The command line accepts four interaction profiles — Rhino, AutoCAD, Fusion and
Plasticity — each mapping its own aliases (`tr`/`z`/`ze` for AutoCAD, `t`/`f`/`d`
for Plasticity, …) onto the same six commands: trim, fit, reset, solve-width,
undo, redo.

Boolean operations are **scoped to axis-aligned coaxial convex-polygon prisms**,
and union/difference additionally require both solids to share an identical
cross-section profile. This is the `kami.cad` kernel's documented limit, not a UI
restriction. The viewport shows only the first result solid.

## Run

```sh
npm install
amu compile --target wasm32-browser app
```

Then open `public/index.html`. The viewport requires WebGPU; without it the app
shows a status message rather than falling back.

## Verify

```sh
kbb -M:test                 # project format + dimensional sketch
amu compile --target wasm32-browser app     # build public/js/app.js
```

## Known gap: `public/index.html` is no longer reproducible from source

`build.cljk` generates `public/index.html` from `kami.cad.ui/page`, but the two
have drifted. Measured on `148d663`:

- `public/index.html` carries **55** element ids; `ui.cljc` (since renamed `src/kami/cad/ui.cljk`) emits **28**, a strict
  subset.
- **24** of the 27 missing ids are looked up by `app.cljs` (since renamed `src/kami/cad/app.cljk`) with
  `getElementById`; **18** of those are passed straight to `.addEventListener`
  (`extrude-solid`, `apply-boolean`, `save-project`, `load-project`,
  `duplicate-section`, `delete-section`, `sketch-height`, `toggle-feature`, …).
  The remaining 6 are written to — e.g. `feature-tree` (`.-innerHTML`) and
  `measurement-result` (`.-textContent`).

So running `build.cljk` today would regenerate a page whose missing nodes make
`init!` throw on the first absent element, disabling the app. The deployed page
works because the committed artifact — not the source — is what is published:
`.github/workflows/pages.yml` uploads `public/` directly, with no build step.
Fetching <https://kotoba-lang.github.io/kami-app-cad/> on 2026-09-05 returned the
same 55 ids as the committed `public/index.html`, byte-for-byte in that set.

Reconcile by porting the extrude/Boolean/feature-tree/persistence/measurement
sections into `ui.cljk` and regenerating, rather than by hand-editing
`public/index.html` again.
