# kami-mangaka-scene-clj

The **clj/EDN authoring tier** over the Rust `kami-mangaka-scene` 3D facade
(ADR-2605141200 / ADR-2606282100). clj is the brain; the Rust facade
(`kami-vrm` + `kami-scene-graph` + `kami-render` + `kami-postfx`) is the GPU arm.

You author a `MangakaScene` as plain Clojure data and project it to the exact
JSON-LD that the Rust `MangakaScene::from_jsonld` reads — or to the
`com.etzhayyim.mangaka.composeScene3d` XRPC payload driven by the
`lg_mangaka.compose_scene_3d` Pregel graph. **Pure data: no GPU, no VRM bytes.**

This is the `kami-live` / `kami-engine-sdk-clj` pattern (scene-as-EDN) applied to
the mangaka 3D facade, and the 3D sibling of `kami-mangaka-render-clj` (2D panels).

## API

```clojure
(require '[kami.mangaka.scene :as s])

(-> (s/scene)
    (s/add-character {:id 0 :rkey "nei" :expression "joy"   ; emotion word → Expression
                      :pose-label "action.reach"            ; lexicon preset label
                      :root-xform (s/transform [0 0 0])})
    (s/set-camera (s/camera {:eye [0 1.4 3] :target [0 1.4 0]
                             :shot "Closeup" :dof (s/dof 3.0 1.8)}))
    (s/set-lights s/three-point)                            ; key+fill+rim presets
    (s/set-environment (s/environment {:biome "water-city" :weather "clear" :seed 42}))
    s/->json)            ; → JSON string for the Rust facade / composeScene3d XRPC
```

Faithful to the Rust public types (`lib.rs`): `Transform` / `CameraSpec` /
`LightSpec` / `EnvironmentSpec` and the `ShotGrammar` / `LightRole` /
`Expression` / `FxKind` enums (variant names verbatim). glam `Vec3` ↔ `[x y z]`,
`Quat` ↔ `[x y z w]`. The three-point light preset values mirror
`LightSpec::three_point_*`.

## Test

```bash
bb test     # enums / transform defaults / normalize / JSON-LD shape + serialization
```
