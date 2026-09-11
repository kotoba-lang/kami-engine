(ns kami.render.weapon-mesh
  "Actual indexed stylized rifle meshes assembled from semantic components."
  (:require [kami.render.character-material :as character-material]
            [kami.render.character-preset :as character]
            [kami.render.equipment-kit :as equipment]
            [kotoba.render.mesh :as mesh]))

(def contract :kotoba.render/weapon-mesh-v2)

(def family-boundary
  {:families #{:stylized :photoreal}
   :implemented-families #{:stylized}
   :same-api? true
   :rule "Family changes mesh construction and shading, never component or socket ids."})

(def sockets
  {:attachment {:semantic-id :humanoid/right-hand :socket :weapon/grip-primary
                :position [0.0843 -0.12 -0.02] :forward [0.0 0.0 1.0] :up [0.0 1.0 0.0]}
   :primary-grip {:semantic-id :weapon/grip-primary
                  :position [0.0843 -0.12 -0.02] :forward [0.0 0.0 1.0] :up [0.0 1.0 0.0]
                  :contact-surface {:component :grip :form :rounded-receiver-grip}}
   :support-grip {:semantic-id :weapon/grip-support
                  :position [0.0 -0.10 0.58] :forward [0.0 0.0 1.0] :up [0.0 1.0 0.0]
                  :contact-surface {:component :handguard :form :rounded-handguard}}})

(def ^:private tier-policy
  {:hero {:segments 12 :stacks 8 :slices 12 :max-triangles 2600
          :enabled #{:receiver :barrel :stock :grip :magazine :optic :muzzle :handguard}}
   :gameplay {:segments 8 :stacks 5 :slices 8 :max-triangles 1200
              :enabled #{:receiver :barrel :stock :grip :magazine :optic :muzzle :handguard}}
   :crowd {:segments 6 :stacks 4 :slices 6 :max-triangles 520
           :enabled #{:receiver :barrel :stock :magazine}}})

(def ^:private component-order
  [:receiver :stock :grip :magazine :handguard :barrel :optic :muzzle])

(defn- normalize3 [[x y z]]
  (let [n (#?(:clj Math/sqrt :cljs js/Math.sqrt) (+ (* x x) (* y y) (* z z)))]
    (if (> n 1.0e-9) [(/ x n) (/ y n) (/ z n)] [0.0 1.0 0.0])))

(defn- transform [[positions normals uvs indices] [sx sy sz] [tx ty tz]]
  {:positions (vec (mapcat (fn [[x y z]] [(+ tx (* sx x)) (+ ty (* sy y)) (+ tz (* sz z))])
                           (partition 3 positions)))
   :normals (vec (mapcat (fn [[x y z]] (normalize3 [(/ x sx) (/ y sy) (/ z sz)]))
                         (partition 3 normals)))
   :uvs (vec uvs) :indices (vec indices)})

(defn- transform-y-cylinder-to-z [[positions normals uvs indices] center-z]
  ;; +90 degrees around X: [x y z] -> [x -z y], preserving winding.
  {:positions (vec (mapcat (fn [[x y z]] [x (- z) (+ center-z y)])
                           (partition 3 positions)))
   :normals (vec (mapcat (fn [[x y z]] [x (- z) y]) (partition 3 normals)))
   :uvs (vec uvs) :indices (vec indices)})

(defn mesh-bounds [generated]
  (let [points (partition 3 (:positions generated))
        mn (reduce #(mapv min %1 %2) [##Inf ##Inf ##Inf] points)
        mx (reduce #(mapv max %1 %2) [##-Inf ##-Inf ##-Inf] points)]
    {:min mn :max mx :center (mapv #(* 0.5 (+ %1 %2)) mn mx)
     :half (mapv #(* 0.5 (- %2 %1)) mn mx)}))

(defn- variant-parameters [seed]
  {:stock-scale (+ 0.92 (* 0.04 (mod seed 5)))
   :handguard-scale (+ 0.94 (* 0.03 (mod (quot seed 5) 5)))
   :optic-height (+ 0.90 (* 0.05 (mod (quot seed 25) 5)))
   :muzzle-scale (+ 0.88 (* 0.06 (mod (quot seed 125) 5)))})

(defn- component-specs [policy variant]
  (let [{:keys [stock-scale handguard-scale optic-height muzzle-scale]} variant]
    {:receiver {:shape :rounded :size [0.20 0.22 0.62] :center [0.0 0.0 0.20] :material-role :weapon}
     :barrel {:shape :cylinder :radius 0.045 :length 0.72 :center-z 0.86
              :segments (:segments policy) :material-role :weapon}
     :stock {:shape :rounded :size [0.17 0.20 (* 0.46 stock-scale)]
             :center [0.0 0.0 -0.34] :material-role :weapon}
     :grip {:shape :rounded :size [0.11 0.32 0.16] :center [0.045 -0.23 -0.01]
            :material-role :accent}
     :magazine {:shape :rounded :size [0.13 0.34 0.19] :center [0.0 -0.25 0.28]
                :material-role :weapon}
     :optic {:shape :rounded :size [0.11 (* 0.13 optic-height) 0.24]
             :center [0.0 0.18 0.30] :material-role :emissive}
     :muzzle {:shape :cylinder :radius 0.066 :length (* 0.17 muzzle-scale) :center-z 1.30
              :segments (:segments policy) :material-role :emissive}
     :handguard {:shape :rounded :size [(* 0.22 handguard-scale) 0.20 0.46]
                 :center [0.0 0.0 0.65] :material-role :accent}}))

(defn- component-mesh [policy {:keys [shape size center radius length center-z segments]}]
  (case shape
    :rounded (transform (mesh/sphere (:stacks policy) (:slices policy)) size center)
    :cylinder (transform-y-cylinder-to-z
               (mesh/cylinder-pipe radius 0.0 length segments) center-z)))

(defn- append-component [{:keys [positions normals uvs indices ranges] :as acc}
                         component-id spec]
  (let [generated (:generated spec)
        vertex-base (quot (count positions) 3)
        index-start (count indices)
        component-indices (mapv #(+ vertex-base %) (:indices generated))
        index-count (count component-indices)]
    {:positions (into positions (:positions generated))
     :normals (into normals (:normals generated))
     :uvs (into uvs (:uvs generated))
     :indices (into indices component-indices)
     :ranges (conj ranges {:component component-id
                           :material-role (:material-role spec)
                           :bounds (mesh-bounds generated)
                           :index-start index-start :index-count index-count
                           :triangle-count (quot index-count 3)})}))

(defn resolve-weapon
  "Generate an actual indexed rifle mesh for one visual tier and entity."
  [{:keys [family tier entity-id character-preset team-palette]
    :or {family :stylized tier :gameplay entity-id :weapon/default
         character-preset :combat-readable team-palette {}}}]
  (when-not (contains? (:implemented-families family-boundary) family)
    (throw (ex-info "Weapon visual family is not implemented"
                    {:family family :implemented (:implemented-families family-boundary)})))
  (let [policy (get tier-policy tier)]
    (when-not policy
      (throw (ex-info "Unknown weapon mesh tier"
                      {:tier tier :known-tiers (set (keys tier-policy))})))
    (let [seed (equipment/stable-seed entity-id)
          variant (variant-parameters seed)
          specs (component-specs policy variant)
          enabled (:enabled policy)
          selected (filter enabled component-order)
          assembled (reduce (fn [acc id]
                              (let [spec (get specs id)
                                    generated (component-mesh policy spec)]
                                (append-component acc id (assoc spec :generated generated))))
                            {:positions [] :normals [] :uvs [] :indices [] :ranges []}
                            selected)
          triangles (quot (count (:indices assembled)) 3)
          character (character/resolve-character
                     {:family family :preset character-preset :silhouette-tier tier})
          materials (character-material/lower-library character {:team-palette team-palette})
          used-roles (set (map :material-role (:ranges assembled)))]
      (when (> triangles (:max-triangles policy))
        (throw (ex-info "Generated weapon exceeds tier triangle budget"
                        {:tier tier :triangles triangles :max-triangles (:max-triangles policy)})))
      {:contract contract
       :family family :tier tier :entity-id entity-id
       :space :weapon-local :lod {:tier tier :segments (:segments policy)
                                  :stacks (:stacks policy) :slices (:slices policy)}
       :mesh {:positions (:positions assembled) :normals (:normals assembled)
              :uvs (:uvs assembled) :indices (:indices assembled)}
       :bounds (mesh-bounds assembled)
       :fit-volume {:kind :capsule :a [0.0 0.0 -0.57] :b [0.0 0.0 1.39]
                    :radius 0.14}
       :material-ranges (:ranges assembled)
       :materials (select-keys materials used-roles)
       :components (into {}
                         (for [id component-order]
                           [id {:semantic-id (keyword "weapon.component" (name id))
                                :enabled? (contains? enabled id)
                                :material-role (:material-role (get specs id))
                                :reason (when-not (contains? enabled id) :lod-merged-or-culled)}]))
       :sockets sockets
       :attachment (:attachment sockets)
       :outline-policy {:mode :screen-space :source :render/style
                        :material-roles used-roles}
       :variation {:seed seed :parameters variant}
       :budget {:triangle-count triangles :max-triangles (:max-triangles policy)
                :headroom (- (:max-triangles policy) triangles)}})))
