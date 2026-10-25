(ns vehicle-scene.garage
  "KAMI Vehicle Scene / Garage — EDN authoring surface for kami-vehicle's
  GARAGE + POWERTRAIN CONFIG (engine torque curves, gearbox ratios,
  Pacejka tire coefficients, and the 6 garage vehicle `SedanSpec`s).
  Restored from the legacy `kami-vehicle-scene` Rust crate's
  `src/garage.rs` (kotoba-lang/kami-engine, deleted in PR #82 'Remove
  Rust workspace from kami-engine', recoverable at commit
  a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa) as part of the clj-wgsl
  migration (ADR-2607010930, com-junkawasaki/root).

  Turns canonical `:vehicle/*` EDN into duck-typed `kami-vehicle`
  engine-struct mirrors (`EngineSpec`/`GearboxSpec`/`TireSpec`/
  `LayoutSpec`/`GarageSpec`), re-using the tolerant `scene` accessors
  (`kotoba-lang/scene`) the same way `scene.edn` is parsed: missing keys
  fall back to defaults, namespaced keywords match on `ns/name`, ints
  coerce to doubles.

  ## Dependency relationships

  - `scene` (kotoba-lang/scene) supplies `kw-key`/`mget`/`num` — the
    tolerant EDN-map accessors.
  - `kotoba-lang/kami-vehicle` is the domain crate this data pairs with,
    but it is being restored *in parallel* by another migration pass and
    is not guaranteed to exist at the time this crate is restored. This
    namespace therefore **duck-types locally** rather than hard-depending
    on it: `to-engine`/`to-gearbox`/`to-pacejka`/`to-layout`/
    `to-sedan-spec` all produce plain CLJC maps shaped like the original
    Rust structs (`Engine`, `Gearbox`, `PacejkaParams`, `DrivelineLayout`,
    `kami_vehicle::models::sedan::SedanSpec`), inferred purely from this
    crate's own field usage (`e.idle_rpm`, `g.ratios`, `p.b_long`, …) —
    no `kami-vehicle` source was consulted or is required. `deps.edn`
    depends only on `kotoba-lang/scene`. A future pass can refactor these
    `to-*` conversions to build real `kotoba-lang/kami-vehicle` records
    once that crate is stable.

  ## Scope note: no soft-body physics build

  The original `build_from_spec`/`build_from_edn` called
  `kami_vehicle::models::sedan::sedan(&spec)` to assemble a full
  soft-body node-graph `Vehicle` — hundreds of lines of physics-engine
  logic that lives entirely in `kami-vehicle`, not in this crate, and
  cannot be inferred from this crate's own source. This port's
  `build-from-spec`/`build-from-edn` instead produce a **resolved build
  PLAN** — a duck-typed bundle of `{:sedan-spec :engine :gearbox :tires}`
  with every per-kind override already applied (engine torque-curve +
  effective max-rpm, gearbox final-drive, sticky tire d-long/d-lat) —
  i.e. everything this crate's original Rust *did* own (EDN -> resolved
  CONFIG), stopping exactly at the boundary where the original delegated
  to `kami_vehicle::models::sedan::sedan()`. A future pass that depends
  on real `kotoba-lang/kami-vehicle` can feed this plan's `:sedan-spec`
  straight into a ported `sedan` constructor to get the original's exact
  `Vehicle` node graph."
  (:require [kotoba.lang.text :as str]
            [scene :as scene]))

;; ---------------------------------------------------------------------
;; Shipped EDN (compile-time embedded, matches
;; resources/kami_vehicle_scene/garage.edn byte-for-byte)
;; ---------------------------------------------------------------------

(def garage-edn
  "The canonical garage + powertrain CONFIG shipped with this namespace.
  This is the source of truth; `builtin-engine`/`builtin-gearbox`/
  `builtin-tire`/`garage-spec-builtin` below are the duck-typed
  compiled-in mirrors (kept numerically identical, per the original
  crate's own parity-tested guarantee that the shipped EDN equals the
  compiled-in `VehicleKind::spec`/`TorqueCurve::*`/`Gearbox::manual_6`/
  `PacejkaParams::*` builders)."
  "{:vehicle/engines
 {:na-2-0-gasoline
  {:idle-rpm 850.0 :max-rpm 7000.0 :inertia 0.18 :friction 35.0
   :torque-curve [[800.0 130.0] [1500.0 160.0] [2500.0 185.0] [3500.0 200.0]
                  [4500.0 200.0] [5500.0 185.0] [6500.0 150.0] [7000.0 0.0]]}
  :turbo-2-0
  {:idle-rpm 850.0 :max-rpm 7000.0 :inertia 0.18 :friction 35.0
   :torque-curve [[800.0 150.0] [1500.0 280.0] [2500.0 370.0] [3000.0 380.0]
                  [4500.0 370.0] [5500.0 320.0] [6500.0 220.0] [7000.0 0.0]]}
  :pickup-v6
  {:idle-rpm 850.0 :max-rpm 6000.0 :inertia 0.18 :friction 35.0
   :torque-curve [[800.0 280.0] [1500.0 380.0] [2500.0 480.0] [3500.0 470.0]
                  [4500.0 380.0] [5500.0 250.0] [6000.0 0.0]]}
  :bus-diesel
  {:idle-rpm 850.0 :max-rpm 3600.0 :inertia 0.18 :friction 35.0
   :torque-curve [[600.0 600.0] [1200.0 1100.0] [1800.0 1200.0] [2400.0 1100.0]
                  [3000.0 800.0] [3600.0 0.0]]}}

 :vehicle/gearboxes
 {:manual-6
  {:ratios [3.50 0.0 3.50 1.95 1.30 1.00 0.80 0.65]
   :final-drive 4.10 :inertia 0.05 :shift-time 0.35}}

 :vehicle/tires
 {:road-dry
  {:b-long 10.0 :c-long 1.65 :d-long 1.0  :e-long 0.97
   :b-lat   8.5 :c-lat  1.30 :d-lat  1.0  :e-lat  0.97}
  :road-wet
  {:b-long  8.0 :c-long 1.65 :d-long 0.70 :e-long 0.95
   :b-lat   7.0 :c-lat  1.30 :d-lat  0.70 :e-lat  0.95}}

 :vehicle/garage
 {:sedan
  {:wheelbase 2.70 :track-width 1.55 :ride-height 0.55 :roof-height 1.00
   :overhang-front 0.95 :overhang-rear 1.10
   :mass-chassis 820.0 :mass-engine 260.0 :mass-cabin 540.0
   :wheel-radius 0.32 :wheel-width 0.22
   :layout :fwd :turbo false
   :engine :na-2-0-gasoline :gearbox :manual-6 :final-drive 4.10 :diff :open
   :tire :road-dry}

  :hatchback
  {:wheelbase 2.45 :track-width 1.50 :ride-height 0.50 :roof-height 1.05
   :overhang-front 0.85 :overhang-rear 0.55
   :mass-chassis 700.0 :mass-engine 180.0 :mass-cabin 420.0
   :wheel-radius 0.30 :wheel-width 0.20
   :layout :fwd :turbo false
   :engine :na-2-0-gasoline :gearbox :manual-6 :final-drive 4.30 :diff :open
   :tire :road-dry :max-rpm 6800.0}

  :suv
  {:wheelbase 2.85 :track-width 1.65 :ride-height 0.65 :roof-height 1.15
   :overhang-front 1.00 :overhang-rear 1.05
   :mass-chassis 1100.0 :mass-engine 300.0 :mass-cabin 700.0
   :wheel-radius 0.36 :wheel-width 0.25
   :layout {:awd {:front-split 0.45}} :turbo true
   :engine :turbo-2-0 :gearbox :manual-6 :final-drive 4.50 :diff :open
   :tire :road-dry}

  :sports
  {:wheelbase 2.55 :track-width 1.62 :ride-height 0.42 :roof-height 0.85
   :overhang-front 0.85 :overhang-rear 0.85
   :mass-chassis 720.0 :mass-engine 240.0 :mass-cabin 380.0
   :wheel-radius 0.34 :wheel-width 0.26
   :layout :rwd :turbo true
   :engine :turbo-2-0 :gearbox :manual-6 :final-drive 3.85 :diff :open
   :tire :road-dry :max-rpm 7800.0 :tire-d-long 1.20 :tire-d-lat 1.20}

  :pickup
  {:wheelbase 3.20 :track-width 1.70 :ride-height 0.60 :roof-height 1.20
   :overhang-front 1.00 :overhang-rear 1.30
   :mass-chassis 1200.0 :mass-engine 320.0 :mass-cabin 480.0
   :wheel-radius 0.38 :wheel-width 0.27
   :layout :rwd :turbo false
   :engine :pickup-v6 :gearbox :manual-6 :final-drive 4.80 :diff :open
   :tire :road-dry}

  :bus
  {:wheelbase 4.50 :track-width 1.90 :ride-height 0.60 :roof-height 2.40
   :overhang-front 0.80 :overhang-rear 1.50
   :mass-chassis 1900.0 :mass-engine 480.0 :mass-cabin 1200.0
   :wheel-radius 0.42 :wheel-width 0.30
   :layout :rwd :turbo false
   :engine :bus-diesel :gearbox :manual-6 :final-drive 5.50 :diff :open
   :tire :road-dry}}}")

;; ---------------------------------------------------------------------
;; Small EDN helpers
;; ---------------------------------------------------------------------

(defn- kw-name
  "Read a keyword VALUE's bare/qualified name (e.g. `:road-dry` ->
  \"road-dry\"). \"\" when absent / non-keyword."
  [v]
  (or (scene/kw-key v) ""))

(defn- num-vec
  "Read a flat numeric vector `[a b c ..]` as a vector of doubles (ints
  coerce)."
  [v]
  (if (vector? v)
    (mapv #(scene/num %) v)
    []))

(defn- pairs
  "Read a vector of `[a b]` pairs `[[rpm nm] ..]` as a vector of `[rpm
  nm]` double pairs."
  [v]
  (if (vector? v)
    (into []
          (keep (fn [p]
                  (when (vector? p)
                    [(scene/num (get p 0)) (scene/num (get p 1))])))
          v)
    []))

(defn layout-from-value
  "Read a driveline `:layout` VALUE: `:fwd` / `:rwd` / `{:awd
  {:front-split ..}}` -> a LayoutSpec map `{:kind :fwd|:rwd|:awd
  :front-split n}` (`:front-split` present only for `:awd`)."
  [v]
  (cond
    (nil? v) {:kind :fwd}
    (keyword? v) (case (kw-name v)
                   "rwd" {:kind :rwd}
                   "awd" {:kind :awd :front-split 0.0}
                   {:kind :fwd})
    (map? v) (if-let [awd (scene/mget v "awd")]
               {:kind :awd :front-split (scene/num (scene/mget awd "front-split"))}
               {:kind :fwd})
    :else {:kind :fwd}))

;; ---------------------------------------------------------------------
;; EngineSpec — duck-typed mirror of a `kami_vehicle::Engine` preset
;; ---------------------------------------------------------------------

(defn engine-spec
  "Build an EngineSpec map `{:idle-rpm :max-rpm :inertia :friction
  :torque-curve [[rpm nm] ...]}` from one engine's parsed EDN map `m`."
  [m]
  {:idle-rpm (scene/num (scene/mget m "idle-rpm"))
   :max-rpm (scene/num (scene/mget m "max-rpm"))
   :inertia (scene/num (scene/mget m "inertia"))
   :friction (scene/num (scene/mget m "friction"))
   :torque-curve (pairs (scene/mget m "torque-curve"))})

(defn to-engine
  "Build a duck-typed real `Engine` map from an EngineSpec `spec` — the
  four scalar params + `{:points [...]}` torque curve, with the
  idle-spun `:omega`/`:running` runtime defaults `Engine::new` sets."
  [spec]
  {:idle-rpm (:idle-rpm spec)
   :max-rpm (:max-rpm spec)
   :inertia (:inertia spec)
   :friction (:friction spec)
   :torque-curve {:points (:torque-curve spec)}
   :omega (* (:idle-rpm spec) (/ (* 2 Math/PI) 60.0))
   :running true})

;; ---------------------------------------------------------------------
;; GearboxSpec — duck-typed mirror of a `kami_vehicle::Gearbox` preset
;; ---------------------------------------------------------------------

(defn gearbox-spec
  "Build a GearboxSpec map `{:ratios :final-drive :inertia
  :shift-time}` from one gearbox's parsed EDN map `m`."
  [m]
  {:ratios (num-vec (scene/mget m "ratios"))
   :final-drive (scene/num (scene/mget m "final-drive"))
   :inertia (scene/num (scene/mget m "inertia"))
   :shift-time (scene/num (scene/mget m "shift-time"))})

(defn to-gearbox
  "Build a duck-typed real `Gearbox` map from a GearboxSpec `spec` — the
  parametric fields, with the `current-gear`/`shift-progress` runtime
  defaults `Gearbox::manual_6` sets (neutral, no shift in progress)."
  [spec]
  {:ratios (:ratios spec)
   :final-drive (:final-drive spec)
   :inertia (:inertia spec)
   :shift-time (:shift-time spec)
   :current-gear 1
   :shift-progress 0.0})

;; ---------------------------------------------------------------------
;; TireSpec — duck-typed mirror of a `kami_vehicle::PacejkaParams` preset
;; ---------------------------------------------------------------------

(defn tire-spec
  "Build a TireSpec map (all 8 long+lat Pacejka coefficients) from one
  tire's parsed EDN map `m`."
  [m]
  {:b-long (scene/num (scene/mget m "b-long"))
   :c-long (scene/num (scene/mget m "c-long"))
   :d-long (scene/num (scene/mget m "d-long"))
   :e-long (scene/num (scene/mget m "e-long"))
   :b-lat (scene/num (scene/mget m "b-lat"))
   :c-lat (scene/num (scene/mget m "c-lat"))
   :d-lat (scene/num (scene/mget m "d-lat"))
   :e-lat (scene/num (scene/mget m "e-lat"))})

(defn to-pacejka
  "Identity-shaped: a TireSpec map already matches the duck-typed
  `PacejkaParams` shape 1:1."
  [spec]
  spec)

;; ---------------------------------------------------------------------
;; GarageSpec — duck-typed mirror of one garage vehicle
;; ---------------------------------------------------------------------

(defn- vehicle-table
  "Resolve the `:vehicle/<table>` sub-map of parsed root `root`, or
  `{:error :no-table :table table}`."
  [root table]
  (let [m (scene/mget root (str "vehicle/" table))]
    (if (map? m) {:ok m} {:error :no-table :table table})))

(defn- root-or-error
  "Parse the root map of EDN `src`, or `{:error :not-a-map}`."
  [src]
  (if-let [root (scene/root-map src)]
    {:ok root}
    {:error :not-a-map}))

(defmacro ^:private if-ok
  "Thread `[binding expr]` pairs, short-circuiting on the first
  `{:error ...}` result. `body` runs (wrapped as `{:ok body}` if it
  isn't already a result map) when every binding succeeded."
  [bindings body]
  (if (empty? bindings)
    `(let [r# ~body] (if (and (map? r#) (contains? r# :error)) r# {:ok r#}))
    (let [[sym expr & more] bindings]
      `(let [r# ~expr]
         (if (:error r#)
           r#
           (let [~sym (:ok r#)]
             (if-ok ~(vec more) ~body)))))))

(defn engines-from-edn
  "Load the `:vehicle/engines` table -> `{id EngineSpec ...}` from EDN
  `src`. Returns `{:ok m}` or `{:error :not-a-map}` / `{:error :no-table
  :table \"engines\"}`."
  [src]
  (if-ok [root (root-or-error src)
          table (vehicle-table root "engines")]
    (into {}
          (keep (fn [[k v]]
                  (when-let [id (scene/kw-key k)]
                    (when (map? v) [id (engine-spec v)]))))
          table)))

(defn gearboxes-from-edn
  "Load the `:vehicle/gearboxes` table -> `{id GearboxSpec ...}`."
  [src]
  (if-ok [root (root-or-error src)
          table (vehicle-table root "gearboxes")]
    (into {}
          (keep (fn [[k v]]
                  (when-let [id (scene/kw-key k)]
                    (when (map? v) [id (gearbox-spec v)]))))
          table)))

(defn tires-from-edn
  "Load the `:vehicle/tires` table -> `{id TireSpec ...}`."
  [src]
  (if-ok [root (root-or-error src)
          table (vehicle-table root "tires")]
    (into {}
          (keep (fn [[k v]]
                  (when-let [id (scene/kw-key k)]
                    (when (map? v) [id (tire-spec v)]))))
          table)))

(defn- garage-spec-from-map
  "Build one GarageSpec map from a `:vehicle/garage` entry's parsed EDN
  map `m`. `:max-rpm` uses the 0.0 \"keep engine preset\" sentinel when
  absent, mirroring the original's authoring convention."
  [m]
  {:wheelbase (scene/num (scene/mget m "wheelbase"))
   :track-width (scene/num (scene/mget m "track-width"))
   :ride-height (scene/num (scene/mget m "ride-height"))
   :roof-height (scene/num (scene/mget m "roof-height"))
   :overhang-front (scene/num (scene/mget m "overhang-front"))
   :overhang-rear (scene/num (scene/mget m "overhang-rear"))
   :mass-chassis (scene/num (scene/mget m "mass-chassis"))
   :mass-engine (scene/num (scene/mget m "mass-engine"))
   :mass-cabin (scene/num (scene/mget m "mass-cabin"))
   :wheel-radius (scene/num (scene/mget m "wheel-radius"))
   :wheel-width (scene/num (scene/mget m "wheel-width"))
   :layout (layout-from-value (scene/mget m "layout"))
   :turbo (boolean (scene/mget m "turbo"))
   :engine (kw-name (scene/mget m "engine"))
   :gearbox (kw-name (scene/mget m "gearbox"))
   :final-drive (scene/num (scene/mget m "final-drive"))
   :diff (kw-name (scene/mget m "diff"))
   :tire (kw-name (scene/mget m "tire"))
   :max-rpm (if-let [r (scene/mget m "max-rpm")] (scene/num r) 0.0)
   :tire-d-long (when-let [d (scene/mget m "tire-d-long")] (scene/num d))
   :tire-d-lat (when-let [d (scene/mget m "tire-d-lat")] (scene/num d))})

(defn garage-from-edn
  "Load the `:vehicle/garage` table -> `{id GarageSpec ...}`."
  [src]
  (if-ok [root (root-or-error src)
          table (vehicle-table root "garage")]
    (into {}
          (keep (fn [[k v]]
                  (when-let [id (scene/kw-key k)]
                    (when (map? v) [id (garage-spec-from-map v)]))))
          table)))

(defn to-sedan-spec
  "Map a GarageSpec `spec`'s geometry/mass/layout fields onto a
  duck-typed `SedanSpec` map (the input to a future `sedan` build).
  Powertrain/tire override ids are NOT part of `SedanSpec` — they are
  applied afterward by `build-from-spec`."
  [spec]
  (select-keys spec [:wheelbase :track-width :ride-height :roof-height
                      :overhang-front :overhang-rear :mass-chassis
                      :mass-engine :mass-cabin :wheel-radius :wheel-width
                      :layout :turbo]))

;; ---------------------------------------------------------------------
;; EDN -> resolved build PLAN (see namespace docstring "Scope note")
;; ---------------------------------------------------------------------

(defn build-from-spec
  "Resolve a GarageSpec `spec` + the `engines`/`tires` tables into a
  build PLAN map `{:name :sedan-spec :engine :gearbox :tire}` with every
  per-kind override applied — the data-driven counterpart of the
  original's `build_from_spec`, stopping at the boundary where it
  delegated to `kami_vehicle::models::sedan::sedan()` (see namespace
  docstring). Returns `{:ok plan}`, or `{:error :no-table :table
  \"engines\"|\"tires\"}` if the referenced preset id is missing."
  [name spec engines tires]
  (if-let [engine (get engines (:engine spec))]
    (let [effective-max-rpm (if (not= 0.0 (:max-rpm spec))
                               (:max-rpm spec)
                               (:max-rpm engine))
          engine* (assoc (to-engine engine) :max-rpm effective-max-rpm)
          gearbox* {:final-drive (:final-drive spec)}
          has-sticky? (or (some? (:tire-d-long spec)) (some? (:tire-d-lat spec)))]
      (if has-sticky?
        (if-let [tire (get tires (:tire spec))]
          (let [base (to-pacejka tire)
                tire* (cond-> base
                        (some? (:tire-d-long spec)) (assoc :d-long (:tire-d-long spec))
                        (some? (:tire-d-lat spec)) (assoc :d-lat (:tire-d-lat spec)))]
            {:ok {:name name
                  :sedan-spec (to-sedan-spec spec)
                  :engine engine*
                  :gearbox gearbox*
                  :tire tire*}})
          {:error :no-table :table "tires"})
        {:ok {:name name
              :sedan-spec (to-sedan-spec spec)
              :engine engine*
              :gearbox gearbox*
              :tire nil}}))
    {:error :no-table :table "engines"}))

(defn build-from-edn
  "Resolve `kind-id` (hyphen/underscore tolerant) against the shipped
  `garage-edn` tables -> a build PLAN via `build-from-spec`. The
  data-driven equivalent of the original's `build_from_edn`."
  [kind-id]
  (let [id (str/replace kind-id "_" "-")
        garage (:ok (garage-from-edn garage-edn))
        engines (:ok (engines-from-edn garage-edn))
        tires (:ok (tires-from-edn garage-edn))]
    (if-let [spec (or (get garage id) (get garage kind-id))]
      (let [key (if (contains? garage id) id kind-id)]
        (build-from-spec key spec engines tires))
      {:error :map-not-found :id kind-id})))

;; ---------------------------------------------------------------------
;; Compiled-in fallback / parity oracle
;; ---------------------------------------------------------------------
;;
;; NOTE on duck-typing: the original crate's `VehicleKind::spec()` +
;; `kami_vehicle::build_vehicle(kind)` (the parity oracle for per-kind
;; powertrain/tire OVERRIDES) live in `kami-vehicle`, which is not
;; available here. Since the original crate's own parity test
;; (`garage_edn_matches_builtin`) *guarantees* the shipped EDN is
;; numerically identical to those compiled-in builders, this port uses
;; the shipped EDN's own (parsed) values as the duck-typed "builtin"
;; oracle — not a second, independently hand-transcribed source.

(def all-vehicle-kinds
  "The 6 garage vehicle kind ids — the iteration source for parity
  tests."
  ["sedan" "hatchback" "suv" "sports" "pickup" "bus"])

(defn engine-id-for
  "The EDN engine-preset id each kind resolves to (matches the original
  `garage.rs::build`'s per-kind engine selection)."
  [kind]
  (case kind
    ("sedan" "hatchback") "na-2-0-gasoline"
    ("suv" "sports") "turbo-2-0"
    "pickup" "pickup-v6"
    "bus" "bus-diesel"
    nil))

(defn builtin-engine
  "The duck-typed compiled-in engine preset for `id` (numerically
  identical to the shipped EDN, per the original crate's own parity
  guarantee — see note above). `nil` for an unknown id."
  [id]
  (get (:ok (engines-from-edn garage-edn)) id))

(defn builtin-gearbox
  "The duck-typed compiled-in `manual-6` gearbox preset. `nil` for any
  other id."
  [id]
  (get (:ok (gearboxes-from-edn garage-edn)) id))

(defn builtin-tire
  "The duck-typed compiled-in tire preset for `id` (`road-dry` /
  `road-wet`). `nil` for an unknown id."
  [id]
  (get (:ok (tires-from-edn garage-edn)) id))

(defn garage-spec-builtin
  "The duck-typed compiled-in GarageSpec mirror for one vehicle `kind`
  id (numerically identical to the shipped EDN, per the original
  crate's own parity guarantee)."
  [kind]
  (get (:ok (garage-from-edn garage-edn)) kind))

(defn shipped-engines
  "Convenience: load the engines table from the shipped `garage-edn`."
  []
  (engines-from-edn garage-edn))

(defn shipped-garage
  "Convenience: load the garage table from the shipped `garage-edn`."
  []
  (garage-from-edn garage-edn))

;; ---------------------------------------------------------------------
;; Differential
;; ---------------------------------------------------------------------

(defn differential-from-id
  "Resolve a `:diff` id to a duck-typed `Differential` map
  (`\"open\"` -> `{:kind :open}`; default open, mirroring the
  original's fallback)."
  [id]
  (case id
    "open" {:kind :open}
    {:kind :open}))
