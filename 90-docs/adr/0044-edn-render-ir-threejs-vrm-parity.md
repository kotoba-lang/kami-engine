# ADR-0044 — EDN render-IR vocabulary for three.js / VRM parity

- Status: accepted (design + phased implementation; **phase 1 done**)
- Date: 2026-06-24
- Builds on: ADR-0031 (kami-vrm / `run_embed_vrm`), ADR-0040 (everything describable is EDN),
  ADR-0041 (play3d adopts the EDN render-IR), ADR-0043 (VRM dance scenes in CLJ/EDN)

## Context

An audit of kami's renderer vs. three.js (WebGL/WebGPURenderer) + `@pixiv/three-vrm` found
the *Rust capability* is largely present (PBR/MToon/SSS/Marschner shaders, VRM parse, spring
bones, node constraints, GPU skinning, KTX2-UASTC) but several features are **not expressible
in the EDN render-IR** — the cross-platform data contract (ADR-0040/0041). The render-IR v1 is
"globals (1 sun + hemisphere ambient) + instanced cuboids (metallic/roughness/emissive)". So a
scene authored in CLJ/EDN cannot yet say "three lights", "this is a skinned VRM mesh", "alpha
cutout", "use this env map", or "play this clip with cubic interpolation".

Gap inventory (audit, 2026-06-24):

| Area | Gap | three.js / three-vrm has |
|---|---|---|
| Lights | directional-only | point / spot / area, multiple |
| Environment | no IBL / env map | `scene.environment`, PMREM |
| Shadows | single directional map | cascades, point/spot shadows |
| Transparency | no alpha-test | `alphaTest` / glTF `MASK` |
| AA | none (post only) | MSAA |
| Materials in IR | fixed PBR/MToon, not data | `ShaderMaterial`, MToon uniforms |
| Skinned/morph in IR | cuboids only | skinned + morph meshes |
| glTF loader | export-only stub | full `GLTFLoader` |
| Animation | LINEAR only, single clip | STEP/CUBICSPLINE, `AnimationMixer` blend, IK, retarget |
| VRM runtime | binds parsed, no applier | `VRMExpressionManager`, first-person layers, `.vrma` |

## Decision

Close each gap by **adding it to the EDN render-IR / scene vocabulary** (ADR-0040 — data, not
code), letting the Rust (`kami-webgpu-rs` / `kami-render`) and web (CLJS) executors adopt it
incrementally. Every addition is **additive and backward compatible**: the v1 `parse_ir`
forward-pass path and its golden tests are untouched; the richer `parse_render_ir` reads new
optional keys (`:lights`, `:camera`, `:env`, `:materials`, `:meshes`, `:post`, …) and old
scenes parse unchanged. GPU executor wiring is staged per phase; WGSL changes in `kami-render`
remain engine-owner-gated (merge gate).

### The EDN vocabulary (target shape)

```edn
{:camera   {:eye [0 2 6] :target [0 1 0] :fov 1.05 :near 0.1 :far 500.0}
 :env      {:ambient [0.2 0.2 0.25] :ground [0.1 0.1 0.1]
            :ibl {:intensity 0.8 :url "studio.hdr"}}
 :lights   [{:kind :directional :color [1 0.96 0.85] :intensity 1.2 :dir [-0.4 -0.85 -0.35] :cast-shadow true}
            {:kind :point :color [1 0.5 0.2] :intensity 3.0 :pos [2 3 0] :range 12.0}
            {:kind :spot  :color [0.6 0.8 1] :pos [0 5 0] :dir [0 -1 0] :range 20.0 :inner 0.3 :outer 0.6}]
 :materials [{:id :skin :model :mtoon :base [1 0.8 0.7] :shade [0.6 0.4 0.4]
              :alpha-mode :mask :alpha-cutoff 0.5 :outline 0.02 :matcap "m.png"}]
 :meshes   [{:id :avatar :url "mitama.vrm" :skin :rig :morphs {:happy 0.0}
             :material :skin :pos [0 0 0]}]
 :animations [{:target :avatar :clip "wave" :interp :cubic :weight 1.0 :fade 0.3}]
 :post     [{:fx :bloom :threshold 1.0 :intensity 0.6} {:fx :fxaa}]
 :globals  {...}                ;; v1 sun/sky — still honoured
 :instances [...] }             ;; v1 cuboids — still honoured
```

### Phases

1. **Lights + camera + environment/IBL — DONE.** `kami_webgpu_rs::{Light, LightKind, Camera,
   Environment, RenderIr, parse_render_ir}`. Multi-light rig (`:lights`), explicit
   `:camera {:fov :near :far :ortho :ortho-size}` (perspective or orthographic — the three.js
   `OrthographicCamera`), `:env {:ambient :ground :ibl :tonemap :exposure :zenith :fog}` (the
   `renderer.toneMapping`/`toneMappingExposure` analogue, default `reinhard`/1.0) — all parsed,
   backward compatible (v1 scenes → empty lights / no camera / env from sky). Tested
   (`render_ir_ext_tests`, 4 tests; golden tests stay green).
2. **Materials + alpha — DONE (data layer).** `kami_webgpu_rs::{Material, MaterialModel,
   AlphaMode}` + `RenderIr.materials` + `RenderIr::material(id)`. `:materials` registry: model
   `:pbr|:mtoon|:unlit`, base/shade colours, metallic/roughness/emissive, MToon outline/rim/
   matcap, and `:alpha-mode :opaque|:mask|:blend` + `:alpha-cutoff` (the cutout gap). **Texture
   maps DONE:** `:base-tex`/`:normal-tex`/`:emissive-tex`/`:mr-tex`/`:ao-tex` host-loaded URL
   references (closing "textures in the IR"; KTX2-UASTC already decoded by kami-render) +
   sampler config `:wrap :repeat|:clamp|:mirror` / `:anisotropy 1..16` (three.js `texture.wrapS`/
   `anisotropy`, closing "no anisotropic filtering in the IR").
   **Physical extensions DONE:** `:clearcoat`/`:clearcoat-roughness` (car paint), `:transmission`/
   `:ior`/`:thickness` (glass refraction), `:sheen` (cloth) — the three.js `MeshPhysicalMaterial`
   surface. Tested (5 tests; golden green). Remaining: instances/meshes reference `:material`
   (phase 3) and the shader wiring (MToon uniforms + alpha discard) — engine-owner-gated.
3. **Skinned + morph meshes — DONE (data layer + dance emitter).**
   `kami_webgpu_rs::{Mesh, MorphWeight}` + `RenderIr.meshes` + `RenderIr::mesh(id)` /
   `Mesh::morph(name)`. `:meshes` carry `:url` (host-loaded VRM/glTF), transform
   (`:pos`/`:rot` quat/`:scale`), `:material` + `:skin` refs, per-frame `:morphs {name w}`, and
   an optional inline `:joints` palette (column-major mat4s) for a fully data-driven host.
   `kami_live::render::show_to_render_ir` now emits the dance performer as a skinned `:meshes`
   avatar (+ `:lights` from the rig, `:materials` MToon, `:camera`) instead of a placeholder
   cuboid — **closing the ADR-0043 gating** (a VRM avatar is now expressed in the render-IR).
   Tested: 4 kami-webgpu-rs + 2 kami-live tests; golden frames green. Remaining: the GPU
   executor consuming `:meshes`/`:joints` via `skinned_mtoon.wgsl` — engine-owner-gated.
4. **Animation — interpolation + blending DONE (kami-skeleton).**
   `kami_skeleton::Interpolation {Linear, Step, CubicSpline}` (glTF `LINEAR`/`STEP`/
   `CUBICSPLINE`; cubic = Catmull-Rom for translation/scale + smoothstep-eased slerp,
   auto-tangent) on `BoneTrack`; `Skeleton::{evaluate_blend, evaluate_crossfade}` —
   `AnimationMixer`-style weighted blend (avg translation/scale, hemisphere-aligned nlerp
   rotation) and cross-fade. Closes the audit gaps "LINEAR-only" + "no blend/cross-fade".
   Tested (6 new tests; existing green). The EDN `:animations` vocabulary is also wired:
   `kami_webgpu_rs::{Animation, AnimInterp}` + `RenderIr.animations` + `animations_for(target)`
   — `:animations [{:target :clip :time :interp :weight :fade}]` are the blend layers a host
   feeds `evaluate_blend` (2 tests; backward compatible). **EDN clip authoring DONE:**
   `kami_skeleton_scene::clip_from_edn(src, bone_index)` parses an EDN clip
   (`{:name :duration :loop :tracks [{:bone :interp :keys [{:t :pos :rot :scale}]}]}`) into an
   `AnimationClip`, resolving bone names via a caller map (VRM humanoid → skeleton index) so a
   clip retargets onto any skeleton (4 tests). Motions are authorable in clj/edn — no binary
   needed; a `.vrma` loader would convert into the same `AnimationClip`. **CCD IK DONE:**
   `Skeleton::solve_ik_ccd(chain, target, iterations, threshold)` — Cyclic Coordinate Descent
   (the `CCDIKSolver` analogue), returns updated local rotations so the chain tip reaches a
   world-space target (foot placement / hand reach); 3 tests (reachable / clamped-far /
   degenerate). **Retarget DONE:** `AnimationClip::retarget(source, target)` remaps tracks
   by bone name (`SkeletonUtils.retargetClip` analogue) so a clip authored for one rig plays on
   another sharing humanoid bone names; unmatched tracks dropped (1 test). **All kami-skeleton
   animation gaps closed** (interp / blend / IK / retarget / EDN clips); only `.vrma` *binary*
   import remains. **Wired into the dance scene:** `:dance/avatar :clip "idle"` (+ EDN clip defs
   in `:dance/clips`) makes `kami_live::render` emit an `:animations` layer at show time, so the
   clip→`:animations`→`evaluate_blend` chain runs end-to-end from one EDN scene (2 tests).
5. **VRM runtime — ExpressionManager DONE (kami-vrm).** `kami_vrm::expression::
   {ExpressionManager, ResolvedExpression, ColorOverride, UvOverride}`. `resolve(weights)`
   accumulates each expression's morph-target binds (× weight), material-colour binds, and
   UV-transform binds, and applies VRM 1.0 override semantics — `Block`/`Blend` of
   blink/lookAt/mouth (so a `happy` that blocks blink suppresses the blink track), plus binary
   snap. The `@pixiv/three-vrm` `VRMExpressionManager` analogue; pure logic, 6 tests. The EDN
   path is wired end-to-end: render-IR `:meshes :expressions {name w}` (`Mesh.expressions` +
   `Mesh::expression`, kami-webgpu-rs) carries the weights, and `kami_live::render` drives the
   dance performer's VRM expressions from the show — cheer → `happy`, beat front → `aa`
   (lipsync), periodic `blink` — mirroring the Live2D param driver (ADR-0045). 1 + 1 tests.
   **First-person culling DONE:** `kami_vrm::firstperson::{FirstPersonResolver, FirstPersonView,
   node_visible}` resolves per-node visibility for first/third-person views (`Both`/`Auto` →
   visible, `ThirdPersonOnly` culled in FP, `FirstPersonOnly` culled in TP) — the
   `VRMFirstPerson` analogue; `hidden_nodes(view)` gives the host cull set. 5 tests. **Phase 5
   complete.**
6. **Render features** — point/spot shadows + cascades, MSAA, alpha-to-coverage, anisotropic
   filtering (lighting/shadow vocabulary under `[:globals …]` in progress). **`:particles`
   DONE:** `kami_webgpu_rs::ParticleBurst` + `RenderIr.particles` parse `:particles [{:pos :color
   :count :speed :life :size :gravity}]`; `kami_live` turns each fired `:fx` reaction
   (confetti/pyro/sparkle) into a burst at the performer, so a host particle pipeline draws
   them (the `:fx` → pixels loop). **`:post` chain
   DONE:** `kami_webgpu_rs::PostEffect` + `RenderIr.post` parse `:post [{:fx … …params}]`
   (generic `params` map → `num`/`vec3` per effect) so the kami-postfx effects (bloom / outline
   / vignette / crt / color-grade / ssao / dof / ssr / aces / …) are EDN-driven and ordered;
   `:dance/post` flows through `kami_live` into each frame's render-IR (2 + 1 tests). The
   render-IR `:post` uses the **`:effect`** tag + canonical ids (`depth-of-field`/`aces-tonemap`/
   `chromatic-aberration`; `:fx` + short ids tolerated as aliases, normalised on parse) so it
   matches **`kami-postfx-scene`** — `kami_postfx_scene::chain_from_render_ir(render_ir)` realises
   the `:post` vector straight into a `kami_postfx::PostFxPipeline` (tolerant; unknown effects
   skipped), closing the loop render-IR `:post` → engine structs. The `-scene` data-tier keeps
   `kami-postfx` itself pure (no kami-scene dep, ADR-0038). Exercised by the cross-crate test.

## Consequences

- The render-IR becomes the single EDN surface for "what three.js does", so a CLJ/EDN scene —
  including a VRM dance scene (ADR-0043) — gains lights, materials, skinned avatars, and post
  without per-platform code. Web and native interpret the same bytes (ADR-0040).
- Strictly additive: v1 scenes and golden frames are unaffected at every phase; new keys are
  opt-in. GPU/WGSL work lands behind the engine-owner merge gate, phase by phase.
- The CLJ/EDN authoring discipline (tolerant loader + `lint` + headless runner, ADR-0043)
  extends to the new vocabulary as each phase lands.
- The data→realizer pipeline is proven end-to-end across crates: a cross-crate integration
  test (`kami-live/tests/full_pipeline.rs`) loads the reference scene, projects each frame to
  the render-IR, and **realises it** — `kami_webgpu_rs::parse_render_ir` (lights/materials/
  meshes/animations/post/env), `kami_skeleton_scene::clip_from_edn` (clip onto a humanoid
  skeleton), and `kami_vrm::ExpressionManager` (mesh expression weights → morphs). Each EDN
  subsystem lands in
  its owning crate's structs, no GPU.
