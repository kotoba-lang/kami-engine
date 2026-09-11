(ns kami.render.character-material
  "Lower character material-preset-v1 envelopes to portable executor materials."
  (:require [kami.render.character-preset :as character]))

(def contract :kotoba.render/portable-material-v1)

(def semantic-role-aliases
  "KAMI-owned semantic role -> stable character material-preset role."
  {:skin :skin :cloth :cloth :metal :metal
   :visor :metal :emissive :metal :accent :cloth :weapon :metal})

(def team-palette-roles #{:cloth :accent})

(def ^:private role-overrides
  {:skin {:emissive {:color [0.0 0.0 0.0] :intensity 0.0}}
   :cloth {:emissive {:color [0.0 0.0 0.0] :intensity 0.0}}
   :metal {:emissive {:color [0.02 0.025 0.04] :intensity 0.04}}
   :visor {:base [0.055 0.16 0.24 0.92] :shade [0.018 0.045 0.08]
           :shade-shift 0.38 :shade-toony 0.70
           :metallic 0.64 :roughness 0.11
           :rim-color [0.25 0.86 1.0] :rim-intensity 0.78 :rim-fresnel 1.8 :rim-lift 0.04
           :highlight {:color [0.72 0.95 1.0] :intensity 0.92 :size 0.055}
           :emissive {:color [0.08 0.62 0.92] :intensity 0.22}}
   :emissive {:base [0.02 0.055 0.08 1.0] :shade [0.005 0.012 0.02]
              :shade-shift 0.28 :shade-toony 0.96
              :metallic 0.18 :roughness 0.30
              :rim-color [0.18 0.82 1.0] :rim-intensity 0.96 :rim-fresnel 1.45 :rim-lift 0.01
              :highlight {:color [0.64 0.96 1.0] :intensity 0.48 :size 0.18}
              :emissive {:color [0.06 0.72 1.0] :intensity 2.4}}
   :accent {:base [0.92 0.48 0.08 1.0] :shade [0.33 0.095 0.018]
            :shade-shift 0.50 :shade-toony 0.94
            :metallic 0.28 :roughness 0.42
            :rim-color [1.0 0.74 0.24] :rim-intensity 0.54 :rim-fresnel 2.35 :rim-lift 0.07
            :highlight {:color [1.0 0.88 0.52] :intensity 0.58 :size 0.16}
            :emissive {:color [0.28 0.055 0.005] :intensity 0.08}}
   :weapon {:base [0.085 0.095 0.12 1.0] :shade [0.012 0.016 0.026]
            :shade-shift 0.40 :shade-toony 0.84
            :metallic 0.94 :roughness 0.18
            :rim-color [0.48 0.66 0.92] :rim-intensity 0.36 :rim-fresnel 2.8 :rim-lift 0.12
            :highlight {:color [0.92 0.96 1.0] :intensity 0.84 :size 0.075}
            :emissive {:color [0.025 0.04 0.07] :intensity 0.06}}})

(def ^:private outline-overrides
  {:visor {:participates? true :weight 0.64 :crease-weight 0.28}
   :emissive {:participates? true :weight 0.42 :crease-weight 0.18}
   :accent {:participates? true :weight 1.0 :crease-weight 0.66}
   :weapon {:participates? true :weight 1.0 :crease-weight 0.82}})

(defn- deep-merge
  [& maps]
  (letfn [(merge* [& xs]
            (let [xs (remove nil? xs)]
              (if (every? map? xs) (apply merge-with merge* xs) (last xs))))]
    (apply merge* (remove nil? maps))))

(defn- validate-team-palette! [team-palette]
  (let [invalid (seq (remove team-palette-roles (keys team-palette)))]
    (when invalid
      (throw (ex-info "Team palette may override only cloth and accent"
                      {:invalid-roles (set invalid) :allowed team-palette-roles})))))

(defn lower-material
  "Lower one semantic role using a resolved character and optional team palette."
  ([resolved-character semantic-role]
   (lower-material resolved-character semantic-role {}))
  ([resolved-character semantic-role {:keys [team-palette] :or {team-palette {}}}]
   (validate-team-palette! team-palette)
   (let [base-role (get semantic-role-aliases semantic-role)]
     (when-not base-role
       (throw (ex-info "Unknown KAMI character material semantic role"
                       {:semantic-role semantic-role
                        :known-roles (set (keys semantic-role-aliases))})))
     (when-not (= :stylized (:family resolved-character))
       (throw (ex-info "Portable photoreal character lowering is not implemented"
                       {:family (:family resolved-character) :implemented #{:stylized}})))
     (let [preset (character/material-for resolved-character base-role)
           source (:material preset)
           surface (deep-merge source (get role-overrides semantic-role)
                               (when-let [color (get team-palette semantic-role)]
                                 {:base color}))
           outline (deep-merge (:outline-policy preset)
                               (get outline-overrides semantic-role))
           emissive (or (:emissive surface) {:color [0.0 0.0 0.0] :intensity 0.0})
           emission (mapv #(* (:intensity emissive) %) (:color emissive))
           rim-color (:rim-color surface)
           shade (:shade surface)]
       {:contract contract
        :id (keyword "character.material" (name semantic-role))
        :family :stylized
        :semantic-role semantic-role
        :source {:contract (:contract preset) :role base-role}
        :model (:model surface)
        :base-color (:base surface)
        :shade {:color shade :shift (:shade-shift surface) :toony (:shade-toony surface)}
        :metallic (:metallic surface)
        :roughness (:roughness surface)
        :emissive emissive
        :highlight (:highlight surface)
        :rim {:color rim-color :intensity (:rim-intensity surface)
              :fresnel (:rim-fresnel surface) :lift (:rim-lift surface)}
        :outline outline
        :executor {:shader :mtoon
                   :uniforms {:albedo (:base surface)
                              :metallic (:metallic surface)
                              :roughness (:roughness surface)
                              :subsurface-color (conj (vec shade) (:shade-shift surface))
                              :sss-r0 (:shade-toony surface)
                              :sss-r1 (:rim-intensity surface)
                              :sss-r2 (:rim-fresnel surface)
                              :hair-scatter (conj (vec rim-color) (:rim-lift surface))
                              :emission-r (nth emission 0)
                              :emission-g (nth emission 1)
                              :emission-b (nth emission 2)}}}))))

(defn lower-library
  "Resolve every KAMI-owned semantic role into an executor material registry."
  ([resolved-character] (lower-library resolved-character {}))
  ([resolved-character opts]
   (into {} (for [role (keys semantic-role-aliases)]
              [role (lower-material resolved-character role opts)]))))
