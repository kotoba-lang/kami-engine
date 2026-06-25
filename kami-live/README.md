# kami-live — live show + dance scenes in CLJ/EDN

A pure-Rust domain layer that turns a `:dance/*` EDN scene into a deterministic,
beat-synced live show driving a **VRM (3D)** and/or **Live2D (2D)** performer, a
crowd, lighting, VJ visuals, reactions, and a post-fx chain — authored entirely
as data (ADR-0043/0044/0045). No GPU deps: the loader runs natively and on web,
and projects each frame to the EDN render-IR the executors already consume.

```
author.clj ─(datalevin)─▶ scene.edn ─(DanceScene::from_edn)─▶ DanceScene
DanceScene::frame(dt) ─▶ DanceFrame { render_ir, actions, live2d, audio, sounds }
   render_ir → kami-webgpu-rs (native) / CLJS (web)   actions → host applies
```

## Authoring: the `:dance/*` scene vocabulary

| Key | Meaning |
|---|---|
| `:dance/show` | `{:bpm :stage :club\|:hall\|:festival :swing :meter [bpb bpp] :performer}` — tempo grid + venue |
| `:dance/avatar` | VRM: `{:vrm :home :scale :look-at :spring-bones :clip "name" :spring {…} :expressions {<name> {:from :cheer\|:beat\|:blink :gain}} :voice {:phonemes [{:at-beat :vowel :a\|:i\|:u\|:e\|:o :dur}]} :vmd "motion.vmd"}`. `:expressions` drives the VRM face from the show (smile-on-cheer / lip-sync-on-beat / blink); `:voice` is a looping vowel timeline driving the mouth (a-i-u-e-o → aa/ih/ou/ee/oh), overriding beat `:aa`; `:vmd` is an MMD motion the host loads via `kami_skeleton_scene::vmd_to_clip`. |
| `:dance/camera` | camera rig framing the performer: `{:offset [dx dy dz] :look [lx ly lz] :fov :shots [{:at-bar :offset :look}]}` — eye = performer + `:offset`. `:shots` is a bar-keyed camera-work choreography (wide → dolly-in → side → pull-back), dollied between shots with a smoothstep. |
| `:dance/stage` | static set pieces dressed into the render-IR `:instances`: `{:props [{:kind :led-wall\|:riser\|:truss\|:speaker :pos [x y z] :size [w h] :color [r g b] :emissive}]}` |
| `:dance/audio` | Web-Audio sound bank (kami.audio EDN recipes): `{:bank {<name> {:wave "sine"\|"square"\|"triangle"\|"sawtooth" :freq :to :dur :gain}}}` — drum/bass/`:sound` cues resolve to these recipes → per-frame `DanceFrame.sounds` (no asset files). |
| `:dance/clips` | EDN animation clips: `[{:name :duration :loop :tracks [{:bone :interp :linear\|:step\|:cubic :keys [{:t :pos :rot :scale}]}]}]` |
| `:dance/live2d` | Live2D (2D): `{:model :home :scale :physics :lipsync :params {…} :motion "name" :motions [{:name :file \| :keys [{:t :params {…}}]}]}` |
| `:dance/setlist` | `[{:title :bpm :bars\|:beats :dance :idle\|:four-on-floor\|:wota\|:kpop-point\|:shuffle\|:hold\|:bounce\|:sway\|:spin\|:headbang\|:clap :audio :opener\|:ballad\|:encore\|{…} :cues [{:beat≥1 :kind :drop\|:breakdown\|:callout :tag}]}]` |
| `:dance/crowd` | `{:fans :cap :pit-bias :seed}` (deterministic placement) |
| `:dance/lighting` | `[{:fixture :front-par\|:back-par\|:spot\|:blinder\|:laser\|:strobe :color :intensity :envelope :hold\|:breathe\|:ramp\|{:pulse d}\|{:strobe duty} :bars :at-bar}]` |
| `:dance/vj` | `[{:pattern :solid\|:stripes\|:pulse\|:rings\|:scope\|:noise :palette :neon-pink\|:cool-wave\|:sunset\|:monochrome\|{:primary :secondary :accent}}]` |
| `:dance/triggers` | reactions: `[{:on :drop\|:breakdown\|:callout\|:custom\|:beat\|:bar\|:phrase\|:track :tag? :every? …actions}]` — action keys `:fx`/`:sound`/`:camera`/… are free-form data. `:fx` particle bursts: `:confetti\|:fireworks\|:pyro\|:sparkle\|:sparkle-blast\|:laser\|:smoke\|:bubbles\|:hearts\|:stars\|:snow\|:petals\|:glitter\|:embers`. |
| `:dance/post` | post chain: `[{:fx :bloom\|:outline\|:vignette\|:crt\|:color-grade\|:pixelate\|:ssao\|:dof\|:ssr\|:aces\|:film-grain\|:chromatic\|:god-rays …params}]` |

## Output: the render-IR (per frame)

`DanceScene::frame(dt).render_ir` is the EDN `kami-webgpu-rs` / web parse:
`:globals :camera :lights :materials :meshes :animations :post :particles :sounds
:camera-shot :instances` — plus `DanceFrame.live2d` (`{:kind :live2d :model :params
{…}}`), `DanceFrame.actions` (fired `:fx`/`:sound`/… reactions), and
`DanceFrame.audio` / `DanceFrame.sounds` (synthesised audio cues + their
`kami.audio` EDN recipes for a Web Audio host). The VRM performer is a skinned
`:meshes` entry with show-driven `:expressions`; the crowd + `:dance/stage` props
are `:instances`; `:camera-shot` reflects the latest `:camera` trigger.

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
| `kami-skeleton` | interp (Linear/Step/CubicSpline), `evaluate_blend`/`evaluate_crossfade`, `solve_ik_ccd`, `AnimationClip::retarget` |
| `kami-skeleton-scene` | `clip_from_edn` (`:dance/clips` → clip); **MMD import**: `vmd_to_clip` (`.vmd` motion → clip, Shift-JIS humanoid map), `pmx_to_model` (`.pmx` → mesh + bones + morphs + textures), `pmx_to_skeleton` (→ `kami_skeleton::Skeleton`) |
| `kami-vrm` | parse / spring-bone / constraints, `ExpressionManager` (morph + material + UV + override), `FirstPersonResolver` |
| `kami-webgpu-rs` | EDN render-IR executor: `parse_render_ir` (lights / camera / env-IBL / materials / meshes / animations / post) |

Reference scene: `kami-clj-play3d/games/dance/` — exercises the whole stack,
asserted by `scene::tests::reference_scene_exercises_full_stack`.

## Real-VRM offscreen render (examples) — ADR-0047

The `:dance/*` data layer is GPU-less, but the repo ships a **reference offscreen
renderer** that takes a real `.vrm` all the way to pixels, proving the render-IR /
`:dance/avatar` data actually drives a three.js-parity VRM. It is a self-contained
wgpu example (engine-owner-gated `run_embed_vrm` stays the production surface,
ADR-0031); the example is the headless proof + algorithm reference.

`examples/common/vrm.rs` is the reusable core (no per-example duplication):

| API | Role |
|---|---|
| `VrmDance::load(&[u8])` | parse a real VRM → rest geometry + `JOINTS_0`/`WEIGHTS_0` + textures + morph targets + skeleton (parent/order/inverse-bind) + `SpringSimulator` |
| `VrmDance::frame(pose, happy, aa, blink, spring)` | one frame, CPU: expression **morph** (via `kami_vrm::ExpressionManager`) → humanoid **FK** from `DancePose` → **spring bones** → joint **palette** |
| `GpuRenderer::new(&model, w, h)` | offscreen wgpu pipeline: GPU **skinning** (storage-buffer palette) + **MToon** toon-shade + rim + **multi-light** + **textures** |
| `GpuRenderer::render(morphed, palette, globals)` | draw one frame → RGBA |

three.js / three-vrm parity covered: real geometry · GPU `SkinnedMesh` · baseColor
textures (UV + alpha-cutout) · MToon toon-shading · render-IR `:lights` (beat-synced
multi-light) · expression morph (blink/aa/happy) · `VRMC_springBone` (hair/gear jiggle).

**clj/edn drives it**: the canonical `examples/vrm_edn.rs` reads `:dance/avatar`
(`:vrm` path · `:spring-bones` on/off · `:scale`) from `scene.edn` and the per-frame
`:lights`/`:env` from the render-IR — change the EDN, change the render.

Example progression (all `cargo run -p kami-live --example <name> --target aarch64-apple-darwin`):
`dance_png` (render-IR → cuboid performer) → `vrm_mesh` (procedural skinned mesh,
no asset) → `vrm_real` (real VRM geometry, static) → **`vrm_edn`** (clj/edn-driven
full VRM dance via `common/vrm.rs`). A `.vrm` asset is required for the real-VRM
examples — e.g. the VRM Consortium sample `Seed-san.vrm` (VRM Public License 1.0)
at `assets/Seed-san.vrm`, or set `:dance/avatar :vrm` to your own.
