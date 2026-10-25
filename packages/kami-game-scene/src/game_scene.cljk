(ns game-scene
  "KAMI Game Scene — EDN authoring surface for `game.animation`'s
  ANIMATION-STATE presets (Nintendo-style juicy motion), plus (in the
  `game-scene.*` submodules) five sibling EDN-authoring surfaces for other
  `game` data tables. Restored from the legacy kami-engine/kami-game-scene
  Rust crate (deleted in kotoba-lang/kami-engine PR #82 'Remove Rust
  workspace from kami-engine', recovered at commit
  a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa) as part of the clj-wgsl
  migration (ADR-2607010930, com-junkawasaki/root).

  This namespace (lib.rs's non-`pub mod` content) turns canonical
  `:game/animations` EDN (an ordered vector of `:clip`-tagged clip maps per
  preset) into a real `game.animation` `AnimationState` map, rebuilt the
  same way the hardcoded preset factories assemble it (`animation/new-state`
  + `animation/with-clip` in order). It re-uses the tolerant `scene`
  accessors the same way games parse `scene.edn` (missing keys fall back to
  defaults, namespaced keywords match on `ns/name`, ints coerce to floats).

  ## Why this is safe (ADR-0038 / ADR-0046)

  The hot per-frame integrator (`game.animation/tick`, the sine/elastic
  easing math) stays pure `game.animation`. An animation preset is
  **init-time CONFIG** — the clip list, their params, and their *initial*
  runtime state (`:timer 0.0`, `:phase :wait`, `:angle 0.0`, ...) read once
  when the `AnimationState` is constructed — so it is safe to move to EDN.
  `game.animation` itself stays untouched; the EDN dependency lives only
  here. The compiled-in `animation/{skibidi-idle,grimace-wobble,item-pickup,
  sigma-idle,ohio-glitch,...}` factories remain as the [[builtin-animation]]
  fallback and are parity-tested against the shipped EDN ([[animations-edn]]).

  ## EDN shape (see `resources/game_scene/animations.edn`)

  ```edn
  {:game/animations
   {:skibidi-idle [{:clip :head-bob :rise-height 2.0 :rise-time 1.0 :hold-time 0.5
                     :drop-time 0.5 :wait-time 2.0 :timer 0.0 :phase :wait}
                    {:clip :spinning :speed 3.0 :angle 0.0}]
    :grimace-wobble [...] :item-pickup [...] :sigma-idle [] :ohio-glitch [...]}}
  ```

  Each preset is an **ordered vector** (clip order is load-bearing — the
  outputs combine front-to-back via `animation/combine-output`). Each clip
  map is tagged by `:clip` (a keyword id naming the `game.animation` clip
  variant); the rest of the keys are that variant's fields, hyphenated. The
  HeadBob `:phase` field is itself a keyword id for the sub-phase (`:rise` /
  `:hold` / `:drop` / `:wait`). Unknown `:clip` ids throw `ex-info` with
  `:game-scene/error :unknown-clip`.

  ## Simplification vs the original Rust

  The original carried `ClipSpec`/`PhaseSpec` — hand-written `PartialEq`
  mirrors of `AnimationClip`/`HeadBobPhase`, needed only because those Rust
  types derived no `PartialEq`. Clojure maps/keywords have structural
  equality for free, so the EDN-rebuilt `AnimationState` can be compared
  `=` directly against `animation/skibidi-idle` et al — no mirror type is
  needed here; [[clip-id]] doubles as the (trivial) `clip-id`/`ClipSpec`
  replacement.

  ## Also in this crate

  `game-scene.catalog` — the Godot game catalog
  (`game.island-gen/godot-game-catalog`) as parity-tested EDN.
  `game-scene.brainrot` — brainrot evolution chains
  (`game.island-gen/brainrot-evolution-chains`) as EDN.
  `game-scene.character` — brainrot character definitions
  (`game.island-gen/brainrot-characters`) as EDN.
  `game-scene.item-catalog` — Sabiotoshi item catalog
  (`game.sabiotoshi/default-item-catalog`) as EDN.
  `game-scene.battle-royale` — battle-royale storm phases + consumable +
  weapon pools as EDN.
  `game-scene.pokoa` — Pokoa species dex + item shop
  (`game.pokoa/pokoa-dex`, `game.pokoa/pokoa-items`) as EDN.

  Zero-dep portable CLJC. Depends on `kotoba-lang/scene` (tolerant EDN
  accessors) and `kotoba-lang/game` (the real `game.animation` engine),
  both already restored in this migration."
  (:require [scene :as scene]
            [game.animation :as animation]))

;; ════════════════════════════════════════════════════════════════════════
;; shipped EDN
;; ════════════════════════════════════════════════════════════════════════

(def animations-edn
  "The canonical animation-preset CONFIG shipped with this crate (the preset
  table). This is the source of truth; the compiled-in preset factories are
  the parity-tested mirror. Embedded as a literal string (rather than
  slurped from a resource) so this namespace loads identically on the JVM
  and in ClojureScript; kept byte-identical to
  `resources/game_scene/animations.edn`."
"{:game/animations
 {:skibidi-idle
  [{:clip :head-bob :rise-height 2.0 :rise-time 1.0 :hold-time 0.5 :drop-time 0.5 :wait-time 2.0 :timer 0.0 :phase :wait}
   {:clip :spinning :speed 3.0 :angle 0.0}]

  :grimace-wobble
  [{:clip :wobble :intensity 0.05 :speed 2.0 :phase 0.0}
   {:clip :bobbing :amplitude 0.2 :frequency 0.5 :phase 0.0}]

  :item-pickup
  [{:clip :bobbing :amplitude 0.3 :frequency 1.5 :phase 0.0}
   {:clip :spinning :speed 2.0 :angle 0.0}
   {:clip :pulse-glow :min-scale 0.9 :max-scale 1.1 :speed 2.0 :phase 0.0}]

  :sigma-idle
  []

  :ohio-glitch
  [{:clip :glitch :interval 0.1 :timer 0.0 :intensity 0.15 :seed 42}]

  ;; ── Spawn + emote presets (gftd:kami/emote animation-preset mapping) ──
  :pop-spawn
  [{:clip :pop-in :target-scale [1.0 1.0 1.0] :duration 0.3 :timer 0.0 :overshoot 1.3}]

  :emote-wave
  [{:clip :bobbing :amplitude 0.15 :frequency 2.0 :phase 0.0}
   {:clip :spinning :speed 0.5 :angle 0.0}]

  :emote-dance
  [{:clip :bobbing :amplitude 0.4 :frequency 3.0 :phase 0.0}
   {:clip :spinning :speed 4.0 :angle 0.0}
   {:clip :wobble :intensity 0.06 :speed 3.0 :phase 0.0}]

  :emote-taunt
  [{:clip :pop-in :target-scale [1.2 1.2 1.2] :duration 0.2 :timer 0.0 :overshoot 1.5}
   {:clip :glitch :interval 0.08 :timer 0.0 :intensity 0.1 :seed 77}]

  :emote-celebrate
  [{:clip :pop-in :target-scale [1.3 1.3 1.3] :duration 0.3 :timer 0.0 :overshoot 1.4}
   {:clip :spinning :speed 6.0 :angle 0.0}
   {:clip :pulse-glow :min-scale 0.85 :max-scale 1.15 :speed 3.0 :phase 0.0}]

  :emote-sad
  [{:clip :bobbing :amplitude 0.05 :frequency 0.3 :phase 0.0}
   {:clip :pulse-glow :min-scale 0.85 :max-scale 0.95 :speed 0.5 :phase 0.0}]

  :emote-rage
  [{:clip :glitch :interval 0.05 :timer 0.0 :intensity 0.25 :seed 99}
   {:clip :squash-stretch :squash-scale [1.4 0.6 1.4] :stretch-scale [0.7 1.4 0.7] :duration 0.2 :timer 0.0 :active true}]

  :nintendo-bounce
  [{:clip :squash-stretch :squash-scale [1.3 0.7 1.3] :stretch-scale [0.85 1.2 0.85] :duration 0.4 :timer 0.0 :active false}]}}
")

;; Names of the presets shipped as the compiled-in oracle (iteration source
;; for `builtin-animation`/parity). Keeping this list here (not in
;; `game.animation`) keeps the engine namespace untouched. Order mirrors the
;; preset-factory declaration order in `animation.cljc`.
(def all-animation-names
  ["skibidi-idle" "grimace-wobble" "item-pickup" "sigma-idle" "ohio-glitch"
   "pop-spawn" "emote-wave" "emote-dance" "emote-taunt" "emote-celebrate"
   "emote-sad" "emote-rage" "nintendo-bounce"])

;; ════════════════════════════════════════════════════════════════════════
;; small typed accessors over the tolerant `scene` helpers
;; ════════════════════════════════════════════════════════════════════════
;;
;; `scene` ships `num` (double); animation clips also need an int-ish
;; "seed" (Glitch), a bool (SquashStretch `active`), and fixed 3-vectors
;; (scale fields). These mirror the rest of the data tier's "absent /
;; malformed degrades to a default" tolerance.

(defn- f32-of
  "Read a numeric field; `0.0` when absent / non-numeric (via `scene/num`)."
  [m key]
  (scene/num (scene/mget m key)))

(defn- round-num [x]
  #?(:clj (long (Math/round (double x))) :cljs (js/Math.round x)))

(defn- u32-of
  "Read a non-negative integer field, coercing via `scene/num` then
  rounding; `0` when absent / non-numeric."
  [m key]
  (let [v (scene/mget m key)]
    (if (some? v) (round-num (scene/num v)) 0)))

(defn- bool-of
  "Read a bool field; `false` when absent / non-boolean."
  [m key]
  (let [v (scene/mget m key)]
    (if (boolean? v) v false)))

(defn- vec3-of
  "Read a 3-vector `[x y z]` field; missing components default to 0.0,
  non-vector -> zeros."
  [m key]
  (scene/vec3 (scene/mget m key)))

(defn head-bob-phase-from-id
  "Map a HeadBob `:phase` keyword id -> the sub-phase keyword. Unknown /
  absent ids fall back to `:wait` (the hardcoded `skibidi-idle` initial
  state), so a missing phase degrades the same tolerant way as the rest of
  the data tier."
  [m key]
  (case (scene/kw-key (scene/mget m key))
    "rise" :rise
    "hold" :hold
    "drop" :drop
    :wait))

(defn head-bob-phase-id
  "The hyphenated `:phase` keyword id for a HeadBob phase keyword (inverse
  of [[head-bob-phase-from-id]]). Phase keywords are already hyphenated
  ids, so this is just `name`."
  [p]
  (name p))

(defn clip-id
  "The hyphenated `:clip` keyword id for an animation clip map (its
  `:type`). Clip `:type`s are already hyphenated ids matching the original
  Rust `AnimationClip` variant names (e.g. `:squash-stretch` ->
  \"squash-stretch\"), so this is just `name` — also the (trivial)
  replacement for the original `ClipSpec::clip_id`."
  [clip]
  (some-> (:type clip) name))

(defn clip-from-map
  "Build one real animation clip (a `game.animation` clip map) from a clip
  map tagged by `:clip`.

  Every field is read with the tolerant accessors, so a key a map omits
  degrades to the same zero/default the rest of the data tier uses. A
  `:clip` id with no matching variant (or a missing `:clip`) throws
  `ex-info` with `:game-scene/error :unknown-clip`."
  [m]
  (let [id (scene/kw-key (scene/mget m "clip"))]
    (case id
      "bobbing"        (animation/bobbing (f32-of m "amplitude") (f32-of m "frequency") (f32-of m "phase"))
      "spinning"       (animation/spinning (f32-of m "speed") (f32-of m "angle"))
      "squash-stretch" (animation/squash-stretch (vec3-of m "squash-scale") (vec3-of m "stretch-scale")
                                                  (f32-of m "duration") (f32-of m "timer") (bool-of m "active"))
      "wobble"         (animation/wobble (f32-of m "intensity") (f32-of m "speed") (f32-of m "phase"))
      "pop-in"         (animation/pop-in (vec3-of m "target-scale") (f32-of m "duration") (f32-of m "timer") (f32-of m "overshoot"))
      "head-bob"       (animation/head-bob (f32-of m "rise-height") (f32-of m "rise-time") (f32-of m "hold-time")
                                            (f32-of m "drop-time") (f32-of m "wait-time") (f32-of m "timer")
                                            (head-bob-phase-from-id m "phase"))
      "pulse-glow"     (animation/pulse-glow (f32-of m "min-scale") (f32-of m "max-scale") (f32-of m "speed") (f32-of m "phase"))
      "glitch"         (animation/glitch (f32-of m "interval") (f32-of m "timer") (f32-of m "intensity") (u32-of m "seed"))
      (throw (ex-info (str "unknown animation clip `" (or id "<missing>") "`")
                       {:game-scene/error :unknown-clip :game-scene/clip (or id "<missing>")})))))

(defn animation-specs
  "Project a whole animation state's clips into its ordered clip-map list
  (the parity contract compares these element-by-element). Since
  `game.animation` clips are already plain comparable maps, this is just
  `:animations` — kept as a named fn for API parity with the original
  `animation_specs`."
  [state]
  (:animations state))

;; ════════════════════════════════════════════════════════════════════════
;; compiled-in oracle
;; ════════════════════════════════════════════════════════════════════════

(defn builtin-animation
  "The compiled-in fallback / parity oracle:
  `animation/{skibidi-idle,grimace-wobble,item-pickup,sigma-idle,ohio-glitch,
  pop-spawn,emote-wave,emote-dance,emote-taunt,emote-celebrate,emote-sad,
  emote-rage,nintendo-bounce}`. Returns nil for an unknown name. This is
  what the shipped EDN is parity-tested against."
  [name]
  (case name
    "skibidi-idle"    (animation/skibidi-idle)
    "grimace-wobble"  (animation/grimace-wobble)
    "item-pickup"     (animation/item-pickup)
    "sigma-idle"      (animation/sigma-idle)
    "ohio-glitch"     (animation/ohio-glitch)
    "pop-spawn"       (animation/pop-spawn)
    "emote-wave"      (animation/emote-wave)
    "emote-dance"     (animation/emote-dance)
    "emote-taunt"     (animation/emote-taunt)
    "emote-celebrate" (animation/emote-celebrate)
    "emote-sad"       (animation/emote-sad)
    "emote-rage"      (animation/emote-rage)
    "nintendo-bounce" (animation/nintendo-bounce)
    nil))

;; ════════════════════════════════════════════════════════════════════════
;; EDN parsing / loading
;; ════════════════════════════════════════════════════════════════════════

(defn- animation-from-vec
  "Build one preset's state from its EDN clip-vector: `new-state` +
  `with-clip` in order, exactly the way the hardcoded factories assemble
  it."
  [clips]
  (reduce (fn [state v]
            (if (map? v)
              (animation/with-clip state (clip-from-map v))
              (throw (ex-info "clip entry is not a map"
                               {:game-scene/error :unknown-clip :game-scene/clip "<not-a-map>"}))))
          (animation/new-state)
          clips))

(defn animations-from-edn
  "Parse the whole `:game/animations` table from EDN `src` into a map keyed
  by the (hyphenated) preset id, each value the rebuilt animation state.

  Throws `ex-info` with `:game-scene/error` of `:not-a-map` (EDN root
  didn't parse to a map) or `:no-table` (`:game/animations` missing or not
  a map) — mirroring the original `Error::NotAMap` / `Error::NoTable`."
  [src]
  (let [root (scene/root-map src)]
    (when (nil? root)
      (throw (ex-info "animations EDN root is not a map" {:game-scene/error :not-a-map})))
    (let [table (scene/mget root "game/animations")]
      (when-not (map? table)
        (throw (ex-info "`:game/animations` missing or not a map" {:game-scene/error :no-table})))
      (reduce (fn [acc [k v]]
                (if-let [id (scene/kw-key k)]
                  (if (vector? v)
                    (assoc acc id (animation-from-vec v))
                    acc)
                  acc))
              {}
              table))))

(defn animation-from-edn
  "Look up & rebuild a single preset state by (hyphenated) `name` from EDN
  `src`. Throws `ex-info` with `:game-scene/error :animation-not-found` if
  the table or the named preset is absent (also propagates
  [[animations-from-edn]]'s errors)."
  [src name]
  (let [root (scene/root-map src)]
    (when (nil? root)
      (throw (ex-info "animations EDN root is not a map" {:game-scene/error :not-a-map})))
    (let [table (scene/mget root "game/animations")]
      (when-not (map? table)
        (throw (ex-info "`:game/animations` missing or not a map" {:game-scene/error :no-table})))
      (let [clips (some (fn [[k v]] (when (= (scene/kw-key k) name) v)) table)]
        (if (vector? clips)
          (animation-from-vec clips)
          (throw (ex-info (str "animation `" name "` not found under `:game/animations`")
                           {:game-scene/error :animation-not-found :game-scene/animation name})))))))

(defn shipped-animations
  "Convenience: load & rebuild all presets from the crate-shipped
  [[animations-edn]]."
  []
  (animations-from-edn animations-edn))

(defn shipped-animation
  "Convenience: load & rebuild one preset from the shipped EDN."
  [name]
  (animation-from-edn animations-edn name))
