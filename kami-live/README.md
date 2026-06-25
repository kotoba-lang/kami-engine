# kami-live — live show + dance scenes in CLJ/EDN

A pure-Rust domain layer that turns a `:dance/*` EDN scene into a deterministic,
beat-synced live show driving a **VRM (3D)** and/or **Live2D (2D)** performer, a
crowd, lighting, VJ visuals, reactions, and a post-fx chain — authored entirely
as data (ADR-0043/0044/0045). No GPU deps: the loader runs natively and on web,
and projects each frame to the EDN render-IR the executors already consume.

```
author.clj ─(datalevin)─▶ scene.edn ─(DanceScene::from_edn)─▶ DanceScene
DanceScene::frame(dt) ─▶ DanceFrame { render_ir, actions, live2d }
   render_ir → kami-webgpu-rs (native) / CLJS (web)   actions → host applies
```

## Authoring: the `:dance/*` scene vocabulary

| Key | Meaning |
|---|---|
| `:dance/show` | `{:bpm :stage :club\|:hall\|:festival :swing :meter [bpb bpp] :performer}` — tempo grid + venue |
| `:dance/avatar` | VRM: `{:vrm :home :scale :look-at :spring-bones :clip "name" :spring {:stiffness :drag :gravity}}`. `:spring` tunes VRMC_springBone host-side; `:look-at` is `true`/`false` or `{:target :camera \| [x y z]}` (gaze tracks the camera or a fixed point). |
| `:dance/clips` | EDN animation clips: `[{:name :duration :loop :tracks [{:bone :interp :linear\|:step\|:cubic :keys [{:t :pos :rot :scale}]}]}]` |
| `:dance/live2d` | Live2D (2D): `{:model :home :scale :physics :lipsync :params {…} :motion "name" :motions [{:name :file \| :keys [{:t :params {…}}]}]}` |
| `:dance/setlist` | `[{:title :bpm :bars\|:beats :dance :idle\|:four-on-floor\|:wota\|:kpop-point\|:shuffle\|:hold :audio :opener\|{…} :cues [{:beat≥1 :kind :drop\|:breakdown\|:callout :tag}]}]` |
| `:dance/crowd` | `{:fans :cap :pit-bias :seed}` (deterministic placement) |
| `:dance/lighting` | `[{:fixture :front-par\|:back-par\|:spot\|:blinder\|:laser\|:strobe :color :intensity :envelope :hold\|:breathe\|:ramp\|{:pulse d}\|{:strobe duty} :bars :at-bar}]` |
| `:dance/vj` | `[{:pattern :solid\|:stripes\|:pulse\|:rings\|:scope\|:noise :palette :neon-pink\|:cool-wave\|:sunset\|:monochrome\|{:primary :secondary :accent}}]` |
| `:dance/triggers` | reactions: `[{:on :drop\|:breakdown\|:callout\|:custom\|:beat\|:bar\|:phrase\|:track :tag? :every? …actions}]` — action keys `:fx`/`:sound`/`:camera`/… are free-form data |
| `:dance/post` | post chain: `[{:fx :bloom\|:outline\|:vignette\|:crt\|:color-grade\|:pixelate\|:ssao\|:dof\|:ssr\|:aces\|:film-grain\|:chromatic\|:god-rays …params}]` |

## Output: the render-IR (per frame)

`DanceScene::frame(dt).render_ir` is the EDN `kami-webgpu-rs` / web parse:
`:globals :camera :lights :materials :meshes :animations :post :camera-shot
:instances` — plus `DanceFrame.live2d` (`{:kind :live2d :model :params {…}}`) and
`DanceFrame.actions` (fired `:fx`/`:sound`/… reactions). The VRM performer is a
skinned `:meshes` entry with show-driven `:expressions`; `:camera-shot` reflects
the latest `:camera` trigger.

## Runtime API

- `DanceScene::from_edn(&str) -> Option<DanceScene>` — tolerant loader.
- `DanceScene::frame(dt) -> DanceFrame` — tick → resolve triggers → render-IR.
- `run_headless(&mut scene, frames, fps) -> RunReport` — GPU-less deterministic run.
- `lint::lint_scene(&str) -> Vec<Lint>` — flags what the tolerant loader silently
  corrects (unknown enums, out-of-range, empty setlist, dangling trigger/clip/motion
  refs, beat-0 cues, unknown post fx).
- `--bin kami-dance <scene.edn> [--frames N] [--fps F] [--emit-ir] [--lint-only]`.

## Capability crates this drives

| Crate | Provides |
|---|---|
| `kami-skeleton` | interp (Linear/Step/CubicSpline), `evaluate_blend`/`evaluate_crossfade`, `solve_ik_ccd`, `AnimationClip::retarget`, `clip_from_edn` |
| `kami-vrm` | parse / spring-bone / constraints, `ExpressionManager` (morph + material + UV + override), `FirstPersonResolver` |
| `kami-webgpu-rs` | EDN render-IR executor: `parse_render_ir` (lights / camera / env-IBL / materials / meshes / animations / post) |

Reference scene: `kami-clj-play3d/games/dance/` — exercises the whole stack,
asserted by `scene::tests::reference_scene_exercises_full_stack`.
