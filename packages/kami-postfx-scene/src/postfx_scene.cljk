(ns postfx-scene
  "kami-postfx-scene — EDN authoring surface for `kami-postfx` POST-PROCESSING
  PIPELINE presets. Restored from the legacy kami-engine/kami-postfx-scene
  Rust crate (`kami-postfx-scene/src/lib.rs`, kotoba-lang/kami-engine @
  a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa, deleted in PR #82 \"Remove Rust
  workspace from kami-engine\") as part of the clj-wgsl migration
  (ADR-2607010930, com-junkawasaki/root).

  The data-tier counterpart of `kami-atmosphere-scene` (already restored as
  `kotoba-lang/kami-atmosphere-scene`) for the post-processing system: it
  turns canonical `:postfx/presets` EDN (an ordered vector of
  `:effect`-tagged effect maps per preset) into a real `postfx` pipeline
  (`{:effects [...] :enabled true}`), rebuilt the same way the hardcoded
  presets are (`postfx/new-pipeline` + `postfx/add` in order). It re-uses the
  tolerant `kotoba-lang/scene` accessors (`scene/mget` / `scene/num` /
  `scene/vec3` / `scene/kw-key` / `scene/root-map`) the same way games parse
  `scene.edn` (missing keys fall back to `0`/`0.0`, namespaced keywords match
  on `ns/name`, ints coerce to floats).

  ## Why this is safe (ADR-0038)

  Hot fullscreen passes / GPU-uniform packing (`bloom-params`, `ssao-params`,
  …) stay in `kotoba-lang/postfx` untouched. A post-fx preset is **init-time
  CONFIG** — read once when the pipeline is assembled at boot — so it is safe
  to move to EDN. The compiled-in `postfx/{nintendo,retro,final-fantasy,
  baminiku-character}` builders remain as the [[builtin-preset]] fallback and
  are parity-tested against the shipped EDN ([[postfx-edn]]).

  ## EDN shape (see [[postfx-edn]] / `resources/postfx.edn`)

  ```edn
  {:postfx/presets
   {:nintendo [{:effect :bloom :threshold 0.8 :intensity 0.3 :radius 4.0}
               {:effect :outline :color [0.15 0.15 0.15 1.0] :width 1.5 :depth-threshold 0.1}
               {:effect :vignette :intensity 0.15 :radius 0.8}]
    :retro [...] :final-fantasy [...] :baminiku-character [...]}}
  ```

  Each preset is an **ordered vector** (the pipeline order is load-bearing).
  Each effect map is tagged by `:effect` (a keyword id naming a `postfx`
  effect constructor); the rest of the keys are that variant's fields,
  hyphenated. Unknown `:effect` ids raise `ex-info` with
  `:postfx-scene/error :unknown-effect`.

  ## Adaptation note (EffectSpec / effect_id)

  The original Rust needed a hand-rolled `EffectSpec` `PartialEq` mirror of
  `PostEffect` (which derives only `Debug, Clone`) purely so two pipelines
  could be compared field-by-field without touching `kami-postfx`. In
  `kotoba-lang/postfx`, effects are already plain Clojure maps (`{:type
  :bloom :threshold .. }`), which compare structurally via `=` for free — so
  no projection type is needed here. [[effect-id]] and [[pipeline-specs]] are
  kept as thin 1:1-named helpers (test/API parity with the Rust surface)
  but are trivial: `effect-id` just reads `:type`, and `pipeline-specs` just
  reads `:effects`.

  Zero-dep portable CLJC. Depends on `kotoba-lang/scene` (tolerant EDN
  accessors) and `kotoba-lang/postfx` (effect constructors, pipeline,
  presets), both already restored in this migration."
  (:require [scene :as scene]
            [postfx :as postfx]))

;; ════════════════════════════════════════════════════════════════════════
;; shipped EDN
;; ════════════════════════════════════════════════════════════════════════

(def postfx-edn
  "The canonical post-processing preset CONFIG shipped with this crate (the
  preset table). This is the source of truth; the compiled-in presets
  (`postfx/nintendo` etc.) are the parity-tested mirror. Embedded as a
  literal string (rather than slurped from a resource) so this namespace
  loads identically on the JVM and in ClojureScript; kept byte-identical to
  `resources/postfx.edn`."
  ";; postfx.edn — canonical CONFIG/DATA for kami-postfx post-processing presets.
;;
;; ADR-0038: hot fullscreen passes / GPU uniform packing stay native Rust; only
;; init-time CONFIG/DATA moves to EDN. A post-fx preset is read ONCE at boot when
;; the pipeline is assembled (PostFxPipeline::new() + add(effect) in order), so it
;; lives here as the source of truth. `kami-postfx`'s compiled-in
;; `PostFxPipeline::{nintendo,retro,final_fantasy,baminiku_character}()` builders
;; remain as the `builtin_preset()` fallback and are parity-tested against this file.
;;
;; Each preset is an ORDERED VECTOR of effect maps (the pipeline order is
;; load-bearing — effects compose front-to-back). Every map is tagged by `:effect`
;; (a keyword id naming the PostEffect variant), and carries the variant's fields as
;; hyphenated keywords (e.g. :depth-threshold maps to the `depth_threshold` field).
;; Vectors are `[..]` (use kami_scene::vec3 / a vec accessor); ints coerce to floats
;; / u32. The `:effect` ids map to PostEffect variants:
;;   :bloom :outline :vignette :crt :color-grade :pixelate :ssao :depth-of-field
;;   :ssr :aces-tonemap :film-grain :chromatic-aberration :god-rays
{:postfx/presets
 ;; Nintendo: soft bloom + cel-shading outline + light vignette (Splatoon / Zelda WW).
 {:nintendo
  [{:effect :bloom :threshold 0.8 :intensity 0.3 :radius 4.0}
   {:effect :outline :color [0.15 0.15 0.15 1.0] :width 1.5 :depth-threshold 0.1}
   {:effect :vignette :intensity 0.15 :radius 0.8}]

  ;; Retro pixel art: downscale + CRT scanlines.
  :retro
  [{:effect :pixelate :pixel-size 4.0}
   {:effect :crt :scanline-intensity 0.3 :curvature 0.02}]

  ;; Final Fantasy quality: SSAO + SSR + bloom + DOF + god-rays + ACES + CA + grain
  ;; + vignette + color-grade (cinematic photorealistic character rendering).
  :final-fantasy
  [{:effect :ssao :radius 0.5 :bias 0.025 :intensity 1.2 :samples 64}
   {:effect :ssr :max-distance 50.0 :steps 64 :thickness 0.3 :fade-edge 0.15}
   {:effect :bloom :threshold 0.9 :intensity 0.15 :radius 6.0}
   {:effect :depth-of-field :focal-distance 2.5 :focal-range 1.5 :bokeh-radius 3.0 :bokeh-shape 1}
   {:effect :god-rays :density 0.96 :weight 0.15 :decay 0.97 :exposure 0.12 :light-pos [0.5 0.3]}
   {:effect :aces-tonemap :exposure 1.1 :curve 0}
   {:effect :chromatic-aberration :intensity 0.002 :samples 5}
   {:effect :film-grain :intensity 0.03 :size 1.5}
   {:effect :vignette :intensity 0.2 :radius 0.85}
   {:effect :color-grade :lift [0.0 -0.01 0.02] :gamma [1.0 1.0 0.98] :gain [1.05 1.02 1.0]}]

  ;; Baminiku LiveStage character: portrait DOF + warm bloom + SSAO + ACES + vignette
  ;; + color-grade.
  :baminiku-character
  [{:effect :ssao :radius 0.3 :bias 0.02 :intensity 0.8 :samples 32}
   {:effect :bloom :threshold 0.85 :intensity 0.2 :radius 5.0}
   {:effect :depth-of-field :focal-distance 2.0 :focal-range 0.8 :bokeh-radius 4.0 :bokeh-shape 1}
   {:effect :aces-tonemap :exposure 1.0 :curve 0}
   {:effect :vignette :intensity 0.25 :radius 0.8}
   {:effect :color-grade :lift [0.01 0.0 -0.01] :gamma [1.02 1.0 0.98] :gain [1.08 1.04 1.0]}]}}
")

(def all-preset-names
  "Names of the presets shipped as the compiled-in oracle (iteration source
  for `builtin-preset`/parity). Keeping this list here (not in `postfx`)
  keeps the engine namespace untouched. Order mirrors the `postfx` builder
  declaration order."
  ["nintendo" "retro" "final-fantasy" "baminiku-character"])

;; ════════════════════════════════════════════════════════════════════════
;; small typed accessors over the tolerant `scene` helpers
;;
;; `scene` ships `num` (double) / `vec3` (3-vector); post-fx also needs a
;; rounded integer-ish reader and the fixed 2-/4-vectors. These mirror
;; `vec3`'s "pad short, zero a non-vector" tolerance so absent / malformed
;; config degrades the same way.
;; ════════════════════════════════════════════════════════════════════════

(defn- round-num
  "Portable round-to-integer, used for the u32-ish fields (`:samples`,
  `:steps`, `:curve`, `:bokeh-shape`). Returns a `Long` on the JVM (so it
  compares `=` with the plain integer literals `postfx`'s hardcoded presets
  use) and a plain (integral-valued) number in ClojureScript."
  [x]
  #?(:clj  (Math/round (double x))
     :cljs (js/Math.round x)))

(defn- u32-of
  "Read a u32-ish field, coercing via `scene/num` then rounding; `0` when
  absent / non-numeric."
  [m key]
  (if-let [v (scene/mget m key)]
    (round-num (scene/num v))
    0))

(defn- f32-of
  "Read a numeric field; `0.0` when absent / non-numeric (via `scene/num`)."
  [m key]
  (scene/num (scene/mget m key)))

(defn- vec2-of
  "Read a 2-vector `[x y]`; missing components default to `0.0`, non-vector
  -> `[0.0 0.0]`."
  [m key]
  (let [v (scene/mget m key)
        s (if (vector? v) v [])
        g (fn [i] (scene/num (get s i)))]
    [(g 0) (g 1)]))

(defn- vec3-of
  "Read a 3-vector `[r g b]` field via the shared `scene/vec3`."
  [m key]
  (scene/vec3 (scene/mget m key)))

(defn- vec4-of
  "Read a 4-vector `[r g b a]`; missing components default to `0.0`,
  non-vector -> `[0.0 0.0 0.0 0.0]`."
  [m key]
  (let [v (scene/mget m key)
        s (if (vector? v) v [])
        g (fn [i] (scene/num (get s i)))]
    [(g 0) (g 1) (g 2) (g 3)]))

;; ════════════════════════════════════════════════════════════════════════
;; effect / pipeline projection helpers
;; ════════════════════════════════════════════════════════════════════════

(defn effect-id
  "The hyphenated `:effect` keyword id for a `postfx` effect map (its
  `:type`, as a string). Inverse of the dispatch in [[effect-from-map]]."
  [effect]
  (some-> (:type effect) name))

(defn pipeline-specs
  "Project a whole pipeline's effects into its ordered effect-map vector.
  The parity contract compares these element-by-element (via plain `=`,
  since `postfx` effects are already structurally-comparable maps)."
  [pipeline]
  (:effects pipeline))

;; ════════════════════════════════════════════════════════════════════════
;; EDN -> real `postfx` effect / pipeline
;; ════════════════════════════════════════════════════════════════════════

(defn effect-from-map
  "Build one real `postfx` effect map from an EDN effect map `m` tagged by
  `:effect`.

  Every field is read with the tolerant accessors, so a key a map omits
  degrades to the same zero/default the rest of the data tier uses. An
  `:effect` id with no matching variant (or a missing `:effect`) throws
  `ex-info` with `:postfx-scene/error :unknown-effect`."
  [m]
  (let [id (some-> (scene/mget m "effect") scene/kw-key)]
    (case id
      "bloom"
      (postfx/bloom {:threshold (f32-of m "threshold")
                      :intensity (f32-of m "intensity")
                      :radius (f32-of m "radius")})

      "outline"
      (postfx/outline {:color (vec4-of m "color")
                        :width (f32-of m "width")
                        :depth-threshold (f32-of m "depth-threshold")})

      "vignette"
      (postfx/vignette {:intensity (f32-of m "intensity")
                         :radius (f32-of m "radius")})

      "crt"
      (postfx/crt {:scanline-intensity (f32-of m "scanline-intensity")
                    :curvature (f32-of m "curvature")})

      "color-grade"
      (postfx/color-grade {:lift (vec3-of m "lift")
                            :gamma (vec3-of m "gamma")
                            :gain (vec3-of m "gain")})

      "pixelate"
      (postfx/pixelate {:pixel-size (f32-of m "pixel-size")})

      "ssao"
      (postfx/ssao {:radius (f32-of m "radius")
                     :bias (f32-of m "bias")
                     :intensity (f32-of m "intensity")
                     :samples (u32-of m "samples")})

      "depth-of-field"
      (postfx/depth-of-field {:focal-distance (f32-of m "focal-distance")
                               :focal-range (f32-of m "focal-range")
                               :bokeh-radius (f32-of m "bokeh-radius")
                               :bokeh-shape (u32-of m "bokeh-shape")})

      "ssr"
      (postfx/ssr {:max-distance (f32-of m "max-distance")
                    :steps (u32-of m "steps")
                    :thickness (f32-of m "thickness")
                    :fade-edge (f32-of m "fade-edge")})

      "aces-tonemap"
      (postfx/aces-tonemap {:exposure (f32-of m "exposure")
                             :curve (u32-of m "curve")})

      "film-grain"
      (postfx/film-grain {:intensity (f32-of m "intensity")
                           :size (f32-of m "size")})

      "chromatic-aberration"
      (postfx/chromatic-aberration {:intensity (f32-of m "intensity")
                                     :samples (u32-of m "samples")})

      "god-rays"
      (postfx/god-rays {:density (f32-of m "density")
                         :weight (f32-of m "weight")
                         :decay (f32-of m "decay")
                         :exposure (f32-of m "exposure")
                         :light-pos (vec2-of m "light-pos")})

      (throw (ex-info (str "unknown post effect `" (or id "<missing>") "`")
                       {:postfx-scene/error :unknown-effect
                        :postfx-scene/effect (or id "<missing>")})))))

(defn- pipeline-from-vec
  "Build one preset's pipeline from its EDN effect-vector: `postfx/new-pipeline`
  + `postfx/add(effect)` in order, exactly the way the hardcoded builders
  assemble it."
  [effects]
  (reduce (fn [p v]
            (if (map? v)
              (postfx/add p (effect-from-map v))
              (throw (ex-info "effect entry is not a map"
                               {:postfx-scene/error :unknown-effect
                                :postfx-scene/effect "<not-a-map>"}))))
          (postfx/new-pipeline)
          effects))

(defn chain-from-render-ir
  "Realise a render-IR `:post` chain into a `postfx` pipeline — the dance
  show's `:dance/post` (projected into the render-IR `:post` key by
  kami-live) becomes the host's post-processing pipeline. Like
  [[preset-from-edn]], but reads the per-frame render-IR `:post` vector
  instead of a named preset. Returns an empty (but enabled) pipeline when
  there is no `:post` key (or it is malformed) — tolerant like the rest of
  the data tier."
  [render-ir-edn]
  (let [root (scene/root-map render-ir-edn)]
    (if (nil? root)
      (postfx/new-pipeline)
      (let [post (scene/mget root "post")]
        (if (vector? post)
          (try
            (pipeline-from-vec post)
            (catch #?(:clj Exception :cljs js/Error) _
              (postfx/new-pipeline)))
          (postfx/new-pipeline))))))

(defn presets-from-edn
  "Parse the whole `:postfx/presets` table from EDN `src` into a map keyed by
  the (hyphenated) preset id, each value the rebuilt pipeline.

  Throws `ex-info` with `:postfx-scene/error` of `:not-a-map` (EDN root
  didn't parse to a map) or `:no-presets` (`:postfx/presets` missing or not
  a map) on failure — mirroring the original `Error::NotAMap` /
  `Error::NoPresets`. Propagates [[effect-from-map]]'s `:unknown-effect`."
  [src]
  (let [root (scene/root-map src)]
    (when (nil? root)
      (throw (ex-info "postfx EDN root is not a map"
                       {:postfx-scene/error :not-a-map})))
    (let [presets (scene/mget root "postfx/presets")]
      (when-not (map? presets)
        (throw (ex-info "`:postfx/presets` missing or not a map"
                         {:postfx-scene/error :no-presets})))
      (reduce (fn [acc [k v]]
                (if-let [id (scene/kw-key k)]
                  (if (vector? v)
                    (assoc acc id (pipeline-from-vec v))
                    acc)
                  acc))
              {}
              presets))))

(defn preset-from-edn
  "Look up & rebuild a single preset pipeline by (hyphenated) `name` from EDN
  `src`. Throws `ex-info` with `:postfx-scene/error :preset-not-found` if
  the table or the named preset is absent (also propagates
  [[presets-from-edn]]'s errors)."
  [src name]
  (let [presets (presets-from-edn src)]
    (if-let [p (get presets name)]
      p
      (throw (ex-info (str "preset `" name "` not found under `:postfx/presets`")
                       {:postfx-scene/error :preset-not-found
                        :postfx-scene/preset name})))))

(defn shipped-presets
  "Convenience: load & rebuild all presets from the crate-shipped
  [[postfx-edn]]."
  []
  (presets-from-edn postfx-edn))

(defn shipped-preset
  "Convenience: load & rebuild one preset from the shipped EDN."
  [name]
  (preset-from-edn postfx-edn name))

;; ════════════════════════════════════════════════════════════════════════
;; builtin fallback / parity oracle
;; ════════════════════════════════════════════════════════════════════════

(defn builtin-preset
  "The compiled-in fallback / parity oracle: the real `postfx/{nintendo,
  retro,final-fantasy,baminiku-character}`. Returns `nil` for an unknown
  name. This is what the shipped EDN is parity-tested against."
  [name]
  (case name
    "nintendo" (postfx/nintendo)
    "retro" (postfx/retro)
    "final-fantasy" (postfx/final-fantasy)
    "baminiku-character" (postfx/baminiku-character)
    nil))
