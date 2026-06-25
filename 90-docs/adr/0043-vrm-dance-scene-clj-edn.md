# ADR-0043 — VRM dance scenes authored in CLJ/EDN

- Status: accepted (authoring layer + loader implemented; host render wiring staged)
- Date: 2026-06-24
- Builds on: ADR-0031 (kami-vrm three-free topology, `run_embed_vrm`), ADR-0036
  (datalevin → scene.edn), ADR-0038 (Rust base + CLJ/Datomic game layer), ADR-0040
  (everything describable is EDN), ADR-0042 (CLJ/EDN everywhere)

## Context

The Rust dance pipeline already exists and is verified:

- **kami-live** — `LiveShow`: a deterministic show clock (`BeatGrid`, BPM/bar/phrase/swing)
  + `Setlist` (tracks, cue points) + `Performer` (`DanceMove` → `DancePose` over beat-time)
  + crowd / lighting / VJ. `tick(dt)` advances; `snapshot().performer_pose` drives a rig.
- **kami-vrm** — VRM 1.0 parse / spring-bone (verlet + colliders) / look-at / node constraints.
- **kami-skeleton** — bone hierarchy + GPU skinning.

What was missing was the **authoring surface**: a way to express *a VRM dance scene* — which
avatar, where it stands, and the beat-synced choreography — as CLJ/EDN, the same way a game
is authored as `logic.clj` + `scene.edn` (ADR-0036). Without it, a dance scene could only be
built imperatively in Rust, contradicting "everything describable is EDN" (ADR-0040) and
"all new gameplay is CLJ behaviour + EDN description" (ADR-0042).

## Decision

A VRM dance scene is **data**: a `:dance/*` EDN shape, authored in CLJ via datalevin
(ADR-0036), parsed by a new loader into the existing `LiveShow`. The avatar mesh/skinning
load stays host-side on the VRM surface (`run_embed_vrm`, ADR-0031) — the loader resolves
only the *binding* and the *choreography clock*, never touches GPU.

### EDN shape (`:dance/*`)

```edn
{:dance/show     {:bpm 128.0 :stage :hall :swing 0.08 :meter [4 8] :performer "Mitama"}
 :dance/avatar   {:vrm "models/mitama.vrm" :home [0.0 1.0 0.0] :scale 1.0
                  :look-at true :spring-bones true}
 :dance/crowd    {:fans 240 :cap 4096 :pit-bias 0.7 :seed 1}
 :dance/lighting [{:fixture :front-par :color [1.0 0.6 0.4] :intensity 0.85
                   :envelope :breathe :bars 64 :at-bar 0}
                  {:fixture :strobe :color [1.0 1.0 1.0] :intensity 1.0
                   :envelope {:strobe 0.25} :bars 16 :at-bar 32} …]
 :dance/vj       [{:pattern :stripes :palette :cool-wave}
                  {:pattern :pulse :palette :neon-pink} …]
 :dance/setlist  [{:title "Chorus" :bpm 128.0 :bars 16 :dance :wota :audio :opener
                   :cues [{:beat 0 :kind :drop :tag "hook"}]}
                  …]}
```

- `:stage` `:club|:hall|:festival` → `StagePreset`; `:swing`/`:meter` → grid groove/signature
- `:dance` `:idle|:four-on-floor|:wota|:kpop-point|:shuffle|:hold` → `DanceMove`
  (auto-selected on track change)
- `:kind` `:drop|:breakdown|:callout` (else `:custom`) → `CueKind`
- `:audio` `:opener` (named preset) **or** an inline program
  `{:drums :four-on-floor|{:kick [v…] …} :bass :c-minor|[{:beat :midi :len :vel}…]
  :lead-arp [midi…] :pad-chord [midi…]}` — the synth is fully EDN, not a Rust-only preset
- `:bars` × beats-per-bar = track length (or `:beats` directly)
- `:dance/crowd` → `CrowdConfig` (deterministic placement from `:seed`)
- `:dance/lighting` → beat-synced `LightingCue`s pushed at `:at-bar`; `:fixture`
  `:front-par|:back-par|:spot|:blinder|:laser|:strobe`, `:envelope`
  `:hold|:breathe|:ramp|:pulse|:strobe` (or `{:pulse decay}` / `{:strobe duty}`)
- `:dance/vj` → per-phrase `VJDeck` program; `:pattern`
  `:solid|:stripes|:pulse|:rings|:scope|:noise`, `:palette` a named const
  (`:neon-pink|:cool-wave|:sunset|:monochrome`) or inline `{:primary :secondary :accent}`
- `:dance/triggers` → EDN-declared reactions: `{:on :drop|:breakdown|:callout|:custom|
  :beat|:bar|:phrase|:track :tag <cue-tag>? :every <n>? …actions}`. `Director::resolve`
  maps each `ShowEvent` → the matching action maps (`:fx`/`:sound`/`:camera`/… are
  free-form data the host applies — new reactions need EDN, not engine code)

The whole `LiveShow` is now describable in EDN — tempo, venue, avatar, crowd,
lighting, VJ visuals, and full choreography — with **zero Rust** per scene.

### Loader

`kami_live::scene::DanceScene::from_edn(&str) -> Option<DanceScene>` returns
`{ title, avatar: AvatarBinding, show: LiveShow }`. It re-uses **kami-scene**'s tolerant
accessors (missing keys default, `:kw`/`"str"` both accepted, int↔float coercion), so a
dance scene parses exactly the way `scene.edn` does. Dependency direction stays acyclic:
`kami-live → kami-scene → kotoba-edn`. Two additive `LiveShowBuilder` methods (`.swing`,
`.meter`) let the EDN determinism parameters reach the grid.

### Authoring + host contract

```
author.clj ─(datalevin: datoms → Datalog query)─▶ scene.edn        (ADR-0036)
scene.edn  ─(DanceScene::from_edn)──────────────▶ LiveShow + AvatarBinding + Director
host: load avatar.vrm via kami-vrm; place at home*scale; run_embed_vrm   (ADR-0031)
each frame: DanceScene::frame(dt) ─▶ DanceFrame { render_ir, actions }
       ├─ render_ir {:globals … :instances [...]} → kami-webgpu-rs / web CLJS draw (ADR-0041)
       ├─ actions [:fx/:sound/:camera …]          → host applies authored reactions
       └─ snapshot().performer_pose               → drive the skinned VRM rig (ADR-0031)
```

The show also *renders* from data: `kami_live::render::show_to_render_ir_edn`
projects each `ShowSnapshot` into the **existing EDN render-IR** (`{:globals …
:instances …}`, ADR-0040/0041) — performer placeholder + lit crowd + VJ-tinted
sky + performer-tracking camera — so the dance scene draws on native
(`kami-webgpu-rs`) and web (CLJS) through one data path, no per-renderer code.
The skinned VRM rig replaces the performer instance on the VRM surface.

Reference scene: `kami-clj-play3d/games/dance/` (`author.clj` + `scene.edn` + `logic.clj`).
The committed `scene.edn` is guarded against the loader by a `kami-live` test
(`scene::tests::authored_example_scene_loads`, `include_str!`).

## Boundary (unchanged ownership)

The VRM render surface remains **`kami-web::run_embed_vrm`** per ADR-0031 — *not* the play3d
hot path. This ADR adds only the additive authoring layer (EDN shape + `kami-live` loader +
the example game) that `run_embed_vrm` plugs into. Wiring `run_embed_vrm` to consume a
`DanceScene` is engine-owner-gated (VRM-surface review) and staged separately, mirroring the
incremental adoption discipline of ADR-0041.

## Consequences

- A VRM dance scene is now write-once CLJ/EDN, consistent with ADR-0040/0042: tempo, venue,
  avatar, and full choreography are data a designer edits via `author.clj`, no Rust.
- Determinism travels: the same `(bpm, t0)` + EDN replays identical poses across clients and
  backends (wasmtime/wasmi parity, ADR-0037) — verified by `kami-live` scene tests.
- No new GPU coupling: kami-live stays pure (no gpu deps); the avatar binding is plain data
  the host interprets, so the loader runs natively and on web alike.
- Authoring is checkable: `kami_live::lint::lint_scene(&edn)` re-reads the raw EDN and
  reports what the tolerant loader would silently correct — unknown enums (`:stage :halll`,
  `:dance :wotaa`), out-of-range values, empty setlists, dangling trigger `:tag`s, and
  beat-0 cues (which never fire — cue dispatch is open-closed). The committed reference scene
  is asserted lint-clean, so typos can't rot the example. (The beat-0 lint was found by the
  headless runner — the example's intro/hook reactions had silently never fired.)
- It runs as a program: `kami-dance <scene.edn> [--frames N] [--fps F] [--emit-ir]` loads,
  lints, and drives the show headless (no GPU), reporting beat/bar reached and the `:fx`
  reactions fired. Deterministic → doubles as a `bb` verify / golden generator. `run_headless`
  is the lib entry; the bin is a thin wrapper.
- Whole-stack guard: the reference scene now drives every authored subsystem at once — VRM
  avatar mesh + EDN clip + show-driven expressions (ADR-0044), Live2D performer (ADR-0045),
  lights / materials / camera / `:post` chain, crowd, and beat-synced reactions — asserted in
  one regression test (`scene::tests::reference_scene_exercises_full_stack`). If any wiring
  across ADR-0043/0044/0045 regresses, it fails.
- Next step (staged): a host integration consuming `DanceScene` on the `run_embed_vrm`
  surface, behind VRM-surface review.
