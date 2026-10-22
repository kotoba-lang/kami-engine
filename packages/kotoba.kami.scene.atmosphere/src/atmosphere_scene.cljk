(ns atmosphere-scene
  "kami-atmosphere-scene — EDN authoring surface for `kami-atmosphere` weather
  CONFIG. Restored from the legacy kami-engine/kami-atmosphere-scene Rust
  crate (deleted from kotoba-lang/kami-engine in PR #82, \"Remove Rust
  workspace from kami-engine\", recovered at commit
  a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa) as part of the clj-wgsl migration
  (ADR-2607010930, com-junkawasaki/root).

  This is the data-tier counterpart of `kami-vehicle-scene` for the
  sky/weather system: it turns canonical `:weather/presets` EDN into real
  weather snapshots, re-using the tolerant `kotoba-lang/scene` accessors
  (`scene/mget` / `scene/num` / `scene/root-map` / `scene/kw-key`) the same
  way games parse `scene.edn` — missing keys fall back to defaults,
  namespaced keywords match on `ns/name`, ints coerce to floats.

  ## Why this is safe (ADR-0038)

  Hot rendering / wind / cloud simulation stays in `kotoba-lang/atmosphere`
  (the CLJC port of the native `kami-atmosphere` engine). A weather preset is
  **init-time CONFIG** — read once when the scene boots to seed a Weather
  snapshot, which the per-frame `atmosphere/weather-tick` then evolves — so
  it is safe to move to EDN. The compiled-in `atmosphere/weather-overcast` /
  `atmosphere/weather-clear` presets remain as the [[builtin-preset]]
  fallback and are parity-tested against the shipped EDN ([[weather-edn]]).

  Unlike the original Rust (which hand-parsed EDN via `kotoba_edn::EdnValue`
  and merged onto a *real* `kami_atmosphere::Weather::default()` instance),
  this namespace merges onto `kotoba-lang/atmosphere`'s
  `atmosphere/default-weather` map — the CLJC mirror of that same default
  shape — via `clojure.edn/read-string` (through `scene/root-map`), so
  nested preset maps are already real Clojure maps with real keyword keys.

  ## EDN shape (see `weather-edn` / `resources/weather.edn`)

  ```edn
  {:weather/presets
   {:overcast {:cloud-coverage 0.95 :cloud-density 1.0 :cloud-altitude 250.0
               :cloud-sharpness 1.2 :wind-speed 8.0 :wind-gust 0.45 :time 0.42}
    :clear    {:cloud-coverage 0.25 :cloud-density 0.6 :wind-speed 4.0 :time 0.4}}}
  ```

  ## Hyphenated EDN keys -> `atmosphere` map paths

  | EDN key            | `atmosphere` path            |
  |---------------------|------------------------------|
  | `:cloud-coverage`   | `[:clouds :coverage]`        |
  | `:cloud-density`    | `[:clouds :density]`         |
  | `:cloud-altitude`   | `[:clouds :altitude]`        |
  | `:cloud-sharpness`  | `[:clouds :sharpness]`       |
  | `:wind-speed`       | `[:wind :speed]`             |
  | `:wind-gust`        | `[:wind :gust-intensity]`    |
  | `:time`             | `[:day-night :time]`         |

  Any key a preset omits inherits `atmosphere/default-weather`'s own value
  (taken from that namespace, never transcribed here), so a partial EDN map
  merges onto the default exactly as the hardcoded presets leave fields
  untouched.

  Zero-dep portable CLJC. Depends on `kotoba-lang/scene` (tolerant EDN
  accessors) and `kotoba-lang/atmosphere` (default Weather/CloudSystem/
  WindSystem/DayNightCycle shapes), both already restored in this migration."
  (:require [scene :as scene]
            [atmosphere :as atmosphere]))

;; ════════════════════════════════════════════════════════════════════════
;; shipped EDN
;; ════════════════════════════════════════════════════════════════════════

(def weather-edn
  "The canonical weather CONFIG shipped with this crate (the preset table).
  This is the source of truth; the compiled-in presets
  (`atmosphere/weather-overcast` / `atmosphere/weather-clear`) are the
  parity-tested mirror. Embedded as a literal string (rather than
  slurped from a resource) so this namespace loads identically on the JVM
  and in ClojureScript; kept byte-identical to `resources/weather.edn`."
  ";; weather.edn — canonical CONFIG/DATA for kami-atmosphere weather presets.
;;
;; ADR-0038: hot rendering / wind / cloud simulation stays native Rust; only
;; init-time CONFIG/DATA moves to EDN. A weather preset is read ONCE when the
;; scene boots (it just seeds a `Weather` snapshot the per-frame `tick` then
;; evolves), so it lives here as the source of truth. `kami-atmosphere`'s
;; compiled-in `Weather::overcast()` / `Weather::clear()` remain as the
;; `builtin_preset()` fallback and are parity-tested against this file.
;;
;; NOTE: preset ids + field keys use hyphens here (idiomatic EDN keywords, e.g.
;; :cloud-coverage); the loader maps each hyphenated key to the matching Rust
;; field on Weather / CloudSystem / WindSystem / DayNightCycle. Any field a
;; preset omits inherits the engine's `Default` (e.g. :clear omits
;; :cloud-altitude / :cloud-sharpness / :wind-gust → CloudSystem/WindSystem
;; defaults), exactly as the hardcoded `Weather::clear()` leaves them untouched.
{:weather/presets
 ;; Overcast: near-total cloud coverage, diffuse grey lighting, moderate wind.
 ;; Matches volcanic / quarry cinematic atmosphere.
 {:overcast {:cloud-coverage  0.95
             :cloud-density   1.0
             :cloud-altitude  250.0
             :cloud-sharpness 1.2
             :wind-speed      8.0
             :wind-gust       0.45
             :time            0.42}
  ;; Clear sunny preset (omitted fields fall back to engine Default).
  :clear    {:cloud-coverage 0.25
             :cloud-density  0.6
             :wind-speed     4.0
             :time           0.4}}}
")

;; ════════════════════════════════════════════════════════════════════════
;; WeatherPreset — the EDN-loaded mirror of the fields a hardcoded
;; `weather-overcast` / `weather-clear` sets
;; ════════════════════════════════════════════════════════════════════════

(def ^:private field-paths
  "Ordered [hyphenated-edn-key atmosphere-map-path] table, single source of
  truth for the EDN key <-> `atmosphere` map path mapping."
  [["cloud-coverage"  [:clouds :coverage]]
   ["cloud-density"   [:clouds :density]]
   ["cloud-altitude"  [:clouds :altitude]]
   ["cloud-sharpness" [:clouds :sharpness]]
   ["wind-speed"      [:wind :speed]]
   ["wind-gust"       [:wind :gust-intensity]]
   ["time"            [:day-night :time]]])

(defn weather->preset
  "Build a WeatherPreset spec (a flat map keyed `:cloud-coverage` etc.) from a
  real `atmosphere` Weather map `w`: read every field this preset describes
  straight off the engine map. This is what the EDN is parity-tested
  against."
  [w]
  (reduce (fn [acc [k path]]
            (assoc acc (keyword k) (get-in w path)))
          {}
          field-paths))

(def weather-preset-defaults
  "The default WeatherPreset spec: every field read from
  `atmosphere/default-weather`. Used as the merge base so a partial EDN
  preset only overrides the keys it actually carries."
  (weather->preset atmosphere/default-weather))

(defn preset-spec-from-map
  "Build a WeatherPreset spec from one preset's parsed EDN map `m`, merging
  present keys onto [[weather-preset-defaults]]. Absent keys keep the
  `atmosphere` default (mirroring the hardcoded presets, which leave
  untouched fields at their Default)."
  [m]
  (reduce (fn [acc [k _path]]
            (if-let [v (scene/mget m k)]
              (assoc acc (keyword k) (scene/num v))
              acc))
          weather-preset-defaults
          field-paths))

(defn preset->weather
  "Apply a WeatherPreset spec `p` onto a fresh `atmosphere/default-weather`,
  yielding a real `atmosphere` Weather map — behaviourally identical to the
  hardcoded `atmosphere/weather-overcast` / `atmosphere/weather-clear`."
  [p]
  (reduce (fn [w [k path]]
            (assoc-in w path (get p (keyword k))))
          atmosphere/default-weather
          field-paths))

;; ════════════════════════════════════════════════════════════════════════
;; builtin fallback / parity oracle
;; ════════════════════════════════════════════════════════════════════════

(def all-preset-names
  "Names of the presets shipped as the compiled-in oracle (iteration source
  for `builtin-preset`/parity). Keeping this list here (not in `atmosphere`)
  keeps the engine namespace untouched."
  ["overcast" "clear"])

(defn builtin-preset
  "The compiled-in fallback / parity oracle: build a WeatherPreset spec
  straight from the hardcoded `atmosphere/weather-overcast` /
  `atmosphere/weather-clear`. Returns nil for an unknown name. This is what
  the shipped EDN is parity-tested against."
  [name]
  (case name
    "overcast" (weather->preset (atmosphere/weather-overcast))
    "clear"    (weather->preset (atmosphere/weather-clear))
    nil))

;; ════════════════════════════════════════════════════════════════════════
;; EDN parsing / loading
;; ════════════════════════════════════════════════════════════════════════

(defn presets-from-edn
  "Parse the whole `:weather/presets` table from EDN `src` into a map keyed
  by the (hyphenated) preset id, each value the merged WeatherPreset spec.

  Throws `ex-info` with `:atmosphere-scene/error` of `:not-a-map` (EDN root
  didn't parse to a map) or `:no-presets` (`:weather/presets` missing or not
  a map) on failure — mirroring the original `Error::NotAMap` /
  `Error::NoPresets`."
  [src]
  (let [root (scene/root-map src)]
    (when (nil? root)
      (throw (ex-info "weather EDN root is not a map"
                       {:atmosphere-scene/error :not-a-map})))
    (let [presets (scene/mget root "weather/presets")]
      (when-not (map? presets)
        (throw (ex-info "`:weather/presets` missing or not a map"
                         {:atmosphere-scene/error :no-presets})))
      (reduce (fn [acc [k v]]
                (if-let [id (scene/kw-key k)]
                  (if (map? v)
                    (assoc acc id (preset-spec-from-map v))
                    acc)
                  acc))
              {}
              presets))))

(defn preset-weather-from-edn
  "Look up a single preset by (hyphenated) `name` from EDN `src`, returning
  the real `atmosphere` Weather map. Throws `ex-info` with
  `:atmosphere-scene/error :preset-not-found` if the table or the named
  preset is absent (also propagates [[presets-from-edn]]'s errors)."
  [src name]
  (let [presets (presets-from-edn src)]
    (if-let [spec (get presets name)]
      (preset->weather spec)
      (throw (ex-info (str "preset `" name "` not found under `:weather/presets`")
                       {:atmosphere-scene/error :preset-not-found
                        :atmosphere-scene/preset name})))))

(defn shipped-presets
  "Convenience: load all presets from the crate-shipped [[weather-edn]]."
  []
  (presets-from-edn weather-edn))

(defn shipped-weather
  "Convenience: load one preset from the shipped EDN as a real `atmosphere`
  Weather map."
  [name]
  (preset-weather-from-edn weather-edn name))

(defn resolve-weather
  "Executor-edge resolver (ADR-0044/0046): resolve a named preset to a real
  `atmosphere` Weather map, loading from the shipped [[weather-edn]] and
  falling back to the compiled-in `atmosphere/weather-overcast` /
  `atmosphere/weather-clear` only if the EDN fails to parse/resolve. Returns
  nil for an unknown name (the caller leaves its weather unchanged).

  This is what a native/GPU consumer (e.g. `kami-map::set_weather_preset`)
  calls instead of matching on the builtin presets directly — so the runtime
  sky is *data* (parity-tested here), retunable without recompiling."
  [name]
  (try
    (shipped-weather name)
    (catch #?(:clj Exception :cljs js/Error) _
      (case name
        "overcast" (atmosphere/weather-overcast)
        "clear"    (atmosphere/weather-clear)
        nil))))
