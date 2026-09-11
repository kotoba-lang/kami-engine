(ns kami.render.character-preset
  "Reusable character look presets layered on KAMI render style-v1.

  The output is pure renderer data. Geometry authors consume `:silhouette`,
  material registries consume `:materials`, and a host consumes `:render/style`.
  Roles stay stable across visual families so a future photoreal sibling does
  not require game code or asset-slot changes."
  (:require [kami.render.style :as style]))

(def contract :kotoba.render/character-preset-v1)
(def material-contract :kotoba.render/material-preset-v1)
(def roles [:skin :cloth :metal])

(def family-boundary
  {:families #{:stylized :photoreal}
   :implemented-families #{:stylized}
   :stable-roles (set roles)
   :model-by-family {:stylized :mtoon :photoreal :pbr}
   :rule "Family changes surface realization, never semantic role ids."})

(def silhouette-tiers
  {:crowd
   {:tier :crowd
    :shape-language {:head-scale 1.10 :shoulder-scale 1.08 :hand-scale 1.15
                     :weapon-scale 1.12 :detail-frequency :low}
    :mesh-budget {:min-triangles 800 :target-triangles 2400 :min-joints 18}
    :readability {:min-screen-height-px 24 :required-landmarks #{:head :torso :hands :weapon}}
    :lod-policy {:levels [{:id :lod0 :max-distance 18.0}
                          {:id :lod1 :max-distance 42.0}
                          {:id :lod2 :max-distance 90.0}]
                 :preserve #{:head :hands :weapon}}}

   :gameplay
   {:tier :gameplay
    :shape-language {:head-scale 1.06 :shoulder-scale 1.05 :hand-scale 1.10
                     :weapon-scale 1.08 :detail-frequency :medium}
    :mesh-budget {:min-triangles 8000 :target-triangles 18000 :min-joints 32}
    :readability {:min-screen-height-px 72
                  :required-landmarks #{:head :torso :hands :feet :weapon :equipment}}
    :lod-policy {:levels [{:id :lod0 :max-distance 14.0}
                          {:id :lod1 :max-distance 32.0}
                          {:id :lod2 :max-distance 70.0}]
                 :preserve #{:face :hands :feet :weapon :equipment}}}

   :hero
   {:tier :hero
    :shape-language {:head-scale 1.04 :shoulder-scale 1.04 :hand-scale 1.06
                     :weapon-scale 1.04 :detail-frequency :high}
    :mesh-budget {:min-triangles 24000 :target-triangles 60000 :min-joints 46}
    :readability {:min-screen-height-px 180
                  :required-landmarks #{:face :hair :hands :feet :weapon :equipment :layering}}
    :lod-policy {:levels [{:id :lod0 :max-distance 10.0}
                          {:id :lod1 :max-distance 26.0}
                          {:id :lod2 :max-distance 60.0}]
                 :preserve #{:face :hair :hands :weapon :costume-layers}}}})

(def ^:private role-surfaces
  {:skin
   {:model :mtoon :base [0.82 0.58 0.46 1.0] :shade [0.52 0.30 0.31]
    :shade-shift 0.44 :shade-toony 0.82
    :rim-color [1.0 0.66 0.52] :rim-intensity 0.22 :rim-fresnel 3.2 :rim-lift 0.18
    :highlight {:color [1.0 0.86 0.72] :intensity 0.16 :size 0.35}
    :metallic 0.0 :roughness 0.72}
   :cloth
   {:model :mtoon :base [0.16 0.28 0.62 1.0] :shade [0.055 0.09 0.26]
    :shade-shift 0.46 :shade-toony 0.90
    :rim-color [0.36 0.58 1.0] :rim-intensity 0.30 :rim-fresnel 2.7 :rim-lift 0.14
    :highlight {:color [0.48 0.66 1.0] :intensity 0.08 :size 0.65}
    :metallic 0.0 :roughness 0.88}
   :metal
   {:model :mtoon :base [0.24 0.27 0.33 1.0] :shade [0.055 0.065 0.09]
    :shade-shift 0.42 :shade-toony 0.76
    :rim-color [0.62 0.78 1.0] :rim-intensity 0.42 :rim-fresnel 2.2 :rim-lift 0.10
    :highlight {:color [0.88 0.94 1.0] :intensity 0.72 :size 0.12}
    :metallic 0.86 :roughness 0.24}})

(def ^:private outline-by-role
  {:skin {:participates? true :weight 0.72 :crease-weight 0.35}
   :cloth {:participates? true :weight 1.0 :crease-weight 0.55}
   :metal {:participates? true :weight 0.88 :crease-weight 0.72}})

(def presets
  {:hero-balanced
   {:preset :hero-balanced :silhouette-tier :hero
    :palette {:skin [0.82 0.58 0.46 1.0]
              :cloth [0.16 0.28 0.62 1.0]
              :metal [0.24 0.27 0.33 1.0]}}
   :combat-readable
   {:preset :combat-readable :silhouette-tier :gameplay
    :palette {:skin [0.76 0.49 0.38 1.0]
              :cloth [0.56 0.10 0.075 1.0]
              :metal [0.16 0.18 0.22 1.0]}}
   :crowd-efficient
   {:preset :crowd-efficient :silhouette-tier :crowd
    :palette {:skin [0.72 0.48 0.36 1.0]
              :cloth [0.18 0.34 0.25 1.0]
              :metal [0.20 0.22 0.25 1.0]}}})

(defn- deep-merge
  [& maps]
  (letfn [(merge* [& xs]
            (let [xs (remove nil? xs)]
              (if (every? map? xs) (apply merge-with merge* xs) (last xs))))]
    (apply merge* (remove nil? maps))))

(defn- material-preset [family role surface outline variation lod-policy]
  {:contract material-contract
   :family family
   :domain :character
   :role role
   :material surface
   :outline-policy outline
   :variation variation
   :lod-policy lod-policy})

(defn resolve-character
  "Resolve a character preset into style-v1, silhouette and role materials.

  Accepted overrides mirror the output shape: `:palette`, `:materials`,
  `:outline-policy`, and `:silhouette-tier`. Photoreal is a reserved sibling
  family and fails until its measured material library is implemented."
  [{:keys [family preset silhouette-tier palette materials outline-policy variation]
    :or {family :stylized preset :hero-balanced}}]
  (when-not (contains? (:implemented-families family-boundary) family)
    (throw (ex-info "Character visual family is not implemented"
                    {:family family
                     :implemented (:implemented-families family-boundary)
                     :reserved (disj (:families family-boundary) :stylized)})))
  (let [preset-data (get presets preset)]
    (when-not preset-data
      (throw (ex-info "Unknown KAMI character preset"
                      {:preset preset :known-presets (set (keys presets))})))
    (let [tier-id (or silhouette-tier (:silhouette-tier preset-data))
          silhouette (get silhouette-tiers tier-id)
          palette (merge (:palette preset-data) palette)
          role-materials (into {}
                               (for [role roles
                                     :let [surface (deep-merge (get role-surfaces role)
                                                               (get materials role)
                                                               (when-let [base (get palette role)]
                                                                 {:base base}))
                                           outline (deep-merge (get outline-by-role role)
                                                               (get outline-policy role))]]
                                 [role (material-preset
                                        family role surface outline
                                        (deep-merge {:seed-source :entity-id
                                                     :base-color-jitter 0.025
                                                     :roughness-jitter 0.04}
                                                    variation)
                                        (:lod-policy silhouette))]))]
      (when-not silhouette
        (throw (ex-info "Unknown character silhouette tier"
                        {:silhouette-tier tier-id
                         :known-tiers (set (keys silhouette-tiers))})))
      {:contract contract
       :family family
       :preset preset
       :render/style (style/normalize {:contract style/contract :profile :stylized})
       :silhouette silhouette
       :materials role-materials
       :outline-policy {:mode :screen-space
                        :source :render/style
                        :roles (into {} (map (fn [[role p]] [role (:outline-policy p)])
                                             role-materials))}
       :lod-policy (:lod-policy silhouette)})))

(defn material-for
  "Resolve one stable semantic material role."
  [character role]
  (or (get-in character [:materials role])
      (throw (ex-info "Character has no material role"
                      {:role role :known-roles (set (keys (:materials character)))}))))
