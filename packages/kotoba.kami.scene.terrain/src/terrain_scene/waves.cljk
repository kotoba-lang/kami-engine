(ns terrain-scene.waves
  "Wave data tier — `kami-terrain`'s default Gerstner ocean waves
  (`terrain/default-waves`) as parity-tested EDN.

  Restored from the legacy kami-engine/kami-terrain-scene Rust crate's
  `src/waves.rs` (deleted from kotoba-lang/kami-engine in PR #82, \"Remove
  Rust workspace from kami-engine\", recovered at commit
  a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa) as part of the clj-wgsl migration
  (ADR-2607010930, com-junkawasaki/root).

  The per-vertex wave displacement (the Gerstner sum) stays native in
  `kotoba-lang/terrain` (`terrain.water`); only the init-time
  *description* — the wave-train table — moves to EDN (ADR-0046 / ADR-0038).
  [[waves-from-edn]] rebuilds the real `terrain` Gerstner-wave map list,
  asserted wave-for-wave equal to the compiled-in `terrain/default-waves`
  in the test namespace.

  Unlike the original Rust (whose `GerstnerWave` is a `Pod`/`Zeroable` GPU
  struct with a padding field and no `PartialEq`/`Serialize`, requiring a
  local `GerstnerWaveSpec` mirror type), `terrain.water/default-waves`
  already returns plain Clojure maps `{:direction [x z] :amplitude ...
  :wavelength ... :speed ... :steepness ...}` with no GPU-alignment padding
  to reconstruct, so the EDN-loaded wave IS the real `terrain` wave map —
  no separate spec type is needed here.

  Zero-dep portable CLJC. Depends on `kotoba-lang/scene` (tolerant EDN
  accessors) and `kotoba-lang/terrain` (`terrain/default-waves` oracle),
  both already restored in this migration."
  (:require [scene :as scene]
            [terrain :as terrain]))

;; ════════════════════════════════════════════════════════════════════════
;; shipped EDN
;; ════════════════════════════════════════════════════════════════════════

(def waves-edn
  "The canonical wave CONFIG shipped with this crate. Embedded as a literal
  string (rather than slurped from a resource) so this namespace loads
  identically on the JVM and in ClojureScript; kept byte-identical to
  `resources/waves.edn`."
  ";; waves.edn — canonical CONFIG/DATA for kami-terrain's default Gerstner ocean waves
;; (`water::default_waves()`).
;;
;; ADR-0046 / ADR-0038: the per-vertex wave displacement (the Gerstner sum, done in the
;; vertex shader / mesh gen) stays native Rust; only the init-time DESCRIPTION — the
;; wave-train table (direction / amplitude / wavelength / speed / steepness) read once to
;; seed the water surface — moves to EDN here. kami-terrain is untouched; `default_waves()`
;; stays the builtin fallback AND the parity oracle (asserted wave-for-wave `==` it, in
;; order, in tests/waves_parity.rs). The GerstnerWave `_pad` is GPU alignment padding, not
;; data, so it is omitted (reconstructed as [0,0]).
{:terrain/waves
 [{:direction [0.8 0.6]   :amplitude 0.8 :wavelength 60.0 :speed 12.0 :steepness 0.4}
  {:direction [-0.3 0.95] :amplitude 0.4 :wavelength 30.0 :speed 8.0  :steepness 0.3}
  {:direction [0.5 -0.87] :amplitude 0.2 :wavelength 15.0 :speed 5.0  :steepness 0.5}
  {:direction [-0.7 -0.7] :amplitude 0.1 :wavelength 8.0  :speed 3.0  :steepness 0.2}]}
")

;; ════════════════════════════════════════════════════════════════════════
;; wave map <- EDN
;; ════════════════════════════════════════════════════════════════════════

(defn- vec2
  "Read a 2-vector `[x z]`; missing components default to 0.0."
  [v]
  (let [s (if (vector? v) v [])
        g (fn [i] (scene/num (get s i)))]
    [(g 0) (g 1)]))

(defn wave-from-map
  "Build a real `terrain` Gerstner-wave map from one wave's EDN map `m`
  (tolerant: missing keys read 0.0)."
  [m]
  {:direction  (vec2 (scene/mget m "direction"))
   :amplitude  (scene/num (scene/mget m "amplitude"))
   :wavelength (scene/num (scene/mget m "wavelength"))
   :speed      (scene/num (scene/mget m "speed"))
   :steepness  (scene/num (scene/mget m "steepness"))})

;; ════════════════════════════════════════════════════════════════════════
;; EDN parsing / loading
;; ════════════════════════════════════════════════════════════════════════

(defn waves-from-edn
  "Parse the `:terrain/waves` table from EDN `src` into an ordered vector of
  real `terrain` Gerstner-wave maps.

  Throws `ex-info` with `:terrain-scene.waves/error` of `:not-a-map` (EDN
  root didn't parse to a map) or `:no-table` (`:terrain/waves` missing or
  not a vector) on failure — mirroring the original `WaveError::NotAMap` /
  `WaveError::NoTable`."
  [src]
  (let [root (scene/root-map src)]
    (when (nil? root)
      (throw (ex-info "waves EDN root is not a map"
                       {:terrain-scene.waves/error :not-a-map})))
    (let [waves (scene/mget root "terrain/waves")]
      (when-not (vector? waves)
        (throw (ex-info "`:terrain/waves` missing or not a vector"
                         {:terrain-scene.waves/error :no-table})))
      (mapv wave-from-map (filter map? waves)))))

(defn builtin-waves
  "The compiled-in oracle: `terrain/default-waves` (the parity oracle
  source, not transcribed)."
  []
  (terrain/default-waves))

(defn shipped-waves
  "Convenience: the waves from the crate-shipped [[waves-edn]]."
  []
  (waves-from-edn waves-edn))
