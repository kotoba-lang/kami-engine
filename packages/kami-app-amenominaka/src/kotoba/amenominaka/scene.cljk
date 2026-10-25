(ns kotoba.amenominaka.scene
  "M0 (ADR-2607100100, `com-junkawasaki/root`): compose a `bim` building
  model with named `kami-*-scene` environment presets (weather/vegetation/
  terrain/postfx) into one walkthrough-ready scene EDN.

  Pure CLJC data transformation — no rendering, no GPU, no native code.
  Each environment domain is resolved through that domain's own
  `shipped-*` convenience function (`atmosphere-scene/shipped-weather`,
  `vegetation-scene/shipped-profile`, `terrain-scene/shipped-biome`,
  `postfx-scene/shipped-preset`), so preset data stays that domain's
  authority — this namespace only bundles the results, it never
  reinterprets or duplicates their EDN.

  `kami-cad-import` is deliberately NOT used here: its `VehicleAssembly`
  model is closed over vehicle part kinds/materials and its geometry
  payload is AABB-only (confirmed by reading its source, not just its
  README) — it cannot stand in for architectural CAD/BIM import. `bim`'s
  own construction API (`bim/project` / `bim/site` / `bim/building` /
  `bim/storey` / `bim/element`) is the actual building-authoring surface
  for this namespace. See ADR-2607100100 Addendum (2026-07-09)."
  (:require [atmosphere-scene]
            [vegetation-scene]
            [terrain-scene]
            [postfx-scene]))

(defn bim-project?
  "True if `m` has the shape `bim/project` produces (a map with a `:sites`
  collection) — the only structural check this namespace makes on the
  building input; `bim` itself owns full element/geometry validation."
  [m]
  (and (map? m) (sequential? (:sites m))))

(defn compose
  "Compose a walkthrough-ready scene EDN from a `bim` building and named
  environment presets.

  `opts`:
    :building   — required, a `bim/project` map (or any map shaped like
                  one — see [[bim-project?]]).
    :weather    — optional, an `atmosphere-scene` preset name (e.g.
                  \"overcast\"). Resolved via `atmosphere-scene/shipped-weather`.
    :vegetation — optional, a collection of `vegetation-scene` profile
                  names (e.g. [\"grass\"]). Each resolved via
                  `vegetation-scene/shipped-profile`; bundled as a map
                  keyed by name.
    :terrain    — optional, a `terrain-scene` biome name (e.g. \"plains\").
                  Resolved via `terrain-scene/shipped-biome`.
    :postfx     — optional, a `postfx-scene` preset name (e.g. \"nintendo\").
                  Resolved via `postfx-scene/shipped-preset`.

  Returns a map with `:scene/building` plus whichever of `:scene/atmosphere`
  `:scene/vegetation` `:scene/terrain` `:scene/postfx` were requested — keys
  for presets not requested are simply absent (not nil-valued).

  An unknown preset name throws the underlying domain's own `ex-info`
  (e.g. `:atmosphere-scene/error :preset-not-found`) — this namespace adds
  no separate name-validation layer."
  [{:keys [building weather vegetation terrain postfx]}]
  (when-not (bim-project? building)
    (throw (ex-info "compose: :building must be a bim/project-shaped map (a map with :sites)"
                     {:kotoba.amenominaka.scene/error :invalid-building
                      :kotoba.amenominaka.scene/building building})))
  (cond-> {:scene/building building}
    weather
    (assoc :scene/atmosphere (atmosphere-scene/shipped-weather weather))

    (seq vegetation)
    (assoc :scene/vegetation
           (into {} (map (fn [name] [name (vegetation-scene/shipped-profile name)])) vegetation))

    terrain
    (assoc :scene/terrain (terrain-scene/shipped-biome terrain))

    postfx
    (assoc :scene/postfx (postfx-scene/shipped-preset postfx))))

(defn valid-scene?
  "True if `scene` is a `compose` output: a map with a valid `:scene/building`.
  Presence of `:scene/atmosphere` / `:scene/vegetation` / `:scene/terrain` /
  `:scene/postfx` is never required — an environment-free scene (building
  only) is still valid."
  [scene]
  (and (map? scene) (bim-project? (:scene/building scene))))
