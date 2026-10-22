(ns terrain-scene
  "kami-terrain-scene — EDN authoring surface for `kami-terrain` BIOME CONFIG.
  Restored from the legacy kami-engine/kami-terrain-scene Rust crate
  (deleted from kotoba-lang/kami-engine in PR #82, \"Remove Rust workspace
  from kami-engine\", recovered at commit
  a8368f9c0d784dbc9d11e8fa8f407aa95c7ce4fa) as part of the clj-wgsl migration
  (ADR-2607010930, com-junkawasaki/root).

  This is the data-tier counterpart of `kami-vehicle-scene` / `kami-atmosphere-
  scene` for the terrain biome system: it turns canonical `:terrain/biomes`
  EDN into the real `kotoba-lang/terrain` heightmap-config/splat-thresholds/
  palette shapes, re-using the tolerant `kotoba-lang/scene` accessors
  (`scene/mget` / `scene/num` / `scene/root-map` / `scene/kw-key`) the same
  way games parse `scene.edn` — missing keys fall back to defaults,
  namespaced keywords match on `ns/name`, ints coerce to floats.

  This crate is referenced BY NAME (`kami_terrain_scene::resolve_biome`) from
  both `kami-app-isekai` and `kami-app-quarry-walk` (both already restored in
  this migration): those crates call `resolve_biome(\"plains\")` /
  `resolve_biome(\"quarry\")` to look up a biome preset by name, falling back
  to `kami-terrain`'s compiled-in `BiomePreset` if the lookup fails. Both
  restorations deliberately scoped this crate OUT of their own port (native
  orchestration only, documented in their READMEs) — this namespace is the
  missing piece that lets a future integration actually wire a biome name
  up to real config.

  ## Why this is safe (ADR-0038)

  Hot heightmap / splatmap / chunk-mesh generation stays in
  `kotoba-lang/terrain` (the CLJC port of the native `kami-terrain` engine).
  A biome preset is **init-time CONFIG** — read once when a terrain chunk is
  generated, to seed a heightmap-config + splat-thresholds + material-
  palette the native generators then consume — so it is safe to move to
  EDN. The compiled-in `terrain/biome-heightmap-config` /
  `terrain/biome-splat-thresholds` / `terrain/biome-palette` (driven by
  `terrain.biome`'s hardcoded `BiomePreset` case forms) remain as the
  [[builtin-biome]] fallback and are parity-tested against the shipped EDN
  ([[biomes-edn]]).

  Unlike the original Rust (which hand-parsed EDN via `kotoba_edn::EdnValue`
  and merged onto real `kami_terrain::HeightmapConfig::default()` /
  `SplatThresholds`/`MaterialPalette` struct instances), this namespace
  merges onto `kotoba-lang/terrain`'s `terrain/default-heightmap-config` map
  (the CLJC mirror of that same default shape) via `clojure.edn/read-string`
  (through `scene/root-map`), so nested biome maps are already real Clojure
  maps with real keyword keys and vectors-of-vectors for palette rows.

  ## EDN shape (see `biomes-edn` / `resources/biomes.edn`)

  ```edn
  {:terrain/biomes
   {:plains {:heightmap {:max-height 80.0 :frequency 0.008 :octaves 7
                         :lacunarity 2.0 :persistence 0.5}
             :splat {:sand-line 15.0 :snow-line 100.0 :rock-slope 0.4}
             :palette {:base [[r g b] ...4] :tip [[r g b] ...4]}}
    :quarry {...} :desert {...} :tundra {...}}}
  ```

  ## Hyphen field keys -> `terrain` shapes

  | EDN key                    | `terrain` shape field                    |
  |-----------------------------|-------------------------------------------|
  | `:heightmap/:max-height`   | `heightmap-config` `:max-height`          |
  | `:heightmap/:frequency`    | `heightmap-config` `:frequency`           |
  | `:heightmap/:octaves`      | `heightmap-config` `:octaves` (integral)  |
  | `:heightmap/:lacunarity`   | `heightmap-config` `:lacunarity`          |
  | `:heightmap/:persistence`  | `heightmap-config` `:persistence`         |
  | `:splat/:sand-line`        | `splat-thresholds` `:sand-line`           |
  | `:splat/:snow-line`        | `splat-thresholds` `:snow-line`           |
  | `:splat/:rock-slope`       | `splat-thresholds` `:rock-slope`          |
  | `:palette/:base`           | `palette` `:base` (4 x `[r g b]`)         |
  | `:palette/:tip`            | `palette` `:tip`  (4 x `[r g b]`)         |

  The heightmap `:seed` is **not** stored in EDN — it is supplied per-call to
  [[biome-spec->heightmap-config]], mirroring the original
  `BiomePreset::heightmap(seed)`. Any heightmap key a biome omits inherits
  `terrain/default-heightmap-config`'s value for that key (read, never
  transcribed), so a partial merge is provably the same.

  Also see [[terrain-scene.waves]] — the default Gerstner ocean waves
  (`terrain/default-waves`) as parity-tested EDN (ADR-0046).

  Zero-dep portable CLJC. Depends on `kotoba-lang/scene` (tolerant EDN
  accessors) and `kotoba-lang/terrain` (default heightmap-config shape +
  compiled-in biome presets), both already restored in this migration."
  (:require [scene :as scene]
            [terrain :as terrain]))

;; ════════════════════════════════════════════════════════════════════════
;; shipped EDN
;; ════════════════════════════════════════════════════════════════════════

(def biomes-edn
  "The canonical biome CONFIG shipped with this crate (the preset table).
  This is the source of truth; the compiled-in presets
  (`terrain/biome-heightmap-config` / `terrain/biome-splat-thresholds` /
  `terrain/biome-palette`) are the parity-tested mirror. Embedded as a
  literal string (rather than slurped from a resource) so this namespace
  loads identically on the JVM and in ClojureScript; kept byte-identical to
  `resources/biomes.edn`."
  ";; biomes.edn — canonical CONFIG/DATA for kami-terrain biome presets.
;;
;; ADR-0038: hot heightmap / splatmap / mesh generation stays native Rust; only
;; init-time CONFIG/DATA moves to EDN. A biome preset is read ONCE when a terrain
;; chunk is generated (it seeds the FBM HeightmapConfig + SplatThresholds +
;; MaterialPalette the native generators then consume), so it lives here as the
;; source of truth. `kami-terrain`'s compiled-in `BiomePreset::{heightmap,
;; splat_thresholds, palette}` remain as the `builtin_biome()` fallback and are
;; parity-tested against this file.
;;
;; NOTE: biome ids + field keys use hyphens here (idiomatic EDN keywords, e.g.
;; :max-height); the loader maps each hyphenated key to the matching Rust field
;; on HeightmapConfig / SplatThresholds / MaterialPalette. The heightmap `seed`
;; is NOT stored here — it is supplied per-call (`to_heightmap_config(seed)`),
;; mirroring `BiomePreset::heightmap(seed)`. Palette colours are `[r g b]` in
;; [0,1] (use kami_scene::vec3); :base / :tip are 4-layer (grass rock sand snow).
{:terrain/biomes
 ;; Plains: lush green rolling hills (default).
 {:plains {:heightmap {:max-height  80.0
                       :frequency   0.008
                       :octaves     7
                       :lacunarity  2.0
                       :persistence 0.5}
           :splat {:sand-line  15.0
                   :snow-line  100.0
                   :rock-slope 0.4}
           :palette {:base [[0.28 0.52 0.15]
                            [0.45 0.40 0.35]
                            [0.76 0.69 0.50]
                            [0.92 0.93 0.95]]
                     :tip [[0.42 0.68 0.22]
                           [0.55 0.50 0.45]
                           [0.85 0.78 0.60]
                           [1.00 1.00 1.00]]}}
  ;; Quarry: rocky mesas, warm ochre rock, dry tan grass. Overcast-friendly.
  :quarry {:heightmap {:max-height  120.0
                       :frequency   0.012
                       :octaves     7
                       :lacunarity  2.1
                       :persistence 0.45}
           :splat {:sand-line  5.0
                   :snow-line  200.0
                   :rock-slope 0.22}
           :palette {:base [[0.48 0.44 0.30]
                            [0.55 0.42 0.28]
                            [0.62 0.55 0.42]
                            [0.85 0.82 0.78]]
                     :tip [[0.66 0.58 0.35]
                           [0.72 0.55 0.35]
                           [0.78 0.70 0.55]
                           [0.95 0.92 0.88]]}}
  ;; Desert: arid dunes, smooth undulations, sand-dominant.
  :desert {:heightmap {:max-height  45.0
                       :frequency   0.006
                       :octaves     5
                       :lacunarity  2.0
                       :persistence 0.45}
           :splat {:sand-line  200.0
                   :snow-line  999.0
                   :rock-slope 0.6}
           :palette {:base [[0.68 0.55 0.32]
                            [0.58 0.42 0.28]
                            [0.82 0.70 0.50]
                            [0.90 0.82 0.70]]
                     :tip [[0.78 0.65 0.40]
                           [0.72 0.52 0.35]
                           [0.92 0.80 0.58]
                           [1.00 0.92 0.78]]}}
  ;; Tundra: flat with sharp snowy peaks, high snow line.
  :tundra {:heightmap {:max-height  110.0
                       :frequency   0.005
                       :octaves     6
                       :lacunarity  2.0
                       :persistence 0.5}
           :splat {:sand-line  10.0
                   :snow-line  55.0
                   :rock-slope 0.45}
           :palette {:base [[0.32 0.42 0.22]
                            [0.40 0.38 0.36]
                            [0.70 0.68 0.55]
                            [0.95 0.96 0.98]]
                     :tip [[0.48 0.58 0.30]
                           [0.55 0.50 0.48]
                           [0.82 0.78 0.65]
                           [1.00 1.00 1.00]]}}}}
")

;; ════════════════════════════════════════════════════════════════════════
;; ALL_BIOME_NAMES — the compiled-in oracle iteration source
;; ════════════════════════════════════════════════════════════════════════

(def all-biome-names
  "Names of the biomes shipped as the compiled-in oracle (iteration source
  for `builtin-biome`/parity). Order mirrors the original `BiomePreset`
  declaration order. Kept here (not in `kotoba-lang/terrain`) to leave the
  engine namespace untouched."
  ["plains" "quarry" "desert" "tundra"])

;; ════════════════════════════════════════════════════════════════════════
;; HeightmapSpec — the EDN-loaded mirror of the fields a hardcoded
;; `terrain/biome-heightmap-config` sets (minus :seed, supplied per-call)
;; ════════════════════════════════════════════════════════════════════════

(def heightmap-spec-defaults
  "The default HeightmapSpec: every field (excluding :seed) read from
  `terrain/default-heightmap-config`."
  (dissoc terrain/default-heightmap-config :seed))

(defn heightmap-config->spec
  "Read every field (minus :seed) off a real `terrain` heightmap-config map
  `c` — the parity oracle / default base."
  [c]
  (dissoc c :seed))

(defn heightmap-spec-from-map
  "Build a HeightmapSpec from one biome's `:heightmap` EDN map `m`, merging
  present keys onto [[heightmap-spec-defaults]]."
  [m]
  (let [or* (fn [k fallback]
              (if-let [v (scene/mget m k)] (scene/num v) fallback))]
    {:max-height  (or* "max-height"  (:max-height heightmap-spec-defaults))
     :frequency   (or* "frequency"   (:frequency heightmap-spec-defaults))
     :octaves     (if-let [v (scene/mget m "octaves")]
                    (long (Math/round (double (scene/num v))))
                    (:octaves heightmap-spec-defaults))
     :lacunarity  (or* "lacunarity"  (:lacunarity heightmap-spec-defaults))
     :persistence (or* "persistence" (:persistence heightmap-spec-defaults))}))

;; ════════════════════════════════════════════════════════════════════════
;; SplatSpec — the EDN-loaded mirror of a hardcoded
;; `terrain/biome-splat-thresholds`
;; ════════════════════════════════════════════════════════════════════════

(defn splat-spec-from-map
  "Build a SplatSpec from one biome's `:splat` EDN map `m` (all three keys
  present in shipped data; absent keys read 0.0 via `scene/num`)."
  [m]
  {:sand-line  (scene/num (scene/mget m "sand-line"))
   :snow-line  (scene/num (scene/mget m "snow-line"))
   :rock-slope (scene/num (scene/mget m "rock-slope"))})

;; ════════════════════════════════════════════════════════════════════════
;; PaletteSpec — the EDN-loaded mirror of a hardcoded `terrain/biome-palette`:
;; 4 base + 4 tip RGB colours (grass / rock / sand / snow)
;; ════════════════════════════════════════════════════════════════════════

(defn- read-layers
  "Read a 4-layer colour array (`[[r g b] [r g b] [r g b] [r g b]]`); missing
  layers default to `[0 0 0]`."
  [v]
  (let [rows (if (vector? v) v [])
        g (fn [i] (scene/vec3 (get rows i)))]
    [(g 0) (g 1) (g 2) (g 3)]))

(defn palette-spec-from-map
  "Build a PaletteSpec from one biome's `:palette` EDN map `m`. `:base` /
  `:tip` are each a vector of four `[r g b]` vectors; missing entries
  default to `[0 0 0]` via `scene/vec3`."
  [m]
  {:base (read-layers (scene/mget m "base"))
   :tip  (read-layers (scene/mget m "tip"))})

;; ════════════════════════════════════════════════════════════════════════
;; BiomeSpec — one biome: the EDN-loaded mirror of the per-biome config a
;; hardcoded `BiomePreset` returns (heightmap + splat + palette)
;; ════════════════════════════════════════════════════════════════════════

(defn biome->spec
  "Build a BiomeSpec from the compiled-in `terrain.biome` oracle: read every
  field straight off the real `terrain` functions for biome keyword `b`.
  This is what the EDN is parity-tested against. A fixed `seed 0.0` is used
  to read the heightmap (the spec carries no seed — it is supplied per-call
  to [[biome-spec->heightmap-config]])."
  [b]
  {:heightmap (heightmap-config->spec (terrain/biome-heightmap-config b 0.0))
   :splat     (terrain/biome-splat-thresholds b)
   :palette   (terrain/biome-palette b)})

(defn biome-spec-from-map
  "Build a BiomeSpec from one biome's EDN map `m`
  (`{:heightmap {..} :splat {..} :palette {..}}`)."
  [m]
  (let [sub (fn [k] (let [v (scene/mget m k)] (when (map? v) v)))]
    {:heightmap (if-let [hm (sub "heightmap")]
                  (heightmap-spec-from-map hm)
                  heightmap-spec-defaults)
     :splat (if-let [sp (sub "splat")]
              (splat-spec-from-map sp)
              {:sand-line 0.0 :snow-line 0.0 :rock-slope 0.0})
     :palette (if-let [pl (sub "palette")]
                (palette-spec-from-map pl)
                {:base [[0.0 0.0 0.0] [0.0 0.0 0.0] [0.0 0.0 0.0] [0.0 0.0 0.0]]
                 :tip  [[0.0 0.0 0.0] [0.0 0.0 0.0] [0.0 0.0 0.0] [0.0 0.0 0.0]]})}))

(defn biome-spec->heightmap-config
  "Convert a BiomeSpec's heightmap sub-spec into the real `terrain`
  heightmap-config map, taking the `seed` per-call — behaviourally
  identical to `terrain/biome-heightmap-config`."
  [spec seed]
  (assoc (:heightmap spec) :seed seed))

(defn biome-spec->splat-thresholds
  "Convert a BiomeSpec's splat sub-spec into the real `terrain`
  splat-thresholds map — behaviourally identical to
  `terrain/biome-splat-thresholds`."
  [spec]
  (:splat spec))

(defn biome-spec->material-palette
  "Convert a BiomeSpec's palette sub-spec into the real `terrain` palette
  map — behaviourally identical to `terrain/biome-palette`."
  [spec]
  (:palette spec))

;; ════════════════════════════════════════════════════════════════════════
;; builtin fallback / parity oracle
;; ════════════════════════════════════════════════════════════════════════

(defn builtin-biome
  "The compiled-in fallback / parity oracle: build a BiomeSpec straight from
  the hardcoded `terrain.biome` functions. Returns nil for an unknown
  `name`. This is what the shipped EDN is parity-tested against."
  [name]
  (when (contains? terrain/biome-values (keyword name))
    (biome->spec (keyword name))))

;; ════════════════════════════════════════════════════════════════════════
;; EDN parsing / loading
;; ════════════════════════════════════════════════════════════════════════

(defn biomes-from-edn
  "Parse the whole `:terrain/biomes` table from EDN `src` into a map keyed
  by the (hyphenated) biome id, each value the merged BiomeSpec.

  Throws `ex-info` with `:terrain-scene/error` of `:not-a-map` (EDN root
  didn't parse to a map) or `:no-biomes` (`:terrain/biomes` missing or not a
  map) on failure — mirroring the original `Error::NotAMap` /
  `Error::NoBiomes`."
  [src]
  (let [root (scene/root-map src)]
    (when (nil? root)
      (throw (ex-info "biomes EDN root is not a map"
                       {:terrain-scene/error :not-a-map})))
    (let [biomes (scene/mget root "terrain/biomes")]
      (when-not (map? biomes)
        (throw (ex-info "`:terrain/biomes` missing or not a map"
                         {:terrain-scene/error :no-biomes})))
      (reduce (fn [acc [k v]]
                (if-let [id (scene/kw-key k)]
                  (if (map? v)
                    (assoc acc id (biome-spec-from-map v))
                    acc)
                  acc))
              {}
              biomes))))

(defn biome-from-edn
  "Look up a single biome by (hyphenated) `name` from EDN `src`. Throws
  `ex-info` with `:terrain-scene/error :biome-not-found` if the table or
  the named biome is absent (also propagates [[biomes-from-edn]]'s errors)."
  [src name]
  (let [biomes (biomes-from-edn src)]
    (if-let [spec (get biomes name)]
      spec
      (throw (ex-info (str "biome `" name "` not found under `:terrain/biomes`")
                       {:terrain-scene/error :biome-not-found
                        :terrain-scene/biome name})))))

(defn shipped-biomes
  "Convenience: load all biomes from the crate-shipped [[biomes-edn]]."
  []
  (biomes-from-edn biomes-edn))

(defn shipped-biome
  "Convenience: load one biome from the shipped EDN."
  [name]
  (biome-from-edn biomes-edn name))

(defn resolve-biome
  "Executor-edge resolver (ADR-0044/0046): resolve a named biome to a
  BiomeSpec, loading from the shipped [[biomes-edn]] and falling back to
  the compiled-in `terrain.biome` functions only if the EDN fails to
  parse/resolve. Returns nil for an unknown biome name.

  A native/GPU consumer (e.g. a `kami-app-*` terrain build — see
  `kami-app-isekai::voxel_world` / `kami-app-quarry-walk`, both of which
  call `kami_terrain_scene::resolve_biome` by name and were restored in
  this migration scoped OUT of this crate) calls this instead of passing a
  hardcoded biome keyword straight to the pipeline — so the heightmap/
  splat/palette are *data* (parity-tested here), retunable without
  recompiling. [[biome-spec->heightmap-config]] /
  [[biome-spec->splat-thresholds]] / [[biome-spec->material-palette]] then
  yield the real `terrain` shapes."
  [name]
  (try
    (shipped-biome name)
    (catch #?(:clj Exception :cljs js/Error) _
      (builtin-biome name))))
