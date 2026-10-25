(ns kami.app-suji.theme
  "App CSS for kami-app-suji — the shell DADS does not ship, written to the
  `--hig-*` token contract.

  `jp-go-dds.tokens/bridge-css` redefines every `--hig-*` property on top of DADS
  primitives, so everything here follows the design system unmodified. Nothing in
  this file states a colour, a font size or a radius of its own: an app that
  re-derives a token owns a second design system whether it meant to or not.

  DADS has no app shell, no editor frame and no split viewport, which is what this
  layer is; it is intentionally small, and it wins over library CSS without any
  specificity fight because library rules ship inside `@layer` and app CSS does not.

  The 3-D viewport's own colours are NOT here — they are linear RGB handed to a GPU
  in `kami.app-suji.scene`, never a stylesheet, so the token contract does not reach
  them and pretending it did would be theatre."
  (:require [jp-go-dds.tokens :as tokens]))

(def shell-css
  "grid, spacing and the canvas frame — all in bridged tokens."
  "
.suji-shell { display:grid; grid-template-rows:auto 1fr; min-height:100dvh; }
.suji-main  { display:grid; grid-template-columns:minmax(0,1.15fr) minmax(320px,0.85fr);
              gap:var(--hig-spacing-4); padding:var(--hig-spacing-4); align-items:start; }
@media (max-width:900px) { .suji-main { grid-template-columns:1fr; } }
.suji-stage { position:relative; aspect-ratio:4/3; min-height:320px; width:100%;
              border-radius:var(--hig-radius-md); overflow:hidden;
              background:var(--hig-color-background-secondary); }
.suji-stage > canvas { display:block; width:100%; height:100%; }
.suji-backend { position:absolute; inset-block-start:var(--hig-spacing-2);
                inset-inline-start:var(--hig-spacing-2);
                font-size:var(--hig-text-caption2-font-size);
                color:var(--hig-color-label-secondary); }
.suji-readout { display:grid; gap:var(--hig-spacing-3); }
.suji-figure  { font-size:var(--hig-text-title1-font-size); font-weight:600;
                color:var(--hig-color-label-primary); }
.suji-unit    { font-size:var(--hig-text-footnote-font-size);
                color:var(--hig-color-label-secondary); }
.suji-control { display:grid; gap:var(--hig-spacing-1); }
.suji-control input[type=range] { width:100%; accent-color:var(--hig-color-accent); }
.suji-bar     { height:var(--hig-spacing-2); border-radius:var(--hig-radius-xs);
                background:var(--hig-color-fill-tertiary); overflow:hidden; }
.suji-bar > i { display:block; height:100%; }
.suji-swatch  { display:inline-block; inline-size:var(--hig-spacing-3);
                block-size:var(--hig-spacing-3); border-radius:var(--hig-radius-xs);
                vertical-align:-0.1em; margin-inline-end:var(--hig-spacing-1); }
.suji-note    { font-size:var(--hig-text-footnote-font-size);
                color:var(--hig-color-label-secondary); }
")

(def app-css*
  "bridge first, then the shell — the bridge has to define the tokens the shell reads."
  (str tokens/bridge-css "\n" tokens/a11y-css "\n" shell-css))
