(ns autodrive-scene
  "KAMI Autodrive Scene — EDN authoring surface for `kami-autodrive`
  PER-VEHICLE-CLASS PRESETS (the drive.gftd.ai autonomy stack's
  per-class config).

  The data-tier counterpart of `kami-vehicle-scene` /
  `kami-atmosphere-scene` / `kami-terrain-scene` / `kami-vegetation-scene`
  / `kami-postfx-scene` for the autonomy (GNC) stack: it turns canonical
  `:autodrive/limits` EDN (per-class kinematic envelopes) and
  `:autodrive/autopilot` EDN (per-class autopilot tuning) into vehicle
  limits / autopilot config data, re-using the tolerant `scene`
  (`kami-scene`) accessors the same way games parse `scene.edn`
  (missing keys fall back to defaults, namespaced keywords match on
  `ns/name`, ints coerce to floats).

  Restored from the legacy kami-engine/kami-autodrive-scene Rust crate
  (deleted in kotoba-lang/kami-engine PR #82 'Remove Rust workspace
  from kami-engine') as part of the clj-wgsl migration
  (ADR-2607010930, com-junkawasaki/root).

  ## Dependency relationship — duck-typed, not a real dep

  The original Rust crate depended on `kami-autodrive` for the real
  `VehicleLimits` / `AutopilotConfig` / `VehicleClass` engine structs,
  using their compiled-in `VehicleClass::limits()` /
  `AutopilotConfig::for_class()` as the `builtin_*()` parity oracle
  asserted `==` against the shipped EDN.

  `kotoba-lang/kami-autodrive` did not exist yet at restoration time
  (it was being restored in parallel by a sibling task), so this
  namespace does NOT depend on it. Instead `builtin-limits` /
  `builtin-autopilot` below are duck-typed local maps mirroring the
  exact values in `CLASSES-EDN` (matching the pattern used for
  `kotoba-lang/kami-vehicle-scene` against a not-yet-existing
  `kami-vehicle`). `VehicleLimits`/`AutopilotConfig` are represented as
  plain hyphen-keyed maps rather than a foreign engine struct — this
  namespace has no compile-time dependency on any domain crate.

  Unlike the original (which hand-parsed EDN via `kotoba_edn::EdnValue`
  since Rust has no native EDN reader), this namespace parses via
  `scene/root-map` (backed by `clojure.edn/read-string`) — keys are
  already real keywords, so `scene/kw-key` / `scene/mget` operate
  directly on them.

  Zero-dep portable CLJC (aside from `scene`, itself zero-dep)."
  (:require [kotoba.lang.text :as str]
            [scene :as scene]))

;; ── shipped EDN CONFIG ──────────────────────────────────────────────

(def classes-edn
  "The canonical per-class preset CONFIG shipped with this namespace
  (both tables). This is the source of truth; `builtin-limits-table` /
  `builtin-autopilot-table` are the duck-typed, parity-tested mirror."
  "{:autodrive/limits
 {:car {:max-speed 25.0 :max-accel 4.0 :max-decel 8.0 :wheelbase 2.7
        :max-steer 0.61 :turn-radius-ref 4.5 :footprint-radius 1.3}
  :ship {:max-speed 8.0 :max-accel 0.5 :max-decel 1.0 :wheelbase 30.0
         :max-steer 0.52 :turn-radius-ref 40.0 :footprint-radius 6.0}
  :drone {:max-speed 15.0 :max-accel 6.0 :max-decel 6.0 :wheelbase 0.5
          :max-steer 1.20 :turn-radius-ref 2.5 :footprint-radius 0.6}
  :aircraft {:max-speed 60.0 :max-accel 3.0 :max-decel 4.0 :wheelbase 15.0
             :max-steer 0.35 :turn-radius-ref 250.0 :footprint-radius 8.0}}
 :autodrive/autopilot
 {:car {:grid-half-extent 60.0 :grid-res 0.5 :z-band [-1.0 1.5] :replan-period 20
        :goal-tol 1.3 :emergency-cone 0.35 :lateral-accel 3.0 :brake-margin 1.6
        :dynamic-obstacles true :camera-z-band [0.3 2.5] :stuck-limit 0
        :recovery-ticks 60}
  :ship {:grid-half-extent 60.0 :grid-res 0.5 :z-band [-1.0 1.5] :replan-period 20
         :goal-tol 6.0 :emergency-cone 0.35 :lateral-accel 3.0 :brake-margin 1.6
         :dynamic-obstacles true :camera-z-band [0.3 2.5] :stuck-limit 0
         :recovery-ticks 60}
  :drone {:grid-half-extent 60.0 :grid-res 0.5 :z-band [-1.0 1.5] :replan-period 20
          :goal-tol 1.0 :emergency-cone 0.35 :lateral-accel 3.0 :brake-margin 1.6
          :dynamic-obstacles true :camera-z-band [0.3 2.5] :stuck-limit 0
          :recovery-ticks 60}
  :aircraft {:grid-half-extent 60.0 :grid-res 0.5 :z-band [-1.0 1.5] :replan-period 20
             :goal-tol 8.0 :emergency-cone 0.35 :lateral-accel 3.0 :brake-margin 1.6
             :dynamic-obstacles true :camera-z-band [0.3 2.5] :stuck-limit 0
             :recovery-ticks 60 :loiter-radius 200.0}}}")

(def all-class-names
  "The four class ids — the iteration source for builtin/parity. Order
  mirrors the original `enum VehicleClass` declaration order."
  ["car" "ship" "drone" "aircraft"])

;; ── errors ───────────────────────────────────────────────────────────
;;
;; Ported as tagged maps rather than an exception hierarchy, so callers
;; can pattern-match on `:kind` the same way the original matched on
;; `Error` variants. `error?` distinguishes an error map from a
;; successful (id -> spec) result map.

(defn error
  "Build an error map of `kind` with optional extra fields."
  [kind & {:as extra}]
  (merge {:kind kind} extra))

(defn error?
  "True if `x` is one of this namespace's error maps."
  [x]
  (and (map? x) (contains? x :kind)))

;; ── small typed accessors over the tolerant `scene` helpers ───────────

(defn- round
  [x]
  #?(:clj (Math/round (double x))
     :cljs (js/Math.round x)))

(defn- f32-of
  "Read a numeric field; 0.0 when absent / non-numeric (via `scene/num`)."
  [m key]
  (scene/num (scene/mget m key)))

(defn- u32-of
  "Read an integer field, coercing via `scene/num` then rounding; 0
  when absent."
  [m key]
  (if-let [v (scene/mget m key)]
    (round (scene/num v))
    0))

(defn- bool-of
  "Read a bool field; false when absent / non-bool."
  [m key]
  (let [v (scene/mget m key)]
    (if (boolean? v) v false)))

(defn- pair-of
  "Read a 2-tuple `[lo hi]`; missing components default to 0.0."
  [m key]
  (let [v (scene/mget m key)
        s (if (vector? v) v [])
        g (fn [i] (scene/num (get s i)))]
    [(g 0) (g 1)]))

;; ── class id <-> class keyword ──────────────────────────────────────

(defn try-class-from-id
  "Checked class-id -> class-keyword lookup (hyphen/underscore
  tolerant, case-insensitive on the bare name); nil for an unknown id."
  [id]
  (case (-> id str/lower (str/replace "_" "-"))
    "car" :car
    "ship" :ship
    "drone" :drone
    "aircraft" :aircraft
    nil))

(defn class-from-id
  "Map a class id to its class keyword; falls back to `:car` for an
  unknown id (mirroring the tolerant data-tier style). Use
  `try-class-from-id` for a checked lookup."
  [id]
  (or (try-class-from-id id) :car))

(defn class-id
  "The hyphenated class id for a class keyword (inverse of
  `try-class-from-id`)."
  [class]
  (name class))

;; ── :autodrive/limits ──────────────────────────────────────────────
;;
;; A `LimitsSpec` / `VehicleLimits` is a plain map with keys
;; :max-speed :max-accel :max-decel :wheelbase :max-steer
;; :turn-radius-ref :footprint-radius (identical shapes here, so
;; `limits-spec->vehicle-limits` / `vehicle-limits->limits-spec` are
;; identity — kept as named fns for API parity with the original).

(defn limits-spec->vehicle-limits [spec] spec)
(defn vehicle-limits->limits-spec [limits] limits)

(defn- limits-spec-from-map
  [m]
  {:max-speed (f32-of m "max-speed")
   :max-accel (f32-of m "max-accel")
   :max-decel (f32-of m "max-decel")
   :wheelbase (f32-of m "wheelbase")
   :max-steer (f32-of m "max-steer")
   :turn-radius-ref (f32-of m "turn-radius-ref")
   :footprint-radius (f32-of m "footprint-radius")})

(def builtin-limits-table
  "The duck-typed compiled-in fallback / parity oracle: mirrors the
  real `VehicleClass::limits()` values (there being no `kami-autodrive`
  dependency here). This is what `classes-edn` is parity-tested
  against."
  {:car {:max-speed 25.0 :max-accel 4.0 :max-decel 8.0 :wheelbase 2.7
         :max-steer 0.61 :turn-radius-ref 4.5 :footprint-radius 1.3}
   :ship {:max-speed 8.0 :max-accel 0.5 :max-decel 1.0 :wheelbase 30.0
          :max-steer 0.52 :turn-radius-ref 40.0 :footprint-radius 6.0}
   :drone {:max-speed 15.0 :max-accel 6.0 :max-decel 6.0 :wheelbase 0.5
           :max-steer 1.20 :turn-radius-ref 2.5 :footprint-radius 0.6}
   :aircraft {:max-speed 60.0 :max-accel 3.0 :max-decel 4.0 :wheelbase 15.0
              :max-steer 0.35 :turn-radius-ref 250.0 :footprint-radius 8.0}})

(defn builtin-limits
  "The duck-typed fallback / parity oracle for one class keyword."
  [class]
  (get builtin-limits-table class))

(defn- autodrive-table
  "Resolve the `:autodrive/<table>` sub-map of `root`, or an
  `:no-table` error."
  [root table]
  (let [t (scene/mget root (str "autodrive/" table))]
    (if (map? t) t (error :no-table :table table))))

(defn limits-specs-from-edn
  "Parse the whole `:autodrive/limits` table from EDN `src` into a map
  keyed by the (hyphenated) class id, each value the loaded limits
  spec. Returns an error map on failure."
  [src]
  (let [root (scene/root-map src)]
    (if-not root
      (error :not-a-map)
      (let [table (autodrive-table root "limits")]
        (if (error? table)
          table
          (reduce (fn [acc [k v]]
                    (let [id (scene/kw-key k)]
                      (if (and id (map? v))
                        (assoc acc id (limits-spec-from-map v))
                        acc)))
                  {}
                  table))))))

(defn limits-from-edn
  "Parse the `:autodrive/limits` table into `id -> VehicleLimits` (the
  data-driven counterpart of `VehicleClass::limits()`). Limits specs
  and `VehicleLimits` are the same map shape here, so this is
  `limits-specs-from-edn` under a name matching the original API."
  [src]
  (limits-specs-from-edn src))

(defn limits-for-from-edn
  "Look up one class's limits (hyphen/underscore tolerant) from EDN."
  [src name]
  (let [id (-> name str/lower (str/replace "_" "-"))
        table (limits-from-edn src)]
    (if (error? table)
      table
      (or (get table id)
          (get table name)
          (error :class-not-found :class name)))))

(defn shipped-limits
  "Load all per-class limits from the namespace-shipped `classes-edn`."
  []
  (limits-from-edn classes-edn))

(defn shipped-limits-for
  "Load one class's limits from the shipped EDN."
  [name]
  (limits-for-from-edn classes-edn name))

;; ── :autodrive/autopilot ────────────────────────────────────────────
;;
;; An `AutopilotSpec` is a plain map of every tunable field except
;; `:limits` (sourced separately from the matching `:autodrive/limits`
;; entry, mirroring `AutopilotConfig::for_class` pulling it from
;; `class.limits()`). An `AutopilotConfig` is the same spec map plus a
;; `:limits` key.

(defn autopilot-spec->autopilot-config
  "Attach `limits` (resolved from the matching `:autodrive/limits`
  entry) to an autopilot spec, producing the full config map."
  [spec limits]
  (assoc spec :limits limits))

(defn autopilot-config->autopilot-spec
  "Project a config map back to its spec (drops `:limits`) — the
  `PartialEq` mirror used for parity comparisons in the original."
  [config]
  (dissoc config :limits))

(defn- autopilot-spec-from-map
  [m]
  {:grid-half-extent (f32-of m "grid-half-extent")
   :grid-res (f32-of m "grid-res")
   :z-band (pair-of m "z-band")
   :replan-period (u32-of m "replan-period")
   :goal-tol (f32-of m "goal-tol")
   :emergency-cone (f32-of m "emergency-cone")
   :lateral-accel (f32-of m "lateral-accel")
   :brake-margin (f32-of m "brake-margin")
   :dynamic-obstacles (bool-of m "dynamic-obstacles")
   :camera-z-band (pair-of m "camera-z-band")
   :stuck-limit (u32-of m "stuck-limit")
   :recovery-ticks (u32-of m "recovery-ticks")
   ;; Absent :loiter-radius = a normal stopping vehicle (nil).
   :loiter-radius (when-let [v (scene/mget m "loiter-radius")]
                    (scene/num v))})

(def builtin-autopilot-table
  "The duck-typed compiled-in fallback / parity oracle: mirrors the
  real `AutopilotConfig::for_class()` values (there being no
  `kami-autodrive` dependency here; each entry's `:limits` comes from
  `builtin-limits-table`)."
  {:car {:grid-half-extent 60.0 :grid-res 0.5 :z-band [-1.0 1.5] :replan-period 20
         :goal-tol 1.3 :emergency-cone 0.35 :lateral-accel 3.0 :brake-margin 1.6
         :dynamic-obstacles true :camera-z-band [0.3 2.5] :stuck-limit 0
         :recovery-ticks 60 :loiter-radius nil}
   :ship {:grid-half-extent 60.0 :grid-res 0.5 :z-band [-1.0 1.5] :replan-period 20
          :goal-tol 6.0 :emergency-cone 0.35 :lateral-accel 3.0 :brake-margin 1.6
          :dynamic-obstacles true :camera-z-band [0.3 2.5] :stuck-limit 0
          :recovery-ticks 60 :loiter-radius nil}
   :drone {:grid-half-extent 60.0 :grid-res 0.5 :z-band [-1.0 1.5] :replan-period 20
           :goal-tol 1.0 :emergency-cone 0.35 :lateral-accel 3.0 :brake-margin 1.6
           :dynamic-obstacles true :camera-z-band [0.3 2.5] :stuck-limit 0
           :recovery-ticks 60 :loiter-radius nil}
   :aircraft {:grid-half-extent 60.0 :grid-res 0.5 :z-band [-1.0 1.5] :replan-period 20
              :goal-tol 8.0 :emergency-cone 0.35 :lateral-accel 3.0 :brake-margin 1.6
              :dynamic-obstacles true :camera-z-band [0.3 2.5] :stuck-limit 0
              :recovery-ticks 60 :loiter-radius 200.0}})

(defn builtin-autopilot
  "The duck-typed fallback / parity oracle for one class keyword;
  attaches `:limits` from `builtin-limits`."
  [class]
  (autopilot-spec->autopilot-config
    (autopilot-config->autopilot-spec (get builtin-autopilot-table class))
    (builtin-limits class)))

(defn autopilot-specs-from-edn
  "Parse the whole `:autodrive/autopilot` table from EDN `src` into a
  map keyed by the (hyphenated) class id, each value the loaded
  autopilot spec (without its `:limits` — resolve those from
  `limits-specs-from-edn`)."
  [src]
  (let [root (scene/root-map src)]
    (if-not root
      (error :not-a-map)
      (let [table (autodrive-table root "autopilot")]
        (if (error? table)
          table
          (reduce (fn [acc [k v]]
                    (let [id (scene/kw-key k)]
                      (if (and id (map? v))
                        (assoc acc id (autopilot-spec-from-map v))
                        acc)))
                  {}
                  table))))))

(defn autopilot-from-edn
  "Parse the `:autodrive/autopilot` table into `id -> AutopilotConfig`,
  resolving each class's `:limits` from the `:autodrive/limits` table
  in the same EDN. The data-driven counterpart of
  `AutopilotConfig::for_class()`."
  [src]
  (let [limits (limits-from-edn src)]
    (if (error? limits)
      limits
      (let [specs (autopilot-specs-from-edn src)]
        (if (error? specs)
          specs
          (reduce (fn [acc [id spec]]
                    (if (error? acc)
                      acc
                      (if-let [l (get limits id)]
                        (assoc acc id (autopilot-spec->autopilot-config spec l))
                        (error :class-not-found :class id))))
                  {}
                  specs))))))

(defn autopilot-for-from-edn
  "Look up one class's autopilot config (hyphen/underscore tolerant)
  from EDN."
  [src name]
  (let [id (-> name str/lower (str/replace "_" "-"))
        table (autopilot-from-edn src)]
    (if (error? table)
      table
      (or (get table id)
          (get table name)
          (error :class-not-found :class name)))))

(defn shipped-autopilot
  "Load all per-class autopilot configs from the shipped `classes-edn`."
  []
  (autopilot-from-edn classes-edn))

(defn shipped-autopilot-for
  "Load one class's autopilot config from the shipped EDN."
  [name]
  (autopilot-for-from-edn classes-edn name))
