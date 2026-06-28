# kami-mangaka-page-clj

Work-agnostic **graphic-novel PAGE composition** (komawari コマ割り + DTP) — the
Tier-1 `mangaka` platform page layer (ADR-2606282100).

Ported from `mangaka.gftd.ai`'s `manga-layouts.ts` `GRAPHIC_NOVEL_TEMPLATES`
(left-to-right reading). It places rendered panel images into a B5 page, draws
panel frames + gutters, and overlays dialogue as speech bubbles and narration as
caption boxes — turning isolated panels into a readable page.

Generic: a `page` is just `{:layout str :panels [{:id :size :narration :dialogue}…]}`
and `img-of` maps a panel-id → image `File` (or nil → placeholder). No story,
character, or world. JVM/Java2D **headless** — no Canvas-2D, no GPU (this is page
DTP, not the engine's wgpu render path).

Sibling crates: `kami-mangaka-render-clj` (2D panel prompts) and
`kami-mangaka-scene` / `kami-mangaka-scene-clj` (3D).

## API

```clojure
(require '[kami.mangaka.page :as page])

(page/template-for 4)          ; → 4-panel %-rect layout (falls back to a grid for n>9)
(page/layout-page page)        ; → {:bleed bool :pairs [[panel [x y w h]] …]} (ネーム-driven)
(page/compose-page! page img-of "out.png")   ; → writes a B5 PNG, returns the path
```

A work supplies `page` maps from its own storyboard and a panel-id→File resolver;
`kami-app-sip-clj`'s `sip.page` is the thin facade that wires `sip.storyboard`.

## Test

```bash
bb test     # templates / layout (splash·grid·ネーム rows) / headless compose-page!
```
