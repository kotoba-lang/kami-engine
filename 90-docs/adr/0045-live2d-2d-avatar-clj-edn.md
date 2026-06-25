# ADR-0045 — Live2D (2D avatar) in CLJ/EDN, driven by the same show

- Status: accepted (data + driver layer done; Cubism runtime host-side, staged)
- Date: 2026-06-24
- Builds on: ADR-0043 (VRM dance scenes in CLJ/EDN), ADR-0044 (EDN render-IR parity),
  ADR-0040 (everything describable is EDN)

## Context

ADR-0043 made a **VRM (3D)** performer fully data-driven. The other common live-avatar
technology is **Live2D / Cubism (2D)** — a layered-texture model whose *parameters*
(`ParamAngleX`, `ParamMouthOpenY`, `ParamEyeLOpen`, …) warp ArtMeshes, with deformer
hierarchy and a small physics sim. An audit confirmed kami had **no Live2D support** (no
`.moc3`/`.model3.json` loader, no parameter-warp, no Cubism physics) — only sprite/tilemap/
SDF-text 2D. So a 2D vtuber-style performer could not be authored.

Key insight: a Live2D avatar and a VRM avatar should be driven by the **same choreography**.
The show clock already produces a `DancePose` + `BeatPhase` per frame; those map cleanly onto
the standard Cubism parameters. One `:dance/setlist` should animate either avatar kind.

## Decision

Add Live2D as a **second avatar kind** in the data + driver layer, parallel to
`AvatarBinding` — *not* by embedding a Cubism runtime (the `.moc3` binary format is heavy;
the ArtMesh warp + Live2D physics stay host-side, exactly as VRM mesh skinning does).
`kami_live::live2d` parses `:dance/live2d`, resolves per-frame Cubism parameters from the
beat-synced pose, and emits a render-IR `:live2d` entry.

```edn
:dance/live2d
{:model   "models/haru.model3.json"
 :home    [0.0 0.0 0.0] :scale 1.0
 :physics true
 :lipsync :ParamMouthOpenY
 :params  {:ParamAngleX 0.0 :ParamEyeLOpen 1.0}   ;; rest values
 :motions [{:name "idle" :file "idle.motion3.json"}]}
```

`Live2DBinding::drive(pose, phase) -> BTreeMap<param, value>` maps the show onto standard
Cubism params (deterministic):

| Cubism param | source |
|---|---|
| `ParamAngleX` / `ParamBodyAngleX` | `pose.root_yaw` |
| `ParamAngleZ` / `ParamBodyAngleZ` | `pose.spine_sway` |
| `ParamAngleY` | `pose.vertical_bob` |
| `ParamBreath` | slow sine of `phase.time` |
| `ParamEyeLOpen` / `ParamEyeROpen` | blink ~every 3 s |
| `:lipsync` param (def. `ParamMouthOpenY`) | mouth opens on each beat front |

`DanceScene` gains `live2d: Option<Live2DBinding>`; `DanceFrame.live2d` carries the
`{:kind :live2d :model … :params {…}}` render entry when bound. So `frame(dt)` drives a VRM
*and/or* a Live2D performer from one tick.

## Consequences

- A 2D (Cubism-style) performer is now authorable in pure CLJ/EDN and animates off the same
  beat grid as the VRM dancer — one choreography, two avatar technologies (ADR-0040).
- Strictly additive: scenes without `:dance/live2d` are unchanged; `DanceFrame.live2d` is
  `None`. 7 tests (5 driver + 2 scene), full suite green.
- Lint-covered: `lint_scene` flags an unbound `:model`, a non-positive `:scale`, and motions
  missing `:file` (alongside the same for `:dance/avatar`'s `:vrm`). The reference scene drives
  **both** a VRM and a Live2D performer from one setlist and stays lint-clean; `kami-dance`
  reports the live Cubism param count (`live2d: 9 Cubism params driven`).
- The host loads `.model3.json`/`.moc3` and applies the per-frame `:params` via a Cubism
  runtime (open reimplementations exist). A native ArtMesh-warp executor in `kami-render`
  (parameter → mesh deform + Live2D physics) is the staged follow-on, engine-owner-gated —
  mirroring how the VRM `:meshes` GPU path (ADR-0044 phase 3) is staged.
- Inline EDN motions: a `:motions` entry can carry parameter keyframes
  (`{:name :loop :keys [{:t :params {ParamX v}}]}`) — the Live2D analogue of `:dance/clips`.
  `Live2DBinding::sample_motion(name, time)` linearly interpolates (looping aware) into a
  parameter map a host layers over the beat-driven base params; a `:file`-only motion returns
  `None` (the host plays the `.motion3.json`). So 2D motions are authorable in clj/edn too.
  `:dance/live2d :motion "name"` selects the active inline motion, which `drive` overlays on
  the beat-driven base params each frame (the Live2D analogue of `:dance/avatar :clip`). Lint
  flags a dangling `:motion` ref and a motion with neither `:file` nor `:keys`.
- Not yet covered (future): Cubism deformer hierarchy fidelity, `.motion3.json` binary
  playback/blend, full Live2D physics3 — all host-runtime concerns; the EDN authoring surface
  is in place.
