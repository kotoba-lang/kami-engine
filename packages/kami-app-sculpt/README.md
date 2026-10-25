# kami-app-sculpt

Digital-clay sculpting app for the kami stack: a WebGPU brush workspace over a
layered sculpt document, built in `.cljc`/`.cljs` and shipped as a single
static page.

This repo is the **app** — UI, keyboard interaction, project file format,
and the shadow-cljs build. The sculpting itself (mesh, brushes, layers,
remesh/repair, topology diagnostics) lives in
[`kotoba-lang/kami-engine-sculpt`](https://github.com/kotoba-lang/kami-engine-sculpt),
and the GPU mesh upload/draw path in
[`kotoba-lang/webgpu`](https://github.com/kotoba-lang/webgpu). Nothing here
re-implements geometry.

## What it does

- **Brushes** — inflate, smooth, pinch, mask, erase-mask, with radius,
  strength and stroke spacing.
- **Masking** — clear, invert, grow, shrink, blur, sharpen; the masked-vertex
  count is reported in the debug state so tests can assert on it.
- **Symmetry** — independent X / Y / Z mirroring.
- **Sculpt layers** — add, duplicate, delete, rename, move up/down, opacity,
  bake down, and toggle visibility (double-click); one active layer receives
  strokes.
- **Topology** — bake & subdivide, voxel remesh at a chosen cell size, repair,
  and boundary hole fill, each reporting its before/after counts next to the
  button.
- **Undo / redo** per stroke, with a revision counter and a save-status flag
  (`clean` / `dirty` / `saved`).
- **Project save / load and EDN import / export.**

### Interaction profiles

The header selects which editor's key layout applies. `kami.sculpt.app`
carries all three:

| profile | keys |
|---|---|
| ZBrush (default) | `B,I` inflate · `B,S` smooth · `B,P` pinch · `M` mask |
| Blender Sculpt | `I` · `S` · `P` · `M` · `E` erase mask |
| Mudbox | `1` sculpt · `2` smooth · `3` pinch · `M` mask · `E` erase mask |

ZBrush uses a two-key prefix buffer; the other two dispatch directly.
`Ctrl+Shift+N` adds a layer. Holding **Shift** temporarily switches to smooth
and restores the previous brush on release, so the selection in the panel
never changes.

## Project file

`kami.sculpt.project` owns the on-disk shape, currently **version 2**:

```clojure
{:kami/document :sculpt-project
 :kami/version  2
 :project/id "head" :project/name "Head"
 :project/sculpt      <sculpt document from kami-engine-sculpt>
 :project/brush       {:mode :inflate :radius 0.6 :strength 0.12 :spacing 0.12}
 :project/symmetry    [:x]
 :project/interaction {:profile :zbrush}
 :project/strokes     0}
```

`open` migrates then validates, and throws rather than returning a partly
formed project. Two inputs migrate: a v1 project (gains
`:project/interaction` and `:project/strokes`), and a bare sculpt document
from the engine (wrapped with defaults). Anything else is rejected.

## Namespaces

| file | role |
|---|---|
| `src/kami/sculpt/project.cljk` | project document, versioning, migration, validation |
| `src/kami/sculpt/ui.cljk` | the page — hiccup via `html.core`, styles via `css.core` |
| `src/kami/sculpt/app.cljk` | browser app: state atom, events, shortcuts, GPU upload |

`ui.cljc` and `project.cljc` are portable `.cljc` and carry no browser
dependency, so both are exercised by the JVM test run.

## Build and run

```bash
kbb -M:test                        # project round-trip / migration / rejection
kbb -M build.cljk                   # regenerate public/index.html from ui/page
npm run build                          # amu compile --target wasm32-browser -> public/js/app.js
```

`public/index.html` is **generated** from `kami.sculpt.ui/page` — edit the
hiccup, not the HTML. The build is idempotent, so
`kbb -M build.cljk && git diff --exit-code public/index.html` is a usable
check that the generator and the shipped page still agree; they had drifted
apart once, and regenerating from the stale generator would have deleted 22
controls that `app.cljs` binds handlers to. Serve `public/` over http (WebGPU needs a secure
context; `localhost` counts) and open it in a WebGPU-capable browser. Without
WebGPU the viewport shows a status message instead of silently rendering
nothing.

`#debug-state` carries a JSON snapshot — vertex/triangle counts, masked
vertices, mode, symmetry, profile, layer names, remesh/repair/hole-fill
results and topology diagnostics — so an end-to-end test can read what the
app believes rather than screenshotting it.

## License

Apache-2.0. See [LICENSE](LICENSE).
