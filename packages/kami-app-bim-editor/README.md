# kami-app-bim-editor

BIM authoring workspace over [`kotoba-lang/bim`](https://github.com/kotoba-lang/bim).
This repo owns the **editor UI and its interaction/orchestration** only. The building
model, structural and MEP analysis, IFC (ISO 16739) import/export, drawing generation,
BCF/OpenCDE issue handling and the cloud collaboration history all live in `bim` and the
libraries it composes; this app calls them and never re-implements them.

- Page markup and stylesheet are generated from `kotoba-lang/html` + `kotoba-lang/css`
  (`src/kami/bim_editor/ui.cljk` → `public/index.html`).
- The 3D viewport is drawn through `kami.webgpu.mesh` (`kotoba-lang/webgpu`), per the
  workspace rule that all 3D goes through the kami-engine render stack.
- Nearest neighbours: `kami-app-cad` is a general CAD editor (no building semantics);
  `org-iso-16739` is the IFC schema mirror that `bim` uses for exchange. This repo is
  the only place where a *building* is authored interactively.

## What the workspace does

The single page (`public/index.html`) has a header, a left authoring rail, the WebGPU
viewport, and a right inspector. Every control is bound by `src/kami/bim_editor/app.cljk`.

| Panel | Authoring operations |
|---|---|
| Elements | wall, floor slab, gable roof, straight stair, path railing, curtain wall, door/window hosted in the selected wall |
| Families | parametric box families with types, advanced schema (parameters, lookup tables, formulas, reference planes, constraints, sketches, template), annotation families, instance placement |
| Engineering | structural roles → analytical model → loads → 2D and six-DOF 3D frame analysis; pipe/duct routing, open and closed-loop MEP network design, electrical panel and distribution analysis, MEP equipment and connectors |
| Rooms / Levels | manual and automatic rooms, level creation, per-level plan views |
| Model tree / Quantity schedule | element browser, quantity takeoff exported as CSV |
| Drawings | view range, annotations, auto-tagging, print setup, title block, drawing-set generation; export as SVG, PDF or DXF |
| Clash detection | run and export clashes (CSV) |
| Transform | move/copy/rotate/mirror/array, align, wall offset/trim/join, layers, slab shape and shaft openings, re-hosting, snapping |
| Header | undo/redo, Save/Load, project EDN import/export, IFC import/export, OpenCDE publish, BCF export, LOD stream refresh |

Interaction profiles (header select) change the keyboard chords: `revit` uses two-letter
chords (`WA` wall, `DR` door, `WN` window, `LL` level, `FL` floor, …), `archicad` and
`vectorworks` use their single-key equivalents. Escape cancels an in-progress drag.

## Persistence and exchange

- **Save / Load** keep the project in `localStorage` under `kami.bim-editor.project.v2`.
- **Export Project / Import** use the project document (`building.coordinated-bim.edn`).
  `kami.bim-editor.project` owns the document schema (`:kami/document :bim-editor-project`,
  currently version 8) and migrates every earlier version, plus a bare `bim` building
  model, on open. Invalid documents are rejected, not repaired.
- **Export IFC / Import IFC** go through `bim.integration` (SPF text).
- **Publish OpenCDE** posts the collaboration sync envelope, and optionally an IFC or
  analysis-result document, to `<base-url>/api/<org>/<repo>/design/sync` with a `CACAO`
  authorization header. The endpoint fields default to `https://itonami.cloud`; the CACAO
  is typed into the page and never stored.
- **Export BCF** writes `building.bcf.json` from the review topics.

## Source layout

| File | Role | Runs on |
|---|---|---|
| `src/kami/bim_editor/project.cljk` | project document, version migration, validation | clj + cljs |
| `src/kami/bim_editor/family_editor.cljk` | family/type creation and advanced schema validation over `bim.integration` | clj + cljs |
| `src/kami/bim_editor/interaction.cljk` | pure camera rays, AABB/plane picking, drag deltas, click/box selection, framing | clj + cljs |
| `src/kami/bim_editor/integration.cljk` | application boundary: coordinated revision, large-model streaming plan, capability readiness, IFC/drawing export, cloud workspace advance and sync request/publication packages | clj + cljs |
| `src/kami/bim_editor/ui.cljk` | the page as hiccup + a `css.core` sheet | clj (build) |
| `src/kami/bim_editor/app.cljk` | browser entry `init!`, the `state` atom, all DOM wiring and WebGPU drawing | cljs only |

Everything except `app.cljs` is portable `.cljc` and is what the tests exercise; the
browser file is glue and is deliberately kept out of the domain.

## Build, test, run

```bash
# tests (JVM, cognitect test-runner over the .cljc modules and the generated page)
kbb -M:test

# regenerate public/index.html from ui/page — do this after any change to ui.cljc
kbb -M build.cljk

# browser bundle → public/js/app.js
npm install && npm run build      # amu compile --target wasm32-browser app
```

`public/` is the deployable static site; open `public/index.html` over any static server.
WebGPU must be available in the browser. There is no WebGL fallback and `init!` has no
error handler: if `gpu/init-canvas!` rejects, the `#gpu-status` overlay simply keeps
saying "Initializing WebGPU…".

`test/kami/bim_editor/ui_test.cljk` asserts that the generated page still contains the
workspace controls it enumerates (a large fixed subset of what `app.cljs` binds). That test guards `ui.cljc`, **not** the committed
`public/index.html`, so regenerate the file whenever the page source changes (the
2026-09-06 README commit did exactly that: the committed page was missing 22 controls
the bundle already bound, including the OpenCDE and closed-loop MEP inputs).

Dependencies are pinned by `:git/sha` in `deps.edn` (`html`, `css`, `bim`,
`org-iso-16739`, `webgpu`, `org-w3-webgpu`, `expr`). The `:test` and `:cljs` aliases
override `io.github.kotoba-lang/physics` to a sibling `../physics` checkout; the override
is inert unless that library enters the graph.

## Known gaps

- The committed `public/js/app.js` is a shadow-cljs **development** build
  (`goog.DEBUG` true, devtools client included), not the `release` output that
  `npm run build` produces. Rebuild before treating `public/` as a release artifact.
- No browser end-to-end test exists yet; the WebGPU path is only covered by the pure
  picking/framing tests in `interaction_test.cljc`.
