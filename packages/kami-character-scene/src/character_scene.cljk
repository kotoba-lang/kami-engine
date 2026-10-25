(ns character-scene
  "KAMI Character Scene — EDN authoring surface for `character`'s
  HAIR-STYLE presets. Restored from the legacy
  kami-engine/kami-character-scene Rust crate's `lib.rs` (deleted in
  kotoba-lang/kami-engine PR #82, 'Remove Rust workspace from
  kami-engine') as part of the clj-wgsl migration (ADR-2607010930,
  com-junkawasaki/root).

  This is the data-tier counterpart of the (not-yet-restored)
  `kami-vehicle-scene` / `kami-postfx-scene` / `kami-autodrive-scene`
  siblings for the parametric hair generator: it turns canonical
  `:character/hair-styles` EDN (a table of named `HairStyle`
  parameter maps) into a plain CLJC map, the same way the original
  Rust's hardcoded preset fns built the real `kami_character::HairStyle`
  engine struct. It re-uses the tolerant `scene` accessors
  (`kotoba-lang/scene`) the same way games parse `scene.edn` (missing
  keys fall back to defaults, namespaced keywords match on
  `ns/name`, ints coerce to floats).

  ## Why this is safe (ADR-0038)

  Hot procedural geometry generation (`character.hair/generate-hair`)
  stays a pure fn keyed on a *simpler* `HairParams` map (`:preset`
  keyword + scale knobs) — see `kotoba-lang/character`'s
  `character.hair` / `character.params`. A hair-STYLE preset (the
  detailed 14-field parameter block the legacy Rust
  `kami_character::HairStyle` struct held: colour, density, curl,
  bangs, etc.) is **init-time CONFIG**, so it is safe to move to EDN.

  ## Deviation from the original pairing

  The original Rust crate merged EDN fields onto
  `kami_character::HairStyle::default()` (an engine struct with 14
  fields: `style`/`length`/`density`/`volume`/`curl`/`part_side`/
  `bangs_length`/`bangs_width`/`color`/`highlight_color`/
  `highlight_ratio`/`root_darken`/`head_radius`/`head_center_y`) and
  parity-tested against `HairStyle::{blonde_long,dark_short,red_wavy,
  brown_curly,afro}()`. When `kotoba-lang/character` was restored
  earlier in this migration wave, its `character.hair` /
  `character.params` used a different, simpler preset-keyword model
  (`character.params/hair-presets`, `character.hair/generate-hair`)
  instead of porting that 14-field struct — so there is no
  `character`-side default to merge onto. This namespace therefore
  carries its own self-contained `default-hair-style` (ported 1:1
  from the original engine's `HairStyle::default()`, which the
  original crate's own tests proved equals `blonde_long()`) and
  `builtin-hair-styles` table (ported 1:1 from the original crate's
  resolved preset values, which is exactly what the shipped
  `hair.edn` already encodes) as the local compiled-in oracle,
  preserving the original parity-test *structure* (EDN vs. compiled
  table) without a cross-crate dependency that no longer exists.

  See `character-scene.face-rig` and `character-scene.anim-blueprint`
  for the two submodules that DO pair cleanly with `character`
  (`character.control-rig/metahuman-face-rig` and
  `character.anim-blueprint/metahuman-default` are unchanged from the
  original Rust shape and serve as the real parity oracles there).

  Zero-dep portable CLJC (aside from `kotoba-lang/scene`)."
  (:require [scene :as scene]))

;; --- shipped EDN -----------------------------------------------------------------

(def hair-edn
  "The canonical hair-style preset CONFIG shipped with this crate. This is
  the source of truth; `builtin-hair-styles` is the parity-tested mirror.
  Also copied verbatim to `resources/character_scene/hair.edn`."
  "{:character/hair-styles
 {:blonde-long {:style :straight :length 0.7 :density 0.8 :volume 0.5 :curl 0.03
                :part-side 0.1 :bangs-length 0.3 :bangs-width 0.5
                :color [0.93 0.86 0.72] :highlight-color [0.97 0.92 0.82]
                :highlight-ratio 0.35 :root-darken 0.7
                :head-radius 0.09 :head-center-y 1.43}
  :dark-short {:style :straight :length 0.2 :density 0.9 :volume 0.3 :curl 0.02
               :part-side 0.0 :bangs-length 0.15 :bangs-width 0.6
               :color [0.12 0.08 0.06] :highlight-color [0.20 0.15 0.12]
               :highlight-ratio 0.15 :root-darken 0.5
               :head-radius 0.09 :head-center-y 1.43}
  :red-wavy {:style :wavy :length 0.6 :density 0.8 :volume 0.7 :curl 0.25
             :part-side -0.2 :bangs-length 0.35 :bangs-width 0.4
             :color [0.55 0.18 0.10] :highlight-color [0.70 0.30 0.18]
             :highlight-ratio 0.25 :root-darken 0.6
             :head-radius 0.09 :head-center-y 1.43}
  :brown-curly {:style :curly :length 0.5 :density 0.9 :volume 0.8 :curl 0.6
                :part-side 0.0 :bangs-length 0.2 :bangs-width 0.5
                :color [0.25 0.15 0.08] :highlight-color [0.35 0.22 0.12]
                :highlight-ratio 0.2 :root-darken 0.5
                :head-radius 0.09 :head-center-y 1.43}
  :afro {:style :afro :length 0.3 :density 1.0 :volume 1.0 :curl 0.9
         :part-side 0.0 :bangs-length 0.0 :bangs-width 0.0
         :color [0.08 0.05 0.03] :highlight-color [0.15 0.10 0.06]
         :highlight-ratio 0.1 :root-darken 0.4
         :head-radius 0.09 :head-center-y 1.43}}}")

;; --- ids ---------------------------------------------------------------------------

(def all-hair-style-names
  "Names of the hair styles shipped as the compiled-in oracle (iteration source
  for `builtin-hair-style`/parity). Order mirrors the original `impl HairStyle`
  declaration order."
  ["blonde-long" "dark-short" "red-wavy" "brown-curly" "afro"])

;; --- errors --------------------------------------------------------------------------
;; Ported 1:1 from the original crate's `Error` enum. Represented as a map with
;; `:type` (one of `:not-a-map` `:no-table` `:style-not-found`) rather than an
;; exception type, matching the tolerant, data-first idiom of this migration.

(defn ->error
  "Build an error map. `:not-a-map` and `:no-table` carry no extra data;
  `:style-not-found` carries the requested `name`."
  ([type] {:type type})
  ([type name] {:type type :name name}))

;; --- HairType <-> hyphenated id ------------------------------------------------------

(defn id-from-hair-type
  "The hyphenated `:style` keyword id for a HairType keyword `t`. Inverse of
  `hair-type-from-id`."
  [t]
  (case t
    :straight "straight"
    :wavy "wavy"
    :curly "curly"
    :afro "afro"
    :braided "braided"))

(defn hair-type-from-id
  "Parse a HairType keyword from its hyphenated keyword id; unknown / missing
  ids degrade to the engine default (`:straight`, matching `HairStyle::default`)."
  [id]
  (case id
    "wavy" :wavy
    "curly" :curly
    "afro" :afro
    "braided" :braided
    :straight))

;; --- default + builtin oracle --------------------------------------------------------

(def default-hair-style
  "A `PartialEq`-mirror-equivalent projection of the original engine
  `kami_character::HairStyle::default()` — every field, hyphenated. Ported 1:1;
  the original crate's own tests proved this equals `blonde_long()`."
  {:style :straight :length 0.7 :density 0.8 :volume 0.5 :curl 0.03
   :part-side 0.1 :bangs-length 0.3 :bangs-width 0.5
   :color [0.93 0.86 0.72] :highlight-color [0.97 0.92 0.82]
   :highlight-ratio 0.35 :root-darken 0.7
   :head-radius 0.09 :head-center-y 1.43})

(def builtin-hair-styles
  "The compiled-in fallback / parity oracle: the resolved values of the
  original engine's `HairStyle::{blonde_long,dark_short,red_wavy,brown_curly,
  afro}()` builders (each of which used `..Self::default()`, so these are the
  fully RESOLVED fields), ported 1:1 from the original crate."
  {"blonde-long" default-hair-style
   "dark-short" {:style :straight :length 0.2 :density 0.9 :volume 0.3 :curl 0.02
                 :part-side 0.0 :bangs-length 0.15 :bangs-width 0.6
                 :color [0.12 0.08 0.06] :highlight-color [0.20 0.15 0.12]
                 :highlight-ratio 0.15 :root-darken 0.5
                 :head-radius 0.09 :head-center-y 1.43}
   "red-wavy" {:style :wavy :length 0.6 :density 0.8 :volume 0.7 :curl 0.25
               :part-side -0.2 :bangs-length 0.35 :bangs-width 0.4
               :color [0.55 0.18 0.10] :highlight-color [0.70 0.30 0.18]
               :highlight-ratio 0.25 :root-darken 0.6
               :head-radius 0.09 :head-center-y 1.43}
   "brown-curly" {:style :curly :length 0.5 :density 0.9 :volume 0.8 :curl 0.6
                  :part-side 0.0 :bangs-length 0.2 :bangs-width 0.5
                  :color [0.25 0.15 0.08] :highlight-color [0.35 0.22 0.12]
                  :highlight-ratio 0.2 :root-darken 0.5
                  :head-radius 0.09 :head-center-y 1.43}
   "afro" {:style :afro :length 0.3 :density 1.0 :volume 1.0 :curl 0.9
           :part-side 0.0 :bangs-length 0.0 :bangs-width 0.0
           :color [0.08 0.05 0.03] :highlight-color [0.15 0.10 0.06]
           :highlight-ratio 0.1 :root-darken 0.4
           :head-radius 0.09 :head-center-y 1.43}})

(defn builtin-hair-style
  "The compiled-in fallback / parity oracle for one style `name`. Returns nil
  for an unknown name."
  [name]
  (get builtin-hair-styles name))

;; --- EDN -> hair-style map ------------------------------------------------------------

(defn to-hair-style
  "Build one real hair-style map from a hair-style EDN map `m`.

  Every field is read with the tolerant `scene` accessors, so a key `m`
  omits degrades to `default-hair-style`'s value for that field
  (`..Self::default()` in EDN form). `:style` is read as a hyphenated
  keyword id; missing -> `:straight`."
  [m]
  (let [d default-hair-style
        style (if-let [id (some-> (scene/mget m "style") scene/kw-key)]
                (hair-type-from-id id)
                (:style d))
        f (fn [key default]
            (if-let [v (scene/mget m key)] (scene/num v) default))
        c (fn [key default]
            (if-let [v (scene/mget m key)] (scene/vec3 v) default))]
    {:style style
     :length (f "length" (:length d))
     :density (f "density" (:density d))
     :volume (f "volume" (:volume d))
     :curl (f "curl" (:curl d))
     :part-side (f "part-side" (:part-side d))
     :bangs-length (f "bangs-length" (:bangs-length d))
     :bangs-width (f "bangs-width" (:bangs-width d))
     :color (c "color" (:color d))
     :highlight-color (c "highlight-color" (:highlight-color d))
     :highlight-ratio (f "highlight-ratio" (:highlight-ratio d))
     :root-darken (f "root-darken" (:root-darken d))
     :head-radius (f "head-radius" (:head-radius d))
     :head-center-y (f "head-center-y" (:head-center-y d))}))

(defn hair-styles-from-edn
  "Parse the whole `:character/hair-styles` table from EDN `src` into a map
  keyed by the (hyphenated) style id, each value the rebuilt hair-style map.
  Returns `[:ok map]` or `[:error error-map]`."
  [src]
  (if-let [root (scene/root-map src)]
    (let [table (scene/mget root "character/hair-styles")]
      (if (map? table)
        [:ok (into {}
                   (keep (fn [[k v]]
                           (when-let [id (scene/kw-key k)]
                             (when (map? v) [id (to-hair-style v)]))))
                   table)]
        [:error (->error :no-table)]))
    [:error (->error :not-a-map)]))

(defn hair-style-from-edn
  "Look up & rebuild a single hair style by (hyphenated) `name` from EDN
  `src`. Returns `[:ok hair-style]` or `[:error error-map]` if the table or
  the named style is absent."
  [src name]
  (if-let [root (scene/root-map src)]
    (let [table (scene/mget root "character/hair-styles")]
      (if (map? table)
        (if-let [m (some (fn [[k v]] (when (and (= (scene/kw-key k) name) (map? v)) v)) table)]
          [:ok (to-hair-style m)]
          [:error (->error :style-not-found name)])
        [:error (->error :no-table)]))
    [:error (->error :not-a-map)]))

(defn shipped-hair-styles
  "Convenience: load & rebuild all hair styles from the crate-shipped `hair-edn`."
  []
  (hair-styles-from-edn hair-edn))

(defn shipped-hair-style
  "Convenience: load & rebuild one hair style from the shipped EDN."
  [name]
  (hair-style-from-edn hair-edn name))
