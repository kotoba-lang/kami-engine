# kami-app-amenominaka (天之御中)

[![CI](https://github.com/kotoba-lang/kami-app-amenominaka/actions/workflows/ci.yml/badge.svg)](https://github.com/kotoba-lang/kami-app-amenominaka/actions/workflows/ci.yml)

Omniverse Kit-equivalent app shell + extension loader — kotoba `.cljc` port
of the retiring Rust `kami-app-amenominaka` crate (ADR-2607010000).

**Status**: the original 5 `pub const` port ("R1.0 path reservation" per
ADR-2605261800) still stands as the upstream-identity contract below —
but the extension loader itself is now real. **`kotoba.amenominaka.scene`
(M0), `kotoba.amenominaka.usd-export` (M1), `kotoba.amenominaka.render-ir`
(M2), the extension loader (`kotoba.amenominaka.application` +
`kotoba.amenominaka.extension` + `kotoba.amenominaka.extensions`, M3), the
interactive UI shell (`kotoba.amenominaka.ui`, M5), glTF export
(`kotoba.amenominaka.gltf-export`, M7), a second free-fly camera mode
(M8), and camera-path recording/playback (`kotoba.amenominaka.timeline`,
M9) — all of ADR-2607100100 (`com-junkawasaki/root`) — are implemented.**

## Scene composition (`kotoba.amenominaka.scene`, M0 of ADR-2607100100)

Composes a [`bim`](https://github.com/kotoba-lang/bim) building model with
named [`kami-atmosphere-scene`](https://github.com/kotoba-lang/kami-atmosphere-scene) /
[`kami-vegetation-scene`](https://github.com/kotoba-lang/kami-vegetation-scene) /
[`kami-terrain-scene`](https://github.com/kotoba-lang/kami-terrain-scene) /
[`kami-postfx-scene`](https://github.com/kotoba-lang/kami-postfx-scene) presets
into one walkthrough-ready scene EDN. Pure CLJC, no rendering.

```clojure
(require '[bim] '[kotoba.amenominaka.scene :as amenominaka-scene])

(def building
  (-> (bim/project "Quarry Walk Lodge")
      (update :sites conj (bim/site {:id 1 :name "Site 1" :geo nil
                                      :placement :identity :buildings [...]}))))

(def scene
  (amenominaka-scene/compose {:building   building
                               :weather    "overcast"
                               :vegetation ["grass"]
                               :terrain    "plains"
                               :postfx     "nintendo"}))
;; => {:scene/building ... :scene/atmosphere ... :scene/vegetation {"grass" ...}
;;     :scene/terrain ... :scene/postfx ...}

(amenominaka-scene/valid-scene? scene) ;; => true
```

`kami-cad-import` is deliberately **not** used for the building side: its
`VehicleAssembly` model is closed over vehicle part kinds/materials and its
geometry payload is AABB-only (confirmed by reading its source) — it cannot
stand in for architectural CAD/BIM import. See ADR-2607100100 Addendum
(2026-07-09) for this correction against the original design.

## USD export (`kotoba.amenominaka.usd-export`, M1 of ADR-2607100100)

Exports a `compose` result to a viewable `.usda` (USD ASCII) text document,
via [`kotoba-lang/usd`](https://github.com/kotoba-lang/usd)'s generic
USDA-from-EDN emitter. No new rendering engine — open the output in
Blender or any USD viewer.

```clojure
(require '[kotoba.amenominaka.usd-export :as usd-export])

(spit "lodge.usda" (usd-export/scene->usda scene))
(spit "default.mtlx" (usd-export/default-material-mtlx))
```

`bim`'s `ElementGeometry` (brep/axis-sweep/mesh-ref/none, carried on every
`bim/element`) has no bridge in `bim` itself to real triangle data
(confirmed by reading `bim.cljc` in full). This namespace computes a real
box mesh only for the one case simple enough to do honestly without
building a BREP tessellator — an axis-sweep element with a rectangle
profile (the common wall/beam case) — and exports every other geometry
kind as a plain `Xform` placeholder, not a faked mesh. Environment presets
(atmosphere/vegetation/terrain/postfx) carry no placement/heightfield/
instance data in M0's scene EDN, so they're recorded as `pr-str`'d custom
attrs on an `Environment` scope rather than fabricated geometry.
`materialx.core` produces one companion neutral-gray `.mtlx` document —
not yet bound into the `.usda` stage (a documented gap, not a guessed
UsdMtlx binding schema). glTF export landed separately — see M7 below.

## glTF export (`kotoba.amenominaka.gltf-export`, M7 of ADR-2607100100)

Exports a `compose` result to a binary glTF 2.0 (`.glb`) document, via
[`org-khronos-gltf`](https://github.com/kotoba-lang/org-khronos-gltf)'s
new multi-node scene builder (`gltf/export-glb-scene-byte-seq`) — that
repo previously only ever assembled a single-node/single-mesh document,
so the general builder landed as part of this milestone too.

```clojure
(require '[kotoba.amenominaka.gltf-export :as gltf-export])

(gltf-export/scene->glb scene) ;; -> byte[] (JVM) / js/Uint8Array (cljs)
```

Mirrors `usd-export`'s exact sites → buildings → storeys → elements tree
walk and geometry-fidelity rule (real box mesh only for an axis-sweep +
rectangle-profile element, everything else a plain transform-only node —
glTF's own equivalent of USD's `Xform` placeholder) — reusing
`usd-export`'s `rectangle-axis-sweep?`/`element-kind-str` directly rather
than re-implementing them, so the two exporters can't silently drift on
which elements get real geometry. Same material simplification as USD
export: one default neutral-gray material (`bim` carries no per-material
color/PBR data).

**Coordinate system**: `bim`/USD are Z-up, but glTF 2.0's spec *requires*
Y-up — every position/normal/translation goes through `zup->yup` (a
determinant-+1 axis permutation, so winding/handedness is preserved).
**Winding**: `usd-export/axis-sweep-rectangle->box-mesh`'s own
face-vertex order turned out to be CW-from-outside, not CCW (harmless for
USD, which never asserted on winding since it doesn't ship per-vertex
normals) — `box-mesh->gltf-mesh` reverses each face before triangulating
so glTF gets both correct outward normals *and* correct front-facing
winding together.

Wired into the M5 UI shell (an "Export glTF" button next to "Export
USD"). Real-browser verified (`test/render/verify_m7_gltf.cljk`): clicks
the real button, captures the real browser download (not a mock), and
parses the downloaded bytes with `org-khronos-gltf`'s own `parse-gltf` —
confirming valid GLB magic, the expected 5-node hierarchy, exactly one
real mesh with 24 vertices (6 faces × 4, properly flat-shaded — not the
box mesh's raw 8 shared points).

## Real-time render-IR (`kotoba.amenominaka.render-ir`, M2 of ADR-2607100100)

Bridges a `compose` result into [`kotoba-lang/webgpu`](https://github.com/kotoba-lang/webgpu)'s
`kami.webgpu.ir` render-IR — the EDN a REAL, already-working browser
WebGPU executor (camera + shadow-mapped directional light + PBR
instancing) consumes. **Correction to ADR-2607100100 D5's original
premise**: `kotoba-lang/webgpu`'s renderer was NOT 2D/UI-only (that's
`dom-gpu`, a different repo) — it's a real 3D renderer already; M2's
actual new work was this EDN bridge, not the renderer.

```clojure
(require '[kotoba.amenominaka.render-ir :as render-ir])

(def ir (render-ir/scene->render-ir scene))
;; => {:globals {:sky {...} :eye [...] :target [...]} :instances [{:pos :color :size :yaw} ...]}
```

`ir/instance`'s box is **square-footprint only** (`:size [w h]` =
footprint-width × height — confirmed from `kami.webgpu.ir`'s own source).
Each `bim` axis-sweep element renders as a coarse massing block
(footprint = the sweep's run length, height = the profile's height) —
the same idiom `kami.webgpu.ir`'s own docstring uses for
`;; a building`, a legitimate early-stage massing representation, not a
mesh-accurate wall. brep/mesh-ref/no-geometry elements carry no
positional data and are skipped, not faked. A terrain preset (if
requested) adds one flat ground-plane instance tinted by the biome's
palette. Vegetation/postfx are not bridged — no placement data exists for
either in M0's scene EDN.

**Verified in a real browser**, not just structurally: `public/m2-demo.html`
(compiled via `amu compile --target wasm32-browser m2-demo`) composes the M0 sample
scene, draws one real frame via `kami.webgpu/init!`+`draw!`, and
`kbb --backend sci -cp test/render test/render/verify_m2_render.cljk` drives a full
(non-headless-shell) Chromium via Playwright — reusing the technique
`kotoba-lang/wasm-webcomponent` proved in ADR-2607078000 Addendum 8,
ported to nbb rather than copied as `.mjs` — to confirm `navigator.gpu`
actually works and capture a screenshot. The captured screenshot shows a
real shaded sky/ground/massing-block render, not a blank canvas.

M2's interactive orbit/fly camera controls are not implemented (the
render-IR carries a static framed camera only).

## Perf investigation (`kotoba.amenominaka.render-stress-demo`, M4 of ADR-2607100100)

Before speculatively building `wgsl` `@compute` GPU-side instancing (the
ADR's original M4 direction), `public/m4-stress-demo.html` +
`kbb --backend sci -cp test/render test/render/verify_m4_stress.cljk` measure whether
M2's CPU-authored instancing actually hits a wall — a grid of N walls at
several scales, drawn 60 real frames in a real browser, reporting
avg/p95/max frame time. It found one: **the real wall was in
`kotoba-lang/webgpu`'s `draw!`**, not a GPU capacity limit — it
unconditionally re-sorted/re-marshaled/re-uploaded the entire instance
buffer every frame even for a completely static scene. Fixed upstream
(`kotoba-lang/webgpu`, instance-buffer caching keyed on `(:instances ir)`
by reference) rather than pursuing `wgsl @compute`, which wouldn't have
addressed this specific bottleneck. Measured improvement: 15,000
instances 34.0ms→0.85ms/frame (~40×). This demo/harness is kept as a
reusable perf-regression check, not a one-time throwaway.

## MAX-INST correctness fix (M6 of ADR-2607100100)

M4/M5's addenda both kept listing "instances past `MAX-INST` (16,384) are
silently dropped by `draw!`" as an unresolved **correctness** gap (not a
perf one — a real risk for genuinely large, city-scale scenes). Root
cause: `kotoba-lang/webgpu`'s `draw!` allocated a fixed-size GPU instance
buffer and `(take MAX-INST raw-instances)`-truncated with no warning.
Notably, this repo's own M4 benchmark couldn't have caught it — it only
ever reported `instanceCount` (the scene's *input* count, always
correct), never what `draw!` actually uploaded.

Fixed upstream: `org-w3-webgpu` gained a `destroy-buffer!` wrapper;
`kotoba-lang/webgpu`'s instance buffer now grows (doubling) via a new
`ensure-inst-buffer!` whenever a frame's instance count exceeds current
capacity, instead of capping it. `render-stress-demo` now also reports
`instBufferCapacity`, and `verify_m4_stress.cljs` asserts it's `>= n` at
the 20,000-instance scale (past the old cap) alongside the existing perf
regression check — both green: capacity correctly grows 16,384→32,768,
and average frame time at 15,000 instances stays ~1.2ms (M4's `O(1)`
per-frame fix wasn't reintroduced, since growth only happens on a real
cache-miss frame).

## Extension loader (`kotoba.amenominaka.application`+`.extension`+`.extensions`, M3 of ADR-2607100100)

The `application`/`extension` protocols are **ported forward from
[`kotoba-lang/kami-nv-compat`](https://github.com/kotoba-lang/kami-nv-compat)'s**
`kotoba.lang.kami-nv-compat.amenominaka.{application,extension}` (a
clean-room `omni.kit.app.IApp`/`omni.ext.IExt` mirror, ADR-2605261800
D6/D10.4) **into this repo as the canonical implementation** — per that
same ADR's own N10 principle that the nv-compat facade shouldn't hold
substantive logic. The loader logic itself is unchanged: `register-
extension!`/`unregister-extension!`/`startup-all`/`update!`/`shutdown-
all` on an `IApplication`, Kahn topological sort over each extension's
`:dependencies` (parents before children on startup, reverse on
shutdown, a cycle throws), plus `parse-extension-toml` — a real
hand-rolled parser for the literal-`.toml`-compat case (`kotoba-lang/toml`
was considered but is EDN→TOML *only*, confirmed by reading its source —
it can't read a literal `extension.toml`).

`kotoba.amenominaka.extensions` registers M0 (`scene`) and M2
(`render-ir`, `:dependencies {"scene" {}}`) as `extension.edn`-manifested
extensions and starts them in dependency order:

```clojure
(require '[kotoba.amenominaka.extensions :as extensions])

(extensions/load!)
;; => {:app #object[...] :order ["scene" "render-ir"] :log [[:startup "scene"] [:startup "render-ir"]]}
```

`extension.edn` manifests are plain EDN data (no custom reader needed —
same shape `parse-extension-toml` produces), authored in-code rather
than as separate files: this repo is a single app, not a multi-package
plugin directory a loader scans at runtime, so there's no real
filesystem-discovery use case yet to build for.

**On "extension → magatama Pregel cell"** (the upstream README's R1.4
scope item this section otherwise closes): re-reading ADR-2605261800 in
full finds **zero occurrences of either word** — the attribution doesn't
survive the source. "magatama" independently names three unrelated
systems elsewhere in this monorepo (a gftdcojp Cloudflare-Worker app
convention, an etzhayyim capital-flow actor, gftdcojp's pymagatama/keiei
daemon) with no Pregel-flavoured technical definition anywhere; the
org's real Pregel-cell system is `kotodama`'s persistent-daemon catalog
(ADR-2605192415), unrelated to extension loading. **This mapping is
retired as a concept here, not deferred as a gap** — extensions have a
real dependency-ordered lifecycle without it.

## Interactive UI shell (`kotoba.amenominaka.ui`, M5 of ADR-2607100100)

`public/shell.html` (compiled by shadow-cljs's `:shell` build,
`kotoba.amenominaka.ui/init!`) is the first real interactive surface over
M0–M2's pipeline — before M5 the only way to see a scene was a static
single-frame demo page. It's built on the org's **default UI/UX design
system** (ADR-2607022800): `shitsuke` (dual-render structure) +
`liquid-glass-ui` (material skin) + `kotoba-ui` (single require point) +
`appkit` (desktop binding — panels/toolbar/dropdowns, not touch/card-first,
so `appkit` over `uikit`). This is DOM chrome (env panel, nav bar) around a
`<canvas>` leaf that M2's `kami.webgpu` draws into directly — the same
"DOM siblings around a canvas" shape as the pre-existing `kami-engine-hud`,
since `liquid-glass-ui` is DOM-only by design (true in-canvas glass
rendering, `liquid-glass.gpu`, is explicit future work upstream, confirmed
absent by reading its own ADR/design docs before starting).

**Environment controls**: four dropdowns (Weather/Terrain/Post FX/
Vegetation) drive `kotoba.amenominaka.scene/compose` on every change,
re-bridge through `kotoba.amenominaka.render-ir/scene->render-ir`, and
redraw. Preset ids are the real shipped ones, read directly from each
`kami-*-scene` repo's own EDN rather than guessed (weather: overcast/
clear; terrain: plains/quarry/desert/tundra; vegetation: grass/fern/palm/
conifer/bush/cactus/moss; postfx: nintendo/retro/final-fantasy/
baminiku-character).

**Orbit/zoom camera**: mouse-drag orbits, wheel zooms/dollies. Camera
moves only `assoc-in` `:eye`/`:target` into the *existing* render-IR —
`:instances` is never touched on a camera-only frame, so M4's
instance-buffer cache (~40× at scale, `identical?`-keyed) stays hot during
every drag/zoom frame; only an actual preset change rebuilds `:instances`
and pays a fresh upload. A second camera mode, free-fly, was added in M8
— see below.

**USD export**: an "Export USD" button wires M1's `usd-export/scene->usda`
(using the *currently selected* presets) to a real browser download via
`Blob`+`URL.createObjectURL`.

**Two confirmed real `kotoba-ui` bugs, routed around, not patched
upstream**: `menu-select` renders an uncontrolled `<select>`/`<option>`
(no `:value`/`:key` React props — React would warn and the displayed value
wouldn't track external state changes), and `button` has no `:on-click`
support at all (only shitsuke's SSR `:act` contract, meaningless in a live
CLJS app). `preset-select`/`btn` in `ui.cljs` are small hand-rolled
controlled replacements with the *same DOM shape* (so the generated CSS
still targets them) — the exact workaround, DOM-shape-preservation
reasoning included, ported from `murakumo-studio/src/murakumo_studio/
ui.cljs`'s real, independently-reproduced fix for the identical bugs.

**Static CSS**: `scripts/gen_kotoba_ui_css.cljk` (`kbb -M:ui-css`) calls
`liquid-glass.tokens/css-variables`+`resolve-dark-tokens` and
`liquid-glass.style/component-css` — pure functions over EDN rule data,
no build step — and writes `public/vendor/kotoba-ui.css` (referenced via
a plain `<link>`, not runtime-injected), same shape as `murakumo-studio`'s
own CSS-gen script. This app is always-dark (a real-time 3D viewport has
no light-mode use case), so Tier A tokens are emitted directly rather than
gated on `prefers-color-scheme`.

**Real-browser verification** (`kbb --backend sci -cp test/render
test/render/verify_m5_ui.cljk`) drives the actual compiled `shell.html` in
a full (non-headless-shell) Chromium and asserts, against the live app —
no mocks: the env panel's four `<select>`s + `#viewport` canvas exist
after mount; a `#debug-state` DOM node (same idiom as M2/M4's `#out`,
since WebGPU canvas pixel readback was found unreliable in this Chromium
build) reflects the default preset state on load; driving a real
`page.selectOption` on the weather dropdown actually flows through
React's `onChange` → `recompute-scene!` and is reflected in
`#debug-state` — concrete behavioral proof the controlled-select
workaround works, not just that it compiles; dragging `#viewport` changes
the rendered frame, compared via two real Chromium screenshots (not JS
canvas readback); and no console errors were logged. One real,
independently-confirmed bug was found and fixed by this verification
itself: the harness's static test server (`test/render/lib/
webgpu_harness.cljs`) had no `.css` MIME-type entry, so Chromium served
`vendor/kotoba-ui.css` as `application/octet-stream` and silently refused
to apply it as a stylesheet — fixed by adding `".css" "text/css;
charset=utf-8"` to its `mime-types` map.

## Free-fly camera (M8 of ADR-2607100100)

A second camera mode alongside M5's orbit — WASD movement (relative to
the current look direction, projected onto the horizontal plane so
looking down while walking forward doesn't fly you into the ground) +
Space/Shift for straight up/down + mouse-look, toggled with the
"Camera: Orbit"/"Camera: Fly (WASD)" button. Needed **no new
`kami.webgpu.ir`/`kami.webgpu` API** — `rig->camera` was always just one
way to produce `:eye`/`:target`; `fly-eye-target` computes them directly
from a `{:pos :yaw :pitch}` state the same way, so `apply-camera!` (and
M4's instance-buffer cache) doesn't care which mode produced them.

Movement is driven by a real `requestAnimationFrame` loop reading a set
of currently-held keys (`fly-keys`), not per-keydown deltas — smooth and
framerate-independent while a key is held, self-terminating when camera
mode switches away. Toggling into fly mode hands off smoothly from
wherever the orbit camera currently is (seeds `:pos`/`:yaw`/`:pitch` from
the current `:eye`/`:target`, no jump cut).

**Real-browser verification** (`test/render/verify_m8_fly_camera.cljk`):
toggles to fly mode and back (both directions confirmed via
`#debug-state`), holds a real `w` keydown for 600ms (Playwright's
`page.keyboard.down`/`up` — genuine `keydown`/`keyup` events, not a
synthetic state mutation) and confirms `:fly :pos` actually moved along
the expected axis *and* the rendered frame visibly changed (two real
Chromium screenshots, not JS canvas readback). Along the way this found
a real timing bug the verification itself had to work around: React 18's
automatic event-handler batching means a Reagent `:on-click` `swap!`
doesn't necessarily flush to the DOM before Playwright's `.click()`
promise resolves — the toggle-back assertion polls `#debug-state`
instead of reading it exactly once immediately after the click.

## `omni.timeline` parity — camera path (`kotoba.amenominaka.timeline`, M9 of ADR-2607100100)

D7's own scoping for `omni.timeline` parity is explicit: **"minimal
keyframe/camera-path"**, not a full USD-stage animation timeline
(scrubbing arbitrary property animation, layers, etc. — a documented
non-goal, same stance M1 already takes on its MaterialX-binding gap).

A camera path is an ordered vector of keyframes,
`[{:t 0.0 :eye [x y z] :target [x y z]} ...]`, auto-spaced 2 seconds
apart. `kotoba.amenominaka.timeline` (`.cljc`, zero dependencies) is pure
data + functions — `add-keyframe`, `duration`, and `eval-at` (linear
interpolation between the two keyframes bounding a given time, clamped
outside the path's range) — portable and unit-tested on the JVM with no
platform code involved.

**"Record Keyframe"** captures wherever the camera currently is —
whether in orbit or fly mode, both just produce an `:eye`/`:target` the
same way (M8 already established this). **"Play"** drives a real
`requestAnimationFrame` loop that interpolates along the path and pushes
`:eye`/`:target` straight into the render-IR each frame — deliberately
*not* routed through `apply-camera!`/`:camera-mode`, since playback is a
transient override of whatever the camera is doing, not a mode switch;
orbit/fly state underneath is untouched and simply resumes once playback
or scrubbing stops. The scrubber (`#timeline-scrub`, a plain
`<input type="range">`) lets you manually preview any point on the path
while paused.

**Real-browser verification** (`test/render/verify_m9_timeline.cljk`):
records two keyframes at genuinely different positions (moves via a real
held `d` keydown in fly mode between them), scrubs to the exact midpoint
and confirms the render-IR's `:eye` is the arithmetic mean of the two
recorded eyes (real linear interpolation, not just a UI-state change —
confirmed to full floating-point precision), plays back over real
wall-clock time until it auto-stops at the path's end, and confirms the
rendered frame visibly changed during playback. This verification found
two real bugs along the way, neither in the app logic itself:
Playwright's `locator.fill()` does not support `type=range` inputs
(`"Malformed value"` — worked around with a direct `.value` write +
dispatched `input`/`change` events), and the same React 18 batching race
M8's verification already found recurred here for the "Record Keyframe"
button (worked around the same way — poll `#debug-state` instead of a
single post-click read).

## Scope (R1.4 deliverable)

The upstream README describes 5 reference-extension parities as the
R1.4 deliverable:

| Upstream item | Status here |
|---|---|
| `extension.toml` loader | done (`parse-extension-toml`, above) |
| Extension lifecycle (`depends_on`) | done (Kahn topo sort, above) |
| extension → magatama Pregel cell | **retired**, not implemented (see above) |
| `omni.usd` parity | done (M1 USD export) |
| `omni.kit.viewport` parity | done (M2 render-IR + real-browser walkthrough) |
| `omni.kit.app` parity | done (the loader itself, M3) |
| `omni.timeline` parity | done — minimal keyframe/camera-path (M9, `kotoba.amenominaka.timeline`; D7 itself scopes this down from a full USD-stage animation timeline) |
| `omni.replicator.core` parity | **out of scope** (synthetic-data/domain-randomization — ADR-2607100100 D6 explicit non-goal, deferred to a future ADR) |

The R1.4 gate as upstream defined it (all 5 extensions working) does not
fully close — `omni.replicator.core` is a permanent non-goal here, so it
structurally can't. All 4 extensions upstream expected this repo to
actually implement are now real.

## Maturity

| | |
|---|---|
| Role | Scene composition + USD/glTF export + render-IR bridge + extension loader + interactive UI shell (orbit + free-fly camera + camera-path playback): all implemented (M0–M3 + M5 + M7 + M8 + M9, ADR-2607100100); M4 investigated + real fix landed upstream |
| Tests | green (41 tests / 170 assertions), plus real-browser WebGPU smoke (screenshot-verified) + perf-regression checks + UI-shell interaction checks + glTF-export download/parse checks + fly-camera movement checks + camera-path playback/interpolation checks, all green on GitHub Actions macOS |
| R1.4 extension loader | implemented (this repo); all 4 required extension parities done, `omni.replicator.core` remains a permanent, explicit non-goal |
| Perf | M2's CPU-authored instancing verified performant to 20k+ instances after the M4 fix landed in `kotoba-lang/webgpu` (was a real ~29fps wall at 15k instances before) |
| UI/UX | M5+M8+M9: environment-preset controls + orbit/free-fly cameras + camera-path record/scrub/playback + USD/glTF export, on the org's default `shitsuke`+`liquid-glass-ui`+`kotoba-ui`+`appkit` stack (ADR-2607022800), real-browser interaction-verified |
| Correctness | M6: the `MAX-INST`=16,384 silent-truncation gap is fixed (`kotoba-lang/webgpu`'s instance buffer now grows on demand); real-browser-verified no truncation at 20k instances with no perf regression |
| Interop | M7: glTF export (`.glb`) alongside USD, sharing the same geometry-fidelity rule; real-browser-verified via a real download + `org-khronos-gltf`'s own parser round-trip |

## Contract

```clojure
(require '[kotoba.amenominaka :as amenominaka])

amenominaka/adr                       ;; => "ADR-2605261800"
amenominaka/phase                     ;; => "R1.0-path-reservation"
amenominaka/kami-name                 ;; => "amenominaka"
amenominaka/nv-compat-target          ;; => "Omniverse Kit (app shell + extension system)"
amenominaka/extension-manifest-format ;; => "extension.toml"

amenominaka/path-reservation
;; => {:adr "ADR-2605261800"
;;     :phase "R1.0-path-reservation"
;;     :kami-name "amenominaka"
;;     :nv-compat-target "Omniverse Kit (app shell + extension system)"
;;     :extension-manifest-format "extension.toml"}
```

No network, no I/O. Portable `.cljc` across JVM / ClojureScript / SCI /
GraalVM.

## License

Apache License 2.0.
