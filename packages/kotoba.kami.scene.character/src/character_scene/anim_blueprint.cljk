(ns character-scene.anim-blueprint
  "Anim-blueprint data tier — `character`'s MetaHuman default animation
  blueprint (`character.anim-blueprint/metahuman-default`) as
  parity-tested EDN. Restored from the legacy
  kami-engine/kami-character-scene Rust crate's `anim_blueprint.rs`
  (deleted in kotoba-lang/kami-engine PR #82) as part of the clj-wgsl
  migration (ADR-2607010930, com-junkawasaki/root).

  The state-machine **evaluation** (`character.anim-blueprint/update` /
  transitions / blend) stays native; only the init-time
  **description** — parameters, layers, states, blend spaces,
  transitions, blend profiles — moves to EDN (ADR-0046 / ADR-0038).
  `blueprint-from-edn` rebuilds a blueprint map in the *exact same*
  shape as `character.anim-blueprint/metahuman-default`, asserted
  component-for-component `=` it in the test suite.

  `:param-type` is DERIVED (`:bool` when the name starts with `is_`,
  else `:float`), mirroring the builtin, so each parameter in the EDN
  only carries `{:name :default}`.

  Depends on `kotoba-lang/scene` (tolerant EDN accessors) only —
  `character.anim-blueprint` is a read-only oracle used by tests, not
  by this namespace's runtime logic."
  (:require [kotoba.lang.text :as str]
            [scene :as scene]))

;; --- shipped EDN -----------------------------------------------------------------------

(def anim-blueprint-edn
  "The canonical anim-blueprint CONFIG shipped with this crate. Also copied
  verbatim to `resources/character_scene/anim_blueprint.edn`."
  "{:character/anim-blueprint
 {:parameters
  [{:name \"speed\"          :default 0.0}
   {:name \"direction\"      :default 0.0}
   {:name \"is_moving\"      :default 0.0}
   {:name \"emotion_happy\"  :default 0.0}
   {:name \"emotion_sad\"    :default 0.0}
   {:name \"emotion_angry\"  :default 0.0}
   {:name \"blink\"          :default 0.0}
   {:name \"look_x\"         :default 0.0}
   {:name \"look_y\"         :default 0.0}
   {:name \"jaw_open\"       :default 0.0}
   {:name \"breath_cycle\"   :default 0.0}]

  :layers
  [{:name \"body\" :blend-mode :override :weight 1.0
    :states
    [{:name \"idle\" :play-rate 1.0 :looping true
      :type {:kind :clip :clip-name \"idle_breathe\"}}
     {:name \"locomotion\" :play-rate 1.0 :looping true
      :type {:kind :blend-space-1d :axis-param \"speed\"
             :entries [{:clip \"walk\" :position 0.3}
                       {:clip \"jog\"  :position 0.6}
                       {:clip \"run\"  :position 1.0}]}}]
    :transitions
    [{:source 0 :target 1 :duration 0.3 :curve :ease-in-out :priority 1
      :conditions [{:param \"is_moving\" :cmp :greater :threshold 0.5}]}
     {:source 1 :target 0 :duration 0.4 :curve :ease-out :priority 1
      :conditions [{:param \"is_moving\" :cmp :less :threshold 0.5}]}]}

   {:name \"face\" :blend-mode :additive :weight 1.0
    :states
    [{:name \"face_idle\" :play-rate 1.0 :looping true
      :type {:kind :clip :clip-name \"face_idle_micro\"}}
     {:name \"face_talking\" :play-rate 1.0 :looping true
      :type {:kind :blend-space-1d :axis-param \"jaw_open\"
             :entries [{:clip \"viseme_rest\" :position 0.0}
                       {:clip \"viseme_open\" :position 1.0}]}}]
    :transitions
    [{:source 0 :target 1 :duration 0.15 :curve :linear :priority 1
      :conditions [{:param \"jaw_open\" :cmp :greater :threshold 0.1}]}
     {:source 1 :target 0 :duration 0.2 :curve :ease-out :priority 1
      :conditions [{:param \"jaw_open\" :cmp :less :threshold 0.1}]}]}]

  :blend-profiles
  [{:name \"upper_body\"
    :bone-weights {:head 1.0 :neck 1.0 :upperChest 0.8 :chest 0.5
                   :leftShoulder 0.9 :rightShoulder 0.9
                   :leftUpperArm 1.0 :rightUpperArm 1.0}}]}}")

;; --- errors ------------------------------------------------------------------------------

(defn ->error
  "Error map: `:type` one of `:not-a-map` `:no-blueprint`."
  [type]
  {:type type})

;; --- tolerant scalar readers ---------------------------------------------------------------

(defn- str-at [m key] (or (scene/mget m key) ""))
(defn- bool-at [m key] (boolean (scene/mget m key)))
(defn- int-at [m key] (long (or (scene/mget m key) 0)))
(defn- usize-at [m key] (max 0 (int-at m key)))
(defn- kw-at [m key] (or (some-> (scene/mget m key) scene/kw-key) ""))
(defn- vec-at [m key] (let [v (scene/mget m key)] (if (vector? v) v [])))

;; --- enum mappers --------------------------------------------------------------------------

(defn- blend-mode [id] (case id "additive" :additive :override))
(defn- blend-curve [id]
  (case id
    "ease-in" :ease-in
    "ease-out" :ease-out
    "ease-in-out" :ease-in-out
    "cubic" :cubic
    :linear))
(defn- comparison [id]
  (case id
    "less" :less
    "equal" :equal
    "not-equal" :not-equal
    "greater-equal" :greater-equal
    "less-equal" :less-equal
    :greater))

;; --- sub-parsers ---------------------------------------------------------------------------

(defn- state-type [m]
  (let [t (scene/mget m "type")]
    (if-not (map? t)
      {:type :pose-snapshot}
      (case (kw-at t "kind")
        "clip" {:type :clip :clip-name (str-at t "clip-name")}
        "blend-space-1d" {:type :blend-space-1d
                           :axis-param (str-at t "axis-param")
                           :entries (into []
                                          (comp (filter map?)
                                                (map (fn [e] {:clip-name (str-at e "clip")
                                                              :position (scene/num (scene/mget e "position"))})))
                                          (vec-at t "entries"))}
        "blend-space-2d" {:type :blend-space-2d
                           :x-param (str-at t "x-param")
                           :y-param (str-at t "y-param")
                           :entries (into []
                                          (comp (filter map?)
                                                (map (fn [e] {:clip-name (str-at e "clip")
                                                              :x (scene/num (scene/mget e "x"))
                                                              :y (scene/num (scene/mget e "y"))})))
                                          (vec-at t "entries"))}
        "layered-blend-per-bone" {:type :layered-blend-per-bone
                                   :base-clip (str-at t "base-clip")
                                   :overlay-clip (str-at t "overlay-clip")
                                   :bone-filter (into [] (filter string?) (vec-at t "bone-filter"))
                                   :blend-param (str-at t "blend-param")}
        {:type :pose-snapshot}))))

(defn- anim-state [m]
  {:name (str-at m "name")
   :state-type (state-type m)
   :play-rate (scene/num (scene/mget m "play-rate"))
   :looping (bool-at m "looping")})

(defn- transition [m]
  {:source (usize-at m "source")
   :target (usize-at m "target")
   :duration (scene/num (scene/mget m "duration"))
   :blend-curve (blend-curve (kw-at m "curve"))
   :conditions (into []
                     (comp (filter map?)
                           (map (fn [c] {:param-name (str-at c "param")
                                         :comparison (comparison (kw-at c "cmp"))
                                         :threshold (scene/num (scene/mget c "threshold"))})))
                     (vec-at m "conditions"))
   :priority (max 0 (int-at m "priority"))})

(defn- layer [m]
  {:name (str-at m "name")
   :blend-mode (blend-mode (kw-at m "blend-mode"))
   :weight (scene/num (scene/mget m "weight"))
   :states (into [] (comp (filter map?) (map anim-state)) (vec-at m "states"))
   :transitions (into [] (comp (filter map?) (map transition)) (vec-at m "transitions"))
   ;; The default blueprint starts at state 0, not mid-transition. These are
   ;; runtime fields the builtin initialises to 0 / nil; a preset may
   ;; override :active-state but otherwise inherits the resting values.
   :active-state (usize-at m "active-state")
   :transition-progress (scene/num (scene/mget m "transition-progress"))
   :transition-target (let [v (scene/mget m "transition-target")]
                         (when (number? v) (max 0 (long v))))})

(defn- blend-profile [m]
  (let [bw (scene/mget m "bone-weights")]
    {:name (str-at m "name")
     :bone-weights (if (map? bw)
                     (into {} (keep (fn [[k v]] (when-let [name (scene/kw-key k)]
                                                   [name (scene/num v)])))
                           bw)
                     {})}))

(defn- anim-param [m]
  (let [name (str-at m "name")
        default (scene/num (scene/mget m "default"))]
    {:name name
     :param-type (if (str/starts-with? name "is_") :bool :float)
     :value default
     :default-value default}))

;; --- public API ----------------------------------------------------------------------------

(defn blueprint-from-edn
  "Build a `character.anim-blueprint`-shaped blueprint map from EDN `src`.
  Returns `[:ok blueprint]` or `[:error error-map]`."
  [src]
  (if-let [root (scene/root-map src)]
    (let [bp (scene/mget root "character/anim-blueprint")]
      (if (map? bp)
        (let [parameters (into {}
                                (comp (filter map?) (map anim-param) (map (fn [p] [(:name p) p])))
                                (vec-at bp "parameters"))
              layers (into [] (comp (filter map?) (map layer)) (vec-at bp "layers"))
              blend-profiles (into [] (comp (filter map?) (map blend-profile)) (vec-at bp "blend-profiles"))]
          [:ok {:parameters parameters :layers layers :blend-profiles blend-profiles}])
        [:error (->error :no-blueprint)]))
    [:error (->error :not-a-map)]))

(defn shipped-blueprint
  "Convenience: build the blueprint from the crate-shipped `anim-blueprint-edn`."
  []
  (blueprint-from-edn anim-blueprint-edn))
