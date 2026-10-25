(ns vegetation-scene
  "kami-vegetation-scene — EDN authoring surface for `kami-vegetation`
  TAXONOMIC PROFILE presets. Restored from the legacy
  `kami-engine/kami-vegetation-scene` Rust crate (deleted from
  kotoba-lang/kami-engine in PR #82, \"Remove Rust workspace from
  kami-engine\", recovered at commit
  a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa) as part of the clj-wgsl
  migration (ADR-2607010930, com-junkawasaki/root).

  This is the data-tier counterpart of `kami-vehicle-scene` /
  `kami-atmosphere-scene` / `kami-terrain-scene` for the vegetation
  taxonomy system: it turns canonical `:vegetation/profiles` EDN into a
  real taxonomic-profile map — the CLJC mirror of the original
  `kami_vegetation::taxonomy::TaxonomicProfile` engine struct — re-using
  the tolerant `kotoba-lang/scene` accessors (`scene/mget` / `scene/num`
  / `scene/vec3` / `scene/root-map` / `scene/kw-key`) the same way games
  parse `scene.edn`: missing keys fall back to defaults, namespaced
  keywords match on `ns/name`, ints coerce to floats.

  ## Why this is safe (ADR-0038)

  Hot mesh generation / Poisson-disk placement / WASM cull stays native
  in `kotoba-lang/vegetation` (the CLJC port of the native
  `kami-vegetation` engine). A taxonomic profile is **init-time
  CONFIG** — read once when a species mesh is generated (the engine's
  `mesh_from_profile` switches on `:canopy` parameterized by
  `:leaf-count` / `:leaf-size` / `:stem-radius-*`) — so it is safe to
  move to EDN. `kotoba-lang/vegetation` itself stays untouched; the EDN
  dependency lives only here. The compiled-in
  `vegetation.taxonomy/{grass,fern,palm,conifer,bush,cactus,moss}`
  builders remain the [[builtin-profile]] fallback and parity oracle,
  and are parity-tested against the shipped EDN ([[vegetation-edn]]).

  Unlike the original Rust (which hand-parsed EDN via
  `kotoba_edn::EdnValue` and merged onto a *real*
  `kami_vegetation::taxonomy::TaxonomicProfile` instance, then resolved
  `common_name` to a `&'static str` via a lookup table since the field
  requires a static lifetime), this namespace parses via
  `clojure.edn/read-string` (through `scene/root-map`) and merges onto
  `vegetation.taxonomy/moss`'s own map — the CLJC mirror of that same
  default shape. A `ProfileSpec` and a `TaxonomicProfile` are the exact
  same flat map shape in CLJC (`vegetation.taxonomy`'s preset builders
  already return `:common-name`/`:division`/... maps), so no separate
  struct or `&'static str` resolution step is needed — [[to-taxonomic-profile]]
  is the identity function, kept only for API parity with the original.

  ## EDN shape (see `vegetation-edn` / `resources/vegetation.edn`)

  ```edn
  {:vegetation/profiles
   {:grass {:canopy :blade :stem-radius-base 0.0 :stem-radius-top 0.0
            :leaf-count 3 :leaf-size 0.18 :height-range [0.7 1.4]
            :color-base [r g b] :color-tip [r g b]
            :common-name \"grass\" :division :angiospermae :habit :grass
            :arrangement :basal :leaf-shape :linear}
    :fern {...} :palm {...} :conifer {...} :bush {...} :cactus {...} :moss {...}}}
  ```

  Enum-valued fields are keyword ids mapped to the matching
  `vegetation.taxonomy` keyword via [[canopy-from-id]],
  [[division-from-id]], [[habit-from-id]], [[arrangement-from-id]],
  [[leaf-shape-from-id]] — mirroring `kami-vegetation`'s equivalent
  (private) `parse_*` fns, exactly as the original Rust crate did, so
  the engine namespace stays untouched.

  Zero-dep portable CLJC. Depends on `kotoba-lang/scene` (tolerant EDN
  accessors) and `kotoba-lang/vegetation` (the `taxonomy` preset
  builders), both already restored in this migration."
  (:require [scene :as scene]
            [vegetation.taxonomy :as taxonomy]))

;; ════════════════════════════════════════════════════════════════════════
;; shipped EDN
;; ════════════════════════════════════════════════════════════════════════

(def vegetation-edn
  "The canonical taxonomic-profile CONFIG shipped with this crate (the
  preset table). This is the source of truth; the compiled-in profiles
  (`vegetation.taxonomy/{grass,fern,palm,conifer,bush,cactus,moss}`)
  are the parity-tested mirror. Embedded as a literal string (rather
  than slurped from a resource) so this namespace loads identically on
  the JVM and in ClojureScript; kept byte-identical to
  `resources/vegetation.edn`."
  ";; vegetation.edn — canonical CONFIG/DATA for kami-vegetation taxonomic profiles.
;;
;; ADR-0038: hot mesh generation / placement / cull stays native Rust; only
;; init-time CONFIG/DATA moves to EDN. A taxonomic profile is read ONCE when a
;; species mesh is generated (`mesh_from_profile(&TaxonomicProfile)` switches on
;; :canopy parameterized by leaf-count / leaf-size / stem-radius), so it lives here
;; as the source of truth. `kami-vegetation`'s compiled-in
;; `taxonomy::{grass,fern,palm,conifer,bush,cactus,moss}()` builders remain as the
;; `builtin_profile()` fallback and are parity-tested against this file.
;;
;; NOTE: profile ids + field keys use hyphens here (idiomatic EDN keywords, e.g.
;; :stem-radius-base); the loader maps each hyphenated key to the matching public
;; field on TaxonomicProfile. Enum-valued fields (:division :habit :arrangement
;; :leaf-shape :canopy) are keyword ids mapped to the matching Rust enum variant.
;; Colours are `[r g b]` in [0,1] (use kami_scene::vec3); :height-range is `[min max]`.
{:vegetation/profiles
 ;; Grass (Poaceae — Angiospermae, Grass habit).
 {:grass {:common-name      \"grass\"
          :division         :angiospermae
          :habit            :grass
          :arrangement      :basal
          :leaf-shape       :linear
          :canopy           :blade
          :height-range     [0.7 1.4]
          :stem-radius-base 0.0
          :stem-radius-top  0.0
          :leaf-count       3
          :leaf-size        0.18
          :color-base       [0.18 0.42 0.08]
          :color-tip        [0.42 0.68 0.15]}
  ;; Fern (Pteridophyta, Herb habit).
  :fern {:common-name      \"fern\"
         :division         :pteridophyta
         :habit            :herb
         :arrangement      :alternate
         :leaf-shape       :pinnate
         :canopy           :fan
         :height-range     [0.8 1.5]
         :stem-radius-base 0.04
         :stem-radius-top  0.02
         :leaf-count       5
         :leaf-size        0.35
         :color-base       [0.12 0.28 0.04]
         :color-tip        [0.3 0.55 0.12]}
  ;; Palm tree (Angiospermae, Tree habit, radial canopy).
  :palm {:common-name      \"palm\"
         :division         :angiospermae
         :habit            :tree
         :arrangement      :whorled
         :leaf-shape       :pinnate
         :canopy           :radial
         :height-range     [0.85 1.25]
         :stem-radius-base 0.08
         :stem-radius-top  0.06
         :leaf-count       7
         :leaf-size        0.55
         :color-base       [0.35 0.22 0.08]
         :color-tip        [0.18 0.45 0.1]}
  ;; Conifer (Gymnospermae, Tree habit, cone canopy).
  :conifer {:common-name      \"conifer\"
            :division         :gymnospermae
            :habit            :tree
            :arrangement      :whorled
            :leaf-shape       :needle
            :canopy           :cone
            :height-range     [0.7 1.3]
            :stem-radius-base 0.09
            :stem-radius-top  0.03
            :leaf-count       3
            :leaf-size        0.42
            :color-base       [0.25 0.18 0.08]
            :color-tip        [0.12 0.3 0.08]}
  ;; Broadleaf bush (Angiospermae, Shrub habit, dome canopy).
  :bush {:common-name      \"bush\"
         :division         :angiospermae
         :habit            :shrub
         :arrangement      :alternate
         :leaf-shape       :ovate
         :canopy           :dome
         :height-range     [0.8 1.4]
         :stem-radius-base 0.06
         :stem-radius-top  0.04
         :leaf-count       6
         :leaf-size        0.33
         :color-base       [0.15 0.28 0.06]
         :color-tip        [0.28 0.48 0.1]}
  ;; Columnar cactus (Angiospermae, Succulent habit, Column canopy).
  :cactus {:common-name      \"cactus\"
           :division         :angiospermae
           :habit            :succulent
           :arrangement      :none
           :leaf-shape       :succulent
           :canopy           :column
           :height-range     [0.6 1.3]
           :stem-radius-base 0.22
           :stem-radius-top  0.18
           :leaf-count       0
           :leaf-size        0.0
           :color-base       [0.22 0.38 0.18]
           :color-tip        [0.32 0.52 0.22]}
  ;; Ground moss (Bryophyta, Mat habit, carpet canopy).
  :moss {:common-name      \"moss\"
         :division         :bryophyta
         :habit            :mat
         :arrangement      :none
         :leaf-shape       :scale
         :canopy           :carpet
         :height-range     [0.15 0.25]
         :stem-radius-base 0.0
         :stem-radius-top  0.0
         :leaf-count       1
         :leaf-size        0.45
         :color-base       [0.16 0.30 0.08]
         :color-tip        [0.32 0.54 0.14]}}}
")

;; ════════════════════════════════════════════════════════════════════════
;; enum id <-> keyword maps (hyphenated keyword ids -> `vegetation.taxonomy` keywords)
;; ════════════════════════════════════════════════════════════════════════
;;
;; `kotoba-lang/vegetation` has the equivalent `parse-*` fns but they are
;; private; mirroring them here keeps the engine namespace untouched. The
;; default arm matches the engine's own JSON-bridge fallback (`taxonomy/from-json-map`)
;; so unknown/absent ids inherit the same fallback variant.

(defn canopy-from-id
  "Map a `:canopy` keyword id (string) to a `vegetation.taxonomy` canopy
  keyword (default `:carpet`, as in the engine)."
  [s]
  (case s
    "blade" :blade
    "fan" :fan
    "dome" :dome
    "cone" :cone
    "radial" :radial
    "column" :column
    :carpet))

(defn id-from-canopy
  "The hyphenated keyword id for a canopy keyword (inverse of [[canopy-from-id]])."
  [c]
  (case c
    :blade "blade"
    :fan "fan"
    :dome "dome"
    :cone "cone"
    :radial "radial"
    :column "column"
    :carpet "carpet"))

(defn division-from-id
  "Map a `:division` keyword id (string) to a `vegetation.taxonomy` division
  keyword (default `:angiospermae`, as in the engine)."
  [s]
  (case s
    "bryophyta" :bryophyta
    "pteridophyta" :pteridophyta
    "gymnospermae" :gymnospermae
    :angiospermae))

(defn habit-from-id
  "Map a `:habit` keyword id (string) to a `vegetation.taxonomy` growth-habit
  keyword (default `:herb`, as in the engine)."
  [s]
  (case s
    "grass" :grass
    "shrub" :shrub
    "tree" :tree
    "succulent" :succulent
    "mat" :mat
    "climber" :climber
    :herb))

(defn arrangement-from-id
  "Map an `:arrangement` keyword id (string) to a `vegetation.taxonomy`
  leaf-arrangement keyword (default `:none`, as in the engine)."
  [s]
  (case s
    "alternate" :alternate
    "opposite" :opposite
    "whorled" :whorled
    "rosette" :rosette
    "basal" :basal
    :none))

(defn leaf-shape-from-id
  "Map a `:leaf-shape` keyword id (string) to a `vegetation.taxonomy`
  leaf-shape keyword (default `:scale`, as in the engine)."
  [s]
  (case s
    "linear" :linear
    "lanceolate" :lanceolate
    "ovate" :ovate
    "palmate" :palmate
    "pinnate" :pinnate
    "needle" :needle
    "succulent" :succulent
    :scale))

;; ════════════════════════════════════════════════════════════════════════
;; ProfileSpec — the EDN-loaded mirror of a hardcoded taxonomy preset
;; ════════════════════════════════════════════════════════════════════════
;; Keys mirror the original Rust `ProfileSpec` struct fields 1:1
;; (snake_case -> kebab-case), and are identical to the map shape
;; `vegetation.taxonomy`'s preset builders already return:
;;   :common-name :division :habit :arrangement :leaf-shape :canopy
;;   :height-range :stem-radius-base :stem-radius-top :leaf-count
;;   :leaf-size :color-base :color-tip

(def profile-spec-defaults
  "The default ProfileSpec: every field read from `vegetation.taxonomy/moss`
  — the same profile the JSON bridge (`vegetation.taxonomy/from-json-map`)
  falls back to for unknown enum ids. Used as the merge base so an EDN
  profile that omits a field inherits a real engine value."
  (taxonomy/moss))

(defn profile-spec-from-map
  "Build a ProfileSpec from one profile's parsed EDN map `m`, merging
  present keys onto [[profile-spec-defaults]]."
  [m]
  (let [d profile-spec-defaults
        id (fn [key] (some-> (scene/mget m key) scene/kw-key))]
    {:common-name (or (scene/mget m "common-name") (:common-name d))
     :division (if-let [s (id "division")] (division-from-id s) (:division d))
     :habit (if-let [s (id "habit")] (habit-from-id s) (:habit d))
     :arrangement (if-let [s (id "arrangement")] (arrangement-from-id s) (:arrangement d))
     :leaf-shape (if-let [s (id "leaf-shape")] (leaf-shape-from-id s) (:leaf-shape d))
     :canopy (if-let [s (id "canopy")] (canopy-from-id s) (:canopy d))
     :height-range (let [rows (scene/mget m "height-range")]
                     (if (vector? rows)
                       [(scene/num (get rows 0)) (scene/num (get rows 1))]
                       (:height-range d)))
     :stem-radius-base (if-let [v (scene/mget m "stem-radius-base")]
                          (scene/num v)
                          (:stem-radius-base d))
     :stem-radius-top (if-let [v (scene/mget m "stem-radius-top")]
                         (scene/num v)
                         (:stem-radius-top d))
     :leaf-count (if-let [v (scene/mget m "leaf-count")]
                   (long (Math/round (double (scene/num v))))
                   (:leaf-count d))
     :leaf-size (if-let [v (scene/mget m "leaf-size")]
                  (scene/num v)
                  (:leaf-size d))
     :color-base (if-let [v (scene/mget m "color-base")]
                   (scene/vec3 v)
                   (:color-base d))
     :color-tip (if-let [v (scene/mget m "color-tip")]
                  (scene/vec3 v)
                  (:color-tip d))}))

(defn to-taxonomic-profile
  "Convert a ProfileSpec into a real `vegetation.taxonomy`-shaped
  TaxonomicProfile map — behaviourally identical to the hardcoded
  `taxonomy/{grass,...,moss}` builders. In the original Rust this
  resolved `common_name` to a `&'static str` via a known-name lookup
  table (the field required a static lifetime); CLJC strings are always
  owned, so a ProfileSpec already *is* a TaxonomicProfile map and this
  is the identity function, kept only for API parity."
  [spec]
  spec)

;; ════════════════════════════════════════════════════════════════════════
;; builtin fallback / parity oracle
;; ════════════════════════════════════════════════════════════════════════

(def all-profile-names
  "Names of the profiles shipped as the compiled-in oracle (iteration
  source for `builtin-profile`/parity). Keeping this list here (not in
  `vegetation.taxonomy`) keeps the engine namespace untouched. Order
  mirrors `vegetation.taxonomy/default-catalog` declaration order."
  ["grass" "fern" "palm" "conifer" "bush" "cactus" "moss"])

(defn builtin-profile
  "The compiled-in fallback / parity oracle: build a ProfileSpec straight
  from the hardcoded `vegetation.taxonomy/{grass,...,moss}` builders.
  Returns nil for an unknown name. This is what the shipped EDN is
  parity-tested against."
  [name]
  (case name
    "grass" (taxonomy/grass)
    "fern" (taxonomy/fern)
    "palm" (taxonomy/palm)
    "conifer" (taxonomy/conifer)
    "bush" (taxonomy/bush)
    "cactus" (taxonomy/cactus)
    "moss" (taxonomy/moss)
    nil))

;; ════════════════════════════════════════════════════════════════════════
;; EDN parsing / loading
;; ════════════════════════════════════════════════════════════════════════

(defn profiles-from-edn
  "Parse the whole `:vegetation/profiles` table from EDN `src` into a map
  keyed by the (hyphenated) profile id, each value the merged
  ProfileSpec.

  Throws `ex-info` with `:vegetation-scene/error` of `:not-a-map` (EDN
  root didn't parse to a map) or `:no-profiles` (`:vegetation/profiles`
  missing or not a map) on failure — mirroring the original
  `Error::NotAMap` / `Error::NoProfiles`."
  [src]
  (let [root (scene/root-map src)]
    (when (nil? root)
      (throw (ex-info "vegetation EDN root is not a map"
                       {:vegetation-scene/error :not-a-map})))
    (let [profiles (scene/mget root "vegetation/profiles")]
      (when-not (map? profiles)
        (throw (ex-info "`:vegetation/profiles` missing or not a map"
                         {:vegetation-scene/error :no-profiles})))
      (reduce (fn [acc [k v]]
                (if-let [id (scene/kw-key k)]
                  (if (map? v)
                    (assoc acc id (profile-spec-from-map v))
                    acc)
                  acc))
              {}
              profiles))))

(defn profile-from-edn
  "Look up a single profile by (hyphenated) `name` from EDN `src`. Throws
  `ex-info` with `:vegetation-scene/error :profile-not-found` if the
  table or the named profile is absent (also propagates
  [[profiles-from-edn]]'s errors)."
  [src name]
  (let [profiles (profiles-from-edn src)]
    (if-let [spec (get profiles name)]
      spec
      (throw (ex-info (str "profile `" name "` not found under `:vegetation/profiles`")
                       {:vegetation-scene/error :profile-not-found
                        :vegetation-scene/profile name})))))

(defn shipped-profiles
  "Convenience: load all profiles from the crate-shipped [[vegetation-edn]]."
  []
  (profiles-from-edn vegetation-edn))

(defn shipped-profile
  "Convenience: load one profile from the shipped EDN."
  [name]
  (profile-from-edn vegetation-edn name))
