# KAMI VRM Dance — clj/edn dance scene

A VRM avatar dancing a beat-synced setlist, authored entirely in **clj + edn**.
The choreography is *data*: `scene.edn` is parsed by
`kami_live::scene::DanceScene::from_edn` into a deterministic `LiveShow`
(beat grid + setlist + performer pose) that the host ticks each frame.

```
author.clj  ──(datalevin: datoms → Datalog query)──▶  scene.edn
scene.edn   ──(kami_live::scene::DanceScene)──────▶  LiveShow + AvatarBinding
LiveShow.tick(dt) / .snapshot().performer_pose  ──▶  drives the VRM rig
logic.clj   ──(kamiclj → game.wasm)──────────────▶  interactive glue (audience)
```

## Files

| File | Owns |
|---|---|
| `author.clj` | The source of truth. Transacts the setlist + cues + avatar binding into datalevin, queries them back, projects `scene.edn`. Edit here, then `clojure -M author.clj`. |
| `scene.edn` | Generated snapshot the host reads. The `:dance/*` shape below. |
| `logic.clj` | Gameplay glue (kami-clj subset → `game.wasm`): spawns the performer entity the rig binds to + an audience ring. The dance itself is not here — it's data. |
| `models/` | Drop the `.vrm` referenced by `:dance/avatar :vrm` here (host-supplied asset). |

## The `:dance/*` EDN shape

```edn
{:dance/show     {:bpm 128.0 :stage :hall :swing 0.08 :meter [4 8] :performer "Mitama"}
 :dance/avatar   {:vrm "models/mitama.vrm" :home [0.0 1.0 0.0] :scale 1.0
                  :look-at true :spring-bones true}
 :dance/crowd    {:fans 240 :cap 4096 :pit-bias 0.7 :seed 1}
 :dance/lighting [{:fixture :front-par :color [1.0 0.6 0.4] :intensity 0.85
                   :envelope :breathe :bars 64 :at-bar 0} ...]
 :dance/setlist  [{:title "Chorus" :bpm 128.0 :bars 16 :dance :wota :audio :opener
                   :cues [{:beat 0 :kind :drop :tag "hook"}]}
                  ...]}
```

- **`:stage`** `:club | :hall | :festival` → `kami_live::StagePreset`
- **`:dance`** `:idle | :four-on-floor | :wota | :kpop-point | :shuffle | :hold` →
  `kami_live::DanceMove` (auto-selected when the track starts)
- **`:kind`** `:drop | :breakdown | :callout` (else `:custom`) → `kami_live::CueKind`
- **`:audio`** `:opener` (named preset) **or** inline
  `{:drums :four-on-floor|{:kick [v…] :snare […]} :bass :c-minor|[{:beat :midi :len :vel}…]
  :lead-arp [midi…] :pad-chord [midi…]}` — drums/bass/arp/pad all describable in EDN
- **`:bars`** × beats-per-bar = track length (or give `:beats` directly)
- **`:dance/crowd`** → `CrowdConfig` (deterministic placement from `:seed`)
- **`:dance/lighting`** → beat-synced `LightingCue`s pushed at `:at-bar`. `:fixture`
  `:front-par|:back-par|:spot|:blinder|:laser|:strobe`; `:envelope`
  `:hold|:breathe|:ramp|:pulse|:strobe` (or `{:pulse decay}` / `{:strobe duty}`)
- **`:dance/vj`** → per-phrase `VJDeck`. `:pattern`
  `:solid|:stripes|:pulse|:rings|:scope|:noise`; `:palette` a named const
  (`:neon-pink|:cool-wave|:sunset|:monochrome`) or inline `{:primary :secondary :accent}`
- **`:dance/clips`** → EDN-authored animation clips (no binary `.vrma`):
  `[{:name :duration :loop :tracks [{:bone :interp :keys [{:t :pos :rot :scale}]}]}]`.
  The host loads each via `kami_skeleton_scene::clip_from_edn`; `:dance/avatar :clip "name"`
  plays one as an `:animations` layer (`evaluate_blend`).
- **`:dance/post`** → ordered post chain injected into each frame's render-IR as `:post`:
  `[{:effect :bloom :threshold :intensity} {:effect :color-grade :lift :gamma :gain} …]`.
  `:effect` ids match `kami-postfx-scene` (bloom / outline / vignette / crt / color-grade /
  pixelate / ssao / depth-of-field / ssr / aces-tonemap / film-grain / chromatic-aberration /
  god-rays), so a host realises the chain via `kami_postfx_scene::effect_from_map`. (`:fx` +
  short ids `dof`/`aces`/`chromatic` are tolerated aliases.)
- **`:dance/triggers`** → EDN-declared reactions to show events.
  `{:on :drop|:breakdown|:callout|:custom|:beat|:bar|:phrase|:track :tag <cue-tag>?
  :every <n>? …actions}`. `DanceScene::director.resolve(event)` returns the matching
  action maps; `:fx`/`:sound`/`:camera`/… are free-form data the host applies.

## Running (headless)

`kami-dance` loads, lints, and runs the show with no GPU — proving the whole
clj/edn path runs as a program (deterministic, so it doubles as a verify):

```sh
cargo run -p kami-live --bin kami-dance -- games/dance/scene.edn --frames 9000 --fps 60
#   lint: clean ✓
#   running "KAMI VRM Dance": 5 tracks, avatar "models/mitama.vrm" — 9000 frames @ 60 fps
#   done: 9000 frames, beat 320 (bar 80), 25 reactions fired
#     avatar: 1 VRM mesh(es) on the data path; live2d: 9 Cubism params driven
#     fx :confetti ×3   fx :dim ×1   fx :pyro ×10
```

This scene drives **both** a VRM (3D) avatar and a Live2D (2D) performer from the
*same* setlist (ADR-0043/0044/0045): the VRM appears as a skinned `:meshes` rig
with show-driven expressions, and the Live2D performer's standard Cubism params
are resolved each frame — one choreography, two avatar technologies.

`--emit-ir` prints the final frame's render-IR EDN; `--lint-only` just validates.

## Linting

The loader is tolerant — a mistyped `:stage :halll` silently becomes `:hall`. To
catch those before they ship, `kami_live::lint::lint_scene(&edn)` reports unknown
enums, out-of-range values, empty setlists, and dangling trigger `:tag`s. This
scene is asserted lint-clean in `kami-live`'s test suite
(`lint::tests::authored_example_is_lint_clean`).

## Host wiring (renderer)

The Rust pieces this feeds already exist: `kami-live` (show clock + dance poses),
`kami-vrm` (parse / spring-bone / look-at), `kami-skeleton` (GPU skinning). The
VRM render surface is **`kami-web::run_embed_vrm`** per ADR-0031 (not the play3d
hot path), so a host integration is:

```rust
let mut scene = kami_live::scene::DanceScene::from_edn(&edn).unwrap();
// load scene.avatar.vrm via kami-vrm; place at scene.avatar.home * scale
scene.show.start();
loop {
    let frame = scene.frame(dt);            // tick → resolve triggers → render-IR
    draw(frame.render_ir_edn());            // kami-webgpu-rs (native) / web CLJS
    for a in &frame.actions { host.apply(a); }   // :fx/:sound/:camera reactions
}
// the skinned VRM rig is driven from scene.show.snapshot().performer_pose.
```

Wiring `run_embed_vrm` to consume a `DanceScene` is engine-owner-gated (VRM
surface review); this directory ships the authoring layer + loader it plugs into.
