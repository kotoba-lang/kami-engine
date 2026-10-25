(ns vehicle-scene
  "KAMI Vehicle Scene — EDN authoring surface for kami-vehicle GROUND CONFIG
  (the surface grip table + the demo-circuit zone map). Restored from the
  legacy `kami-vehicle-scene` Rust crate's `src/lib.rs`
  (kotoba-lang/kami-engine, deleted in PR #82 'Remove Rust workspace from
  kami-engine', recoverable at commit
  a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa) as part of the clj-wgsl
  migration (ADR-2607010930, com-junkawasaki/root).

  Turns canonical `:ground/*` EDN (the surface grip table + the
  demo-circuit zone map) into duck-typed `kami-vehicle` engine-struct
  mirrors (`SurfaceParams`, `MapGround`/`SurfaceZone`), re-using the
  tolerant `scene` accessors (`kotoba-lang/scene`) the same way games
  parse `scene.edn`: missing keys fall back to defaults, namespaced
  keywords match on `ns/name`, ints coerce to doubles.

  ## Why this is safe (ADR-0038, inherited from the original crate)

  Hot physics stays native (in the eventual `kami-vehicle` port); the
  surface coefficients and map zones are init-time CONFIG — read once at
  load, never touched by the 2 kHz solver — so they are safe to author as
  data.

  ## Dependency relationships

  - `scene` (kotoba-lang/scene) supplies `kw-key`/`mget`/`num`/`vec3`/
    `root-map` — the tolerant EDN-map accessors.
  - `kotoba-lang/kami-vehicle` — the domain crate this data pairs with —
    is being restored *in parallel* by another migration pass and is not
    guaranteed to exist yet at the time this crate is restored. Rather
    than hard-depend on it, this namespace (and `vehicle-scene.garage`)
    **locally duck-types** the shapes it needs (`SurfaceParams`,
    `SurfaceZone`/`MapGround`, `Engine`/`Gearbox`/`PacejkaParams`/
    `DrivelineLayout`/`SedanSpec`) as plain CLJC maps, inferred entirely
    from this crate's own original Rust source (field names/types read
    off the struct definitions and `#[test]`s in `src/lib.rs` and
    `src/garage.rs`) — no `kami-vehicle` source was needed or consulted.
    A future pass can refactor to depend on `kotoba-lang/kami-vehicle`
    directly once both crates are stable; see `vehicle-scene.garage`'s
    docstring for the fuller rationale.

  ## Hyphen <-> underscore

  Surface ids are authored with hyphens (`:asphalt-dry`) — idiomatic EDN
  — but the original `kami_vehicle::SurfaceKind::from_id` expected
  underscores (`asphalt_dry`). `surface-id-from-kw` mirrors that
  translation for any future integration; the duck-typed builtin table
  below is keyed the same (hyphenated) way as the EDN, since we do not
  have a real `SurfaceKind` enum to compare against."
  (:require [kotoba.lang.text] [scene :as scene]
            [vehicle-scene.garage :as garage]))

;; Re-export the garage/powertrain surface, matching the original
;; `lib.rs`'s `pub use garage::{...}`.
(def build-from-edn garage/build-from-edn)
(def build-from-spec garage/build-from-spec)
(def builtin-engine garage/builtin-engine)
(def builtin-gearbox garage/builtin-gearbox)
(def builtin-tire garage/builtin-tire)
(def differential-from-id garage/differential-from-id)
(def engines-from-edn garage/engines-from-edn)
(def garage-from-edn garage/garage-from-edn)
(def gearboxes-from-edn garage/gearboxes-from-edn)
(def shipped-engines garage/shipped-engines)
(def shipped-garage garage/shipped-garage)
(def tires-from-edn garage/tires-from-edn)
(def all-vehicle-kinds garage/all-vehicle-kinds)
(def garage-edn garage/garage-edn)

;; ---------------------------------------------------------------------
;; Shipped EDN (compile-time embedded, matches
;; resources/kami_vehicle_scene/ground.edn byte-for-byte)
;; ---------------------------------------------------------------------

(def ground-edn
  "The canonical ground CONFIG shipped with this namespace (surface table
  + maps). This is the source of truth; `builtin-surface-table` /
  `builtin-demo-circuit` below are the duck-typed compiled-in mirrors
  (kept numerically identical, per the original crate's own
  parity-tested guarantee that the shipped EDN equals the compiled-in
  `SurfaceKind` enum + `MapGround::demo_circuit`)."
  "{:ground/surfaces
 {:asphalt-dry {:friction-mu 1.00 :grip-modifier 1.00 :tint [0.20 0.20 0.22] :name \"Dry Asphalt\"}
  :asphalt-wet {:friction-mu 0.70 :grip-modifier 0.70 :tint [0.16 0.18 0.24] :name \"Wet Asphalt\"}
  :gravel      {:friction-mu 0.55 :grip-modifier 0.55 :tint [0.45 0.42 0.38] :name \"Gravel\"}
  :sand        {:friction-mu 0.40 :grip-modifier 0.45 :tint [0.85 0.75 0.55] :name \"Sand\"}
  :snow        {:friction-mu 0.30 :grip-modifier 0.35 :tint [0.95 0.95 0.98] :name \"Snow\"}
  :ice         {:friction-mu 0.10 :grip-modifier 0.10 :tint [0.75 0.85 0.95] :name \"Ice\"}
  :mud         {:friction-mu 0.35 :grip-modifier 0.40 :tint [0.30 0.22 0.15] :name \"Mud\"}
  :grass       {:friction-mu 0.55 :grip-modifier 0.60 :tint [0.30 0.55 0.25] :name \"Grass\"}}

 :ground/map
 {:demo-circuit
  {:default :grass
   :zones
   [{:x-min  -4.0 :x-max  4.0 :z-min -100.0 :z-max 100.0 :surface :asphalt-dry}
    {:x-min  -4.0 :x-max  4.0 :z-min    8.0 :z-max  20.0 :surface :asphalt-wet}
    {:x-min  -4.0 :x-max  4.0 :z-min   30.0 :z-max  42.0 :surface :ice}
    {:x-min  -4.0 :x-max  4.0 :z-min   55.0 :z-max  75.0 :surface :snow}
    {:x-min   8.0 :x-max 30.0 :z-min  -20.0 :z-max  20.0 :surface :sand}
    {:x-min   8.0 :x-max 30.0 :z-min   25.0 :z-max  60.0 :surface :gravel}
    {:x-min -30.0 :x-max -8.0 :z-min  -20.0 :z-max  20.0 :surface :mud}
    {:x-min -30.0 :x-max -8.0 :z-min   25.0 :z-max  60.0 :surface :snow}
    {:x-min  -4.0 :x-max  4.0 :z-min  -45.0 :z-max -30.0 :surface :mud}]}}}")

;; ---------------------------------------------------------------------
;; SurfaceParams — plain-map mirror of a loaded surface (duck-typed
;; `kami_vehicle::SurfaceKind::coefficients()`/`tint()`/`display_name()`)
;; ---------------------------------------------------------------------

(defn surface-params
  "Build a SurfaceParams map `{:friction-mu :grip-modifier :tint :name}`
  from one surface's parsed EDN map `m`. `:name` defaults to `id` when
  absent (mirroring the original's `unwrap_or_else(|| id.clone())`)."
  [id m]
  {:friction-mu (scene/num (scene/mget m "friction-mu"))
   :grip-modifier (scene/num (scene/mget m "grip-modifier"))
   :tint (scene/vec3 (scene/mget m "tint"))
   :name (or (scene/mget m "name") id)})

;; ---------------------------------------------------------------------
;; Hyphen <-> underscore id translation
;; ---------------------------------------------------------------------

(defn surface-id-from-kw
  "Resolve a surface keyword KEY (or VALUE) `k` to its underscore id
  string. `:asphalt-dry` -> \"asphalt_dry\". `nil` for non-keywords."
  [k]
  (when-let [full (scene/kw-key k)]
    (kotoba.lang.text/replace full "-" "_")))

(defn surface-hyphen-id
  "Resolve a surface keyword KEY (or VALUE) `k` to its hyphenated id
  string (the id form used to key `builtin-surface-table` and the
  shipped EDN itself). `:asphalt-dry` -> \"asphalt-dry\"."
  [k]
  (scene/kw-key k))

;; ---------------------------------------------------------------------
;; Compiled-in fallback / parity oracle
;; ---------------------------------------------------------------------
;;
;; NOTE on duck-typing: the original crate's `ALL_SURFACE_KINDS` +
;; `SurfaceKind::{coefficients,tint,display_name}` live in `kami-vehicle`,
;; which is not available here (see namespace docstring). Since the
;; original crate's own parity test (`surfaces_edn_matches_builtin`)
;; *guarantees* the shipped EDN is numerically identical to that
;; compiled-in enum, this port uses the shipped EDN's own values as the
;; duck-typed "builtin" table — it is not a second, independently
;; hand-transcribed source of truth, it is the same numbers the original
;; crate asserted must match.

(def all-surface-ids
  "Hyphenated ids of the 8 surfaces shipped as the compiled-in oracle
  (iteration source for `builtin-surface-table`/parity tests)."
  ["asphalt-dry" "asphalt-wet" "gravel" "sand" "snow" "ice" "mud" "grass"])

(defn surface-table-from-edn
  "Parse a surface-id -> SurfaceParams map from the `:ground/surfaces`
  map in EDN `src`. Returns `{:ok {id params ...}}`, or
  `{:error :not-a-map}` / `{:error :no-surfaces}`."
  [src]
  (if-let [root (scene/root-map src)]
    (let [surfaces (scene/mget root "ground/surfaces")]
      (if (map? surfaces)
        {:ok (into {}
                   (keep (fn [[k v]]
                           (when-let [id (surface-hyphen-id k)]
                             (when (map? v)
                               [id (surface-params id v)]))))
                   surfaces)}
        {:error :no-surfaces}))
    {:error :not-a-map}))

(defn builtin-surface-table
  "The duck-typed compiled-in fallback: every shipped surface's
  SurfaceParams, keyed by hyphenated id. Numerically identical to the
  shipped `ground-edn` (see namespace note above)."
  []
  (:ok (surface-table-from-edn ground-edn)))

(defn get-by-id
  "Look up SurfaceParams by hyphenated id string in `table` (a map as
  returned by `surface-table-from-edn`/`builtin-surface-table`). Falls
  back to the `asphalt-dry` entry (mirroring the original's `from_id`
  fallback) when the id is absent."
  [table id]
  (or (get table id)
      (get table "asphalt-dry")))

(defn surface-kind-from-value
  "Resolve a surface keyword VALUE (e.g. `:ice`) to its hyphenated id
  string. Unknown / non-keyword -> `\"asphalt-dry\"`."
  [v]
  (or (surface-hyphen-id v) "asphalt-dry"))

;; ---------------------------------------------------------------------
;; MapGround / SurfaceZone — duck-typed mirrors
;; ---------------------------------------------------------------------

(defn map-from-edn
  "Build a duck-typed MapGround map `{:default id :zones [...]}` (each
  zone `{:x-min :x-max :z-min :z-max :surface id}`) from the
  `:ground/map :<map-id>` EDN in `src`. `map-id` is the hyphenated
  keyword name as authored (e.g. \"demo-circuit\"). Returns `{:ok m}` or
  `{:error :not-a-map}` / `{:error :map-not-found :id map-id}`."
  [src map-id]
  (if-let [root (scene/root-map src)]
    (let [maps (scene/mget root "ground/map")]
      (if (map? maps)
        (let [m (scene/mget maps map-id)]
          (if (map? m)
            (let [default (if-let [d (scene/mget m "default")]
                             (surface-kind-from-value d)
                             "asphalt-dry")
                  zones (->> (scene/mget m "zones")
                             (filter map?)
                             (mapv (fn [zm]
                                     {:x-min (scene/num (scene/mget zm "x-min"))
                                      :x-max (scene/num (scene/mget zm "x-max"))
                                      :z-min (scene/num (scene/mget zm "z-min"))
                                      :z-max (scene/num (scene/mget zm "z-max"))
                                      :surface (if-let [s (scene/mget zm "surface")]
                                                 (surface-kind-from-value s)
                                                 default)})))]
              {:ok {:default default :zones zones}})
            {:error :map-not-found :id map-id}))
        {:error :map-not-found :id map-id}))
    {:error :not-a-map}))

(defn surface-at
  "The surface id in effect at `(x, z)` for a MapGround map `m` (as
  returned by `map-from-edn`): the LAST zone (in authoring order) whose
  bounds contain the point wins — later, more specific zones (e.g. an
  ice patch) override earlier, broader ones (e.g. the whole-road
  asphalt strip they're carved out of) — else `(:default m)`. Duck-typed
  mirror of the original's `MapGround::surface_at`."
  [m x z]
  (reduce (fn [acc {:keys [x-min x-max z-min z-max surface]}]
            (if (and (<= x-min x x-max) (<= z-min z z-max))
              surface
              acc))
          (:default m)
          (:zones m)))

(defn builtin-demo-circuit
  "The duck-typed compiled-in `demo-circuit` MapGround, loaded from the
  shipped `ground-edn` (numerically identical to the original's
  `MapGround::demo_circuit()`, per the original crate's own parity
  guarantee — see namespace note above)."
  []
  (:ok (map-from-edn ground-edn "demo-circuit")))

;; ---------------------------------------------------------------------
;; Convenience: shipped EDN
;; ---------------------------------------------------------------------

(defn shipped-surface-table
  "Convenience: load the surface table from the namespace-shipped
  `ground-edn`."
  []
  (surface-table-from-edn ground-edn))

(defn shipped-demo-circuit
  "Convenience: load the demo-circuit MapGround from the shipped EDN."
  []
  (map-from-edn ground-edn "demo-circuit"))
